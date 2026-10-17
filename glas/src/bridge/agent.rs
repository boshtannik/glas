//! The dialogue worker: one background thread owns the `glas-core` `Core` and
//! talks to the bridge over two channels (`ADR-0008`).
//!
//! The heavy work (LLM intent inference, STT, TTS) must never block the Qt
//! main thread. The bridge sends [`Command`]s and drains the returned
//! [`Event`]s from `poll()`; all model calls happen on this thread.

use std::sync::mpsc::{self, Receiver, Sender};

use glas_core::builtins::default_registry;
use glas_core::engine::{EngineError, TextGen};
use glas_core::memory::SimpleMemory;
use glas_core::{Command, Core, Event};
use glas_system::ProcessSystem;

use crate::settings::Settings;

/// How many tokens the intent model may emit (a short JSON object).
const INTENT_MAX_NEW: usize = 128;

/// The `TextGen` seam, backed by the on-device `candle` model (`infer`).
struct IntentGen;

impl TextGen for IntentGen {
    fn generate(&self, prompt: &str) -> Result<String, EngineError> {
        // Greedy decoding: the intent JSON must be deterministic.
        crate::infer::generate(prompt, INTENT_MAX_NEW, 0.0).map_err(EngineError::Failed)
    }
}

/// Handle to the dialogue worker thread.
pub struct Agent {
    cmd_tx: Sender<Command>,
    ev_rx: Receiver<Event>,
}

impl Default for Agent {
    fn default() -> Self {
        Self::spawn()
    }
}

impl Agent {
    /// Start the worker thread and construct the core inside it.
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
        let (ev_tx, ev_rx) = mpsc::channel::<Event>();

        let spawned = std::thread::Builder::new()
            .name("glas-core".into())
            .spawn(move || {
                let turns = Settings::load().max_turns.max(1) as usize;
                let mut core = Core::new(
                    default_registry(),
                    Box::new(SimpleMemory::new(turns)),
                    Box::new(IntentGen),
                    Box::new(ProcessSystem),
                );
                log::info!("core worker started (context window {turns})");
                while let Ok(cmd) = cmd_rx.recv() {
                    // Stream every event the moment it happens, so the UI can
                    // show live stages (ADR-0008) instead of one batch at the end.
                    core.handle_into(cmd, &mut |ev| {
                        let _ = ev_tx.send(ev);
                    });
                }
                log::info!("core worker stopped");
            });

        if let Err(e) = spawned {
            log::error!("failed to start the core worker: {e}");
        }
        Self { cmd_tx, ev_rx }
    }

    /// Send a command to the core (non-blocking).
    pub fn send(&self, cmd: Command) {
        if self.cmd_tx.send(cmd).is_err() {
            log::warn!("core worker is gone; dropping command");
        }
    }

    /// Non-blocking: the next event emitted by the core, if any.
    pub fn try_event(&self) -> Option<Event> {
        self.ev_rx.try_recv().ok()
    }
}
