use crate::waveform::Waveform;
use std::time::Duration;

/// Renders `waveform` at `frequency_hz` for `duration` at `sample_rate_hz`.
/// Shared by the "Create Wave" GUI action and (indirectly, via identical
/// math) the export mixdown, so playback and export never drift apart.
pub fn render_waveform_clip(
    waveform: Waveform,
    frequency_hz: f32,
    duration: Duration,
    sample_rate_hz: u32,
) -> Vec<f32> {
    let sample_count = (duration.as_secs_f32() * sample_rate_hz as f32).round() as usize;
    let phase_step = frequency_hz / sample_rate_hz as f32;

    let mut phase = 0f32;
    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        samples.push(waveform.sample(phase));
        phase = (phase + phase_step) % 1.0;
    }
    samples
}
