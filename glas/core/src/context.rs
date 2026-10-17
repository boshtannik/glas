//! The dialogue context — a compact sliding window of recent turns.

use std::collections::VecDeque;

/// Who produced a context turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The user's utterance.
    User,
    /// A resolved action (skill result), stored compactly.
    Action,
}

/// One entry of the context window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

impl Turn {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            text: text.into(),
        }
    }

    pub fn action(text: impl Into<String>) -> Self {
        Self {
            role: Role::Action,
            text: text.into(),
        }
    }
}

/// A bounded FIFO of turns. Old turns fall off and are not returned to the model
/// (see `Контекст-и-память`).
#[derive(Debug, Clone)]
pub struct Window {
    turns: VecDeque<Turn>,
    max: usize,
}

impl Window {
    pub fn new(max: usize) -> Self {
        Self {
            turns: VecDeque::new(),
            max: max.max(1),
        }
    }

    pub fn push(&mut self, turn: Turn) {
        self.turns.push_back(turn);
        while self.turns.len() > self.max {
            self.turns.pop_front();
        }
    }

    pub fn turns(&self) -> impl Iterator<Item = &Turn> {
        self.turns.iter()
    }

    pub fn len(&self) -> usize {
        self.turns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    /// Render as a compact, model-friendly block (`U:` / `A:` lines).
    pub fn render(&self) -> String {
        let mut out = String::new();
        for turn in &self.turns {
            let tag = match turn.role {
                Role::User => "U",
                Role::Action => "A",
            };
            out.push_str(tag);
            out.push_str(": ");
            out.push_str(&turn.text);
            out.push('\n');
        }
        out
    }
}
