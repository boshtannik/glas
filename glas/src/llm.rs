//! LLM model store for thinking models.
//!
//! The catalog is browsed **live from the Hugging Face API**:
//! - the list comes from paginated search results (`filter=gguf` +
//!   `filter=text-generation`, newest download ranking) and is trimmed to the
//!   device by parsing the parameter count out of the repo name (no extra
//!   request);
//! - the exact GGUF file, its real size and the download URL are resolved
//!   lazily — with a single `?blobs=true` request per repo — only when the user
//!   opens a card, then cached.
//!
//! Both are cached in Glas's own `llm_catalog.json` / `llm_meta.json`, kept
//! apart from opencode's shared `catalog.json` (which holds STT/TTS).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use reqwest::header::{HeaderMap, LINK};
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

/// Glas's own LLM list cache (from the Hugging Face search).
fn catalog_path() -> PathBuf {
    store_dir().join("llm_catalog.json")
}

/// Cache of the per-repo resolved GGUF file/size/URL, keyed by repo id.
fn meta_path() -> PathBuf {
    store_dir().join("llm_meta.json")
}

fn state_path() -> PathBuf {
    store_dir().join("voice_state.json")
}

/// One model in the browsing list. `size`/`quant`/`urls` live in the resolved
/// cache ([`Detail`]); `est` is the name-derived size estimate used for the
/// badge until the card is opened.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelEntry {
    #[serde(alias = "id")]
    repo: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    params: String,
    #[serde(default)]
    downloads: u64,
    /// Rough size estimate from the parsed parameter count (bytes).
    #[serde(default)]
    est: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Catalog {
    #[serde(default)]
    version: String,
    #[serde(default)]
    models: Vec<ModelEntry>,
}

/// Resolved facts about one repo: the GGUF file we'd download.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Detail {
    size: u64,
    url: String,
    quant: String,
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

struct Page {
    entries: Vec<ModelEntry>,
    next: Option<String>,
}

/// Model catalog + selection + active downloads (LLM only).
pub struct LlmStore {
    catalog: Catalog,
    state: VoiceState,
    downloads: Vec<Download>,
    errors: HashSet<String>,
    /// Free-text search over repo name (applied server-side).
    query: String,
    /// Cursor for the next page of search results (None = end reached).
    next_cursor: Option<String>,
    loading_more: bool,
    list_error: String,
    /// Page-1 refresh in flight.
    updating: bool,
    update_error: String,
    /// `(append?, query, result)` of a list request.
    list_rx: Option<mpsc::Receiver<(bool, String, Result<Page, String>)>>,
    /// Repo whose `?blobs=true` detail is being fetched.
    detail_repo: Option<String>,
    detail_rx: Option<mpsc::Receiver<(String, Result<Detail, String>)>>,
    /// Repos whose detail fetch failed (so we don't retry in a loop).
    detail_failed: HashSet<String>,
    /// Resolved per-repo details, keyed by repo id.
    resolved: HashMap<String, Detail>,
    /// Model whose download should start as soon as its detail resolves.
    pending_download: Option<String>,
    /// Cached device free memory/disk. Refreshed only when data arrives (new
    /// list, resolved detail, finished download, page entered) — not on every
    /// poll, so the fit badge isn't recomputed live.
    ram_avail: u64,
    disk_avail: u64,
}

impl Default for LlmStore {
    fn default() -> Self {
        Self::load()
    }
}

fn file_name(url: &str) -> String {
    url.split('?')
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
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

/// Rough bytes of GGUF weights per 1B parameters at Q4_K_M.
const BYTES_PER_PARAM_Q4: f64 = 0.62e9;

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

// ───────────────────── name-based sizing (no requests) ─────────────────────

/// Parse the parameter count (in billions) out of a repo name.
///
/// Handles `7b`, `1.5B`, `360M`, and MoE tags like `30B-A3B` (takes the
/// largest figure, i.e. the total parameter count).
fn parse_params_b(name: &str) -> Option<f32> {
    let bytes = name.as_bytes();
    let mut i = 0;
    let mut best: Option<f32> = None;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            let mut dot = false;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && !dot)) {
                if bytes[i] == b'.' {
                    dot = true;
                }
                i += 1;
            }
            let num = name[start..i].parse::<f32>().ok();
            let suffix = bytes.get(i).copied();
            let value = match (num, suffix) {
                (Some(v), Some(b'b')) | (Some(v), Some(b'B')) => Some(v),
                (Some(v), Some(b'm')) | (Some(v), Some(b'M')) => Some(v / 1000.0),
                _ => None,
            };
            if let Some(v) = value {
                best = Some(best.map_or(v, |b: f32| b.max(v)));
            }
        } else {
            i += 1;
        }
    }
    best.filter(|b| *b > 0.0)
}

/// "1.5B" / "360M" from a count in billions.
fn format_params(b: f32) -> String {
    if b >= 1.0 {
        if (b - b.round()).abs() < 0.05 {
            format!("{}B", b.round() as u64)
        } else {
            format!("{b:.1}B")
        }
    } else {
        format!("{}M", (b * 1000.0).round() as u64)
    }
}

/// Name-derived Q4 size estimate (bytes) used for the pre-open badge.
fn estimate_bytes(params_b: f32) -> u64 {
    (params_b as f64 * BYTES_PER_PARAM_Q4) as u64
}

/// Largest parameter count (billions) we admit for the available RAM, with the
/// same ~25% headroom the fit badge uses. Cheap and device-specific.
fn max_params_b(ram_avail: u64) -> f32 {
    if ram_avail == 0 {
        return 8.0;
    }
    let usable = ram_avail.saturating_sub(RAM_RESERVE) as f64;
    (usable / (BYTES_PER_PARAM_Q4 * 1.25)) as f32
}

// ───────────────────────── Hugging Face API ─────────────────────────

const HF_API: &str = "https://huggingface.co/api/models";
const PAGE_LIMIT: u32 = 30;

/// GGUF quantization preference, best value first; matched case-insensitively
/// as a substring of the file name.
const QUANT_PREF: &[&str] = &[
    "q4_k_m", "q4_k_s", "q4_0", "q5_k_m", "q3_k_m", "q8_0", "q6_k", "q2_k", "iq4_xs",
];

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("harbour-glas/0.1")
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("http client: {e}"))
}

/// Cursor for the next page, from the `Link: …; rel="next"` header.
fn parse_next_cursor(headers: &HeaderMap) -> Option<String> {
    let link = headers.get(LINK)?.to_str().ok()?;
    let start = link.find("cursor=")? + "cursor=".len();
    let rest = &link[start..];
    let end = rest
        .find(|c| c == '>' || c == '&' || c == ',')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some(rest[..end].to_string())
}

/// Percent-encode a value for a URL query (unreserved chars kept verbatim).
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// One page of HF search results, trimmed to the device by parameter count.
/// `query` is sent server-side as HF's `search` parameter (full text).
fn hf_search(cursor: Option<&str>, max_b: f32, query: &str) -> Result<Page, String> {
    let client = http_client()?;
    let mut url = format!(
        "{HF_API}?filter=gguf&filter=text-generation&sort=downloads&direction=-1&limit={PAGE_LIMIT}"
    );
    if !query.is_empty() {
        url.push_str("&search=");
        url.push_str(&pct_encode(query));
    }
    if let Some(c) = cursor {
        url.push_str("&cursor=");
        url.push_str(c);
    }
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("запрос: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP: {e}"))?;
    let next = parse_next_cursor(resp.headers());
    let body = resp.text().map_err(|e| format!("чтение: {e}"))?;
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(&body).map_err(|e| format!("JSON: {e}"))?;

    let mut entries = Vec::new();
    for m in arr {
        let repo = m
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if repo.is_empty() {
            continue;
        }
        let params_b = parse_params_b(&repo);
        if let Some(b) = params_b {
            if b > max_b {
                continue;
            }
        }
        entries.push(ModelEntry {
            name: repo.clone(),
            repo,
            params: params_b.map(format_params).unwrap_or_default(),
            downloads: m.get("downloads").and_then(|v| v.as_u64()).unwrap_or(0),
            est: params_b.map(estimate_bytes).unwrap_or(0),
        });
    }
    Ok(Page { entries, next })
}

fn hf_size(f: &serde_json::Value) -> u64 {
    f.get("lfs")
        .and_then(|l| l.get("size"))
        .and_then(|s| s.as_u64())
        .or_else(|| f.get("size").and_then(|s| s.as_u64()))
        .unwrap_or(0)
}

/// Pick the best GGUF from `(size, path)` candidates: the first matching
/// [`QUANT_PREF`], else the smallest.
fn pick_gguf(candidates: &[(u64, String)]) -> Option<(u64, String, String)> {
    QUANT_PREF
        .iter()
        .find_map(|want| {
            candidates
                .iter()
                .find(|(_, path)| path.to_ascii_lowercase().contains(want))
                .map(|(size, path)| (*size, path.clone(), want.to_ascii_uppercase()))
        })
        .or_else(|| {
            candidates
                .iter()
                .min_by_key(|(size, _)| *size)
                .map(|(size, path)| (*size, path.clone(), String::new()))
        })
}

/// Resolve one repo to a concrete GGUF file via `?blobs=true` (one request).
fn hf_detail(repo: &str) -> Result<Detail, String> {
    let client = http_client()?;
    let url = format!("https://huggingface.co/api/models/{repo}?blobs=true");
    let body = client
        .get(&url)
        .send()
        .map_err(|e| format!("запрос: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP: {e}"))?
        .text()
        .map_err(|e| format!("чтение: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("JSON: {e}"))?;
    let candidates: Vec<(u64, String)> = v
        .get("siblings")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    let path = f.get("rfilename").and_then(|p| p.as_str())?;
                    let low = path.to_ascii_lowercase();
                    if !low.ends_with(".gguf") || low.contains("mmproj") || low.contains("-of-") {
                        return None;
                    }
                    Some((hf_size(f), path.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let (size, path, quant) = pick_gguf(&candidates).ok_or("в репозитории нет GGUF")?;
    if size == 0 {
        return Err("нулевой размер файла".to_string());
    }
    let url = format!("https://huggingface.co/{repo}/resolve/main/{path}?download=true");
    Ok(Detail { size, url, quant })
}

impl LlmStore {
    pub fn load() -> Self {
        let mut store = LlmStore {
            catalog: Self::read_catalog(),
            state: Self::read_state(),
            downloads: Vec::new(),
            errors: HashSet::new(),
            query: String::new(),
            next_cursor: None,
            loading_more: false,
            list_error: String::new(),
            updating: false,
            update_error: String::new(),
            list_rx: None,
            detail_repo: None,
            detail_rx: None,
            detail_failed: HashSet::new(),
            resolved: Self::read_resolved(),
            pending_download: None,
            ram_avail: 0,
            disk_avail: 0,
        };
        store.refresh_device();
        store.validate_state();
        store
    }

    fn read_catalog() -> Catalog {
        std::fs::read_to_string(catalog_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_catalog(&self) -> std::io::Result<()> {
        let dir = store_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(catalog_path(), serde_json::to_string(&self.catalog)?)
    }

    fn read_resolved() -> HashMap<String, Detail> {
        std::fs::read_to_string(meta_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_resolved(&self) -> std::io::Result<()> {
        let dir = store_dir();
        std::fs::create_dir_all(&dir)?;
        std::fs::write(meta_path(), serde_json::to_string(&self.resolved)?)
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

    fn model_dir(repo: &str) -> PathBuf {
        models_root().join(repo)
    }

    fn is_downloaded(&self, repo: &str) -> bool {
        match self.resolved.get(repo) {
            Some(d) if !d.url.is_empty() => {
                let dir = Self::model_dir(repo);
                let name = file_name(&d.url);
                !name.is_empty() && dir.join(name).is_file()
            }
            _ => false,
        }
    }

    /// First downloaded LLM model ("" if none). Downloaded models live in the
    /// resolved cache, so they stay reachable even after the browsing list is
    /// replaced by a new search.
    fn auto_llm(&self) -> String {
        let mut repos: Vec<&String> = self.resolved.keys().collect();
        repos.sort();
        repos
            .into_iter()
            .find(|r| self.is_downloaded(r))
            .cloned()
            .unwrap_or_default()
    }

    /// Every downloaded model repo (regardless of the current search).
    fn downloaded_repos(&self) -> Vec<String> {
        let mut repos: Vec<String> = self
            .resolved
            .keys()
            .filter(|r| self.is_downloaded(r))
            .cloned()
            .collect();
        repos.sort();
        repos
    }

    /// Re-read free memory/disk. Called when new data arrives, not per poll.
    fn refresh_device(&mut self) {
        self.ram_avail = mem_available_bytes();
        self.disk_avail = disk_available_bytes(&store_dir());
    }

    /// Repair the selection: the chosen model may have been removed by another
    /// app (the store is shared with opencode).
    pub fn validate_state(&mut self) {
        let ok = !self.state.llm.is_empty() && self.is_downloaded(&self.state.llm);
        if !ok {
            let auto = self.auto_llm();
            if auto != self.state.llm {
                self.state.llm = auto;
                let _ = self.save_state();
            }
        }
    }

    // ── list pagination ────────────────────────────────────────────────

    /// Start a list request: page 1 (`append = false`, replaces any in-flight
    /// request — so a new query cancels stale results) or the next page.
    fn start_list(&mut self, append: bool) {
        if append {
            if self.loading_more || self.updating {
                return;
            }
            let Some(cursor) = self.next_cursor.clone() else {
                return;
            };
            self.loading_more = true;
            let query = self.query.clone();
            let max_b = max_params_b(mem_available_bytes());
            let (tx, rx) = mpsc::channel();
            self.list_rx = Some(rx);
            std::thread::spawn(move || {
                let res = hf_search(Some(&cursor), max_b, &query);
                let _ = tx.send((true, query, res));
            });
        } else {
            self.updating = true;
            self.loading_more = false;
            self.update_error.clear();
            self.next_cursor = None;
            let query = self.query.clone();
            let max_b = max_params_b(mem_available_bytes());
            let (tx, rx) = mpsc::channel();
            self.list_rx = Some(rx);
            std::thread::spawn(move || {
                let res = hf_search(None, max_b, &query);
                let _ = tx.send((false, query, res));
            });
        }
    }

    fn poll_list(&mut self) {
        let msg = match self.list_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(m) => Some(m),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some((false, String::new(), Err("worker stopped".to_string())))
                }
            },
            None => None,
        };
        let Some((append, _query, res)) = msg else {
            return;
        };
        self.list_rx = None;
        if append {
            self.loading_more = false;
        } else {
            self.updating = false;
        }
        match res {
            Ok(page) => {
                self.next_cursor = page.next;
                if append {
                    let known: HashSet<String> =
                        self.catalog.models.iter().map(|m| m.repo.clone()).collect();
                    for e in page.entries {
                        if !known.contains(&e.repo) {
                            self.catalog.models.push(e);
                        }
                    }
                } else {
                    self.catalog.models = page.entries;
                    self.catalog.version = format!("hf-{}", now_secs());
                }
                self.list_error.clear();
                self.refresh_device();
                let _ = self.save_catalog();
                log::info!("llm: list {} models", self.catalog.models.len());
            }
            Err(e) => {
                log::warn!("llm: list failed: {e}");
                if append {
                    self.list_error = e;
                } else {
                    self.update_error = e;
                }
            }
        }
    }

    // ── lazy per-repo detail ───────────────────────────────────────────

    fn start_detail(&mut self, repo: &str) {
        if self.resolved.contains_key(repo)
            || self.detail_failed.contains(repo)
            || self.detail_repo.as_deref() == Some(repo)
        {
            return;
        }
        self.detail_repo = Some(repo.to_string());
        let (tx, rx) = mpsc::channel();
        self.detail_rx = Some(rx);
        let repo_s = repo.to_string();
        std::thread::spawn(move || {
            let res = hf_detail(&repo_s);
            let _ = tx.send((repo_s, res));
        });
    }

    fn poll_detail(&mut self) {
        let msg = match self.detail_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(m) => Some(m),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some((String::new(), Err("worker stopped".to_string())))
                }
            },
            None => None,
        };
        let Some((repo, res)) = msg else { return };
        self.detail_rx = None;
        self.detail_repo = None;
        match res {
            Ok(d) => {
                log::info!("llm: resolved {repo} ({} {})", human_size(d.size), d.quant);
                self.resolved.insert(repo.clone(), d);
                self.refresh_device();
                let _ = self.save_resolved();
                if self.pending_download.as_deref() == Some(repo.as_str()) {
                    self.pending_download = None;
                    self.start_download(&repo);
                }
            }
            Err(e) => {
                log::warn!("llm: detail {repo} failed: {e}");
                if self.pending_download.as_deref() == Some(repo.as_str()) {
                    self.pending_download = None;
                    self.errors.insert(repo.clone());
                }
                self.detail_failed.insert(repo);
            }
        }
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
                self.refresh_device();
            } else {
                i += 1;
            }
        }
    }

    // ── snapshots ──────────────────────────────────────────────────────

    /// JSON consumed by the QML:
    /// `{query, llm, version, updating, update_error, loading_more, has_more,
    ///   list_error, device, on_device, downloaded[], catalog[]}`.
    pub fn status_json(&mut self) -> String {
        self.reap_finished();
        self.poll_list();
        self.poll_detail();
        self.validate_state();

        let (ram_avail, disk_avail) = (self.ram_avail, self.disk_avail);

        // Downloaded models are independent of the search: built from the
        // resolved cache so they never disappear when the list is replaced.
        let mut downloaded = Vec::new();
        let mut on_device_count = 0usize;
        let mut on_device_bytes = 0u64;
        for repo in self.downloaded_repos() {
            let entry = self.entry_for(&repo);
            downloaded.push(self.model_json(&entry, ram_avail, disk_avail));
            on_device_count += 1;
            on_device_bytes = on_device_bytes.saturating_add(
                self.resolved.get(&repo).map(|d| d.size).unwrap_or(0),
            );
        }

        // Catalog: HF search results (already filtered server-side), minus the
        // models shown in the downloaded section.
        let catalog: Vec<serde_json::Value> = self
            .catalog
            .models
            .iter()
            .filter(|m| !self.is_downloaded(&m.repo))
            .map(|m| self.model_json(m, ram_avail, disk_avail))
            .collect();

        serde_json::json!({
            "query": self.query,
            "llm": self.state.llm,
            "version": self.catalog.version,
            "updating": self.updating,
            "update_error": self.update_error,
            "loading_more": self.loading_more,
            "has_more": self.next_cursor.is_some(),
            "list_error": self.list_error,
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
    /// Triggers the lazy `?blobs=true` resolution on first open.
    pub fn model_json_by_id(&mut self, id: &str) -> String {
        self.reap_finished();
        self.poll_detail();
        self.validate_state();
        if !self.resolved.contains_key(id) {
            self.start_detail(id);
        }
        let (ram_avail, disk_avail) = (self.ram_avail, self.disk_avail);
        if let Some(m) = self.catalog.models.iter().find(|m| m.repo == id) {
            return self.model_json(m, ram_avail, disk_avail).to_string();
        }
        // Fall back to the resolved cache: downloaded / previously-opened models
        // stay reachable even after the browsing list was replaced.
        if self.resolved.contains_key(id) || self.downloads.iter().any(|d| d.model_id == id) {
            let entry = self.entry_for(id);
            return self.model_json(&entry, ram_avail, disk_avail).to_string();
        }
        "{}".to_string()
    }

    /// A list entry for a repo, from the catalog when present, else synthesized
    /// from the repo name + resolved size (for downloaded models no longer in
    /// the current search results).
    fn entry_for(&self, repo: &str) -> ModelEntry {
        if let Some(m) = self.catalog.models.iter().find(|m| m.repo == repo) {
            return m.clone();
        }
        let params_b = parse_params_b(repo);
        ModelEntry {
            repo: repo.to_string(),
            name: repo.to_string(),
            params: params_b.map(format_params).unwrap_or_default(),
            downloads: 0,
            est: self.resolved.get(repo).map(|d| d.size).unwrap_or(0),
        }
    }

    /// One model as a display object (sizes, fit, state, human strings).
    fn model_json(&self, m: &ModelEntry, ram_avail: u64, disk_avail: u64) -> serde_json::Value {
        let detail = self.resolved.get(&m.repo);
        let size = detail.map(|d| d.size).unwrap_or(m.est);
        let approx = detail.is_none();
        let quant = detail.map(|d| d.quant.clone()).unwrap_or_default();
        let (fit, need) = fit_for(size, ram_avail, disk_avail);
        let downloading = self.downloads.iter().find(|d| d.model_id == m.repo);
        let downloaded = self.is_downloaded(&m.repo);
        let resolving = self.detail_repo.as_deref() == Some(m.repo.as_str())
            && !self.resolved.contains_key(&m.repo);
        let state = if downloading.is_some() {
            "downloading"
        } else if self.errors.contains(&m.repo) {
            "error"
        } else {
            ""
        };
        let done = downloading.map(|d| self.download_done(d)).unwrap_or(0);
        serde_json::json!({
            "model_id": m.repo,
            "name": m.name,
            "params": m.params,
            "downloads": m.downloads,
            "size": size,
            "size_human": human_size(size),
            "approx": approx,
            "quant": quant,
            "downloaded": downloaded,
            "resolving": resolving,
            "detail_error": self.detail_failed.contains(&m.repo),
            "state": state,
            "done": done,
            "done_human": human_size(done),
            "progress": if size > 0 { (done as f64 / size as f64).min(1.0) } else { 0.0 },
            "fit": fit,
            "need": need,
            "need_human": human_size(need),
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

    /// Handle a command from the QML.
    pub fn command(&mut self, cmd: &str, val: &str) {
        match cmd {
            "llm_select" => {
                self.state.llm = val.to_string();
                let _ = self.save_state();
            }
            "llm_search" => {
                if self.query != val {
                    self.query = val.to_string();
                    self.start_list(false);
                }
            }
            "llm_page_active" => self.refresh_device(),
            "llm_load_more" => self.start_list(true),
            "llm_catalog_update" => self.start_list(false),
            "llm_download" => self.start_download(val),
            "llm_download_cancel" => self.cancel_download(val),
            "llm_delete" => self.delete_model(val),
            other => log::info!("llm_command ignored: {other}"),
        }
    }

    fn start_download(&mut self, repo: &str) {
        if self.downloads.iter().any(|d| d.model_id == repo) {
            return;
        }
        let detail = match self.resolved.get(repo) {
            Some(d) if !d.url.is_empty() => d.clone(),
            _ => {
                // Not resolved yet: fetch the file list, then download.
                log::info!("llm: download {repo} — resolving first");
                self.detail_failed.remove(repo);
                self.pending_download = Some(repo.to_string());
                self.start_detail(repo);
                return;
            }
        };
        let dir = Self::model_dir(repo);
        let _ = std::fs::create_dir_all(&dir);
        self.errors.remove(repo);
        let name = file_name(&detail.url);
        if name.is_empty() {
            return;
        }
        log::info!("llm: downloading {repo} -> {name}");

        let cancel = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicI32::new(0));
        let finished = Arc::new(AtomicBool::new(false));
        let error = Arc::new(AtomicBool::new(false));

        let final_path = dir.join(&name);
        let part_path = dir.join(format!("{name}.part"));
        let url = detail.url.clone();

        let (c, p, f, e) = (
            cancel.clone(),
            pid.clone(),
            finished.clone(),
            error.clone(),
        );
        let (fp, pp) = (final_path.clone(), part_path.clone());
        std::thread::spawn(move || {
            let spawned = Command::new("curl")
                .arg("-L")
                .arg("--fail")
                .arg("--silent")
                .arg("--show-error")
                .arg("-o")
                .arg(&pp)
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
                        let _ = std::fs::remove_file(&pp);
                    } else if ok {
                        let _ = std::fs::rename(&pp, &fp);
                    } else {
                        e.store(true, Ordering::SeqCst);
                        let _ = std::fs::remove_file(&pp);
                    }
                }
                Err(_) => {
                    e.store(true, Ordering::SeqCst);
                }
            }
            f.store(true, Ordering::SeqCst);
        });

        self.downloads.push(Download {
            model_id: repo.to_string(),
            total: detail.size,
            cancel,
            pid,
            finished,
            error,
            targets: vec![(final_path, part_path)],
        });
    }

    fn cancel_download(&mut self, repo: &str) {
        if let Some(d) = self.downloads.iter().find(|d| d.model_id == repo) {
            d.cancel.store(true, Ordering::SeqCst);
            let pid = d.pid.load(Ordering::SeqCst);
            if pid > 0 {
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
            }
        }
        self.errors.remove(repo);
    }

    fn delete_model(&mut self, repo: &str) {
        self.cancel_download(repo);
        let dir = Self::model_dir(repo);
        let _ = std::fs::remove_dir_all(&dir);
        self.errors.remove(repo);
        self.resolved.remove(repo);
        self.detail_failed.remove(repo);
        let _ = self.save_resolved();
        if self.state.llm == repo {
            self.state.llm.clear();
        }
        let _ = self.save_state();
        log::info!("llm: deleted {repo}");
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

/// Resolve the selected GGUF LLM model (first `.gguf` in the model dir).
pub fn resolve_llm() -> Option<PathBuf> {
    if let Ok(text) = std::fs::read_to_string(state_path()) {
        if let Ok(state) = serde_json::from_str::<VoiceState>(&text) {
            if !state.llm.is_empty() {
                let dir = models_root().join(&state.llm);
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }
    None
}
