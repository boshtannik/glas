//! LLM model store for thinking models.
//!
//! Provides model download/selection for GGUF LLM models (role `llm`).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};


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

/// Glas's own LLM catalog, kept apart from opencode's shared `catalog.json`
/// (which holds STT/TTS) so neither app overwrites the other's entries.
/// Written by the Hugging Face updater; takes precedence when present.
fn llm_catalog_path() -> PathBuf {
    store_dir().join("llm_catalog.json")
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
    #[serde(alias = "engine")]
    #[serde(default)]
    engine: String,
    #[serde(alias = "role")]
    #[serde(default)]
    role: String,
    #[serde(default)]
    lang_id: String,
    #[serde(default)]
    size: String,
    #[serde(default, alias = "desc", alias = "summary")]
    description: String,
    #[serde(default, alias = "quantization", alias = "quant_type")]
    quant: String,
    #[serde(default, alias = "param_count", alias = "params_b")]
    params: String,
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
    llm: String,
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

/// Model catalog + selection + active downloads (LLM only).
pub struct LlmStore {
    catalog: Catalog,
    state: VoiceState,
    downloads: Vec<Download>,
    errors: std::collections::HashSet<String>,
    /// Free-text search over name/id/quant/description (applied server-side).
    query: String,
    /// Language filter ("auto" = no filter).
    filter_lang: String,
    /// In-flight Hugging Face catalog update.
    updating: bool,
    /// Last catalog-update failure ("" when the last run succeeded).
    update_error: String,
    /// Receives the result of the background catalog fetch.
    update_rx: Option<mpsc::Receiver<Result<usize, String>>>,
}

impl Default for LlmStore {
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

// ───────────────────────── device capability & fit ─────────────────────────

/// Startup/scratch overhead of the llama.cpp runtime (rough).
const RUNTIME_OVERHEAD: u64 = 160 * 1024 * 1024;

/// RAM we keep free so the system (and Glas itself) stays responsive.
const RAM_RESERVE: u64 = 512 * 1024 * 1024;

/// Free memory (`MemAvailable`) in bytes; 0 if unknown.
///
/// Swap is deliberately NOT counted: on Sailfish it's zram (compressed RAM),
/// so it holds no extra capacity — adding `SwapFree` would make a 5.3 GB phone
/// look like it has ~5.6 GB free.
fn mem_available_bytes() -> u64 {
    let mut avail = 0u64;
    if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
        for line in text.lines() {
            let mut it = line.split_whitespace();
            if it.next().unwrap_or("") == "MemAvailable:" {
                if let Some(kb) = it.next().and_then(|v| v.parse::<u64>().ok()) {
                    avail = kb.saturating_mul(1024);
                }
            }
        }
    }
    avail
}

/// Free bytes on the filesystem holding `path`; 0 if unknown.
fn disk_available_bytes(path: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let mut c_path = path.as_os_str().as_bytes().to_vec();
    c_path.push(0);
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr() as *const libc::c_char, &mut st) == 0 {
            (st.f_bavail as u64).saturating_mul(st.f_frsize as u64)
        } else {
            0
        }
    }
}

/// Human-readable size ("" for zero).
fn human_size(b: u64) -> String {
    if b == 0 {
        return String::new();
    }
    let g = b as f64 / (1024.0 * 1024.0 * 1024.0);
    if g >= 1.0 {
        return format!("{g:.1} GB");
    }
    let m = b as f64 / (1024.0 * 1024.0);
    if m >= 1.0 {
        return format!("{} MB", m.round() as u64);
    }
    format!("{} KB", b / 1024)
}

/// Estimate whether a model fits: `need` vs usable RAM and free disk.
/// `usable RAM = MemAvailable - RAM_RESERVE`. Returns
/// `("green"|"yellow"|"red"|"", need_bytes)`.
fn fit_for(size: u64, ram_avail: u64, disk_avail: u64) -> (&'static str, u64) {
    if size == 0 {
        return ("", 0);
    }
    let need = size
        .saturating_add(size / 8)
        .saturating_add(RUNTIME_OVERHEAD);
    let usable = ram_avail.saturating_sub(RAM_RESERVE);
    let fits_ram = ram_avail == 0 || need <= usable;
    let fits_disk = disk_avail == 0 || size <= disk_avail;
    let fit = if !fits_ram || !fits_disk {
        "red"
    } else if (ram_avail > 0 && need.saturating_mul(10) > usable.saturating_mul(8))
        || (disk_avail > 0 && size.saturating_mul(10) > disk_avail.saturating_mul(8))
    {
        "yellow"
    } else {
        "green"
    };
    (fit, need)
}

// ───────────────────────── Hugging Face catalog ─────────────────────────

/// A hand-picked Hugging Face repo surfaced as an LLM. The GGUF file and its
/// exact size are resolved live (see [`fetch_llm_catalog`]); we only pin the
/// repo id, a stable model id and the display metadata.
struct HfRepo {
    repo: &'static str,
    id: &'static str,
    name: &'static str,
    lang: &'static str,
    params: &'static str,
    desc: &'static str,
}

/// Curated GGUF candidates for llama.cpp. Kept deliberately small.
const CURATED: &[HfRepo] = &[
    HfRepo {
        repo: "Qwen/Qwen2.5-0.5B-Instruct-GGUF",
        id: "qwen2.5-0.5b-instruct-q4_k_m",
        name: "Qwen2.5 0.5B Instruct",
        lang: "multilang",
        params: "0.5B",
        desc: "Самая лёгкая Qwen2.5 — быстрый оффлайн-черновик и короткие ответы.",
    },
    HfRepo {
        repo: "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
        id: "qwen2.5-1.5b-instruct-q4_k_m",
        name: "Qwen2.5 1.5B Instruct",
        lang: "multilang",
        params: "1.5B",
        desc: "Баланс качества и размера; хороша для извлечения интентов.",
    },
    HfRepo {
        repo: "Qwen/Qwen2.5-3B-Instruct-GGUF",
        id: "qwen2.5-3b-instruct-q4_k_m",
        name: "Qwen2.5 3B Instruct",
        lang: "multilang",
        params: "3B",
        desc: "Заметно умнее, но тяжелее — на грани по памяти.",
    },
    HfRepo {
        repo: "HuggingFaceTB/SmolLM2-360M-Instruct-GGUF",
        id: "smollm2-360m-instruct-q8_0",
        name: "SmolLM2 360M Instruct",
        lang: "en",
        params: "360M",
        desc: "Крошечная англоязычная модель для самых слабых устройств.",
    },
    HfRepo {
        repo: "HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF",
        id: "smollm2-1.7b-instruct-q4_k_m",
        name: "SmolLM2 1.7B Instruct",
        lang: "en",
        params: "1.7B",
        desc: "Крупнее 360M; только английский.",
    },
    HfRepo {
        repo: "bartowski/gemma-2-2b-it-GGUF",
        id: "gemma-2-2b-it-q4_k_m",
        name: "Gemma 2 2B Instruct",
        lang: "multilang",
        params: "2B",
        desc: "Google Gemma 2 — сильные многоязычные ответы.",
    },
];

/// GGUF quantization preference, best value first; matched case-insensitively
/// as a substring of the file name.
const QUANT_PREF: &[&str] = &[
    "q4_k_m", "q4_k_s", "q4_0", "q5_k_m", "q3_k_m", "q8_0", "q6_k", "q2_k", "iq4_xs",
];

/// Real file size, preferring the LFS entry (the plain `size` can be a pointer).
fn hf_size(f: &serde_json::Value) -> u64 {
    f.get("lfs")
        .and_then(|l| l.get("size"))
        .and_then(|s| s.as_u64())
        .or_else(|| f.get("size").and_then(|s| s.as_u64()))
        .unwrap_or(0)
}

/// Pick the best GGUF in a repo tree: the first matching [`QUANT_PREF`] file
/// (skipping split shards and projector helpers), else the smallest gguf.
/// Returns `(size, path, quant_label)`.
fn pick_gguf(files: &[serde_json::Value]) -> Option<(u64, String, String)> {
    let candidates: Vec<(u64, String)> = files
        .iter()
        .filter_map(|f| {
            let path = f.get("path").and_then(|p| p.as_str())?;
            let low = path.to_ascii_lowercase();
            if !low.ends_with(".gguf") || low.contains("mmproj") || low.contains("-of-") {
                return None;
            }
            Some((hf_size(f), path.to_string()))
        })
        .collect();
    let hit = QUANT_PREF.iter().find_map(|want| {
        candidates
            .iter()
            .find(|(_, path)| path.to_ascii_lowercase().contains(want))
            .map(|(size, path)| (*size, path.clone(), want.to_ascii_uppercase()))
    });
    hit.or_else(|| {
        candidates
            .iter()
            .min_by_key(|(size, _)| *size)
            .map(|(size, path)| (*size, path.clone(), String::new()))
    })
}

/// Resolve one curated repo to a catalog entry via the Hugging Face tree API.
fn fetch_one(
    client: &reqwest::blocking::Client,
    spec: &HfRepo,
) -> Result<serde_json::Value, String> {
    let api = format!(
        "https://huggingface.co/api/models/{}/tree/main?recursive=true",
        spec.repo
    );
    let body = client
        .get(&api)
        .send()
        .map_err(|e| format!("запрос: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP: {e}"))?
        .text()
        .map_err(|e| format!("чтение: {e}"))?;
    let files: Vec<serde_json::Value> =
        serde_json::from_str(&body).map_err(|e| format!("JSON: {e}"))?;
    let (size, path, quant) = pick_gguf(&files).ok_or("в репозитории нет GGUF")?;
    if size == 0 {
        return Err("нулевой размер файла".to_string());
    }
    let url = format!(
        "https://huggingface.co/{}/resolve/main/{}?download=true",
        spec.repo, path
    );
    Ok(serde_json::json!({
        "model_id": spec.id,
        "name": spec.name,
        "engine": "llm_llamacpp",
        "role": "llm",
        "lang_id": spec.lang,
        "size": size.to_string(),
        "description": spec.desc,
        "quant": quant,
        "params": spec.params,
        "urls": [url],
    }))
}

/// Query the Hugging Face API for every curated repo and write the resulting
/// catalog to [`llm_catalog_path`]. Returns the number of models written.
fn fetch_llm_catalog() -> Result<usize, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("harbour-glas/0.1")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let mut models = Vec::new();
    for spec in CURATED {
        match fetch_one(&client, spec) {
            Ok(v) => models.push(v),
            Err(e) => log::warn!("llm: {}: {e}", spec.repo),
        }
    }
    if models.is_empty() {
        return Err("Hugging Face: ни одной модели не получено".to_string());
    }
    let count = models.len();
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let catalog = serde_json::json!({
        "schema": 1,
        "version": format!("hf-{secs}"),
        "langs": [
            { "id": "multilang", "name": "Multilingual" },
            { "id": "en", "name": "English" },
        ],
        "models": models,
    });
    let dir = store_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("папка данных: {e}"))?;
    let tmp = dir.join("llm_catalog.json.tmp");
    let dest = llm_catalog_path();
    std::fs::write(&tmp, serde_json::to_string_pretty(&catalog).unwrap_or_default())
        .map_err(|e| format!("запись каталога: {e}"))?;
    std::fs::rename(&tmp, &dest).map_err(|e| format!("сохранить каталог: {e}"))?;
    log::info!("llm: HF catalog updated: {count} models -> {}", dest.display());
    Ok(count)
}

impl LlmStore {
    pub fn load() -> Self {
        let mut store = LlmStore {
            catalog: Self::read_catalog(),
            state: Self::read_state(),
            downloads: Vec::new(),
            errors: std::collections::HashSet::new(),
            query: String::new(),
            filter_lang: "auto".to_string(),
            updating: false,
            update_error: String::new(),
            update_rx: None,
        };
        // Default to the single installed STT model if nothing is chosen yet.
        store.validate_state();
        store
    }

    fn read_catalog() -> Catalog {
        for path in [llm_catalog_path(), catalog_path(), packaged_catalog()] {
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

    fn is_llm(&self, m: &ModelEntry) -> bool {
        let r = m.role.to_ascii_lowercase();
        let e = m.engine.to_ascii_lowercase();
        r == "llm" || e == "llm" || e.starts_with("llm")
    }

    /// First downloaded LLM model ("" if none).
    fn auto_llm(&self) -> String {
        self.catalog
            .models
            .iter()
            .find(|m| self.is_llm(m) && self.is_downloaded(m))
            .map(|m| m.model_id.clone())
            .unwrap_or_default()
    }

    /// Re-index the disk and repair the selection: the chosen model may have
    /// been removed by another app (the store is shared with opencode), so
    /// fall back to whatever is actually present.
    pub fn validate_state(&mut self) {
        let llm_ok = !self.state.llm.is_empty()
            && self
                .catalog
                .models
                .iter()
                .any(|m| m.model_id == self.state.llm && self.is_llm(m) && self.is_downloaded(m));
        let mut changed = false;
        if !llm_ok {
            let auto = self.auto_llm();
            if auto != self.state.llm {
                self.state.llm = auto;
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

    /// Kick off a Hugging Face catalog refresh in the background (idempotent).
    fn start_catalog_update(&mut self) {
        if self.updating {
            return;
        }
        self.updating = true;
        self.update_error.clear();
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        std::thread::spawn(move || {
            let res = fetch_llm_catalog();
            let _ = tx.send(res);
        });
    }

    /// Non-blocking drain of the background catalog fetch.
    fn poll_update(&mut self) {
        let msg = match self.update_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(m) => Some(m),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("worker stopped".to_string()))
                }
            },
            None => None,
        };
        if let Some(res) = msg {
            self.update_rx = None;
            self.updating = false;
            match res {
                Ok(n) => {
                    self.catalog = Self::read_catalog();
                    self.update_error.clear();
                    log::info!("llm: catalog reloaded from HF ({n} models)");
                }
                Err(e) => {
                    log::warn!("llm: catalog update failed: {e}");
                    self.update_error = e;
                }
            }
        }
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

    /// JSON consumed by the QML:
    /// `{query, llm, lang, version, langs, device, downloaded[], catalog[]}`.
    pub fn status_json(&mut self) -> String {
        self.reap_finished();
        self.poll_update();
        self.validate_state();

        let ram_avail = mem_available_bytes();
        let disk_avail = disk_available_bytes(&store_dir());

        let mut downloaded = Vec::new();
        let mut catalog = Vec::new();
        for m in &self.catalog.models {
            if !self.is_llm(m) || !self.matches(m) {
                continue;
            }
            let obj = self.model_json(m, ram_avail, disk_avail);
            if self.is_downloaded(m) {
                downloaded.push(obj);
            } else {
                catalog.push(obj);
            }
        }

        let mut on_device_count = 0usize;
        let mut on_device_bytes = 0u64;
        for m in &self.catalog.models {
            if self.is_llm(m) && self.is_downloaded(m) {
                on_device_count += 1;
                on_device_bytes =
                    on_device_bytes.saturating_add(m.size.parse::<u64>().unwrap_or(0));
            }
        }

        serde_json::json!({
            "query": self.query,
            "llm": self.state.llm,
            "lang": self.state.lang,
            "version": self.catalog.version,
            "langs": self.catalog.langs,
            "updating": self.updating,
            "update_error": self.update_error,
            "device": {
                "ram_available": ram_avail,
                "ram_human": human_size(ram_avail),
                "disk_available": disk_avail,
                "disk_human": human_size(disk_avail),
            },
            "on_device": {
                "count": on_device_count,
                "bytes": on_device_bytes,
                "human": human_size(on_device_bytes),
            },
            "downloaded": downloaded,
            "catalog": catalog,
        })
        .to_string()
    }

    /// One model by id (ignores the search filter) — used by the detail card.
    pub fn model_json_by_id(&mut self, id: &str) -> String {
        self.reap_finished();
        self.poll_update();
        self.validate_state();
        let ram_avail = mem_available_bytes();
        let disk_avail = disk_available_bytes(&store_dir());
        let found = self
            .catalog
            .models
            .iter()
            .find(|m| m.model_id.as_str() == id && self.is_llm(m));
        match found {
            Some(m) => self.model_json(m, ram_avail, disk_avail).to_string(),
            None => "{}".to_string(),
        }
    }

    /// Query + language filter (moved out of QML on purpose).
    fn matches(&self, m: &ModelEntry) -> bool {
        if !self.filter_lang.is_empty() && self.filter_lang != "auto" {
            let l = m.lang_id.to_ascii_lowercase();
            if l != "multilang" && l != "multilingual" && m.lang_id != self.filter_lang {
                return false;
            }
        }
        if self.query.is_empty() {
            return true;
        }
        let q = self.query.to_lowercase();
        m.name.to_lowercase().contains(q.as_str())
            || m.model_id.to_lowercase().contains(q.as_str())
            || m.quant.to_lowercase().contains(q.as_str())
            || m.description.to_lowercase().contains(q.as_str())
    }

    fn lang_name(&self, id: &str) -> String {
        if id == "multilang" || id == "multilingual" {
            return "Multilingual".to_string();
        }
        for l in &self.catalog.langs {
            if l.id == id {
                return if l.name.is_empty() {
                    l.id.clone()
                } else {
                    l.name.clone()
                };
            }
        }
        id.to_string()
    }

    /// One model as a display object (sizes, fit, state, human strings).
    fn model_json(&self, m: &ModelEntry, ram_avail: u64, disk_avail: u64) -> serde_json::Value {
        let size = m.size.parse::<u64>().unwrap_or(0);
        let (fit, need) = fit_for(size, ram_avail, disk_avail);
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
        serde_json::json!({
            "model_id": m.model_id,
            "name": m.name,
            "engine": if m.role.is_empty() { m.engine.clone() } else { m.role.clone() },
            "lang_id": m.lang_id,
            "lang_name": self.lang_name(&m.lang_id),
            "size": size,
            "size_human": human_size(size),
            "quant": m.quant,
            "params": m.params,
            "description": m.description,
            "downloaded": downloaded,
            "state": state,
            "done": done,
            "done_human": human_size(done),
            "progress": if size > 0 { (done as f64 / size as f64).min(1.0) } else { 0.0 },
            "fit": fit,
            "need": need,
            "need_human": human_size(need),
        })
    }

    /// Handle a command from the QML.
    pub fn command(&mut self, cmd: &str, val: &str) {
        match cmd {
            "voice_select_llm" | "llm_select" => {
                self.state.llm = val.to_string();
                let _ = self.save_state();
            }
            "voice_lang" => {
                self.state.lang = val.to_string();
                let _ = self.save_state();
            }
            "llm_search" => {
                self.query = val.to_string();
            }
            "llm_filter_lang" => {
                self.filter_lang = val.to_string();
            }
            "voice_download" | "llm_download" => self.start_download(val),
            "voice_download_cancel" | "llm_download_cancel" => self.cancel_download(val),
            "voice_delete" | "llm_delete" => self.delete_model(val),
            "voice_catalog_update" | "llm_catalog_update" => self.start_catalog_update(),
            other => log::info!("llm_command ignored: {other}"),
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
        if self.state.llm == model_id {
            self.state.llm.clear();
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
/// Resolve the selected GGUF LLM model (first .gguf/bin in the model dir).
pub fn resolve_llm() -> Option<PathBuf> {
    if let Ok(text) = std::fs::read_to_string(state_path()) {
        if let Ok(state) = serde_json::from_str::<VoiceState>(&text) {
            if !state.llm.is_empty() {
                let dir = models_root().join(&state.llm);
                for name in ["ggml-model.gguf", "model.gguf"] {
                    let p = dir.join(name);
                    if p.is_file() { return Some(p); }
                }
                if let Some(p) = first_bin(&dir) { return Some(p); }
            }
        }
    }
    let root = models_root();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                let id = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if let Ok(cat) = std::fs::read_to_string(catalog_path()) {
                    let _ = cat;
                }
                for name in ["ggml-model.gguf", "model.gguf"] {
                    let p = dir.join(name);
                    if p.is_file() { return Some(p); }
                }
                if let Some(p) = first_bin(&dir) { return Some(p); }
            }
        }
    }
    None
}
