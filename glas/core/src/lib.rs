//! Glas dialogue core.
//!
//! Pure, host-testable logic of the assistant: the turn state machine, dialogue
//! sessions (clarification / confirmation / exit), the `Skill` registry and the
//! intent pipeline. No Qt, no I/O — hardware and models are reached through the
//! traits in [`engine`] and [`memory`], so the whole flow runs under `cargo test`.
//!
//! See `vault/ADR/0007-исполнение-навыков.md`, `0008-ядро-worker-очередь.md`,
//! `0011-сессия-и-подтверждение.md`, `0012-диалог-контекст-выход-чтение.md`.

pub mod builtins;
pub mod command;
pub mod context;
pub mod dialog;
pub mod engine;
pub mod event;
pub mod intent;
pub mod keys;
pub mod memory;
pub mod registry;
pub mod skill;
pub mod state;
pub mod system;

pub use command::Command;
pub use dialog::Core;
pub use event::Event;
pub use state::TurnState;
