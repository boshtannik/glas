//! Single source of truth for the QML ↔ worker protocol.
//!
//! `serde` derives both the command name and the JSON fields from this one
//! definition, so a rename or a typo in a key is impossible. QML mirrors the
//! command names as constants (see `vault/Архитектура.md`).

use serde::{Deserialize, Serialize};

/// Commands that QML sends to the worker as JSON `{"cmd": <name>, ...fields}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Cmd {
    /// Start listening on the microphone.
    Listen,
    /// Stop recording.
    Stop,
    /// Cancel the current processing.
    Cancel,
    /// Undo an action by id.
    Undo { id: String },
    /// Request the list of skills.
    SkillList,
    /// Enable/disable a skill.
    SkillEnable { id: String, on: bool },
    /// Import a skill from a URL.
    SkillImportUrl { url: String },
    /// Request the current settings.
    GetSettings,
    /// List stored long-term memory facts.
    MemoryList,
    /// Store a preference fact (requires user confirmation in the UI).
    MemorySet { key: String, value: String },
    /// Forget a preference fact.
    MemoryForget { key: String },
}

impl Cmd {
    /// Serialize into the JSON envelope used by the command queue.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
