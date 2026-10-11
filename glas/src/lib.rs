#![allow(non_snake_case)]

//! Glas — a voice assistant that controls the phone, for Sailfish OS.
//!
//! Skeleton (stop-point #1): the Rust ↔ QML bridge plus an empty window.
//! Next up (see `vault/План.md`): audio, intent (llama.cpp),
//! skills, history.

pub mod bridge;
pub mod cmd;
pub mod settings;

#[cfg(feature = "voice")]
pub mod audio;
