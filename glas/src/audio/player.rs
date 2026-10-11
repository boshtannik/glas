use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

/// WAV playback via `cpal` (ALSA on Sailfish) — no external binaries.
pub struct Player;

#[derive(Debug, Error)]
pub enum PlayError {
    #[error("нет устройства вывода")]
    NoOutputDevice,
    #[error("нет поддерживаемого формата: {0}")]
    NoSupportedFormat(String),
    #[error("не удалось открыть поток: {0}")]
    StreamError(String),
    #[error("ошибка чтения WAV: {0}")]
    Wav(#[from] hound::Error),
    #[error("пустой/нечитаемый WAV")]
    EmptyWav,
}

impl Player {
    pub fn new() -> Self {
        Self
    }

    /// Play a WAV file. Returns after completion.
    pub fn play(&self, wav: impl AsRef<Path>) -> Result<(), PlayError> {
        let mut reader = hound::WavReader::open(wav)?;
        let spec = reader.spec();
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap_or(0) as f32 / 32768.0)
            .collect();
        if samples.is_empty() {
            return Err(PlayError::EmptyWav);
        }

        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(PlayError::NoOutputDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| PlayError::NoSupportedFormat(e.to_string()))?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let (tx, rx) = mpsc::channel::<f32>();
        let mut pos = 0usize;
        let samples_slice = samples;
        // Pour samples into the channel from the playback thread: read them all into memory at once.
        for s in samples_slice {
            let _ = tx.send(s);
            pos += 1;
        }

        let err_fn = |e| eprintln!("cpal output error: {e}");
        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                device.build_output_stream(&config, move |data: &mut [f32], _| {
                    for slot in data.iter_mut() {
                        *slot = rx.try_recv().unwrap_or(0.0);
                    }
                }, err_fn, None)
            }
            cpal::SampleFormat::I16 => {
                device.build_output_stream(&config, move |data: &mut [i16], _| {
                    for slot in data.iter_mut() {
                        let v = rx.try_recv().unwrap_or(0.0);
                        *slot = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
                    }
                }, err_fn, None)
            }
            _ => return Err(PlayError::NoSupportedFormat(sample_format.to_string())),
        }
        .map_err(|e| PlayError::StreamError(e.to_string()))?;

        stream.play().map_err(|e| PlayError::StreamError(e.to_string()))?;
        // Wait for the channel to empty (all samples sent -> played).
        let empty_wait = Duration::from_secs_f32(
            pos as f32 / spec.sample_rate as f32 + 0.2,
        );
        thread::sleep(empty_wait);
        drop(stream);
        Ok(())
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}