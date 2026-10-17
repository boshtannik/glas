//! Commands flowing *into* the core (UI / worker → core).

/// A command delivered to the core's single entry point.
///
/// The app layer funnels UI actions, timer ticks and thread results into this
/// enum; the core answers with a list of [`crate::Event`]s. There is no
/// `Unknown` variant on purpose — every inbound message must be modelled
/// explicitly (see `ADR-0008`).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Push-to-talk pressed: begin capturing audio.
    CaptureStart,
    /// Push-to-talk released: stop capturing and start the turn.
    CaptureStop,
    /// Abort the current turn — the Cancel button or a repeated PTT press
    /// (barge-in). Drops any pending session.
    Cancel,
    /// Answer a pending confirmation.
    Confirm { approved: bool },
    /// A transcript is available (from STT, the dev CLI or a test). This is the
    /// entry point of a turn once audio has been recognised.
    Transcribed { text: String },
    /// Release idle resources and stop (app shutdown).
    Shutdown,
}
