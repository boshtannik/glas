//! Memory — a pure context provider (preferences line + sliding window).
//!
//! Per the target architecture the memory module does no ranking or
//! summarising; it only supplies context. The trait is kept deliberately small
//! so it can grow later (e.g. `compact`) without touching the core.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::context::{Turn, Window};

#[derive(Debug, Error, Clone, PartialEq)]
pub enum MemoryError {
    #[error("invalid key: {0}")]
    InvalidKey(String),
    #[error("key not found: {0}")]
    NotFound(String),
}

/// Context provider used when building the prompt.
pub trait Memory {
    /// A compact line of durable preferences (always in the system prompt).
    fn system_line(&self) -> String;

    /// The recent turns (sliding window).
    fn window(&self) -> Vec<Turn>;

    /// Record a turn into the window.
    fn record(&mut self, turn: Turn);

    /// Store a durable preference fact.
    fn set(&mut self, key: &str, value: &str) -> Result<(), MemoryError>;

    /// Remove a durable preference fact.
    fn forget(&mut self, key: &str) -> Result<(), MemoryError>;

    /// Convenience: record a user utterance.
    fn record_user(&mut self, text: &str) {
        self.record(Turn::user(text));
    }

    /// Convenience: record a resolved action line.
    fn record_action(&mut self, text: &str) {
        self.record(Turn::action(text));
    }
}

/// A simple in-memory implementation for tests and the dev CLI.
pub struct SimpleMemory {
    facts: BTreeMap<String, String>,
    window: Window,
}

impl SimpleMemory {
    pub fn new(max_turns: usize) -> Self {
        Self {
            facts: BTreeMap::new(),
            window: Window::new(max_turns),
        }
    }
}

impl Default for SimpleMemory {
    fn default() -> Self {
        Self::new(8)
    }
}

impl Memory for SimpleMemory {
    fn system_line(&self) -> String {
        if self.facts.is_empty() {
            return String::new();
        }
        let pairs: Vec<String> = self
            .facts
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("Known user preferences: {}", pairs.join("; "))
    }

    fn window(&self) -> Vec<Turn> {
        self.window.turns().cloned().collect()
    }

    fn record(&mut self, turn: Turn) {
        self.window.push(turn);
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), MemoryError> {
        if key.is_empty() {
            return Err(MemoryError::InvalidKey(key.to_string()));
        }
        self.facts.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn forget(&mut self, key: &str) -> Result<(), MemoryError> {
        if self.facts.remove(key).is_none() {
            return Err(MemoryError::NotFound(key.to_string()));
        }
        Ok(())
    }
}
