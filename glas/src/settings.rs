//! Application settings (`~/.config/harbour-glas/settings.json`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// User settings. Defaults are hard-coded here; a partial save survives the
/// addition of new fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// UI and recognition language ("ru"/"en").
    #[serde(default = "default_language")]
    pub language: String,
    /// How many turns to keep in the sliding context window.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Context token limit.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// TTS mode: "off" | "button" | "auto".
    #[serde(default = "default_tts_mode")]
    pub tts_mode: String,
    /// Seconds of idleness after which models are unloaded from memory
    /// (0 = never unload, keep the model resident).
    #[serde(default = "default_idle_unload_secs")]
    pub idle_unload_secs: u64,
    /// Absolute path to the whisper GGML model (empty = auto-detect).
    #[serde(default)]
    pub stt_model: String,
    /// Threads passed to `whisper-cli`.
    #[serde(default = "default_stt_threads")]
    pub stt_threads: u32,
}

fn default_language() -> String {
    "ru".into()
}
fn default_max_turns() -> u32 {
    8
}
fn default_max_tokens() -> u32 {
    700
}
fn default_tts_mode() -> String {
    "button".into()
}
fn default_idle_unload_secs() -> u64 {
    120
}
fn default_stt_threads() -> u32 {
    4
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: default_language(),
            max_turns: default_max_turns(),
            max_tokens: default_max_tokens(),
            tts_mode: default_tts_mode(),
            idle_unload_secs: default_idle_unload_secs(),
            stt_model: String::new(),
            stt_threads: default_stt_threads(),
        }
    }
}

impl Settings {
    /// Path to the settings file.
    pub fn path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".config/harbour-glas/settings.json")
    }

    /// Load: on a missing/corrupt file, fall back to defaults.
    pub fn load() -> Self {
        let path = Self::path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Atomic write (tmp + rename).
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    /// JSON representation for QML.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
