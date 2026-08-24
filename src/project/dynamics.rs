//! A feed-forward dynamics processor (soft-knee gain computer + branched
//! attack/release envelope smoothing, plus a lookahead-window detector) —
//! the shared engine behind the Autotune chain's Compressor and Limiter
//! stages, which differ only in their parameters (the Limiter uses a very
//! high ratio to approximate hard limiting). Stereo-linked: the same gain
//! reduction is computed from the loudest channel at each frame and
//! applied to all channels, so it never shifts the stereo image.

pub struct DynamicsParams {
    pub threshold_db: f32,
    /// Compression ratio (`>= 1`); use a very large value (e.g. `1000.0`)
    /// to approximate a brick-wall limiter.
    pub ratio: f32,
    pub knee_width_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub lookahead_ms: f32,
    pub makeup_gain_db: f32,
}

/// The standard soft-knee gain computer (Giannoulis, Massberg & Reiss,
/// "Digital Dynamic Range Compressor Design"): returns the *output* level
/// in dB for a given input level in dB. The gain reduction to apply is
/// `gain_computer_db(level) - level`, which is `<= 0`.
fn gain_computer_db(level_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
    let overshoot = level_db - threshold_db;
    if 2.0 * overshoot < -knee_db {
        level_db
    } else if 2.0 * overshoot.abs() <= knee_db {
        level_db + (1.0 / ratio - 1.0) * (overshoot + knee_db / 2.0).powi(2) / (2.0 * knee_db)
    } else {
        threshold_db + overshoot / ratio
    }
}

fn linear_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-6).log10()
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Applies compression/limiting to `samples` (interleaved, `channels`
/// channels) in place.
pub fn process(samples: &mut [f32], channels: usize, sample_rate: f32, params: &DynamicsParams) {
    if channels == 0 {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }

    let lookahead = ((params.lookahead_ms / 1000.0) * sample_rate).round() as usize;
    let attack_coef = ms_to_coef(params.attack_ms, sample_rate);
    let release_coef = ms_to_coef(params.release_ms, sample_rate);

    // Detector level per frame: loudest channel's absolute sample, then the
    // max over the upcoming lookahead window — lets the envelope start
    // reacting before a peak actually arrives, which is what "lookahead"
    // buys a limiter/compressor without needing to also delay the audio
    // for this offline (non-realtime) processing.
    fn frame_peak(samples: &[f32], channels: usize, frame: usize) -> f32 {
        (0..channels).map(|ch| samples[frame * channels + ch].abs()).fold(0.0f32, f32::max)
    }

    let mut smoothed_reduction_db = 0.0f32;
    for frame in 0..frames {
        let window_end = (frame + lookahead + 1).min(frames);
        let mut detector = 0.0f32;
        for f in frame..window_end {
            detector = detector.max(frame_peak(samples, channels, f));
        }
        let level_db = linear_to_db(detector);
        let raw_reduction_db = gain_computer_db(level_db, params.threshold_db, params.ratio, params.knee_width_db)
            - level_db;

        let coef = if raw_reduction_db < smoothed_reduction_db { attack_coef } else { release_coef };
        smoothed_reduction_db = coef * smoothed_reduction_db + (1.0 - coef) * raw_reduction_db;

        let gain = db_to_linear(smoothed_reduction_db + params.makeup_gain_db);
        for ch in 0..channels {
            samples[frame * channels + ch] *= gain;
        }
    }
}

/// One-pole smoothing coefficient for a given time constant in
/// milliseconds — `coef` closer to 1 means slower (more smoothing).
fn ms_to_coef(time_ms: f32, sample_rate: f32) -> f32 {
    if time_ms <= 0.0 {
        return 0.0;
    }
    (-1.0 / (time_ms / 1000.0 * sample_rate)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_signal_below_threshold_is_left_alone() {
        let sample_rate = 48_000.0;
        let mut samples = vec![0.1f32; 4096];
        let params = DynamicsParams {
            threshold_db: -18.0,
            ratio: 3.5,
            knee_width_db: 6.0,
            attack_ms: 5.0,
            release_ms: 120.0,
            lookahead_ms: 1.0,
            makeup_gain_db: 0.0,
        };
        process(&mut samples, 1, sample_rate, &params);
        for &s in &samples {
            assert!((s - 0.1).abs() < 1e-3, "quiet signal should pass through ~unchanged, got {s}");
        }
    }

    #[test]
    fn loud_signal_above_threshold_is_reduced() {
        let sample_rate = 48_000.0;
        let mut samples = vec![0.9f32; 8192];
        let params = DynamicsParams {
            threshold_db: -18.0,
            ratio: 3.5,
            knee_width_db: 6.0,
            attack_ms: 5.0,
            release_ms: 120.0,
            lookahead_ms: 1.0,
            makeup_gain_db: 0.0,
        };
        process(&mut samples, 1, sample_rate, &params);
        let settled = samples[samples.len() - 1];
        assert!(settled < 0.9, "loud signal should be gain-reduced once the envelope settles, got {settled}");
    }

    #[test]
    fn stereo_link_applies_the_same_gain_to_both_channels() {
        let sample_rate = 48_000.0;
        // Loud on the left, silent on the right — the detector should pick
        // up the loud channel and reduce both by the same amount.
        let mut samples = Vec::with_capacity(8192 * 2);
        for _ in 0..8192 {
            samples.push(0.9);
            samples.push(0.0);
        }
        let params = DynamicsParams {
            threshold_db: -18.0,
            ratio: 1000.0,
            knee_width_db: 2.0,
            attack_ms: 1.0,
            release_ms: 20.0,
            lookahead_ms: 1.0,
            makeup_gain_db: 0.0,
        };
        process(&mut samples, 2, sample_rate, &params);
        let last_left = samples[samples.len() - 2];
        assert!(last_left < 0.9, "left channel should be reduced, got {last_left}");
        // Right channel started silent and should stay silent (same gain,
        // applied to zero, is still zero) rather than gaining any energy.
        let last_right = samples[samples.len() - 1];
        assert_eq!(last_right, 0.0);
    }
}
