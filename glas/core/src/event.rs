//! Events emitted *by* the core (core → UI / app).

use crate::state::TurnState;

/// Something the core wants the outside world to know about.
///
/// The UI renders these; the app routes speech to TTS on [`Event::Reply`] and
/// [`Event::NeedsConfirmation`]. No catch-all variant — every event is explicit
/// (see `ADR-0008`).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The turn state machine moved to a new state.
    StateChanged(TurnState),
    /// Echo of the recognised (or typed) utterance, for the dialogue log.
    Transcribed { text: String },
    /// A multi-step session opened (confirmation / clarification); the skill
    /// that holds it is named (`ADR-0011`).
    SessionStarted { session_id: u64, skill_id: String },
    /// The open session ended (answered, cancelled or exited).
    SessionEnded { session_id: u64 },
    /// A dangerous action is pending; the user must approve or reject it.
    NeedsConfirmation { skill_id: String, reply: String },
    /// A skill finished executing.
    ActionResult {
        skill_id: String,
        summary: String,
        ok: bool,
    },
    /// The assistant has something to say (and usually speak).
    Reply { text: String },
    /// Nothing matched the utterance.
    UnknownUtterance { text: String },
    /// The resolved intent has no registered skill.
    NoSuchSkill { intent: String },
    /// The resolved skill exists but is disabled in the settings.
    SkillDisabled { id: String },
    /// A durable preference was stored.
    MemorySet { key: String, value: String },
    /// A durable preference was forgotten.
    MemoryForget { key: String },
    /// An internal error (engine failure, bad state, …).
    Error { message: String },
    /// The turn is over; the core is back to [`TurnState::Idle`].
    TurnDone,
}
