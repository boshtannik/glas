use std::path::PathBuf;

use crate::audio::player::Player;
use crate::audio::recorder::Recorder;
use crate::audio::stt::{SttError, WhisperStt};
use crate::audio::tts::{PiperTts, TtsError};

/// Voice engine configuration (STT + TTS) — paths and parameters.
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    pub whisper_bin: PathBuf,
    pub whisper_model: PathBuf,
    pub whisper_lang: String,
    pub piper_bin: PathBuf,
    pub piper_model: PathBuf,
    pub piper_config: PathBuf,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            whisper_bin: "/usr/local/bin/whisper-cli".into(),
            whisper_model: "/usr/local/share/whisper/ggml-small.bin".into(),
            whisper_lang: "auto".into(),
            piper_bin: "/usr/local/bin/piper".into(),
            piper_model: "/usr/local/share/piper/ru_RU-dmitri-medium.onnx".into(),
            piper_config: "/usr/local/share/piper/ru_RU-dmitri-medium.onnx.json".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("synthesize failed: {0}")]
    Stt(#[from] SttError),
    #[error("tts failed: {0}")]
    Tts(#[from] TtsError),
    #[error("record failed: {0}")]
    Record(#[from] crate::audio::recorder::RecordError),
    #[error("play failed: {0}")]
    Play(#[from] crate::audio::player::PlayError),
}

/// Voice engine: recording + recognition + synthesis + playback.
pub struct VoiceEngine {
    cfg: VoiceConfig,
    recorder: Recorder,
    player: Player,
}

impl VoiceEngine {
    pub fn new(cfg: VoiceConfig) -> Self {
        Self { cfg, recorder: Recorder::new(), player: Player::new() }
    }

    /// Recognize speech from the microphone for `secs` seconds → text.
    ///
    /// This is a blocking operation (analog stream); in the GUI call it via threads.
    pub fn listen(&self, secs: u32, wav_tmp: impl Into<PathBuf>) -> Result<String, VoiceError> {
        let wav = wav_tmp.into();
        self.recorder.record(&wav, secs)?;
        let stt = WhisperStt::new(&self.cfg.whisper_bin, &self.cfg.whisper_model)
            .with_lang(&self.cfg.whisper_lang);
        let text = stt.transcribe(&wav)?;
        let _ = std::fs::remove_file(&wav);
        Ok(text)
    }

    /// Synthesize the response and play it (blocks until playback finishes).
    pub fn speak(&self, text: &str, wav_tmp: impl Into<PathBuf>) -> Result<(), VoiceError> {
        let wav = wav_tmp.into();
        let tts = PiperTts::new(&self.cfg.piper_bin, &self.cfg.piper_model, &self.cfg.piper_config);
        tts.synthesize_to_wav(text, &wav)?;
        let _ = self.player.play(&wav);
        let _ = std::fs::remove_file(&wav);
        Ok(())
    }
}