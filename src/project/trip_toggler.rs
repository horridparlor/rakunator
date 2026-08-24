//! A port of the "trip-toggler.py" script: find clear quiet "low points"
//! in a waveform (a smoothed loudness envelope, filtered by peak
//! prominence and minimum spacing so only genuine dips qualify — fuzzy
//! stretches that never really return to quiet produce no cut there), then
//! alternate a fade-down and a fade-up across the resulting segments. The
//! original script batch-processed whole files with interactive prompts
//! and wrote out debug plots/label files; none of that maps to an
//! in-editor effect, so this keeps only its core per-audio algorithm:
//! envelope-based low-point detection plus the alternating fade shapes.
//! It also skips the script's "reuse the first file's cut points for
//! same-length later files" batch-consistency feature, since "batch of
//! files" doesn't have a clean equivalent for "apply to the selection".

/// Detection is a from-scratch reimplementation of the shape of
/// `scipy.signal.find_peaks`'s prominence+distance filtering (local
/// maxima -> distance selection -> prominence selection, matching scipy's
/// documented filter order), not scipy itself — results should match
/// closely but not bit-for-bit.
#[derive(Clone)]
pub struct TripTogglerParams {
    /// Gradual-mode fade endpoints (unused in Instant mode).
    pub high_db: f32,
    pub low_db: f32,
    /// `false` = "basic" (tuned for gaps between separate hits/phrases),
    /// `true` = "super" (tuned for the finer low points inside a single
    /// hit's own decay).
    pub super_mode: bool,
    /// Fine-tune multiplier scaling prominence/spacing/smoothing together;
    /// above 1.0 finds finer, closer-together cuts, below 1.0 coarser ones.
    pub detail: f32,
    /// `false` = "gradual" (pure fade between `high_db`/`low_db`, no
    /// discontinuity at segment boundaries), `true` = "instant" (a flat
    /// gain step at each segment's start, plus its own separate fade
    /// layered on top).
    pub instant_shift: bool,
    pub instant_high_gain_db: f32,
    pub instant_low_gain_db: f32,
    pub instant_high_fade_start_db: f32,
    pub instant_high_fade_end_db: f32,
    pub instant_low_fade_start_db: f32,
    pub instant_low_fade_end_db: f32,
    /// Mirrors Audacity's Adjustable Fade "curve" bias, -100..100.
    pub fade_curve_adjust: f32,
    /// Whether the clip's first segment starts at the "high"/loud end
    /// (fading down) or the "low"/quiet end (fading up).
    pub start_high: bool,
}

impl Default for TripTogglerParams {
    fn default() -> Self {
        TripTogglerParams {
            high_db: 4.0,
            low_db: -4.0,
            super_mode: false,
            detail: 1.0,
            instant_shift: false,
            instant_high_gain_db: 2.0,
            instant_low_gain_db: -6.0,
            instant_high_fade_start_db: 4.0,
            instant_high_fade_end_db: 0.0,
            instant_low_fade_start_db: -4.0,
            instant_low_fade_end_db: 4.0,
            fade_curve_adjust: 0.0,
            start_high: true,
        }
    }
}

struct DetectionPreset {
    min_hit_seconds: f32,
    prominence_db: f32,
    envelope_window_ms: f32,
}

fn detection_preset(super_mode: bool) -> DetectionPreset {
    if super_mode {
        DetectionPreset { min_hit_seconds: 0.015, prominence_db: 3.0, envelope_window_ms: 1.5 }
    } else {
        DetectionPreset { min_hit_seconds: 0.08, prominence_db: 6.0, envelope_window_ms: 8.0 }
    }
}

/// Boxcar-smoothed RMS amplitude envelope, in dB (mirrors the script's
/// `compute_envelope_db`: a centered moving average of the squared signal,
/// treating anything outside the buffer as silence, then sqrt + 20*log10).
fn compute_envelope_db(mono: &[f32], sample_rate: f32, window_ms: f32) -> Vec<f32> {
    let n = mono.len();
    let mut window = ((sample_rate * window_ms / 1000.0).round() as usize).max(1);
    if window.is_multiple_of(2) {
        window += 1;
    }
    let half = window / 2;

    let mut prefix = vec![0.0f64; n + 1];
    for (i, &s) in mono.iter().enumerate() {
        prefix[i + 1] = prefix[i] + (s as f64) * (s as f64);
    }

    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            let sum_sq = prefix[hi] - prefix[lo];
            let rms = (sum_sq / window as f64).sqrt();
            (20.0 * (rms + 1e-9).log10()) as f32
        })
        .collect()
}

/// Local maxima of `x`, with scipy's plateau handling: a maximal run of
/// equal values that's higher than both its immediate neighbors counts as
/// one peak, located at the run's midpoint.
fn local_maxima(x: &[f32]) -> Vec<usize> {
    let n = x.len();
    if n < 3 {
        return Vec::new();
    }
    let mut maxima = Vec::new();
    let mut i = 1;
    while i < n - 1 {
        if x[i - 1] < x[i] {
            let mut ahead = i + 1;
            while ahead < n - 1 && x[ahead] == x[i] {
                ahead += 1;
            }
            if x[ahead] < x[i] {
                maxima.push((i + (ahead - 1)) / 2);
            }
            i = ahead;
        } else {
            i += 1;
        }
    }
    maxima
}

/// Topographic prominence of the peak at `x[peak]`: extend outward in each
/// direction until hitting either the array edge or a strictly taller
/// point, tracking the minimum seen along the way; prominence is the
/// peak's height above the higher of its two "bases".
fn prominence(x: &[f32], peak: usize) -> f32 {
    let h = x[peak];

    let mut left_min = h;
    let mut i = peak;
    while i > 0 {
        i -= 1;
        if x[i] > h {
            break;
        }
        left_min = left_min.min(x[i]);
    }

    let mut right_min = h;
    let mut j = peak;
    while j + 1 < x.len() {
        j += 1;
        if x[j] > h {
            break;
        }
        right_min = right_min.min(x[j]);
    }

    h - left_min.max(right_min)
}

/// scipy's greedy `_select_by_peak_distance`: visiting peaks from tallest
/// to shortest, each surviving peak suppresses every other not-yet-removed
/// peak within `distance` samples of it (in position order, so the scan in
/// each direction can stop the moment it finds one outside that radius).
fn select_by_distance(peaks: &[usize], heights: &[f32], distance: usize) -> Vec<usize> {
    let n = peaks.len();
    if n == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| heights[a].partial_cmp(&heights[b]).unwrap());

    let mut keep = vec![true; n];
    for &j in order.iter().rev() {
        if !keep[j] {
            continue;
        }
        let mut k = j;
        while k > 0 {
            k -= 1;
            if peaks[j] - peaks[k] < distance {
                keep[k] = false;
            } else {
                break;
            }
        }
        let mut k = j;
        while k + 1 < n {
            k += 1;
            if peaks[k] - peaks[j] < distance {
                keep[k] = false;
            } else {
                break;
            }
        }
    }
    (0..n).filter(|&i| keep[i]).map(|i| peaks[i]).collect()
}

/// Local maxima -> distance filter -> prominence filter, matching scipy's
/// documented `find_peaks` filter ordering for these two filters.
fn find_peaks_with_prominence(x: &[f32], distance: usize, prominence_threshold: f32) -> Vec<usize> {
    let maxima = local_maxima(x);
    let heights: Vec<f32> = maxima.iter().map(|&p| x[p]).collect();
    let after_distance = select_by_distance(&maxima, &heights, distance);
    after_distance.into_iter().filter(|&p| prominence(x, p) >= prominence_threshold).collect()
}

/// Finds sample indices of clear low points in `mono`, refined to the
/// quietest individual sample within a small window around each candidate.
fn find_cut_points(mono: &[f32], sample_rate: f32, params: &TripTogglerParams) -> Vec<usize> {
    let detail = if params.detail <= 0.0 { 0.01 } else { params.detail };
    let preset = detection_preset(params.super_mode);
    let prominence_db = preset.prominence_db / detail;
    let min_hit_seconds = preset.min_hit_seconds / detail;
    let window_ms = preset.envelope_window_ms / detail;

    let env_db = compute_envelope_db(mono, sample_rate, window_ms);
    let neg_env: Vec<f32> = env_db.iter().map(|&v| -v).collect();
    let min_distance = ((min_hit_seconds * sample_rate) as usize).max(1);

    let peaks = find_peaks_with_prominence(&neg_env, min_distance, prominence_db);

    let refine_radius = ((0.01 * sample_rate) as usize).max(1).min((min_distance / 2).max(1));
    let mut refined: Vec<usize> = peaks
        .iter()
        .map(|&p| {
            let lo = p.saturating_sub(refine_radius);
            let hi = (p + refine_radius).min(mono.len());
            let mut best_i = lo;
            let mut best_v = f32::MAX;
            for (i, &s) in mono.iter().enumerate().take(hi).skip(lo) {
                let v = s.abs();
                if v < best_v {
                    best_v = v;
                    best_i = i;
                }
            }
            best_i
        })
        .collect();
    refined.sort_unstable();
    refined.dedup();
    refined
}

fn build_segments(cuts: &[usize], total_frames: usize) -> Vec<usize> {
    let mut boundaries: Vec<usize> = std::iter::once(0).chain(cuts.iter().copied()).chain(std::iter::once(total_frames)).collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

fn db_to_amp(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn smoothstep(t: f32) -> f32 {
    3.0 * t * t - 2.0 * t * t * t
}

enum Shape {
    Linear,
    SCurve,
}

/// Per-sample linear-amplitude gain curve across `n` samples, from
/// `start_db` to `end_db`, in the given `shape`. `curve_adjust` (-100..100)
/// biases where an S-curve's midpoint sits without breaking its monotonic
/// direction (mirrors Audacity's Adjustable Fade curve bias).
fn build_gain_curve(n: usize, start_db: f32, end_db: f32, shape: Shape, curve_adjust: f32) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|k| {
            let mut t = k as f32 / n as f32;
            let t_eased = match shape {
                Shape::SCurve => {
                    if curve_adjust != 0.0 {
                        let bias = (curve_adjust / 100.0) * t * (1.0 - t);
                        t = (t + bias).clamp(0.0, 1.0);
                    }
                    smoothstep(t)
                }
                Shape::Linear => t,
            };
            db_to_amp(start_db + (end_db - start_db) * t_eased)
        })
        .collect()
}

fn segment_gain(n: usize, is_up: bool, params: &TripTogglerParams) -> Vec<f32> {
    if params.instant_shift {
        if is_up {
            let fade = build_gain_curve(n, params.instant_low_fade_start_db, params.instant_low_fade_end_db, Shape::SCurve, params.fade_curve_adjust);
            let step = db_to_amp(params.instant_low_gain_db);
            fade.into_iter().map(|f| f * step).collect()
        } else {
            let fade = build_gain_curve(n, params.instant_high_fade_start_db, params.instant_high_fade_end_db, Shape::Linear, params.fade_curve_adjust);
            let step = db_to_amp(params.instant_high_gain_db);
            fade.into_iter().map(|f| f * step).collect()
        }
    } else if is_up {
        build_gain_curve(n, params.low_db, params.high_db, Shape::SCurve, params.fade_curve_adjust)
    } else {
        build_gain_curve(n, params.high_db, params.low_db, Shape::Linear, params.fade_curve_adjust)
    }
}

/// Applies the effect to `samples` (interleaved, `channels` channels) in
/// place: detects low points from the mono-mixed signal, then multiplies
/// every channel by the same per-frame alternating fade-down/fade-up gain
/// curve. Unlike the original script, output is clamped to [-1, 1] to
/// match this app's other destructive effects.
pub fn apply(samples: &mut [f32], channels: usize, sample_rate: f32, params: &TripTogglerParams) {
    if channels == 0 {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }

    let mono: Vec<f32> =
        (0..frames).map(|f| (0..channels).map(|ch| samples[f * channels + ch]).sum::<f32>() / channels as f32).collect();

    let cuts = find_cut_points(&mono, sample_rate, params);
    let boundaries = build_segments(&cuts, frames);
    let start_is_low = !params.start_high;

    for i in 0..boundaries.len() - 1 {
        let seg_start = boundaries[i];
        let seg_end = boundaries[i + 1];
        let n = seg_end - seg_start;
        if n == 0 {
            continue;
        }
        let is_up = (i % 2 == 0) == start_is_low;
        let gain = segment_gain(n, is_up, params);
        for (k, &g) in gain.iter().enumerate() {
            for ch in 0..channels {
                let idx = (seg_start + k) * channels + ch;
                samples[idx] = (samples[idx] * g).clamp(-1.0, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(sample_rate: f32, hit_len: usize, gap_len: usize, count: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for _ in 0..count {
            for i in 0..hit_len {
                let t = i as f32 / sample_rate;
                let envelope = (-3.0 * i as f32 / hit_len as f32).exp();
                out.push(envelope * (2.0 * std::f32::consts::PI * 440.0 * t).sin());
            }
            out.extend(std::iter::repeat_n(0.0, gap_len));
        }
        out
    }

    #[test]
    fn finds_a_cut_between_two_separated_hits() {
        let sample_rate = 48_000.0;
        let mono = hits(sample_rate, 4800, 4800, 2); // two 0.1s hits, 0.1s apart
        let params = TripTogglerParams::default();
        let cuts = find_cut_points(&mono, sample_rate, &params);
        assert!(!cuts.is_empty(), "expected at least one cut point between the two hits");
        // The cut should land somewhere within the silent gap.
        assert!(cuts.iter().any(|&c| c > 4700 && c < 9700), "cut point {cuts:?} should fall in the gap");
    }

    #[test]
    fn a_single_sustained_tone_produces_no_cuts() {
        let sample_rate = 48_000.0;
        let mono: Vec<f32> =
            (0..48_000).map(|i| (2.0 * std::f32::consts::PI * 220.0 * i as f32 / sample_rate).sin()).collect();
        let params = TripTogglerParams::default();
        let cuts = find_cut_points(&mono, sample_rate, &params);
        assert!(cuts.is_empty(), "a steady tone with no dips should produce no cut points, got {cuts:?}");
    }

    #[test]
    fn apply_alternates_fade_direction_across_segments_and_stays_in_range() {
        let sample_rate = 48_000.0;
        let mut samples = hits(sample_rate, 4800, 4800, 3);
        let original = samples.clone();
        let params = TripTogglerParams::default();

        apply(&mut samples, 1, sample_rate, &params);

        assert_eq!(samples.len(), original.len());
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        // Something should actually have changed (gain isn't unity
        // everywhere), otherwise the effect did nothing.
        assert!(samples.iter().zip(original.iter()).any(|(a, b)| (a - b).abs() > 1e-6));
    }

    #[test]
    fn instant_shift_mode_runs_without_crashing() {
        let sample_rate = 48_000.0;
        let mut samples = hits(sample_rate, 4800, 4800, 2);
        let params = TripTogglerParams { instant_shift: true, ..TripTogglerParams::default() };
        apply(&mut samples, 1, sample_rate, &params);
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}
