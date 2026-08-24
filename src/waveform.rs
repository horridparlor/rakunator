use std::f32::consts::PI;

/// Shared waveform shapes so playback (realtime) and export (offline
/// rendering) generate identical samples from a single source of truth.
#[derive(Clone, Copy)]
pub enum Waveform {
    Sine,
    Square,
    Triangle,
    Sawtooth,
}

impl Waveform {
    /// Maps a phase in [0, 1) to a sample in [-1, 1].
    pub fn sample(self, phase: f32) -> f32 {
        match self {
            Waveform::Sine => (phase * 2.0 * PI).sin(),
            Waveform::Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Triangle => {
                if phase < 0.5 {
                    4.0 * phase - 1.0
                } else {
                    3.0 - 4.0 * phase
                }
            }
            Waveform::Sawtooth => 2.0 * phase - 1.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Waveform::Sine => "sine",
            Waveform::Square => "square",
            Waveform::Triangle => "triangle",
            Waveform::Sawtooth => "sawtooth",
        }
    }
}