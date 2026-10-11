//! Настройки приложения (`~/.config/harbour-glas/settings.json`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Пользовательские настройки. Значения по умолчанию зашиты здесь; частичное
/// сохранение переживает добавление новых полей.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Язык интерфейса и распознавания ("ru"/"en").
    #[serde(default = "default_language")]
    pub language: String,
    /// Сколько пар реплик держать в скользящем окне контекста.
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// Лимит токенов контекста.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Режим TTS: "off" | "button" | "auto".
    #[serde(default = "default_tts_mode")]
    pub tts_mode: String,
    /// Через сколько секунд простоя выгружать модели из памяти.
    #[serde(default = "default_idle_unload_secs")]
    pub idle_unload_secs: u64,
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
    300
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: default_language(),
            max_turns: default_max_turns(),
            max_tokens: default_max_tokens(),
            tts_mode: default_tts_mode(),
            idle_unload_secs: default_idle_unload_secs(),
        }
    }
}

impl Settings {
    /// Путь к файлу настроек.
    pub fn path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".config/harbour-glas/settings.json")
    }

    /// Загрузка: при отсутствии/повреждении файла — значения по умолчанию.
    pub fn load() -> Self {
        let path = Self::path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Атомарная запись (tmp + rename).
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    /// JSON-представление для QML.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
