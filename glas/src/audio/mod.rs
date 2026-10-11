//! Audio stack: recording, playback, STT (whisper.cpp), TTS (piper), VAD.
//!
//! This whole module is built only with the `voice` feature (it depends on
//! `cpal`/`hound`, which need ALSA dev headers). Without the feature the
//! skeleton still compiles.

pub mod player;
pub mod recorder;
pub mod stt;
pub mod tts;
pub mod vad;
pub mod voice;
