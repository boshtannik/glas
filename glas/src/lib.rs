#![allow(non_snake_case)]

//! Glas — голосовой ассистент управления телефоном для Sailfish OS.
//!
//! Каркас (стоп-точка №1): связка Rust ↔ QML + пустое окно.
//! Дальше по плану (см. `vault/План.md`): audio, intent (llama.cpp),
//! skills, history.

pub mod bridge;
pub mod cmd;
pub mod settings;

#[cfg(feature = "voice")]
pub mod audio;
