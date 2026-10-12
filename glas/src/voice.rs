//! On-device voice support.
//!
//! Two independent halves:
//! * a push-to-talk pipeline (PulseAudio `parecord` + `whisper-cli`);
//! * a model store shared with harbour-opencode (same `models/` directory and
//!   `catalog.json`), so the same GGML/ONNX files are never downloaded twice.
//!
//! The store speaks the same JSON protocol as the opencode QML
//! (`voice_status_json` / `voice_command`).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Bundled whisper.cpp command line tool.
pub const WHISPER_BIN: &str = "/usr/libexec/harbour-glas/whisper-cli";
/// PulseAudio source used for push-to-talk.
pub const INPUT_SOURCE: &str = "source.primary_input";
/// Bundled Piper (TTS) binary. Its home directory holds `espeak-ng-data`, so
/// piper must run with `PIPER_HOME` as its working directory.
pub const PIPER_HOME: &str = "/usr/libexec/harbour-glas/piper";
pub const PIPER_BIN: &str = "/usr/libexec/harbour-glas/piper/piper";

// ───────────────────────────── push-to-talk ─────────────────────────────

/// Push-to-talk recorder backed by a `parecord` child process.
#[derive(Default)]
pub struct Recorder {
    child: Option<Child>,
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        self.child.is_some()
    }

    /// Start capturing a 16 kHz mono 16-bit WAV into `wav`.
    pub fn start(&mut self, wav: &Path) -> std::io::Result<()> {
        if self.child.is_some() {
            return Ok(());
        }
        let child = Command::new("parecord")
            .arg(format!("--device={INPUT_SOURCE}"))
            .arg("--format=s16le")
            .arg("--rate=16000")
            .arg("--channels=1")
            .arg("--file-format=wav")
            .arg(wav)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.child = Some(child);
        Ok(())
    }

    /// SIGTERM so `parecord` finalises the WAV header, then reap the child.
    pub fn stop(&mut self) -> std::io::Result<()> {
        if let Some(mut child) = self.child.take() {
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
            let _ = child.wait();
        }
        Ok(())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Transcribe `wav` with `whisper-cli`.
///
/// `audio_ctx` is the `-c` override added by the bundled patch: whisper encodes
/// a full 30 s window by default, shrinking it to the real clip length cuts
/// latency several-fold. `0` keeps the default.
pub fn transcribe(
    model: &Path,
    lang: &str,
    threads: u32,
    wav: &Path,
    audio_ctx: u32,
) -> Result<String, String> {
    let mut cmd = Command::new(WHISPER_BIN);
    cmd.arg("-m")
        .arg(model)
        .arg("-l")
        .arg(lang)
        .arg("-t")
        .arg(threads.max(1).to_string())
        .arg("-nt")
        .stdin(Stdio::null());
    if audio_ctx > 0 {
        cmd.arg("-c").arg(audio_ctx.to_string());
    }
    cmd.arg("-f").arg(wav);

    let out = cmd.output().map_err(|e| format!("whisper: {e}"))?;
    if !out.status.success() {
        let err: String = String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(400)
            .collect();
        return Err(format!("whisper failed: {err}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Encoder context size for a 16 kHz mono 16-bit WAV, in 20 ms frames.
pub fn wav_audio_ctx(wav: &Path) -> u32 {
    let bytes = std::fs::metadata(wav).map(|m| m.len()).unwrap_or(0);
    let samples = bytes.saturating_sub(44) / 2;
    let secs = samples as f64 / 16_000.0;
    let ctx = (secs * 50.0).ceil() as u32 + 50;
    ctx.clamp(100, 1500)
}

// ───────────────────────────── text-to-speech ─────────────────────────────

/// Directory of the on-disk TTS cache (synthesized WAVs keyed by content).
fn tts_cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/defaultuser"));
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".cache"));
    cache.join("harbour-glas").join("tts")
}

/// Stable cache path for a `(model, spoken)` pair, so replaying the same
/// answer reuses the WAV instead of running piper again.
fn tts_cache_path(model: &str, spoken: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    model.hash(&mut h);
    0u8.hash(&mut h);
    spoken.hash(&mut h);
    tts_cache_dir().join(format!("{:016x}.wav", h.finish()))
}

/// Caps for the persistent TTS cache (LRU by mtime when exceeded).
const MAX_TTS_FILES: usize = 200;
const MAX_TTS_BYTES: u64 = 200 * 1024 * 1024; // 200 MB

/// Trim the cache down to the caps above. The newest file (just written) is
/// never evicted first.
pub fn enforce_cache_limit() {
    let dir = tts_cache_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut total: u64 = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("wav") {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        total += md.len();
        files.push((md.modified().unwrap_or(std::time::UNIX_EPOCH), md.len(), p));
    }
    if files.len() <= MAX_TTS_FILES && total <= MAX_TTS_BYTES {
        return;
    }
    files.sort_by_key(|(t, _, _)| *t); // oldest first
    let mut count = files.len();
    for (_, len, p) in files {
        if count <= MAX_TTS_FILES && total <= MAX_TTS_BYTES {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total = total.saturating_sub(len);
            count -= 1;
        }
    }
}

/// Remove inline markup: code spans, emphasis/strike markers, table pipes and
/// link targets (`[text](url)` keeps `text`).
fn clean_inline_tts(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' | '*' | '_' | '~' => {}
            '|' => out.push(' '),
            '[' | ']' => {}
            '(' => {
                for d in chars.by_ref() {
                    if d == ')' {
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Turn markdown into plain prose for the speech engine: drop headings, lists,
/// block quotes, code fences and horizontal rules.
pub fn strip_markdown_for_tts(md: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || t.is_empty() {
            continue;
        }
        if t.chars().all(|c| "-*_~".contains(c)) && t.chars().count() >= 3 {
            continue;
        }
        let rest = t
            .trim_start_matches(|c| c == '#' || c == '>' || c == '|')
            .trim_start();
        let rest = rest
            .strip_prefix('-')
            .or_else(|| rest.strip_prefix('*'))
            .or_else(|| rest.strip_prefix('+'))
            .map(|s| s.trim_start())
            .unwrap_or(rest);
        let rest = {
            let bytes = rest.as_bytes();
            let mut i = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > 0 && i < bytes.len() && bytes[i] == b'.' {
                rest[i + 1..].trim_start()
            } else {
                rest
            }
        };
        let cleaned = clean_inline_tts(rest);
        if !cleaned.is_empty() {
            out.push_str(&cleaned);
            out.push(' ');
        }
    }
    out.trim().to_string()
}

/// Synthesis voice selected in `voice_state.json` ("" if none).
pub fn selected_tts() -> String {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str::<VoiceState>(&s).ok())
        .map(|s| s.tts)
        .unwrap_or_default()
}

/// Resolve the selected Piper voice: `(model.onnx, config.onnx.json)`.
pub fn resolve_piper() -> Option<(PathBuf, PathBuf)> {
    let id = selected_tts();
    if id.is_empty() {
        return None;
    }
    let dir = models_root().join(&id);
    let mut onnx = None;
    let mut json = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.ends_with(".onnx.json") {
            json = Some(path);
        } else if name.ends_with(".onnx") {
            onnx = Some(path);
        }
    }
    match (onnx, json) {
        (Some(m), Some(c)) if m.is_file() && c.is_file() => Some((m, c)),
        _ => None,
    }
}

/// Synthesize `text` with the selected voice into the on-disk cache and return
/// the WAV path.
///
/// The Rust process never touches the audio device: playback is done by the UI
/// (QML `MediaPlayer`), exactly as in the reference app. `cancel` is polled
/// after synthesis so a "stop" press discards the freshly written file.
pub fn synthesize(text: &str, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let (model, config) = resolve_piper()
        .ok_or_else(|| "No speech voice installed (Settings → Voice models)".to_string())?;
    if !Path::new(PIPER_BIN).is_file() {
        return Err(format!("piper not found at {PIPER_BIN}"));
    }
    let spoken = strip_markdown_for_tts(text);
    if spoken.is_empty() {
        return Err("empty text".into());
    }
    let id = selected_tts();
    let dir = tts_cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let final_wav = tts_cache_path(&id, &spoken);
    if final_wav.is_file() {
        log::info!("tts: cache hit {}", final_wav.display());
        return Ok(final_wav);
    }

    let out_wav = dir.join(format!(".tmp-{}.wav", std::process::id()));
    let mut child = Command::new(PIPER_BIN)
        .current_dir(PIPER_HOME)
        .arg("-m")
        .arg(&model)
        .arg("-c")
        .arg(&config)
        .arg("-f")
        .arg(&out_wav)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("piper: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write as _;
        stdin
            .write_all(spoken.as_bytes())
            .map_err(|e| format!("piper stdin: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err: String = String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(200)
            .collect();
        return Err(format!("piper failed: {err}"));
    }
    if cancel.load(Ordering::SeqCst) {
        log::info!("tts: cancelled, discarding file");
        let _ = std::fs::remove_file(&out_wav);
        return Err("cancelled".into());
    }
    std::fs::rename(&out_wav, &final_wav).map_err(|e| e.to_string())?;
    enforce_cache_limit();
    log::info!("tts: synthesized {}", final_wav.display());
    Ok(final_wav)
}

// ───────────────────────────── model store ─────────────────────────────

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Shared data directory: harbour-opencode's when present (so both apps use
/// one set of models), otherwise Glas's own.
pub fn store_dir() -> PathBuf {
    let oc = home().join(".local/share/harbour-opencode");
    if oc.is_dir() {
        oc
    } else {
        home().join(".local/share/harbour-glas")
    }
}

/// Directory holding all downloaded models.
pub fn models_root() -> PathBuf {
    store_dir().join("models")
}

fn catalog_path() -> PathBuf {
    store_dir().join("catalog.json")
}

/// Packaged catalog used as a fallback when the working copy is missing.
fn packaged_catalog() -> PathBuf {
    let oc = PathBuf::from("/usr/share/harbour-opencode/models.json");
    if oc.is_file() {
        oc
    } else {
        PathBuf::from("/usr/share/harbour-glas/models.json")
    }
}

fn state_path() -> PathBuf {
    store_dir().join("voice_state.json")
}

/// A catalog entry (fields mirror `catalog.json`).
#[derive(Debug, Clone, Deserialize)]
struct ModelEntry {
    model_id: String,
    name: String,
    engine: String,
    lang_id: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    hidden: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Lang {
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct Catalog {
    #[serde(default)]
    version: String,
    #[serde(default)]
    langs: Vec<Lang>,
    #[serde(default)]
    models: Vec<ModelEntry>,
}

/// Selection persisted next to the models (shared with opencode).
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
struct VoiceState {
    #[serde(default)]
    lang: String,
    #[serde(default)]
    stt: String,
    #[serde(default)]
    tts: String,
    #[serde(default)]
    tts_mode: String,
}

/// A running download of one model.
struct Download {
    model_id: String,
    total: u64,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicI32>,
    finished: Arc<AtomicBool>,
    error: Arc<AtomicBool>,
    /// `(final, part)` paths, used for live progress and cleanup.
    targets: Vec<(PathBuf, PathBuf)>,
}

/// Model catalog + selection + active downloads.
pub struct VoiceStore {
    catalog: Catalog,
    state: VoiceState,
    downloads: Vec<Download>,
    errors: std::collections::HashSet<String>,
}

impl Default for VoiceStore {
    fn default() -> Self {
        Self::load()
    }
}

fn file_name(url: &str) -> String {
    url.rsplit('/').next().unwrap_or("").to_string()
}

fn first_bin(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("bin") {
            return Some(path);
        }
    }
    None
}

impl VoiceStore {
    pub fn load() -> Self {
        let mut store = VoiceStore {
            catalog: Self::read_catalog(),
            state: Self::read_state(),
            downloads: Vec::new(),
            errors: std::collections::HashSet::new(),
        };
        // Default to the single installed STT model if nothing is chosen yet.
        store.validate_state();
        store
    }

    fn read_catalog() -> Catalog {
        for path in [catalog_path(), packaged_catalog()] {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cat) = serde_json::from_str::<Catalog>(&text) {
                    return cat;
                }
            }
        }
        Catalog::default()
    }

    fn read_state() -> VoiceState {
        std::fs::read_to_string(state_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_state(&self) -> std::io::Result<()> {
        let path = state_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, serde_json::to_string(&self.state)?)
    }

    /// First downloaded model of an engine ("" if none).
    fn auto_model(&self, engine: &str) -> String {
        self.catalog
            .models
            .iter()
            .find(|m| m.engine == engine && self.is_downloaded(m))
            .map(|m| m.model_id.clone())
            .unwrap_or_default()
    }

    /// Re-index the disk and repair the selection: the chosen model may have
    /// been removed by another app (the store is shared with opencode), so
    /// fall back to whatever is actually present.
    pub fn validate_state(&mut self) {
        let stt_ok = !self.state.stt.is_empty()
            && self
                .catalog
                .models
                .iter()
                .any(|m| m.model_id == self.state.stt && self.is_downloaded(m));
        let tts_ok = !self.state.tts.is_empty()
            && self
                .catalog
                .models
                .iter()
                .any(|m| m.model_id == self.state.tts && self.is_downloaded(m));
        let mut changed = false;
        if !stt_ok {
            let auto = self.auto_model("stt_whisper");
            if auto != self.state.stt {
                self.state.stt = auto;
                changed = true;
            }
        }
        if !tts_ok {
            let auto = self.auto_model("tts_piper");
            if auto != self.state.tts {
                self.state.tts = auto;
                changed = true;
            }
        }
        if changed {
            let _ = self.save_state();
        }
    }

    fn model_dir(&self, m: &ModelEntry) -> PathBuf {
        models_root().join(&m.model_id)
    }

    fn is_downloaded(&self, m: &ModelEntry) -> bool {
        if m.urls.is_empty() {
            return false;
        }
        let dir = self.model_dir(m);
        m.urls.iter().all(|url| {
            let name = file_name(url);
            !name.is_empty() && dir.join(name).is_file()
        })
    }

    fn download_done(&self, d: &Download) -> u64 {
        let mut done = 0;
        for (final_path, part_path) in &d.targets {
            done += std::fs::metadata(final_path).map(|m| m.len()).unwrap_or(0);
            done += std::fs::metadata(part_path).map(|m| m.len()).unwrap_or(0);
        }
        done.min(d.total)
    }

    fn reap_finished(&mut self) {
        let mut i = 0;
        while i < self.downloads.len() {
            if self.downloads[i].finished.load(Ordering::SeqCst) {
                let d = self.downloads.remove(i);
                if d.error.load(Ordering::SeqCst) {
                    self.errors.insert(d.model_id.clone());
                } else {
                    self.errors.remove(&d.model_id);
                }
                for (_, part_path) in &d.targets {
                    let _ = std::fs::remove_file(part_path);
                }
            } else {
                i += 1;
            }
        }
    }

    /// JSON consumed by the QML: `{models, stt, tts, lang, langs}`.
    pub fn status_json(&mut self) -> String {
        self.reap_finished();
        self.validate_state();
        let mut models = Vec::new();
        for m in &self.catalog.models {
            if m.engine != "stt_whisper" && m.engine != "tts_piper" {
                continue;
            }
            let downloading = self.downloads.iter().find(|d| d.model_id == m.model_id);
            let downloaded = self.is_downloaded(m);
            let state = if downloading.is_some() {
                "downloading"
            } else if self.errors.contains(&m.model_id) {
                "error"
            } else {
                ""
            };
            let done = downloading.map(|d| self.download_done(d)).unwrap_or(0);
            models.push(serde_json::json!({
                "model_id": m.model_id,
                "name": m.name,
                "size": m.size.parse::<u64>().unwrap_or(0),
                "lang_id": m.lang_id,
                "engine": m.engine,
                "downloaded": downloaded,
                "state": state,
                "done": done,
                "hidden": m.hidden,
            }));
        }
        serde_json::json!({
            "models": models,
            "stt": self.state.stt,
            "tts": self.state.tts,
            "lang": self.state.lang,
            "langs": self.catalog.langs,
            "version": self.catalog.version,
        })
        .to_string()
    }

    /// Handle a `voice_command` from the QML.
    pub fn command(&mut self, cmd: &str, val: &str) {
        match cmd {
            "voice_select_stt" => {
                self.state.stt = val.to_string();
                let _ = self.save_state();
            }
            "voice_select_tts" => {
                self.state.tts = val.to_string();
                let _ = self.save_state();
            }
            "voice_lang" => {
                self.state.lang = val.to_string();
                let _ = self.save_state();
            }
            "voice_download" => self.start_download(val),
            "voice_download_cancel" => self.cancel_download(val),
            "voice_delete" => self.delete_model(val),
            "voice_catalog_update" => {
                self.catalog = Self::read_catalog();
                log::info!("voice: catalog reloaded ({} models)", self.catalog.models.len());
            }
            other => log::info!("voice_command ignored: {other}"),
        }
    }

    fn start_download(&mut self, model_id: &str) {
        if self.downloads.iter().any(|d| d.model_id == model_id) {
            return;
        }
        let model = match self.catalog.models.iter().find(|m| m.model_id == model_id) {
            Some(m) => m.clone(),
            None => return,
        };
        let dir = self.model_dir(&model);
        let _ = std::fs::create_dir_all(&dir);
        self.errors.remove(model_id);
        log::info!("voice: downloading {model_id} ({} files)", model.urls.len());

        let cancel = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicI32::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        let error = Arc::new(AtomicBool::new(false));

        let mut jobs: Vec<(String, PathBuf, PathBuf)> = Vec::new();
        for url in &model.urls {
            let name = file_name(url);
            if name.is_empty() {
                continue;
            }
            jobs.push((url.clone(), dir.join(&name), dir.join(format!("{name}.part"))));
        }

        let (c, p, f, e) = (
            cancel.clone(),
            pid.clone(),
            finished.clone(),
            error.clone(),
        );
        let thread_jobs = jobs.clone();
        std::thread::spawn(move || {
            for (url, final_path, part_path) in thread_jobs {
                if c.load(Ordering::SeqCst) {
                    break;
                }
                let spawned = Command::new("curl")
                    .arg("-L")
                    .arg("--fail")
                    .arg("--silent")
                    .arg("--show-error")
                    .arg("-o")
                    .arg(&part_path)
                    .arg(&url)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match spawned {
                    Ok(mut child) => {
                        p.store(child.id() as i32, Ordering::SeqCst);
                        let ok = child.wait().map(|s| s.success()).unwrap_or(false);
                        p.store(0, Ordering::SeqCst);
                        if c.load(Ordering::SeqCst) {
                            let _ = std::fs::remove_file(&part_path);
                            break;
                        }
                        if ok {
                            let _ = std::fs::rename(&part_path, &final_path);
                        } else {
                            e.store(true, Ordering::SeqCst);
                            let _ = std::fs::remove_file(&part_path);
                            break;
                        }
                    }
                    Err(_) => {
                        e.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
            f.store(true, Ordering::SeqCst);
        });

        self.downloads.push(Download {
            model_id: model_id.to_string(),
            total: model.size.parse().unwrap_or(0),
            cancel,
            pid,
            finished,
            error,
            targets: jobs.into_iter().map(|(_, a, b)| (a, b)).collect(),
        });
    }

    fn cancel_download(&mut self, model_id: &str) {
        if let Some(d) = self.downloads.iter().find(|d| d.model_id == model_id) {
            d.cancel.store(true, Ordering::SeqCst);
            let pid = d.pid.load(Ordering::SeqCst);
            if pid > 0 {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
            }
        }
        self.errors.remove(model_id);
    }

    fn delete_model(&mut self, model_id: &str) {
        self.cancel_download(model_id);
        let dir = models_root().join(model_id);
        let _ = std::fs::remove_dir_all(&dir);
        self.errors.remove(model_id);
        if self.state.stt == model_id {
            self.state.stt.clear();
        }
        if self.state.tts == model_id {
            self.state.tts.clear();
        }
        let _ = self.save_state();
        log::info!("voice: deleted {model_id}");
    }
}

/// Resolve the whisper GGML model: explicit setting, the selection in
/// `voice_state.json`, then any installed `*.bin`.
pub fn resolve_model(setting: &str) -> Option<PathBuf> {
    if !setting.is_empty() {
        let path = PathBuf::from(setting);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(text) = std::fs::read_to_string(state_path()) {
        if let Ok(state) = serde_json::from_str::<VoiceState>(&text) {
            if !state.stt.is_empty() {
                if let Some(path) = first_bin(&models_root().join(&state.stt)) {
                    return Some(path);
                }
            }
        }
    }
    let root = models_root();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                if let Some(path) = first_bin(&dir) {
                    return Some(path);
                }
            }
        }
    }
    None
}
