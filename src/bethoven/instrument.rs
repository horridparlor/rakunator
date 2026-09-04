//! Procedural synthesis for Bethoven's 13 instruments. There are no sample
//! assets anywhere in this project, so every instrument is built from
//! oscillators (in the same spirit as `crate::waveform::Waveform`), a
//! shared ADSR envelope, and — for the percussion voices — simple one-pole
//! filtered noise. Each note is rendered up front into a plain mono
//! `Vec<f32>`, matching `project::generate::render_waveform_clip`: nothing
//! here runs inside the realtime mixer.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Instrument {
    Piano,
    Strings,
    Bass,
    Guitar,
    Synth,
    Drum,
    Snare,
    HiHat,
    /// A bright, fast plucky lead, pitched an octave above the row it's
    /// placed on — good for quick high runs.
    Lead,
    /// A classic unpitched analog-cowbell hit (two clanging square-wave
    /// tones), ignoring the note's row like the other percussion voices.
    Cowbell,
    /// A modern tuned 808-style sub kick: a punchy pitch-drop "thump" that
    /// settles on the row's note, for trap/hip-hop style basslines.
    Bass808,
    /// A pure, smooth two-octaves-down sine — the deepest, simplest voice,
    /// for sustained sub pads rather than 808's percussive punch.
    Sub,
    /// A bowed-string voice built to approximate a real violin as closely as
    /// procedural synthesis (no samples) allows: rich sawtooth-based
    /// harmonics, a slow bow-driven attack, vibrato that eases in as the
    /// note settles, a touch of tremolo, and constant faint bow noise.
    Violin,
}

impl Instrument {
    pub const ALL: [Instrument; 13] = [
        Instrument::Piano,
        Instrument::Strings,
        Instrument::Violin,
        Instrument::Bass,
        Instrument::Guitar,
        Instrument::Synth,
        Instrument::Lead,
        Instrument::Sub,
        Instrument::Bass808,
        Instrument::Drum,
        Instrument::Snare,
        Instrument::HiHat,
        Instrument::Cowbell,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Instrument::Piano => "Piano",
            Instrument::Strings => "Strings",
            Instrument::Bass => "Bass",
            Instrument::Guitar => "Guitar",
            Instrument::Synth => "Synth",
            Instrument::Drum => "Drum",
            Instrument::Snare => "Snare",
            Instrument::HiHat => "Hi-Hat",
            Instrument::Lead => "Lead",
            Instrument::Cowbell => "Cowbell",
            Instrument::Bass808 => "808",
            Instrument::Sub => "Sub",
            Instrument::Violin => "Violin",
        }
    }

    /// Whether this instrument is pitched by the piano roll row it's placed
    /// on. Drum/Snare/HiHat/Cowbell are conventional unpitched one-shot
    /// hits (like a real drum machine); 808 is pitched despite being
    /// percussive, matching how it's actually used (a tuned sub kick).
    pub fn is_pitched(self) -> bool {
        !matches!(self, Instrument::Drum | Instrument::Snare | Instrument::HiHat | Instrument::Cowbell)
    }
}

/// A standard attack/decay/sustain/release envelope, expressed in seconds
/// (`sustain_level` is the held gain, 0..=1). `gain_at` prioritizes the
/// release tail over attack/decay so a note shorter than attack+decay+release
/// still fades out cleanly instead of clipping to silence.
struct Envelope {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

impl Envelope {
    fn gain_at(&self, t: f32, duration: f32) -> f32 {
        let release_start = (duration - self.release).max(0.0);
        if t >= release_start {
            let rt = ((t - release_start) / self.release.max(1e-6)).min(1.0);
            (self.sustain * (1.0 - rt)).max(0.0)
        } else if t < self.attack {
            if self.attack <= 0.0 {
                1.0
            } else {
                t / self.attack
            }
        } else if t < self.attack + self.decay {
            let dt = (t - self.attack) / self.decay.max(1e-6);
            1.0 + (self.sustain - 1.0) * dt
        } else {
            self.sustain
        }
    }
}

/// Converts a MIDI-style note number (69 = A4 = 440 Hz) to frequency.
fn midi_to_freq(midi_note: u8) -> f32 {
    440.0 * 2f32.powf((midi_note as f32 - 69.0) / 12.0)
}

/// A tiny deterministic PRNG (xorshift32) — good enough for noise-based
/// percussion, and deterministic per-note so tests are reproducible without
/// pulling in a `rand` dependency.
struct Xorshift32(u32);

impl Xorshift32 {
    fn new(seed: u32) -> Self {
        Xorshift32(seed.max(1))
    }

    /// Next value in [-1.0, 1.0].
    fn next_signed(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn white_noise(len: usize, seed: u32) -> Vec<f32> {
    let mut rng = Xorshift32::new(seed);
    (0..len).map(|_| rng.next_signed()).collect()
}

/// One-pole lowpass, in place conceptually but returns a new buffer.
fn one_pole_lowpass(input: &[f32], sample_rate_hz: f32, cutoff_hz: f32) -> Vec<f32> {
    let rc = 1.0 / (2.0 * PI * cutoff_hz);
    let dt = 1.0 / sample_rate_hz;
    let alpha = dt / (rc + dt);
    let mut y = 0.0f32;
    input
        .iter()
        .map(|&x| {
            y += alpha * (x - y);
            y
        })
        .collect()
}

/// One-pole highpass (complement of the lowpass above).
fn one_pole_highpass(input: &[f32], sample_rate_hz: f32, cutoff_hz: f32) -> Vec<f32> {
    let rc = 1.0 / (2.0 * PI * cutoff_hz);
    let dt = 1.0 / sample_rate_hz;
    let alpha = rc / (rc + dt);
    let mut y = 0.0f32;
    let mut prev_x = 0.0f32;
    input
        .iter()
        .map(|&x| {
            y = alpha * (y + x - prev_x);
            prev_x = x;
            y
        })
        .collect()
}

fn sine(phase: f32) -> f32 {
    (phase * 2.0 * PI).sin()
}

fn sawtooth(phase: f32) -> f32 {
    2.0 * (phase - phase.floor()) - 1.0
}

fn square(phase: f32) -> f32 {
    if phase.fract() < 0.5 {
        1.0
    } else {
        -1.0
    }
}

fn triangle(phase: f32) -> f32 {
    let p = phase.fract();
    if p < 0.5 {
        4.0 * p - 1.0
    } else {
        3.0 - 4.0 * p
    }
}

/// Renders one note of `instrument` at `midi_note` for `duration`, scaled
/// by `gain` (0.0..=1.0), as a mono buffer at `sample_rate_hz`. Buffer
/// length always exactly matches `duration` — for the percussion voices,
/// whose natural decay is shorter than most note-box lengths, the tail of
/// the buffer is simply near-silent rather than truncated.
pub fn render_note(instrument: Instrument, midi_note: u8, duration: Duration, gain: f32, sample_rate_hz: u32) -> Vec<f32> {
    let sr = sample_rate_hz as f32;
    let dur_s = duration.as_secs_f32();
    let n = (dur_s * sr).round().max(1.0) as usize;
    let gain = gain.clamp(0.0, 1.0);
    if gain <= 0.0 || n == 0 {
        return vec![0.0; n];
    }

    let freq = midi_to_freq(midi_note);
    let mut out = match instrument {
        Instrument::Piano => render_piano(freq, n, sr),
        Instrument::Strings => render_strings(freq, n, sr),
        Instrument::Violin => render_violin(freq, n, sr),
        Instrument::Bass => render_bass(freq, n, sr),
        Instrument::Guitar => render_guitar(freq, n, sr),
        Instrument::Synth => render_synth(freq, n, sr),
        Instrument::Lead => render_lead(freq, n, sr),
        Instrument::Sub => render_sub(freq, n, sr),
        Instrument::Bass808 => render_808(freq, n, sr),
        Instrument::Drum => render_drum(n, sr),
        Instrument::Snare => render_snare(n, sr, midi_note),
        Instrument::HiHat => render_hihat(n, sr, midi_note),
        Instrument::Cowbell => render_cowbell(n, sr),
    };
    for s in out.iter_mut() {
        *s = (*s * gain).clamp(-1.0, 1.0);
    }
    out
}

fn render_piano(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.004, decay: 0.3, sustain: 0.15, release: 0.2 };
    let harmonics = [(1.0, 1.0), (2.0, 0.5), (3.0, 0.25), (4.0, 0.125)];
    let norm = harmonics.iter().map(|(_, a)| a).sum::<f32>();
    let mut phases = [0.0f32; 4];
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let mut s = 0.0;
            for (h, (mult, amp)) in phases.iter_mut().zip(harmonics.iter()) {
                *h = (*h + mult * freq / sr) % 1.0;
                s += sine(*h) * amp;
            }
            (s / norm) * env.gain_at(t, dur_s)
        })
        .collect()
}

fn render_strings(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.15, decay: 0.1, sustain: 0.85, release: 0.3 };
    let mut p1 = 0.0f32;
    let mut p2 = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            p1 = (p1 + freq * 0.995 / sr) % 1.0;
            p2 = (p2 + freq * 1.005 / sr) % 1.0;
            let s = (sawtooth(p1) + sawtooth(p2)) * 0.5;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

/// A bowed violin, approximated within the limits of pure synthesis (no
/// samples, no physical bow-string model): a detuned sawtooth pair gives
/// the rich, roughly-Helmholtz harmonic spectrum a bowed string actually
/// produces; a slow attack matches how a bow takes a moment to set the
/// string singing; vibrato eases in over the first ~150ms the way a
/// player's hand naturally does once a note settles, rather than being on
/// from the start; a slight tremolo and constant faint bow-noise floor add
/// the small imperfections that make a bowed tone sound alive instead of a
/// static, "dead" waveform; and the spectrum is shaped (low end trimmed,
/// top end tamed) to sit in a violin's own register instead of a raw
/// sawtooth's harsh full-range buzz.
fn render_violin(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.09, decay: 0.06, sustain: 0.82, release: 0.22 };
    let mut p1 = 0.0f32;
    let mut p2 = 0.0f32;
    let mut vib_phase = 0.0f32;
    let mut trem_phase = 0.0f32;
    let dur_s = n as f32 / sr;
    let noise = white_noise(n, 0x0B10_1157 ^ (freq as u32));
    let raw: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let vib_amount = (t / 0.15).min(1.0) * 0.006;
            vib_phase = (vib_phase + 5.4 / sr) % 1.0;
            let vibrato = 1.0 + sine(vib_phase) * vib_amount;
            trem_phase = (trem_phase + 4.0 / sr) % 1.0;
            let tremolo = 1.0 + sine(trem_phase) * 0.04;
            let f = freq * vibrato;
            p1 = (p1 + f / sr) % 1.0;
            p2 = (p2 + f * 1.004 / sr) % 1.0;
            let tone = (sawtooth(p1) + sawtooth(p2)) * 0.5;
            tone * tremolo + noise[i] * 0.03
        })
        .collect();
    let shaped = one_pole_lowpass(&one_pole_highpass(&raw, sr, 180.0), sr, 6500.0);
    shaped
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let t = i as f32 / sr;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

fn render_bass(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.01, decay: 0.15, sustain: 0.7, release: 0.15 };
    let low_freq = freq / 2.0;
    let mut p = 0.0f32;
    let mut pt = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            p = (p + low_freq / sr) % 1.0;
            pt = (pt + low_freq / sr) % 1.0;
            let s = sine(p) * 0.7 + triangle(pt) * 0.3;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

fn render_guitar(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.003, decay: 0.25, sustain: 0.05, release: 0.15 };
    let mut p1 = 0.0f32;
    let mut p2 = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            p1 = (p1 + freq * 0.995 / sr) % 1.0;
            p2 = (p2 + freq * 1.005 / sr) % 1.0;
            let s = (sawtooth(p1) + sawtooth(p2)) * 0.5;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

/// A warm "supersaw" pad: three detuned sawtooths (a touch of chorus-like
/// shimmer instead of one bare oscillator) run through the lowpass twice
/// (a steeper, smoother rolloff than a single stage) to tame the raw
/// sawtooth's harsh upper harmonics/aliasing — much gentler than the old
/// naive square+saw blend.
fn render_synth(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.03, decay: 0.15, sustain: 0.7, release: 0.25 };
    let detunes = [-0.006, 0.0, 0.006];
    let mut phases = [0.0f32; 3];
    let raw: Vec<f32> = (0..n)
        .map(|_| {
            let mut s = 0.0;
            for (p, d) in phases.iter_mut().zip(detunes.iter()) {
                *p = (*p + freq * (1.0 + d) / sr) % 1.0;
                s += sawtooth(*p);
            }
            s / 3.0
        })
        .collect();
    let filtered = one_pole_lowpass(&one_pole_lowpass(&raw, sr, 2200.0), sr, 2200.0);
    let dur_s = n as f32 / sr;
    filtered
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let t = i as f32 / sr;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

/// A bright, fast pluck an octave above the row it's on, with a slight
/// detune between its square/saw layers for a bit of shimmer — meant for
/// quick high runs rather than sustained chords.
fn render_lead(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.002, decay: 0.12, sustain: 0.35, release: 0.08 };
    let f = freq * 2.0;
    let mut p1 = 0.0f32;
    let mut p2 = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            p1 = (p1 + f / sr) % 1.0;
            p2 = (p2 + f * 1.01 / sr) % 1.0;
            let s = square(p1) * 0.5 + sawtooth(p2) * 0.5;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

/// A pure, smooth sine two octaves below the row it's on — the deepest,
/// simplest voice: a sustained sub pad rather than a percussive hit.
fn render_sub(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.05, decay: 0.1, sustain: 0.9, release: 0.3 };
    let f = freq / 4.0;
    let mut phase = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            phase = (phase + f / sr) % 1.0;
            sine(phase) * env.gain_at(t, dur_s)
        })
        .collect()
}

/// A modern tuned "808" sub kick: like `render_drum`'s punchy pitch-drop
/// thump, but settling on the row's own note (one octave down) instead of a
/// fixed unpitched frequency, and with a longer decay-only tail — for
/// trap/hip-hop style basslines you actually play a melody with.
fn render_808(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.002, decay: 0.5, sustain: 0.0, release: 0.2 };
    let target = freq / 2.0;
    let pitch_env_s = 0.1;
    let mut phase = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let drop = (t / pitch_env_s).min(1.0);
            let f = target * 3.0 + (target - target * 3.0) * drop;
            phase = (phase + f / sr) % 1.0;
            sine(phase) * env.gain_at(t, dur_s)
        })
        .collect()
}

/// A classic analog cowbell: two non-harmonically-related square tones
/// (~587Hz/~845Hz, the same ratio real cowbell emulations use for that
/// metallic clang) with the low end trimmed by a highpass. Pitch is
/// ignored, like the other percussion voices.
fn render_cowbell(n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.001, decay: 0.28, sustain: 0.0, release: 0.05 };
    let mut p1 = 0.0f32;
    let mut p2 = 0.0f32;
    let raw: Vec<f32> = (0..n)
        .map(|_| {
            p1 = (p1 + 587.0 / sr) % 1.0;
            p2 = (p2 + 845.0 / sr) % 1.0;
            square(p1) * 0.5 + square(p2) * 0.5
        })
        .collect();
    let filtered = one_pole_highpass(&raw, sr, 400.0);
    let dur_s = n as f32 / sr;
    filtered
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let t = i as f32 / sr;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

/// Kick drum: a sine oscillator whose own frequency drops from ~150Hz to
/// ~50Hz over the first ~80ms ("pitch envelope"), shaped by a fast
/// decay-only amplitude envelope. Pitch (the note's row) is ignored, like a
/// real drum machine's one-shot kick sample.
fn render_drum(n: usize, sr: f32) -> Vec<f32> {
    let env = Envelope { attack: 0.001, decay: 0.18, sustain: 0.0, release: 0.05 };
    let pitch_env_s = 0.08;
    let mut phase = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let drop = (t / pitch_env_s).min(1.0);
            let freq = 150.0 + (50.0 - 150.0) * drop;
            phase = (phase + freq / sr) % 1.0;
            sine(phase) * env.gain_at(t, dur_s)
        })
        .collect()
}

/// Snare: a low tonal component plus lowpass-filtered noise, short
/// decay-only envelope. Pitch is ignored (one-shot hit); `seed_note` only
/// varies the noise seed so back-to-back snare hits at different rows still
/// sound like independent hits rather than identical samples.
fn render_snare(n: usize, sr: f32, seed_note: u8) -> Vec<f32> {
    let env = Envelope { attack: 0.001, decay: 0.12, sustain: 0.0, release: 0.05 };
    let noise = one_pole_lowpass(&white_noise(n, 0x5EED_0000 ^ seed_note as u32), sr, 4000.0);
    let mut p = 0.0f32;
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            p = (p + 180.0 / sr) % 1.0;
            let s = triangle(p) * 0.4 + noise[i] * 0.6;
            s * env.gain_at(t, dur_s)
        })
        .collect()
}

/// Hi-hat: highpass-filtered noise with a very short decay-only envelope.
/// Pitch is ignored (one-shot hit).
fn render_hihat(n: usize, sr: f32, seed_note: u8) -> Vec<f32> {
    let env = Envelope { attack: 0.001, decay: 0.05, sustain: 0.0, release: 0.02 };
    let noise = one_pole_highpass(&white_noise(n, 0x4A17_0000 ^ seed_note as u32), sr, 7000.0);
    let dur_s = n as f32 / sr;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            noise[i] * env.gain_at(t, dur_s)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_renders_correct_length_and_range() {
        let sr = 48_000u32;
        let dur = Duration::from_millis(500);
        let expected_len = (0.5 * sr as f32).round() as usize;
        for inst in Instrument::ALL {
            let buf = render_note(inst, 60, dur, 1.0, sr);
            assert_eq!(buf.len(), expected_len, "{:?} length", inst);
            assert!(buf.iter().all(|s| s.is_finite() && (-1.0..=1.0).contains(s)), "{:?} range", inst);
            assert!(buf.iter().any(|&s| s != 0.0), "{:?} produced only silence", inst);
        }
    }

    #[test]
    fn zero_gain_is_silent() {
        let buf = render_note(Instrument::Piano, 60, Duration::from_millis(200), 0.0, 48_000);
        assert!(buf.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn percussion_instruments_are_not_pitched() {
        assert!(!Instrument::Drum.is_pitched());
        assert!(!Instrument::Snare.is_pitched());
        assert!(!Instrument::HiHat.is_pitched());
        assert!(!Instrument::Cowbell.is_pitched());
        assert!(Instrument::Piano.is_pitched());
        assert!(Instrument::Lead.is_pitched());
        assert!(Instrument::Sub.is_pitched());
        assert!(Instrument::Bass808.is_pitched());
        assert!(Instrument::Violin.is_pitched());
    }

    #[test]
    fn instrument_count_is_13() {
        assert_eq!(Instrument::ALL.len(), 13);
    }
}
