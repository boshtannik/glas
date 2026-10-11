//! Аудио-стек: запись, воспроизведение, STT (whisper.cpp), TTS (piper), VAD.
//!
//! Весь модуль собирается только с фичей `voice` (зависит от `cpal`/`hound`,
//! которым нужен ALSA-dev). Без фичи каркас всё равно собирается.

pub mod player;
pub mod recorder;
pub mod stt;
pub mod tts;
pub mod vad;
pub mod voice;
