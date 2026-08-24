use super::db_to_gain;
use serde::{Deserialize, Serialize};

/// Which physical channel gets the fade-in vs the fade-out in `apply` — see
/// `PanToggleParams`. Derives `Serialize`/`Deserialize` directly (rather
/// than a separate on-disk shadow type, as `persistence` uses for
/// `Project`) since it's a plain two-variant enum with no invariants to
/// keep off of disk — it's persisted as part of the GUI's app-level
/// Effects settings (see `gui::settings_persistence`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PanToggleDirection {
    Left,
    Right,
}

impl Default for PanToggleDirection {
    fn default() -> Self {
        PanToggleDirection::Left
    }
}

/// Settings for the Pan Toggle effect (`Project::apply_pan_toggle`): a
/// stereo clip's two channels are conceptually split apart, one ramped
/// linearly from `low_db` up to `high_db` across the clip's duration
/// (fading in) and the other ramped the reverse way, from `high_db` down to
/// `low_db` (fading out), then recombined into the stereo output.
/// `direction` picks which physical side gets the fade-in.
#[derive(Clone)]
pub struct PanToggleParams {
    pub high_db: f32,
    pub low_db: f32,
    pub direction: PanToggleDirection,
}

impl Default for PanToggleParams {
    fn default() -> Self {
        PanToggleParams { high_db: 6.0, low_db: -4.0, direction: PanToggleDirection::Left }
    }
}

/// Applies the effect to `samples` (interleaved stereo) in place: each
/// frame's left/right samples are scaled by a linearly-ramped gain, one
/// side rising from `low_db` to `high_db` and the other falling from
/// `high_db` to `low_db` across the buffer — `direction` decides which side
/// is which (see `PanToggleParams`). A no-op unless `channels == 2` (a mono
/// buffer has no left/right to toggle between).
pub fn apply(samples: &mut [f32], channels: usize, params: &PanToggleParams) {
    if channels != 2 {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let last = (frames.max(1) - 1).max(1) as f32;
    let left_fades_in = params.direction == PanToggleDirection::Left;

    for frame in 0..frames {
        let t = frame as f32 / last;
        let fade_in_gain = db_to_gain(params.low_db + (params.high_db - params.low_db) * t);
        let fade_out_gain = db_to_gain(params.high_db + (params.low_db - params.high_db) * t);
        let (left_gain, right_gain) =
            if left_fades_in { (fade_in_gain, fade_out_gain) } else { (fade_out_gain, fade_in_gain) };
        samples[frame * 2] *= left_gain;
        samples[frame * 2 + 1] *= right_gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constant-amplitude stereo "signal" so the output directly reflects
    /// the applied gain at each frame, not a coincidental sample value.
    fn stereo_tone(frames: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            out.push(1.0);
            out.push(1.0);
        }
        out
    }

    #[test]
    fn mono_buffer_is_untouched() {
        let mut samples = vec![0.5, -0.3, 0.2];
        let original = samples.clone();
        apply(&mut samples, 1, &PanToggleParams::default());
        assert_eq!(samples, original);
    }

    #[test]
    fn left_direction_fades_left_up_and_right_down() {
        let mut samples = stereo_tone(1000);
        let params = PanToggleParams { high_db: 6.0, low_db: -4.0, direction: PanToggleDirection::Left };
        apply(&mut samples, 2, &params);

        let first_left = samples[0].abs();
        let last_left = samples[(999) * 2].abs();
        let first_right = samples[1].abs();
        let last_right = samples[999 * 2 + 1].abs();

        // Left starts near the low point and ends near the high point;
        // right does the reverse.
        assert!(last_left > first_left, "left channel should grow louder over time");
        assert!(last_right < first_right, "right channel should grow quieter over time");
    }

    #[test]
    fn right_direction_is_the_mirror_of_left() {
        let mut left_dir = stereo_tone(1000);
        let mut right_dir = left_dir.clone();
        let base = PanToggleParams::default();
        apply(&mut left_dir, 2, &PanToggleParams { direction: PanToggleDirection::Left, ..base.clone() });
        apply(&mut right_dir, 2, &PanToggleParams { direction: PanToggleDirection::Right, ..base });

        for frame in 0..1000 {
            let l = frame * 2;
            let r = frame * 2 + 1;
            assert!((left_dir[l] - right_dir[r]).abs() < 1e-6);
            assert!((left_dir[r] - right_dir[l]).abs() < 1e-6);
        }
    }

    #[test]
    fn endpoints_match_configured_db_values() {
        let mut samples = stereo_tone(2);
        let params = PanToggleParams { high_db: 6.0, low_db: -4.0, direction: PanToggleDirection::Left };
        let original = samples.clone();
        apply(&mut samples, 2, &params);

        let expected_low = db_to_gain(-4.0);
        let expected_high = db_to_gain(6.0);
        assert!((samples[0] - original[0] * expected_low).abs() < 1e-4);
        assert!((samples[1] - original[1] * expected_high).abs() < 1e-4);
        assert!((samples[2] - original[2] * expected_high).abs() < 1e-4);
        assert!((samples[3] - original[3] * expected_low).abs() < 1e-4);
    }
}
