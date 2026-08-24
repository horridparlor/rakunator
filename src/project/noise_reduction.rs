//! A blind spectral noise gate — the Autotune chain's stand-in for
//! Audacity's Noise Reduction effect. Audacity's real Noise Reduction
//! subtracts a spectrum captured earlier from a user-selected noise-only
//! region; that profile isn't part of the exported chain string (only the
//! fact that "current settings" were reused), so there's no actual profile
//! to act on here. Instead this estimates a noise floor per frequency bin
//! directly from the clip itself (the per-bin minimum magnitude across the
//! whole clip — a standard "minimum statistics" blind estimate) and gates
//! each STFT frame against it.

use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;

const WINDOW_LEN: usize = 2048;
const HOP: usize = 512;
/// Matches Audacity Noise Reduction's default "Noise reduction (dB)".
const REDUCTION_DB: f32 = 6.0;
/// How many multiples of the noise floor a bin needs to reach before it's
/// treated as fully "signal" (gain ramped back up to unity).
const RAMP_TOP_RATIO: f32 = 4.0;

fn hann(len: usize) -> Vec<f32> {
    (0..len).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / len as f32).cos()).collect()
}

/// Reduces stationary background noise in `samples` (interleaved,
/// `channels` channels) in place, per channel independently.
pub fn reduce_noise(samples: &mut [f32], channels: usize) {
    if channels == 0 {
        return;
    }
    for ch in 0..channels {
        reduce_noise_channel(samples, channels, ch);
    }
}

fn reduce_noise_channel(samples: &mut [f32], channels: usize, ch: usize) {
    let frames = samples.len() / channels;
    if frames < WINDOW_LEN {
        return; // Too short for a meaningful spectral estimate — leave as-is.
    }
    let window = hann(WINDOW_LEN);
    let bins = WINDOW_LEN / 2 + 1;
    let num_frames = (frames - WINDOW_LEN) / HOP + 1;

    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(WINDOW_LEN);
    let c2r = planner.plan_fft_inverse(WINDOW_LEN);

    let mut floor = vec![f32::MAX; bins];
    let mut spectra: Vec<Vec<Complex<f32>>> = Vec::with_capacity(num_frames);
    for f in 0..num_frames {
        let start = f * HOP;
        let mut buf = r2c.make_input_vec();
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = samples[(start + i) * channels + ch] * window[i];
        }
        let mut spectrum = r2c.make_output_vec();
        r2c.process(&mut buf, &mut spectrum).expect("noise reduction forward FFT shape mismatch");
        for (b, bin) in spectrum.iter().enumerate() {
            floor[b] = floor[b].min(bin.norm());
        }
        spectra.push(spectrum);
    }

    let min_gain = 10f32.powf(-REDUCTION_DB / 20.0);
    let mut out = vec![0.0f32; frames];
    let mut ola = vec![0.0f32; frames];
    let mut time = c2r.make_output_vec();
    let norm = 1.0 / WINDOW_LEN as f32;
    for (f, spectrum) in spectra.into_iter().enumerate() {
        let start = f * HOP;
        let mut spectrum = spectrum;
        for (b, bin) in spectrum.iter_mut().enumerate() {
            let mag = bin.norm();
            let fl = floor[b];
            let ratio = if fl > 1e-9 { mag / fl } else { 1.0 };
            let gain = if ratio <= 1.0 {
                min_gain
            } else {
                let x = ((ratio - 1.0) / (RAMP_TOP_RATIO - 1.0)).clamp(0.0, 1.0);
                min_gain + (1.0 - min_gain) * x
            };
            *bin *= gain;
        }
        c2r.process(&mut spectrum, &mut time).expect("noise reduction inverse FFT shape mismatch");
        for i in 0..WINDOW_LEN {
            out[start + i] += time[i] * norm * window[i];
            ola[start + i] += window[i] * window[i];
        }
    }

    for i in 0..frames {
        let denom = ola[i].max(1e-6);
        samples[i * channels + ch] = (out[i] / denom).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-noise (no external RNG dependency): a sum of a
    /// few incommensurate sine waves, which — unlike a single tone —
    /// spreads energy across many bins the way broadband noise would.
    fn pseudo_noise(sample_rate: f32, frames: usize, amplitude: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                let t = i as f32 / sample_rate;
                amplitude
                    * 0.34
                    * ((2.0 * std::f32::consts::PI * 733.0 * t).sin()
                        + (2.0 * std::f32::consts::PI * 1117.0 * t).sin()
                        + (2.0 * std::f32::consts::PI * 1999.0 * t).sin())
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    #[test]
    fn quiet_background_noise_is_attenuated() {
        let sample_rate = 48_000.0;
        let frames = sample_rate as usize * 2;
        let mut samples = pseudo_noise(sample_rate, frames, 0.02);
        let before_rms = rms(&samples);

        reduce_noise(&mut samples, 1);

        let after_rms = rms(&samples);
        assert!(after_rms < before_rms * 0.9, "expected uniform background noise to be reduced: {before_rms} -> {after_rms}");
    }

    #[test]
    fn a_loud_tone_over_a_noise_floor_survives_mostly_intact() {
        let sample_rate = 48_000.0;
        let frames = sample_rate as usize * 2;
        let mut samples = pseudo_noise(sample_rate, frames, 0.02);
        let burst_start = frames / 2 - 4096;
        let burst_end = frames / 2 + 4096;
        for (i, s) in samples[burst_start..burst_end].iter_mut().enumerate() {
            let t = i as f32 / sample_rate;
            *s += 0.6 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        }
        let before_burst_rms = rms(&samples[burst_start + 1024..burst_end - 1024]);

        reduce_noise(&mut samples, 1);

        let after_burst_rms = rms(&samples[burst_start + 1024..burst_end - 1024]);
        assert!(
            after_burst_rms > before_burst_rms * 0.7,
            "loud tone burst should survive largely intact: {before_burst_rms} -> {after_burst_rms}"
        );
    }

    #[test]
    fn very_short_clips_are_left_unchanged() {
        let mut samples = vec![0.5f32; 100];
        let original = samples.clone();
        reduce_noise(&mut samples, 1);
        assert_eq!(samples, original);
    }
}
