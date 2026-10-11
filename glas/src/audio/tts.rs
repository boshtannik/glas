use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::Write;

use thiserror::Error;

/// Text-to-Speech via the Piper CLI.
pub struct PiperTts {
    bin: PathBuf,
    model: PathBuf,
    config: PathBuf,
}

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("piper не найден по пути {0}")]
    NotFound(PathBuf),
    #[error("piper failed: {0}")]
    Run(String),
    #[error("запись stdin не удалась")]
    StdinWrite,
}

impl PiperTts {
    pub fn new(bin: impl Into<PathBuf>, model: impl Into<PathBuf>, config: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into(), model: model.into(), config: config.into() }
    }

    /// Synthesize text → WAV file (with a header).
    pub fn synthesize_to_wav(&self, text: &str, out_path: impl AsRef<Path>) -> Result<PathBuf, TtsError> {
        if !self.bin.exists() {
            return Err(TtsError::NotFound(self.bin.clone()));
        }
        let mut child = Command::new(&self.bin)
            .arg("-m").arg(&self.model)
            .arg("-c").arg(&self.config)
            .arg("-f").arg(out_path.as_ref())
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| TtsError::Run(e.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).map_err(|_| TtsError::StdinWrite)?;
        }
        let status = child.wait().map_err(|e| TtsError::Run(e.to_string()))?;
        if !status.success() {
            return Err(TtsError::Run("piper exited non-zero".into()));
        }
        Ok(out_path.as_ref().to_path_buf())
    }
}