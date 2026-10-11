use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::WavWriter;
use thiserror::Error;

/// Target sample rate for STT.
const TARGET_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;

/// Microphone audio recording via `cpal` (ALSA on Sailfish) — no external binaries.
pub struct Recorder {
    default_secs: u32,
}

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("нет устройства захвата")]
    NoInputDevice,
    #[error("нет поддерживаемого формата: {0}")]
    NoSupportedFormat(String),
    #[error("не удалось открыть поток: {0}")]
    StreamError(String),
    #[error("ошибка WAV-записи: {0}")]
    Wav(#[from] hound::Error),
}

impl Recorder {
    pub fn new() -> Self {
        Self { default_secs: 5 }
    }

    pub fn with_default_secs(mut self, secs: u32) -> Self {
        self.default_secs = secs;
        self
    }

    /// Record `duration_secs` seconds from the microphone into a WAV file (16kHz mono s16).
    pub fn record(&self, path: impl AsRef<Path>, duration_secs: u32) -> Result<PathBuf, RecordError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(RecordError::NoInputDevice)?;

        let supported = device
            .default_input_config()
            .map_err(|e| RecordError::NoSupportedFormat(e.to_string()))?;
        let src_format = supported.sample_format();
        let src_rate = supported.sample_rate().0;
        let (tx, rx) = mpsc::channel::<f32>();

        let err_fn = |e| eprintln!("cpal input error: {e}");
        let config: cpal::StreamConfig = supported.into();

        let stream = match src_format {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(&config, move |data: &[f32], _| {
                    for &s in data {
                        let _ = tx.send(s);
                    }
                }, err_fn, None)
            }
            cpal::SampleFormat::I16 => {
                device.build_input_stream(&config, move |data: &[i16], _| {
                    for &s in data {
                        let _ = tx.send(s as f32 / 32768.0);
                    }
                }, err_fn, None)
            }
            cpal::SampleFormat::U16 => {
                device.build_input_stream(&config, move |data: &[u16], _| {
                    for &s in data {
                        let _ = tx.send(s as f32 / 32768.0 - 1.0);
                    }
                }, err_fn, None)
            }
            other => return Err(RecordError::NoSupportedFormat(other.to_string())),
        }
        .map_err(|e| RecordError::StreamError(e.to_string()))?;

        stream.play().map_err(|e| RecordError::StreamError(e.to_string()))?;

        // Collect samples for the specified time.
        let mut buf: Vec<f32> = Vec::with_capacity((src_rate as usize) * duration_secs as usize);
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(duration_secs as u64) {
            while let Ok(s) = rx.try_recv() {
                buf.push(s);
            }
            // Avoid an empty spin loop.
            thread::yield_now();
        }
        drop(stream);
        // Drain the remainder from the channel.
        while let Ok(s) = rx.try_recv() {
            buf.push(s);
        }

        // Resampling by averaging groups: src_rate -> 16k.
        let ratio = src_rate as f32 / TARGET_RATE as f32;
        let group = ratio.max(1.0).round() as usize;

        let spec = hound::WavSpec {
            channels: CHANNELS,
            sample_rate: TARGET_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let out = path.as_ref().to_path_buf();
        let mut writer = WavWriter::create(&out, spec)?;
        let mut out_count = 0usize;
        let max_out = TARGET_RATE as usize * duration_secs as usize;
        for chunk in buf.chunks(group) {
            if out_count >= max_out {
                break;
            }
            let avg: f32 = chunk.iter().sum::<f32>() / chunk.len() as f32;
            let v = (avg * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(v)?;
            out_count += 1;
        }

        Ok(out)
    }

    /// Record with the default duration.
    pub fn record_default(&self, path: impl AsRef<Path>) -> Result<PathBuf, RecordError> {
        self.record(path, self.default_secs)
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}