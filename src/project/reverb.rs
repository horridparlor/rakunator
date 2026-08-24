//! A Freeverb-style reverb (parallel comb filters feeding series allpass
//! filters, per channel) parameterized to match Audacity's built-in Reverb
//! effect's controls (RoomSize/Reverberance/HfDamping/ToneLow/ToneHigh/
//! WetGain/DryGain/StereoWidth/Delay/WetOnly). Audacity's own Reverb isn't
//! Freeverb internally, so this won't be byte-identical, but Freeverb is
//! the standard reference algorithm these same knobs are modeled on, and
//! it's used both by the standalone Reverb effect and inside the Autotune
//! chain.

use crate::project::eq::{self, Biquad};

pub struct ReverbParams {
    pub room_size: f32,
    pub reverberance: f32,
    pub hf_damping: f32,
    pub tone_low: f32,
    pub tone_high: f32,
    pub wet_gain_db: f32,
    pub dry_gain_db: f32,
    pub stereo_width: f32,
    pub pre_delay_ms: f32,
    pub wet_only: bool,
}

impl Default for ReverbParams {
    /// Audacity's own Reverb effect defaults.
    fn default() -> Self {
        ReverbParams {
            room_size: 75.0,
            reverberance: 50.0,
            hf_damping: 50.0,
            tone_low: 100.0,
            tone_high: 100.0,
            wet_gain_db: -1.0,
            dry_gain_db: -1.0,
            stereo_width: 100.0,
            pre_delay_ms: 10.0,
            wet_only: false,
        }
    }
}

const COMB_TUNING_SAMPLES_44100: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING_SAMPLES_44100: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD_SAMPLES_44100: f32 = 23.0;
const FIXED_INPUT_GAIN: f32 = 0.015;
const ALLPASS_FEEDBACK: f32 = 0.5;

struct Comb {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
    damp1: f32,
    damp2: f32,
    filter_store: f32,
}

impl Comb {
    fn new(delay_samples: usize, feedback: f32, damp1: f32, damp2: f32) -> Self {
        Comb { buffer: vec![0.0; delay_samples.max(1)], pos: 0, feedback, damp1, damp2, filter_store: 0.0 }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.pos];
        self.filter_store = output * self.damp2 + self.filter_store * self.damp1;
        self.buffer[self.pos] = input + self.filter_store * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }
}

struct AllPass {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl AllPass {
    fn new(delay_samples: usize, feedback: f32) -> Self {
        AllPass { buffer: vec![0.0; delay_samples.max(1)], pos: 0, feedback }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buf_out = self.buffer[self.pos];
        let output = -input + buf_out;
        self.buffer[self.pos] = input + buf_out * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }
}

/// One channel's parallel-combs-into-series-allpasses reverb tank. `spread`
/// offsets every delay line by a fixed number of samples, which is how
/// Freeverb decorrelates the left/right channels for stereo width without
/// needing separate input signals.
struct ReverbNetwork {
    combs: Vec<Comb>,
    allpasses: Vec<AllPass>,
}

impl ReverbNetwork {
    fn new(sample_rate: f32, room_scale: f32, feedback: f32, damp1: f32, damp2: f32, spread: usize) -> Self {
        let sr_scale = sample_rate / 44_100.0;
        let combs = COMB_TUNING_SAMPLES_44100
            .iter()
            .map(|&t| Comb::new((t as f32 * sr_scale * room_scale) as usize + spread, feedback, damp1, damp2))
            .collect();
        let allpasses = ALLPASS_TUNING_SAMPLES_44100
            .iter()
            .map(|&t| AllPass::new((t as f32 * sr_scale) as usize + spread, ALLPASS_FEEDBACK))
            .collect();
        ReverbNetwork { combs, allpasses }
    }

    fn process(&mut self, input: f32) -> f32 {
        let scaled = input * FIXED_INPUT_GAIN;
        let mut out = 0.0;
        for comb in &mut self.combs {
            out += comb.process(scaled);
        }
        for allpass in &mut self.allpasses {
            out = allpass.process(out);
        }
        out
    }
}

fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Applies the reverb to `samples` (interleaved, `channels` channels) in
/// place. Both reverb tanks (left/right, when stereo) are fed the same
/// mono-mixed input, differing only in their delay-line lengths — the
/// classic Freeverb technique for stereo decorrelation.
pub fn apply(samples: &mut [f32], channels: usize, sample_rate: f32, params: &ReverbParams) {
    if channels == 0 {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }

    let room_scale = 0.6 + (params.room_size / 100.0).clamp(0.0, 1.0) * 0.8;
    let feedback = 0.7 + (params.reverberance / 100.0).clamp(0.0, 1.0) * 0.28;
    let damp1 = (params.hf_damping / 100.0).clamp(0.0, 1.0) * 0.4;
    let damp2 = 1.0 - damp1;
    let spread = (STEREO_SPREAD_SAMPLES_44100 * (sample_rate / 44_100.0)
        * (params.stereo_width / 100.0).clamp(0.0, 1.0))
    .round() as usize;
    let delay_samples = ((params.pre_delay_ms / 1000.0) * sample_rate).round().max(0.0) as i64;

    let mut network_l = ReverbNetwork::new(sample_rate, room_scale, feedback, damp1, damp2, 0);
    let stereo = channels >= 2;
    let mut network_r = stereo.then(|| ReverbNetwork::new(sample_rate, room_scale, feedback, damp1, damp2, spread));

    let mut wet_l = vec![0.0f32; frames];
    let mut wet_r = vec![0.0f32; frames];
    for frame in 0..frames {
        let src_frame = frame as i64 - delay_samples;
        let mono_in = if src_frame < 0 {
            0.0
        } else {
            let src = src_frame as usize;
            (0..channels).map(|ch| samples[src * channels + ch]).sum::<f32>() / channels as f32
        };
        wet_l[frame] = network_l.process(mono_in);
        if let Some(network_r) = network_r.as_mut() {
            wet_r[frame] = network_r.process(mono_in);
        }
    }

    let tone_high_cutoff = (1000.0 + (params.tone_high / 100.0).clamp(0.0, 1.0) * 19_000.0).min(sample_rate * 0.49);
    let tone_low_cutoff = 20.0 + (1.0 - (params.tone_low / 100.0).clamp(0.0, 1.0)) * 480.0;
    let tone_stages: [Biquad; 2] =
        [Biquad::lowpass(sample_rate, tone_high_cutoff, std::f32::consts::FRAC_1_SQRT_2),
        Biquad::highpass(sample_rate, tone_low_cutoff, std::f32::consts::FRAC_1_SQRT_2)];
    eq::apply_cascade(&tone_stages, &mut wet_l, 1);
    if stereo {
        eq::apply_cascade(&tone_stages, &mut wet_r, 1);
    }

    let dry_gain = if params.wet_only { 0.0 } else { db_to_linear(params.dry_gain_db) };
    let wet_gain = db_to_linear(params.wet_gain_db);
    for frame in 0..frames {
        for ch in 0..channels {
            let wet = if stereo && ch == 1 { wet_r[frame] } else { wet_l[frame] };
            let idx = frame * channels + ch;
            samples[idx] = (samples[idx] * dry_gain + wet * wet_gain).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wet_only_produces_no_dry_signal_at_the_very_start() {
        let sample_rate = 48_000.0;
        let mut samples = vec![0.0f32; 4096];
        samples[0] = 1.0;
        let params = ReverbParams { wet_only: true, pre_delay_ms: 0.0, ..ReverbParams::default() };
        apply(&mut samples, 1, sample_rate, &params);
        // With no pre-delay, wet-only output should still start essentially
        // at zero (the reverb tank needs time to build up), unlike a dry
        // signal which would show the impulse immediately.
        assert!(samples[0].abs() < 0.05, "expected near-silence before the tank fills, got {}", samples[0]);
    }

    #[test]
    fn produces_a_decaying_tail_after_an_impulse() {
        let sample_rate = 48_000.0;
        let mut samples = vec![0.0f32; 48_000];
        samples[0] = 1.0;
        let params = ReverbParams::default();
        apply(&mut samples, 1, sample_rate, &params);
        let has_energy_after_impulse = samples[2000..40_000].iter().any(|&s| s.abs() > 1e-4);
        assert!(has_energy_after_impulse, "expected a reverb tail well after the impulse");
    }

    #[test]
    fn stereo_width_zero_makes_channels_identical() {
        let sample_rate = 48_000.0;
        let mut samples = vec![0.0f32; 4096 * 2];
        for i in 0..4096 {
            samples[i * 2] = ((i as f32) * 0.01).sin();
            samples[i * 2 + 1] = ((i as f32) * 0.01).sin();
        }
        let params = ReverbParams { stereo_width: 0.0, ..ReverbParams::default() };
        apply(&mut samples, 2, sample_rate, &params);
        for i in 0..4096 {
            assert!((samples[i * 2] - samples[i * 2 + 1]).abs() < 1e-6, "channels should match with zero width");
        }
    }
}
