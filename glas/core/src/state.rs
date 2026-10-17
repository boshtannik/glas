//! The turn state machine.

use std::fmt;

/// Stages of a single "turn" (one user interaction), per `ADR-0008`:
///
/// ```text
/// Idle → Recording → Transcribing → Thinking
///      → [AwaitingConfirmation] → Executing → Speaking → Idle
/// ```
///
/// `AwaitingConfirmation` and a clarification pause both keep the *session* open
/// (see `ADR-0011`); the microphone itself is only open during `Recording`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnState {
    /// Waiting for the user to press push-to-talk.
    Idle,
    /// Capturing audio (push-to-talk held).
    Recording,
    /// Recognising speech (STT).
    Transcribing,
    /// Building the prompt and running intent inference.
    Thinking,
    /// Waiting for the user to approve a dangerous action.
    AwaitingConfirmation,
    /// Running the skill.
    Executing,
    /// Speaking the reply (TTS) / showing it.
    Speaking,
}

impl TurnState {
    /// Stable lowercase identifier (for logs and the UI protocol).
    pub fn as_str(self) -> &'static str {
        match self {
            TurnState::Idle => "idle",
            TurnState::Recording => "recording",
            TurnState::Transcribing => "transcribing",
            TurnState::Thinking => "thinking",
            TurnState::AwaitingConfirmation => "awaiting_confirmation",
            TurnState::Executing => "executing",
            TurnState::Speaking => "speaking",
        }
    }
}

impl fmt::Display for TurnState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
