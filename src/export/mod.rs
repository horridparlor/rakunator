mod config;
mod mp3;
mod wav;

use crate::waveform::Waveform;
use config::SAMPLE_RATE_HZ;
use std::time::Duration;

/// Renders `waveform` at `frequency_hz` for `duration` at the export sample
/// rate, then writes it to `~/Downloads/<waveform-name>.wav` and `.mp3`.
pub fn export_wave(waveform: Waveform, frequency_hz: f32, duration: Duration) {
    let samples = render_samples(waveform, frequency_hz, duration);

    let downloads_dir = dirs::download_dir().expect("could not determine Downloads directory");
    let base_name = waveform.name();

    let wav_path = downloads_dir.join(format!("{base_name}.wav"));
    let mp3_path = downloads_dir.join(format!("{base_name}.mp3"));

    wav::write_wav(&wav_path, &samples);
    mp3::write_mp3(&mp3_path, &samples);

    println!("wrote {}", wav_path.display());
    println!("wrote {}", mp3_path.display());
}

fn render_samples(waveform: Waveform, frequency_hz: f32, duration: Duration) -> Vec<f32> {
    let sample_count = (duration.as_secs_f32() * SAMPLE_RATE_HZ as f32).round() as usize;
    let phase_step = frequency_hz / SAMPLE_RATE_HZ as f32;

    let mut phase = 0f32;
    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        samples.push(waveform.sample(phase));
        phase = (phase + phase_step) % 1.0;
    }
    samples
}
