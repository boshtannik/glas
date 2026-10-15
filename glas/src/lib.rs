#![allow(non_snake_case)]

//! Glas — a voice assistant that controls the phone, for Sailfish OS.
//!
//! The Rust ↔ QML bridge plus a window; on-device voice (push-to-talk STT,
//! piper TTS), a `candle` LLM inference path, the model stores (voice + LLM)
//! and the long-term memory store (`glas-memory`). Still ahead (see
//! `vault/План.md`): intent extraction, skills, history.

pub mod bridge;
pub mod cmd;
pub mod infer;
pub mod settings;
pub mod voice;

pub use glas_memory as memory;

#[cfg(feature = "voice")]
pub mod audio;

pub mod llm;
