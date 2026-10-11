//! Glas long-term memory: a small, file-backed store of user preferences.
//!
//! The dialog itself is kept in a sliding window (see ADR-0003); this module
//! holds the *durable* part — stable key/value facts (name, language, time
//! format, …) that are injected into the LLM system prompt on every turn, so
//! they survive when a turn falls out of the window (see ADR-0006).
//!
//! All memory logic lives here in Rust: validation, limits, rendering of the
//! prompt line, and persistence. The UI only calls into it (and must ask the
//! user for confirmation before writing — the model never writes silently).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// On-disk format version.
pub const PROFILE_SCHEMA: u32 = 1;
/// Maximum number of facts kept in the profile.
pub const MAX_FACTS: usize = 64;
/// Maximum key length, in characters.
pub const MAX_KEY_LEN: usize = 48;
/// Maximum value length, in characters.
pub const MAX_VALUE_LEN: usize = 200;
/// Soft budget for the rendered prompt line, in characters (~400 tokens).
pub const MAX_PROMPT_CHARS: usize = 1600;

/// Who wrote a fact. Kept for auditability / the "Memory" page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Written after an explicit user request/confirmation.
    User,
    /// Proposed by the model and confirmed by the user.
    Model,
}

/// A single remembered preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    pub value: String,
    /// Unix time (seconds) of the last update.
    pub updated: u64,
    pub source: Source,
}

/// The durable profile: a deterministic map of facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default = "default_schema")]
    pub schema: u32,
    #[serde(default)]
    pub facts: BTreeMap<String, Fact>,
}

fn default_schema() -> u32 {
    PROFILE_SCHEMA
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            schema: PROFILE_SCHEMA,
            facts: BTreeMap::new(),
        }
    }
}

/// Why a write was rejected. Messages are user-facing (shown in the UI).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MemoryError {
    #[error("the key must not be empty")]
    EmptyKey,
    #[error("the key is too long (max {MAX_KEY_LEN} characters)")]
    KeyTooLong,
    #[error("invalid key: use only a-z, 0-9, '_' and '.'")]
    InvalidKey,
    #[error("the value must not be empty")]
    EmptyValue,
    #[error("the value is too long (max {MAX_VALUE_LEN} characters)")]
    ValueTooLong,
    #[error("too many facts (max {MAX_FACTS}); forget something first")]
    TooManyFacts,
}

/// Validate a key: lowercase ascii, digits, `_` and `.`, within the length limit.
fn validate_key(key: &str) -> Result<(), MemoryError> {
    if key.is_empty() {
        return Err(MemoryError::EmptyKey);
    }
    if key.chars().count() > MAX_KEY_LEN {
        return Err(MemoryError::KeyTooLong);
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
    {
        return Err(MemoryError::InvalidKey);
    }
    Ok(())
}

impl Profile {
    /// Insert or update a fact. Replaces an existing key without counting
    /// against [`MAX_FACTS`]; rejects the write when the profile is full.
    pub fn set(
        &mut self,
        key: &str,
        value: &str,
        source: Source,
        now: u64,
    ) -> Result<(), MemoryError> {
        let key = key.trim();
        validate_key(key)?;
        let value = value.trim();
        if value.is_empty() {
            return Err(MemoryError::EmptyValue);
        }
        if value.chars().count() > MAX_VALUE_LEN {
            return Err(MemoryError::ValueTooLong);
        }
        if !self.facts.contains_key(key) && self.facts.len() >= MAX_FACTS {
            return Err(MemoryError::TooManyFacts);
        }
        self.facts.insert(
            key.to_string(),
            Fact {
                value: value.to_string(),
                updated: now,
                source,
            },
        );
        Ok(())
    }

    /// Remove a fact. Returns `true` if it existed.
    pub fn forget(&mut self, key: &str) -> bool {
        self.facts.remove(key.trim()).is_some()
    }

    pub fn get(&self, key: &str) -> Option<&Fact> {
        self.facts.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// The line injected into the system prompt. Empty when there are no facts.
    /// Facts are rendered in key order and truncated to [`MAX_PROMPT_CHARS`].
    pub fn prompt_line(&self) -> String {
        if self.facts.is_empty() {
            return String::new();
        }
        const PREFIX: &str = "Known user preferences: ";
        let mut used = PREFIX.chars().count();
        let mut parts: Vec<String> = Vec::new();
        for (key, fact) in &self.facts {
            let piece = format!("{key}={}", fact.value);
            used += piece.chars().count() + 2; // "; "
            if used > MAX_PROMPT_CHARS {
                break;
            }
            parts.push(piece);
        }
        format!("{PREFIX}{}", parts.join("; "))
    }

    /// JSON representation for the QML "Memory" page.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// File-backed profile store.
pub struct Memory {
    path: PathBuf,
    profile: Profile,
}

impl Memory {
    /// Default location: `~/.local/share/harbour-glas/profile.json`.
    pub fn path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".local/share/harbour-glas/profile.json")
    }

    /// Load from `path`; a missing or corrupt file yields an empty profile.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let profile = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Profile>(&s).ok())
            .unwrap_or_default();
        Memory { path, profile }
    }

    /// Open the default profile.
    pub fn open_default() -> Self {
        Memory::open(Self::path())
    }

    pub fn path_ref(&self) -> &Path {
        &self.path
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Set a fact and persist immediately.
    pub fn set(
        &mut self,
        key: &str,
        value: &str,
        source: Source,
        now: u64,
    ) -> Result<(), MemoryError> {
        self.profile.set(key, value, source, now)?;
        // Persisting is best-effort relative to the in-memory update: a failed
        // write is logged by the caller, but the fact still applies this run.
        let _ = self.save();
        Ok(())
    }

    /// Forget a fact and persist. Returns `true` if it existed.
    pub fn forget(&mut self, key: &str) -> std::io::Result<bool> {
        let removed = self.profile.forget(key);
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    pub fn prompt_line(&self) -> String {
        self.profile.prompt_line()
    }

    pub fn facts_json(&self) -> serde_json::Value {
        self.profile.to_json()
    }

    /// Atomic write (tmp + rename), creating parent directories as needed.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(&self.profile)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.path)
    }
}

impl Default for Memory {
    fn default() -> Self {
        Memory::open_default()
    }
}

/// Current Unix time in seconds.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temp path plus a guard that removes it on drop.
    struct TempPath(PathBuf);

    impl TempPath {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let mut dir = std::env::temp_dir();
            dir.push(format!("glas-mem-{}-{}-{}", tag, std::process::id(), nanos));
            dir.push("profile.json");
            TempPath(dir)
        }

        fn file(&self) -> &Path {
            &self.0
        }

        fn dir(&self) -> PathBuf {
            self.0.parent().unwrap().to_path_buf()
        }
    }

    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.dir());
        }
    }

    #[test]
    fn set_and_get() {
        let mut p = Profile::default();
        p.set("name", "Jack", Source::User, 100).unwrap();
        let f = p.get("name").unwrap();
        assert_eq!(f.value, "Jack");
        assert_eq!(f.updated, 100);
        assert_eq!(f.source, Source::User);
    }

    #[test]
    fn trims_key_and_value() {
        let mut p = Profile::default();
        p.set("  language  ", "  ru  ", Source::User, 1).unwrap();
        assert_eq!(p.get("language").unwrap().value, "ru");
        assert!(p.get("  language  ").is_none());
    }

    #[test]
    fn rejects_bad_keys() {
        let mut p = Profile::default();
        assert_eq!(p.set("", "v", Source::User, 1), Err(MemoryError::EmptyKey));
        assert_eq!(p.set("   ", "v", Source::User, 1), Err(MemoryError::EmptyKey));
        assert_eq!(
            p.set("Bad Key", "v", Source::User, 1),
            Err(MemoryError::InvalidKey)
        );
        assert_eq!(
            p.set("ключ", "v", Source::User, 1),
            Err(MemoryError::InvalidKey)
        );
        let long = "a".repeat(MAX_KEY_LEN + 1);
        assert_eq!(
            p.set(&long, "v", Source::User, 1),
            Err(MemoryError::KeyTooLong)
        );
    }

    #[test]
    fn allows_nested_dot_keys() {
        let mut p = Profile::default();
        p.set("tts.voice", "irina", Source::User, 1).unwrap();
        assert_eq!(p.get("tts.voice").unwrap().value, "irina");
    }

    #[test]
    fn rejects_bad_values() {
        let mut p = Profile::default();
        assert_eq!(p.set("k", "", Source::User, 1), Err(MemoryError::EmptyValue));
        assert_eq!(p.set("k", "   ", Source::User, 1), Err(MemoryError::EmptyValue));
        let long = "x".repeat(MAX_VALUE_LEN + 1);
        assert_eq!(
            p.set("k", &long, Source::User, 1),
            Err(MemoryError::ValueTooLong)
        );
    }

    #[test]
    fn updating_existing_key_does_not_hit_the_limit() {
        let mut p = Profile::default();
        for i in 0..(MAX_FACTS + 10) {
            p.set("name", &format!("n{i}"), Source::User, i as u64)
                .unwrap();
        }
        assert_eq!(p.facts.len(), 1);
    }

    #[test]
    fn enforces_max_facts() {
        let mut p = Profile::default();
        for i in 0..MAX_FACTS {
            p.set(&format!("k{i}"), "v", Source::User, 0).unwrap();
        }
        assert_eq!(p.set("overflow", "v", Source::User, 0), Err(MemoryError::TooManyFacts));
        // Updating an existing key still works when full.
        assert!(p.set("k0", "v2", Source::User, 0).is_ok());
    }

    #[test]
    fn forget_reports_existence() {
        let mut p = Profile::default();
        p.set("name", "Jack", Source::User, 1).unwrap();
        assert!(p.forget("name"));
        assert!(!p.forget("name"));
        assert!(!p.forget("missing"));
    }

    #[test]
    fn prompt_line_is_empty_without_facts() {
        assert_eq!(Profile::default().prompt_line(), "");
    }

    #[test]
    fn prompt_line_lists_facts_in_key_order() {
        let mut p = Profile::default();
        p.set("time_format", "24h", Source::Model, 1).unwrap();
        p.set("name", "Jack", Source::User, 1).unwrap();
        p.set("language", "ru", Source::User, 1).unwrap();
        assert_eq!(
            p.prompt_line(),
            "Known user preferences: language=ru; name=Jack; time_format=24h"
        );
    }

    #[test]
    fn prompt_line_respects_the_char_budget() {
        let mut p = Profile::default();
        for i in 0..MAX_FACTS {
            let value = "z".repeat(MAX_VALUE_LEN);
            p.set(&format!("key{i:02}"), &value, Source::User, 0).unwrap();
        }
        let line = p.prompt_line();
        assert!(line.chars().count() <= MAX_PROMPT_CHARS);
        assert!(line.starts_with("Known user preferences: "));
    }

    #[test]
    fn save_creates_parent_dirs_and_round_trips() {
        let tmp = TempPath::new("roundtrip");
        assert!(!tmp.file().exists());
        {
            let mut m = Memory::open(tmp.file());
            m.set("name", "Jack", Source::User, 42).unwrap();
            m.set("language", "ru", Source::User, 43).unwrap();
        }
        let reopened = Memory::open(tmp.file());
        assert_eq!(reopened.profile().get("name").unwrap().value, "Jack");
        assert_eq!(reopened.profile().get("language").unwrap().value, "ru");
        assert_eq!(
            reopened.profile().get("name").unwrap().source,
            Source::User
        );
    }

    #[test]
    fn forget_persists() {
        let tmp = TempPath::new("forget");
        let mut m = Memory::open(tmp.file());
        m.set("name", "Jack", Source::User, 1).unwrap();
        assert!(m.forget("name").unwrap());
        let reopened = Memory::open(tmp.file());
        assert!(reopened.profile().is_empty());
    }

    #[test]
    fn corrupt_file_falls_back_to_empty() {
        let tmp = TempPath::new("corrupt");
        std::fs::create_dir_all(tmp.dir()).unwrap();
        std::fs::write(tmp.file(), "{ not json").unwrap();
        let m = Memory::open(tmp.file());
        assert!(m.profile().is_empty());
    }

    #[test]
    fn missing_file_is_empty() {
        let tmp = TempPath::new("missing");
        let m = Memory::open(tmp.file());
        assert!(m.profile().is_empty());
        assert_eq!(m.prompt_line(), "");
    }
}
