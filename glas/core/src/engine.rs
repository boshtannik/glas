//! Engines — the traits that isolate heavy/IO hardware behind a seam.
//!
//! The core only talks to these traits; real implementations (whisper.cpp,
//! piper, candle) live outside (in the app / a `glas-models` crate) and are
//! wired on the device. The fakes here let the whole flow run under `cargo test`
//! (see `ADR-0009`).

use std::collections::VecDeque;
use std::sync::Mutex;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum EngineError {
    #[error("engine failure: {0}")]
    Failed(String),
    #[error("no scripted response left")]
    Exhausted,
}

/// Text generation (the LLM behind a seam, `ADR-0009`).
pub trait TextGen {
    /// Load the model into memory. Real backends override this; the seam keeps a
    /// no-op default so fakes and scripted generators need only `generate`.
    fn load(&self) -> Result<(), EngineError> {
        Ok(())
    }

    fn generate(&self, prompt: &str) -> Result<String, EngineError>;

    /// Unload the model and free its memory. No-op by default.
    fn unload(&self) -> Result<(), EngineError> {
        Ok(())
    }
}

/// Speech-to-text.
pub trait SpeechToText {
    /// Transcribe an audio clip (raw WAV bytes for now).
    fn transcribe(&self, audio: &[u8]) -> Result<String, EngineError>;
}

/// Text-to-speech.
pub trait TextToSpeech {
    /// Synthesize `text`, returning a path to the produced audio file.
    fn synthesize(&self, text: &str) -> Result<String, EngineError>;
}

/// A scripted generator: returns queued responses in order, one per call.
/// Used by tests and the dev CLI.
pub struct FakeGen {
    responses: Mutex<VecDeque<String>>,
}

impl FakeGen {
    pub fn new(script: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(script.into()),
        }
    }

    /// Add a response to the back of the queue.
    pub fn push(&self, response: impl Into<String>) {
        self.responses.lock().unwrap().push_back(response.into());
    }
}

impl TextGen for FakeGen {
    fn generate(&self, _prompt: &str) -> Result<String, EngineError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(EngineError::Exhausted)
    }
}

/// A fixed-transcript STT (tests).
pub struct FakeStt {
    pub transcript: String,
}

impl SpeechToText for FakeStt {
    fn transcribe(&self, _audio: &[u8]) -> Result<String, EngineError> {
        Ok(self.transcript.clone())
    }
}

/// A no-op TTS (tests): returns a stable fake path.
pub struct FakeTts;

impl TextToSpeech for FakeTts {
    fn synthesize(&self, text: &str) -> Result<String, EngineError> {
        Ok(format!("/tmp/glas-tts-{}.wav", text.len()))
    }
}
