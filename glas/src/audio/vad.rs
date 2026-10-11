//! Простой энергетический VAD — детектор тишины для авто-остановки записи.

/// Минимальная длительность тишины (в кадрах), после которой считаем, что
/// пользователь закончил говорить.
pub const SILENCE_FRAMES: u32 = 20;

/// Среднеквадратичная мощность кадра.
pub fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

/// Кадр считается тишиной, если его RMS ниже порога.
pub fn is_silence(frame: &[f32], threshold: f32) -> bool {
    rms(frame) < threshold
}
