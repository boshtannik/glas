//! Единый источник протокола QML ↔ воркер.
//!
//! `serde` генерирует и имя команды, и поля JSON из этого одного определения:
//! переименование или опечатка ключа невозможна. QML зеркалит имена команд
//! константами (см. `vault/Архитектура.md`).

use serde::{Deserialize, Serialize};

/// Команды, которые QML шлёт воркеру как JSON `{"cmd": <name>, ...fields}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Cmd {
    /// Начать прослушивание микрофона.
    Listen,
    /// Остановить запись.
    Stop,
    /// Отменить текущую обработку.
    Cancel,
    /// Откатить действие по id.
    Undo { id: String },
    /// Запросить список навыков.
    SkillList,
    /// Включить/выключить навык.
    SkillEnable { id: String, on: bool },
    /// Импортировать навык по ссылке.
    SkillImportUrl { url: String },
    /// Запросить текущие настройки.
    GetSettings,
}

impl Cmd {
    /// Сериализация в JSON-конверт очереди команд.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
