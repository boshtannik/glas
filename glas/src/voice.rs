//! Minimal on-device voice pipeline.
//!
//! Push-to-talk recording is delegated to `parecord` (PulseAudio, so no ALSA
//! device juggling) and speech-to-text to the bundled `whisper-cli`. No cpal /
//! hound is involved, so this module builds without the `voice` feature.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Bundled whisper.cpp command line tool.
pub const WHISPER_BIN: &str = "/usr/libexec/harbour-glas/whisper-cli";
/// PulseAudio source used for push-to-talk.
pub const INPUT_SOURCE: &str = "source.primary_input";

/// Push-to-talk recorder backed by a `parecord` child process.
#[derive(Default)]
pub struct Recorder {
    child: Option<Child>,
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        self.child.is_some()
    }

    /// Start capturing a 16 kHz mono 16-bit WAV into `wav`.
    pub fn start(&mut self, wav: &Path) -> std::io::Result<()> {
        if self.child.is_some() {
            return Ok(());
        }
        let child = Command::new("parecord")
            .arg(format!("--device={INPUT_SOURCE}"))
            .arg("--format=s16le")
            .arg("--rate=16000")
            .arg("--channels=1")
            .arg("--file-format=wav")
            .arg(wav)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.child = Some(child);
        Ok(())
    }

    /// SIGTERM so `parecord` finalises the WAV header, then reap the child.
    pub fn stop(&mut self) -> std::io::Result<()> {
        if let Some(mut child) = self.child.take() {
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
            let _ = child.wait();
        }
        Ok(())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Transcribe `wav` with `whisper-cli`.
///
/// `audio_ctx` is the `-c` override added by the bundled patch: whisper encodes
/// a full 30 s window by default, shrinking it to the real clip length cuts
/// latency several-fold. `0` keeps the default.
pub fn transcribe(
    model: &Path,
    lang: &str,
    threads: u32,
    wav: &Path,
    audio_ctx: u32,
) -> Result<String, String> {
    let mut cmd = Command::new(WHISPER_BIN);
    cmd.arg("-m")
        .arg(model)
        .arg("-l")
        .arg(lang)
        .arg("-t")
        .arg(threads.max(1).to_string())
        .arg("-nt")
        .stdin(Stdio::null());
    if audio_ctx > 0 {
        cmd.arg("-c").arg(audio_ctx.to_string());
    }
    cmd.arg("-f").arg(wav);

    let out = cmd.output().map_err(|e| format!("whisper: {e}"))?;
    if !out.status.success() {
        let err: String = String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(400)
            .collect();
        return Err(format!("whisper failed: {err}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Encoder context size for a 16 kHz mono 16-bit WAV, in 20 ms frames.
pub fn wav_audio_ctx(wav: &Path) -> u32 {
    let bytes = std::fs::metadata(wav).map(|m| m.len()).unwrap_or(0);
    let samples = bytes.saturating_sub(44) / 2;
    let secs = samples as f64 / 16_000.0;
    let ctx = (secs * 50.0).ceil() as u32 + 50;
    ctx.clamp(100, 1500)
}

/// Directory where downloaded GGML/ONNX models live.
pub fn models_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local/share/harbour-glas/models")
}

/// First `*.bin` GGML model found under [`models_dir`] (recursively).
pub fn find_installed_model() -> Option<PathBuf> {
    let mut stack = vec![models_dir()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("bin") {
                return Some(path);
            }
        }
    }
    None
}

/// Resolve the whisper model: explicit setting, then the Glas models dir, then
/// the model bundled with harbour-opencode (development fallback).
pub fn resolve_model(setting: &str) -> Option<PathBuf> {
    if !setting.is_empty() {
        let path = PathBuf::from(setting);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(path) = find_installed_model() {
        return Some(path);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let fallback = home.join(
        ".local/share/harbour-opencode/models/multilang_whisper_futo_multilingual244/voice-input-multilingual-244.bin",
    );
    if fallback.is_file() {
        return Some(fallback);
    }
    None
}
