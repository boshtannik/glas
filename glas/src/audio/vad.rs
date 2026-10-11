//! A simple energy-based VAD — a silence detector for auto-stopping a recording.

/// Minimum silence duration (in frames) after which the user is considered to
/// have finished speaking.
pub const SILENCE_FRAMES: u32 = 20;

/// Root-mean-square power of a frame.
pub fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

/// A frame counts as silence when its RMS is below the threshold.
pub fn is_silence(frame: &[f32], threshold: f32) -> bool {
    rms(frame) < threshold
}
