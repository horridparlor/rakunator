//! WSOLA (Waveform-Similarity Overlap-Add) time/pitch modification — the
//! shared engine behind the Tempo Up/Down and Sliding Stretch effects (and
//! the Rattle effect's own tempo/pitch shaping). Tempo is achieved by
//! walking through the source at a rate governed purely by the tempo
//! ratio (a textbook time-stretch: pitch is untouched because the
//! synthesis window itself is never resampled). Pitch is achieved
//! independently by reading a `pitch_ratio`-scaled amount of source into
//! each fixed-length synthesis window and resampling *that* back down to
//! the fixed window length (a granular pitch shifter) — so the two knobs
//! don't affect each other, and both are ramped smoothly from "initial" to
//! "final" across the source's duration.

/// One "sliding stretch" schedule: tempo/pitch ramp from an initial value
/// (at the start of the source) to a final value (at its end). A constant
/// (non-ramped) Tempo Up/Down effect just sets `initial == final`.
#[derive(Clone, Copy)]
pub struct RampParams {
    /// Percent change in playback speed (Audacity's "Change Tempo"
    /// convention): `+100.0` finishes twice as fast (half the duration),
    /// `-50.0` finishes at half speed (double the duration).
    pub initial_tempo_percent: f32,
    pub final_tempo_percent: f32,
    pub initial_pitch_semitones: f32,
    pub final_pitch_semitones: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Periodic (DFT-even) Hann window — overlapped at exactly half its length
/// (as used here), a sum of these windows is constant, which is what makes
/// plain overlap-add reconstruct cleanly.
fn periodic_hann(len: usize) -> Vec<f32> {
    (0..len).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / len as f32).cos()).collect()
}

/// Linearly resamples `grain` (single channel) to exactly `dst_len` samples.
fn resample_grain(grain: &[f32], dst_len: usize) -> Vec<f32> {
    let src_len = grain.len();
    if src_len == dst_len {
        return grain.to_vec();
    }
    if src_len < 2 || dst_len < 2 {
        return vec![grain.first().copied().unwrap_or(0.0); dst_len];
    }
    (0..dst_len)
        .map(|i| {
            let pos = i as f32 * (src_len - 1) as f32 / (dst_len - 1) as f32;
            let idx = pos.floor() as usize;
            let frac = pos - idx as f32;
            let a = grain[idx.min(src_len - 1)];
            let b = grain[(idx + 1).min(src_len - 1)];
            a + (b - a) * frac
        })
        .collect()
}

fn normalized_correlation(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    dot / (na.sqrt() * nb.sqrt() + 1e-9)
}

/// Applies the tempo/pitch ramp to `source` (interleaved, `channels`
/// channels) and returns a new buffer. The output length follows from
/// however long it takes to consume the source at the given tempo
/// schedule, so it's generally *not* the same length as `source`.
pub fn apply_time_pitch_ramp(source: &[f32], channels: usize, sample_rate: f32, params: &RampParams) -> Vec<f32> {
    if channels == 0 || source.is_empty() {
        return Vec::new();
    }
    let total_frames = source.len() / channels;

    let window_len = (((sample_rate * 0.04) as usize) / 2 * 2).max(64); // ~40ms, forced even
    let synth_hop = window_len / 2;
    // A tight tolerance (a few ms) around the tempo-driven ideal position —
    // just enough to dodge phase discontinuities, not so much that the
    // search can drift arbitrarily far off the intended hop on tonal
    // material (periodic signals correlate well at many offsets).
    let search_radius = ((sample_rate * 0.005) as usize).clamp(1, synth_hop / 2);
    let hann = periodic_hann(window_len);

    let mono: Vec<f32> = (0..total_frames)
        .map(|f| (0..channels).map(|ch| source[f * channels + ch]).sum::<f32>() / channels as f32)
        .collect();
    let mono_at = |frame: i64| -> f32 {
        if frame < 0 || frame as usize >= total_frames {
            0.0
        } else {
            mono[frame as usize]
        }
    };
    let sample_at = |frame: i64, ch: usize| -> f32 {
        if frame < 0 || frame as usize >= total_frames {
            0.0
        } else {
            source[frame as usize * channels + ch]
        }
    };

    let tempo_ratio_at = |t: f32| -> f32 {
        let pct = lerp(params.initial_tempo_percent, params.final_tempo_percent, t);
        (1.0 + pct / 100.0).clamp(0.1, 10.0)
    };
    let pitch_ratio_at = |t: f32| -> f32 {
        let semitones = lerp(params.initial_pitch_semitones, params.final_pitch_semitones, t);
        2f32.powf(semitones / 12.0)
    };

    let mut out: Vec<f32> = Vec::new();
    let mut synth_pos = 0usize;
    let mut src_start: i64 = 0;
    // Worst case (minimum clamped tempo ratio) still makes bounded progress
    // per iteration, so this is a generous but finite safety cap rather
    // than a normal exit condition.
    let max_iterations = ((total_frames as f32 / (synth_hop as f32 * 0.1)).ceil() as usize) + 64;

    for _ in 0..max_iterations {
        if src_start >= total_frames as i64 {
            break;
        }
        let t = (src_start as f32 / total_frames.max(1) as f32).clamp(0.0, 1.0);
        let tempo_ratio = tempo_ratio_at(t);
        let pitch_ratio = pitch_ratio_at(t);

        let grain_src_len = ((window_len as f32) * pitch_ratio).round().max(1.0) as usize;
        let needed_frames = synth_pos + window_len;
        if out.len() < needed_frames * channels {
            out.resize(needed_frames * channels, 0.0);
        }
        for ch in 0..channels {
            let raw: Vec<f32> = (0..grain_src_len).map(|i| sample_at(src_start + i as i64, ch)).collect();
            let resampled = resample_grain(&raw, window_len);
            for (i, &s) in resampled.iter().enumerate() {
                out[(synth_pos + i) * channels + ch] += s * hann[i];
            }
        }

        // WSOLA search: place the next window near the tempo-driven ideal
        // position, but nudged to best match (via cross-correlation) the
        // overlap tail this window actually left behind, keeping the
        // splice phase-coherent instead of introducing clicks/phasiness.
        let nominal_advance = ((synth_hop as f32) * tempo_ratio).round() as i64;
        let ideal_next = src_start + nominal_advance;
        let reference: Vec<f32> =
            (0..synth_hop).map(|i| mono_at(src_start + synth_hop as i64 + i as i64)).collect();

        // Try offset 0 first, then alternate outward (-1, +1, -2, +2, ...)
        // so that ties in score — common on tonal/periodic material, which
        // correlates well at many offsets — resolve to the offset closest
        // to the tempo-driven ideal rather than drifting arbitrarily.
        let mut best_offset = 0i64;
        let mut best_score = f32::MIN;
        let mut offsets = Vec::with_capacity(2 * search_radius + 1);
        offsets.push(0i64);
        for d in 1..=search_radius as i64 {
            offsets.push(-d);
            offsets.push(d);
        }
        for offset in offsets {
            let candidate_start = ideal_next + offset;
            let candidate: Vec<f32> = (0..synth_hop).map(|i| mono_at(candidate_start + i as i64)).collect();
            let score = normalized_correlation(&reference, &candidate);
            if score > best_score {
                best_score = score;
                best_offset = offset;
            }
        }

        src_start = ideal_next + best_offset;
        synth_pos += synth_hop;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_ramp(tempo_percent: f32, pitch_semitones: f32) -> RampParams {
        RampParams {
            initial_tempo_percent: tempo_percent,
            final_tempo_percent: tempo_percent,
            initial_pitch_semitones: pitch_semitones,
            final_pitch_semitones: pitch_semitones,
        }
    }

    fn sine(freq: f32, sample_rate: f32, frames: usize) -> Vec<f32> {
        (0..frames).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin()).collect()
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count()
    }

    #[test]
    fn identity_ramp_keeps_roughly_the_same_length() {
        let sample_rate = 48_000.0;
        let source = sine(220.0, sample_rate, sample_rate as usize * 2);
        let out = apply_time_pitch_ramp(&source, 1, sample_rate, &flat_ramp(0.0, 0.0));
        let ratio = out.len() as f32 / source.len() as f32;
        assert!((ratio - 1.0).abs() < 0.05, "expected near-identical length, got ratio {ratio}");
    }

    #[test]
    fn speeding_up_tempo_shortens_output() {
        let sample_rate = 48_000.0;
        let source = sine(220.0, sample_rate, sample_rate as usize * 2);
        let out = apply_time_pitch_ramp(&source, 1, sample_rate, &flat_ramp(100.0, 0.0));
        let ratio = out.len() as f32 / source.len() as f32;
        assert!((ratio - 0.5).abs() < 0.1, "expected ~half length at +100% tempo, got ratio {ratio}");
    }

    #[test]
    fn slowing_down_tempo_lengthens_output() {
        let sample_rate = 48_000.0;
        let source = sine(220.0, sample_rate, sample_rate as usize * 2);
        let out = apply_time_pitch_ramp(&source, 1, sample_rate, &flat_ramp(-50.0, 0.0));
        let ratio = out.len() as f32 / source.len() as f32;
        assert!((ratio - 2.0).abs() < 0.3, "expected ~double length at -50% tempo, got ratio {ratio}");
    }

    #[test]
    fn pitch_shift_preserves_duration_and_raises_frequency() {
        let sample_rate = 48_000.0;
        let frames = sample_rate as usize * 2;
        let source = sine(220.0, sample_rate, frames);
        let out = apply_time_pitch_ramp(&source, 1, sample_rate, &flat_ramp(0.0, 12.0));

        let ratio = out.len() as f32 / source.len() as f32;
        assert!((ratio - 1.0).abs() < 0.1, "pitch-only shift should preserve duration, got ratio {ratio}");

        // +12 semitones = one octave up = double frequency = double the
        // zero-crossing rate over a comparable stretch of steady-state
        // signal (skip the first/last windows, which are edge transients).
        let margin = frames / 8;
        let in_crossings = zero_crossings(&source[margin..frames - margin]);
        let out_len = out.len();
        let out_margin = out_len / 8;
        let out_crossings = zero_crossings(&out[out_margin..out_len - out_margin]);
        let scale = (out_len - 2 * out_margin) as f32 / (frames - 2 * margin) as f32;
        let normalized_out_crossings = out_crossings as f32 / scale;
        let observed_ratio = normalized_out_crossings / in_crossings as f32;
        assert!((observed_ratio - 2.0).abs() < 0.3, "expected ~2x zero-crossing rate, got {observed_ratio}");
    }
}
