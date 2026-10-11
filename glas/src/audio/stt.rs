use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

/// Speech-to-Text via the whisper.cpp CLI.
pub struct WhisperStt {
    /// Path to the `whisper-cli` binary.
    bin: PathBuf,
    /// Path to the `ggml-*.bin` model.
    model: PathBuf,
    /// Language (`ru`, `en`, `auto`, etc.).
    lang: String,
    /// Number of threads.
    threads: usize,
}

#[derive(Debug, Error)]
pub enum SttError {
    #[error("whisper-cli не найден по пути {0}")]
    NotFound(PathBuf),
    #[error("whisper-cli failed: {0}")]
    Run(String),
}

impl WhisperStt {
    pub fn new(bin: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into(), model: model.into(), lang: "auto".into(), threads: 4 }
    }

    pub fn with_lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = lang.into();
        self
    }

    pub fn with_threads(mut self, n: usize) -> Self {
        self.threads = n;
        self
    }

    /// Recognize speech in a WAV file. Returns the text.
    pub fn transcribe(&self, wav: impl AsRef<Path>) -> Result<String, SttError> {
        if !self.bin.exists() {
            return Err(SttError::NotFound(self.bin.clone()));
        }
        let out = Command::new(&self.bin)
            .arg("-m").arg(&self.model)
            .arg("-l").arg(&self.lang)
            .arg("-t").arg(self.threads.to_string())
            .arg("-f").arg(wav.as_ref())
            .arg("--no-prints")
            .arg("-nt")
            .output()
            .map_err(|e| SttError::Run(e.to_string()))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(SttError::Run(stderr.chars().take(200).collect()));
        }

        // whisper-cli -nt outputs clean text (without timestamps)
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(text)
    }
}
