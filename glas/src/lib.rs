#![allow(non_snake_case)]

//! Glas — a voice assistant that controls the phone, for Sailfish OS.
//!
//! Skeleton (stop-point #1): the Rust ↔ QML bridge plus a window, and the
//! long-term memory store (`glas-memory`). Next up (see `vault/Plan.md`):
//! audio, intent (llama.cpp), skills, history.

pub mod bridge;
pub mod cmd;
pub mod settings;
pub mod voice;

pub use glas_memory as memory;

#[cfg(feature = "voice")]
pub mod audio;
