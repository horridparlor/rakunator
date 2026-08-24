//! Biquad IIR building blocks (RBJ "Audio EQ Cookbook" formulas) for the
//! fixed-curve EQ effects in the Effects menu ("Give to Speech", "Telephone"),
//! plus a linear-phase FIR "filter curve" builder used by the Autotune
//! chain: it spline-interpolates a set of (frequency, dB) control points
//! into a target magnitude response and designs an FIR filter from it by
//! inverse FFT — the same "sampled curve -> IFFT -> windowed kernel"
//! approach Audacity's "Filter Curve" EQ effect uses.

use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;

#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    /// A peaking (bell) EQ: boosts/cuts a band centered on `center_hz` by
    /// `gain_db`; `q` (center frequency / bandwidth) sets how narrow the
    /// bell is.
    pub fn peaking(sample_rate: f32, center_hz: f32, q: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * center_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// A 2nd-order Butterworth-shaped highpass at `cutoff_hz`; `q` sets the
    /// resonance (0.7071 = a single maximally-flat stage — cascade several
    /// stages at the standard higher-order Butterworth Q values for a
    /// steeper roll-off, as `telephone_stages` does).
    pub fn highpass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// Lowpass counterpart of `highpass`.
    pub fn lowpass(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// Runs one sample through this stage using transposed direct form 2
    /// (`z1`/`z2` are this stage's carried-over state — callers keep a
    /// separate `(z1, z2)` pair per independently-filtered channel).
    fn process(&self, x: f32, z1: &mut f32, z2: &mut f32) -> f32 {
        let y = self.b0 * x + *z1;
        *z1 = self.b1 * x + *z2 - self.a1 * y;
        *z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// The two Q values that give a proper 4th-order (24 dB/octave) Butterworth
/// response when two 2nd-order sections at the same cutoff are cascaded —
/// the standard cascaded-biquad recipe for higher-order Butterworth filters.
const BUTTERWORTH_4TH_ORDER_QS: [f32; 2] = [0.541_196, 1.306_563];

/// A dip built for speech clarity: a single peaking (bell) stage cutting
/// -4 dB across roughly 2 kHz-5 kHz (centered on their geometric mean, `Q`
/// set from the band's width) to tame harshness in that presence range.
pub fn give_to_speech_stage(sample_rate: f32) -> Biquad {
    const LOW_HZ: f32 = 2000.0;
    const HIGH_HZ: f32 = 5000.0;
    const GAIN_DB: f32 = -4.0;
    let center = (LOW_HZ * HIGH_HZ).sqrt();
    let q = center / (HIGH_HZ - LOW_HZ);
    Biquad::peaking(sample_rate, center, q, GAIN_DB)
}

/// Approximates Audacity's "Telephone" Equalization preset: a bandpass
/// covering the classic telephone voice bandwidth (~300 Hz - 3400 Hz, ITU
/// G.711), built from two cascaded 4th-order Butterworth sections
/// (highpass + lowpass) for a steep-ish roll-off outside that band without
/// being a brick wall — matching the curve shape in telephone-effect.png.
pub fn telephone_stages(sample_rate: f32) -> Vec<Biquad> {
    const LOW_CUTOFF_HZ: f32 = 300.0;
    const HIGH_CUTOFF_HZ: f32 = 3400.0;
    BUTTERWORTH_4TH_ORDER_QS
        .iter()
        .map(|&q| Biquad::highpass(sample_rate, LOW_CUTOFF_HZ, q))
        .chain(BUTTERWORTH_4TH_ORDER_QS.iter().map(|&q| Biquad::lowpass(sample_rate, HIGH_CUTOFF_HZ, q)))
        .collect()
}

/// Filters `samples` (interleaved, `channels` channels) in place through a
/// cascade of biquad `stages`, applied in order, with independent filter
/// state kept per channel (so a stereo signal's left/right don't leak into
/// each other).
pub fn apply_cascade(stages: &[Biquad], samples: &mut [f32], channels: usize) {
    if channels == 0 || stages.is_empty() {
        return;
    }
    let frames = samples.len() / channels;
    let mut state = vec![(0.0f32, 0.0f32); stages.len()];
    for ch in 0..channels {
        for slot in state.iter_mut() {
            *slot = (0.0, 0.0);
        }
        for frame in 0..frames {
            let idx = frame * channels + ch;
            let mut x = samples[idx];
            for (stage, (z1, z2)) in stages.iter().zip(state.iter_mut()) {
                x = stage.process(x, z1, z2);
            }
            samples[idx] = x;
        }
    }
}

/// One (frequency Hz, gain dB) control point of a filter curve.
#[derive(Clone, Copy)]
pub struct CurvePoint {
    pub hz: f32,
    pub db: f32,
}

/// The "FilterCurve" control points from the Autotune chain's Audacity
/// macro export — a de-essed, presence-boosted EQ curve used as the second
/// stage of `Project::apply_autotune`.
#[allow(clippy::excessive_precision)] // kept at full precision to match the source Audacity export verbatim
pub const AUTOTUNE_FILTER_CURVE: [CurvePoint; 14] = [
    CurvePoint { hz: 79.455203, db: -30.0 },
    CurvePoint { hz: 79.455203, db: -0.084506989 },
    CurvePoint { hz: 199.8574, db: 0.084506989 },
    CurvePoint { hz: 199.8574, db: -2.7887306 },
    CurvePoint { hz: 395.05487, db: -2.6197166 },
    CurvePoint { hz: 395.05487, db: 0.084506989 },
    CurvePoint { hz: 3947.732, db: 0.42253494 },
    CurvePoint { hz: 3947.732, db: 1.7746487 },
    CurvePoint { hz: 5882.7336, db: 1.9436626 },
    CurvePoint { hz: 5882.7336, db: 0.42253494 },
    CurvePoint { hz: 8839.337, db: 0.42253494 },
    CurvePoint { hz: 8987.4745, db: 1.6056347 },
    CurvePoint { hz: 9847.7397, db: 1.6056347 },
    CurvePoint { hz: 10012.777, db: 0.42253494 },
];

/// Natural cubic spline over a set of (x, y) points — `y_at` evaluates it
/// anywhere in `[x[0], x[last]]`; callers are responsible for flat
/// extrapolation outside that range (see `interpolate_curve`).
struct NaturalCubicSpline {
    xs: Vec<f32>,
    ys: Vec<f32>,
    /// Per-knot tangent slopes (Fritsch-Carlson monotone cubic Hermite —
    /// "PCHIP"), chosen so the curve never overshoots past its
    /// neighboring control points. A plain natural cubic spline was tried
    /// first and rejected: this curve's sharp corners are encoded as
    /// near-duplicate x points a hair's breadth apart (see
    /// `prepare_points`), and a global spline solve responds to that near-
    /// vertical segment with enormous ringing in every other segment —
    /// producing dB values in the thousands, which then overflow to
    /// infinity/NaN once converted to a linear magnitude.
    m: Vec<f32>,
}

impl NaturalCubicSpline {
    fn new(xs: Vec<f32>, ys: Vec<f32>) -> Self {
        let n = xs.len();
        if n < 2 {
            return NaturalCubicSpline { xs, ys, m: vec![0.0; n] };
        }
        let secant = |k: usize| (ys[k + 1] - ys[k]) / (xs[k + 1] - xs[k]);
        let mut m = vec![0.0f32; n];
        m[0] = secant(0);
        m[n - 1] = secant(n - 2);
        for k in 1..n - 1 {
            let d_prev = secant(k - 1);
            let d_next = secant(k);
            m[k] = if d_prev == 0.0 || d_next == 0.0 || d_prev.signum() != d_next.signum() {
                0.0
            } else {
                // Weighted harmonic mean (Fritsch-Butland) — keeps the
                // tangent bounded between the two secants automatically.
                let w1 = 2.0 * (xs[k + 1] - xs[k]) + (xs[k] - xs[k - 1]);
                let w2 = (xs[k + 1] - xs[k]) + 2.0 * (xs[k] - xs[k - 1]);
                (w1 + w2) / (w1 / d_prev + w2 / d_next)
            };
        }
        // Fritsch-Carlson limiter: clamp each interval's two tangents so
        // the Hermite segment can't overshoot its endpoints' y-values.
        for k in 0..n - 1 {
            let d = secant(k);
            if d == 0.0 {
                m[k] = 0.0;
                m[k + 1] = 0.0;
                continue;
            }
            let alpha = m[k] / d;
            let beta = m[k + 1] / d;
            let sq = alpha * alpha + beta * beta;
            if sq > 9.0 {
                let tau = 3.0 / sq.sqrt();
                m[k] = tau * alpha * d;
                m[k + 1] = tau * beta * d;
            }
        }
        NaturalCubicSpline { xs, ys, m }
    }

    fn eval(&self, x: f32) -> f32 {
        let n = self.xs.len();
        if n == 0 {
            return 0.0;
        }
        if n == 1 {
            return self.ys[0];
        }
        // Find the segment [xs[i], xs[i+1]] containing x (xs is sorted).
        let i = match self.xs.binary_search_by(|probe| probe.partial_cmp(&x).unwrap()) {
            Ok(idx) => idx.min(n - 2),
            Err(idx) => idx.saturating_sub(1).min(n - 2),
        };
        let h = self.xs[i + 1] - self.xs[i];
        if h <= 0.0 {
            return self.ys[i];
        }
        let t = (x - self.xs[i]) / h;
        let y0 = self.ys[i];
        let y1 = self.ys[i + 1];
        let m0 = self.m[i] * h;
        let m1 = self.m[i + 1] * h;
        // Standard cubic Hermite basis.
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * y0 + h10 * m0 + h01 * y1 + h11 * m1
    }
}

/// Sorts `points` by frequency and nudges apart any that land at (or
/// extremely near) the same frequency — a filter curve export can contain
/// exact-duplicate frequencies to encode a sharp corner (two control points
/// a hair's breadth apart in the real editor), which would otherwise make
/// the spline's x-coordinates non-monotonic.
fn prepare_points(points: &[CurvePoint]) -> Vec<CurvePoint> {
    let mut sorted: Vec<CurvePoint> = points.to_vec();
    sorted.sort_by(|a, b| a.hz.partial_cmp(&b.hz).unwrap());
    for i in 1..sorted.len() {
        let min_gap = (sorted[i - 1].hz.abs() * 1e-4).max(1e-3);
        if sorted[i].hz <= sorted[i - 1].hz + min_gap {
            sorted[i].hz = sorted[i - 1].hz + min_gap;
        }
    }
    sorted
}

/// Interpolates a filter curve's dB value at `query_hz`: natural cubic
/// spline over (log2(hz), dB) between the given `points`, held flat at the
/// nearest endpoint's dB outside their frequency range — matching how
/// Audacity's Filter Curve editor draws a curve that doesn't span the
/// whole visible spectrum.
pub struct FilterCurve {
    spline: NaturalCubicSpline,
    low_db: f32,
    high_db: f32,
    low_log: f32,
    high_log: f32,
}

impl FilterCurve {
    pub fn new(points: &[CurvePoint]) -> Self {
        let prepared = prepare_points(points);
        let xs: Vec<f32> = prepared.iter().map(|p| p.hz.max(1.0).log2()).collect();
        let ys: Vec<f32> = prepared.iter().map(|p| p.db).collect();
        let low_db = ys.first().copied().unwrap_or(0.0);
        let high_db = ys.last().copied().unwrap_or(0.0);
        let low_log = xs.first().copied().unwrap_or(0.0);
        let high_log = xs.last().copied().unwrap_or(0.0);
        FilterCurve { spline: NaturalCubicSpline::new(xs, ys), low_db, high_db, low_log, high_log }
    }

    pub fn db_at(&self, hz: f32) -> f32 {
        let x = hz.max(1.0).log2();
        if x <= self.low_log {
            self.low_db
        } else if x >= self.high_log {
            self.high_db
        } else {
            self.spline.eval(x)
        }
    }
}

/// Builds a linear-phase FIR of `length` taps (odd, so it has a single
/// center sample) approximating the dB-vs-frequency curve traced by
/// `points`: samples the curve across `0..=nyquist`, converts to linear
/// magnitude, and inverse-FFTs a zero-phase spectrum into a symmetric
/// kernel — the "sampled magnitude -> IFFT -> window" recipe behind
/// frequency-sampling FIR design (and behind Audacity's Filter Curve EQ).
pub fn design_filter_curve_fir(points: &[CurvePoint], length: usize, sample_rate: f32) -> Vec<f32> {
    let length = if length.is_multiple_of(2) { length + 1 } else { length }.max(3);
    let curve = FilterCurve::new(points);

    // fft_len must be even for RealFftPlanner and comfortably larger than
    // `length` so the designed kernel (after windowing/truncation) isn't
    // dominated by wraparound from the circular IFFT.
    let fft_len = (length * 2).next_power_of_two();

    let mut planner = RealFftPlanner::<f32>::new();
    let c2r = planner.plan_fft_inverse(fft_len);
    let mut spectrum = c2r.make_input_vec();
    for (k, bin) in spectrum.iter_mut().enumerate() {
        let hz = k as f32 * sample_rate / fft_len as f32;
        let db = curve.db_at(hz);
        let mag = 10f32.powf(db / 20.0);
        *bin = Complex::new(mag, 0.0);
    }
    let mut time_domain = c2r.make_output_vec();
    c2r.process(&mut spectrum, &mut time_domain).expect("inverse FFT shape mismatch");

    // Un-normalize (this FFT pair is unnormalized) and circularly shift so
    // the zero-phase response's center lands in the middle of the kernel.
    let norm = 1.0 / fft_len as f32;
    let half = length / 2;
    let mut kernel = vec![0.0f32; length];
    for (n, slot) in kernel.iter_mut().enumerate() {
        let shift = n as i64 - half as i64;
        let src = shift.rem_euclid(fft_len as i64) as usize;
        *slot = time_domain[src] * norm;
    }

    // Taper with a Blackman window to control the ringing a hard truncation
    // of the sampled-curve kernel would otherwise introduce.
    let n_minus_1 = (length - 1) as f32;
    for (n, s) in kernel.iter_mut().enumerate() {
        let t = 2.0 * std::f32::consts::PI * n as f32 / n_minus_1;
        let w = 0.42 - 0.5 * t.cos() + 0.08 * (2.0 * t).cos();
        *s *= w;
    }

    kernel
}

/// Convolves `samples` (interleaved, `channels` channels) with FIR `kernel`
/// in place, via FFT-based overlap-add — fast enough for kernels in the
/// thousands of taps, which a plain O(len * kernel_len) convolution would
/// not be. Output is trimmed back to the same length as the input, shifted
/// by the kernel's (odd-length, so integral) group delay so the result
/// stays time-aligned with the original.
pub fn convolve_fir(kernel: &[f32], samples: &mut [f32], channels: usize) {
    if channels == 0 || kernel.is_empty() {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let group_delay = kernel.len() / 2;

    // Block size chosen so fft_len is a convenient power of two comfortably
    // larger than the kernel (classic overlap-add sizing).
    let fft_len = (kernel.len() * 4).next_power_of_two();
    let block_len = fft_len - kernel.len() + 1;

    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(fft_len);
    let c2r = planner.plan_fft_inverse(fft_len);

    let mut kernel_time = r2c.make_input_vec();
    kernel_time[..kernel.len()].copy_from_slice(kernel);
    let mut kernel_spectrum = r2c.make_output_vec();
    r2c.process(&mut kernel_time, &mut kernel_spectrum).expect("kernel FFT shape mismatch");

    let norm = 1.0 / fft_len as f32;

    for ch in 0..channels {
        let mut out = vec![0.0f32; frames + kernel.len()];
        let mut pos = 0usize;
        let mut block_in = r2c.make_input_vec();
        let mut block_spectrum = r2c.make_output_vec();
        let mut block_out = c2r.make_output_vec();
        while pos < frames {
            let this_block = block_len.min(frames - pos);
            for slot in block_in.iter_mut() {
                *slot = 0.0;
            }
            for i in 0..this_block {
                block_in[i] = samples[(pos + i) * channels + ch];
            }
            r2c.process(&mut block_in, &mut block_spectrum).expect("block FFT shape mismatch");
            for (b, k) in block_spectrum.iter_mut().zip(kernel_spectrum.iter()) {
                *b *= k;
            }
            // The DC and Nyquist bins of a real-signal spectrum are purely
            // real; floating-point round-off from the multiplication above
            // can leave a tiny non-zero imaginary residue there, which
            // `ComplexToReal::process` rejects outright rather than
            // tolerating — so re-zero it explicitly before the inverse FFT.
            if let Some(first) = block_spectrum.first_mut() {
                first.im = 0.0;
            }
            if let Some(last) = block_spectrum.last_mut() {
                last.im = 0.0;
            }
            c2r.process(&mut block_spectrum, &mut block_out).expect("block IFFT shape mismatch");
            for (i, v) in block_out.iter().enumerate() {
                let out_idx = pos + i;
                if out_idx < out.len() {
                    out[out_idx] += v * norm;
                }
            }
            pos += this_block;
        }
        for frame in 0..frames {
            let src = frame + group_delay;
            samples[frame * channels + ch] = out.get(src).copied().unwrap_or(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaking_cuts_gain_at_center_frequency() {
        let sample_rate = 48_000.0;
        let stage = give_to_speech_stage(sample_rate);
        let center_hz = (2000.0f32 * 5000.0).sqrt();

        let sine_gain_db = |freq: f32| -> f32 {
            let mut z1 = 0.0;
            let mut z2 = 0.0;
            let n = 4096;
            let mut in_energy = 0.0;
            let mut out_energy = 0.0;
            for i in 0..n {
                let x = (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin();
                let y = stage.process(x, &mut z1, &mut z2);
                if i > n / 2 {
                    in_energy += x * x;
                    out_energy += y * y;
                }
            }
            10.0 * (out_energy / in_energy).log10()
        };

        let gain_at_center = sine_gain_db(center_hz);
        assert!((gain_at_center - (-4.0)).abs() < 0.5, "expected ~-4dB at center, got {gain_at_center}");

        let gain_far_below = sine_gain_db(200.0);
        assert!(gain_far_below.abs() < 0.5, "expected ~0dB far from the band, got {gain_far_below}");
    }

    #[test]
    fn telephone_stages_pass_midband_and_cut_extremes() {
        let sample_rate = 48_000.0;
        let stages = telephone_stages(sample_rate);

        let response_db = |freq: f32| -> f32 {
            let mut samples = vec![0.0f32; 8192];
            for (i, s) in samples.iter_mut().enumerate() {
                *s = (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin();
            }
            let in_energy: f32 = samples[samples.len() / 2..].iter().map(|s| s * s).sum();
            apply_cascade(&stages, &mut samples, 1);
            let out_energy: f32 = samples[samples.len() / 2..].iter().map(|s| s * s).sum();
            10.0 * (out_energy / in_energy).log10()
        };

        assert!(response_db(1000.0) > -1.0, "midband should pass essentially untouched");
        assert!(response_db(80.0) < -20.0, "well below the band should be heavily cut");
        assert!(response_db(9000.0) < -20.0, "well above the band should be heavily cut");
    }

    #[test]
    fn filter_curve_handles_near_duplicate_corner_points_without_blowing_up() {
        // AUTOTUNE_FILTER_CURVE encodes several sharp corners as pairs of
        // control points a hair's breadth apart in frequency — a plain
        // (non-monotone) cubic spline responds to that near-vertical
        // segment with huge ringing in every other segment, producing
        // dB values in the thousands that overflow to inf/NaN once
        // converted to a linear magnitude. Guard against that regressing.
        let curve = FilterCurve::new(&AUTOTUNE_FILTER_CURVE);
        let min_db = AUTOTUNE_FILTER_CURVE.iter().map(|p| p.db).fold(f32::MAX, f32::min);
        let max_db = AUTOTUNE_FILTER_CURVE.iter().map(|p| p.db).fold(f32::MIN, f32::max);
        let mut hz = 20.0f32;
        while hz < 20_000.0 {
            let db = curve.db_at(hz);
            assert!(db.is_finite(), "db_at({hz}) should be finite, got {db}");
            assert!(
                db >= min_db - 1.0 && db <= max_db + 1.0,
                "db_at({hz}) = {db} overshoots the curve's own range [{min_db}, {max_db}]"
            );
            hz *= 1.01;
        }
    }

    #[test]
    fn filter_curve_fir_matches_flat_boost_everywhere() {
        let sample_rate = 48_000.0;
        let points = [CurvePoint { hz: 100.0, db: 6.0 }, CurvePoint { hz: 10_000.0, db: 6.0 }];
        let kernel = design_filter_curve_fir(&points, 511, sample_rate);

        let mut samples = vec![0.0f32; 4096];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sample_rate).sin();
        }
        let in_peak = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        convolve_fir(&kernel, &mut samples, 1);
        let out_peak =
            samples[samples.len() / 4..3 * samples.len() / 4].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

        let gain_db = 20.0 * (out_peak / in_peak).log10();
        assert!((gain_db - 6.0).abs() < 1.0, "expected ~+6dB flat boost, got {gain_db}");
    }
}
