use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClipId(pub u32);

/// A clip is a pre-rendered mono audio buffer placed at `start_sample` on
/// its track's timeline. Pre-rendering (rather than keeping e.g. a
/// waveform+phase generator) keeps the mixer's per-sample lookup uniform
/// regardless of how the clip's audio was produced, and makes cut/copy/
/// paste/duplicate a cheap `Arc` clone.
#[derive(Clone)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    pub start_sample: u64,
    pub samples: Arc<[f32]>,
}

impl Clip {
    pub fn len_samples(&self) -> u64 {
        self.samples.len() as u64
    }

    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.len_samples()
    }

    /// This clip's contribution at absolute project sample index `n`, or
    /// `None` if `n` falls outside the clip's span.
    pub fn sample_at(&self, n: u64) -> Option<f32> {
        if n < self.start_sample || n >= self.end_sample() {
            return None;
        }
        Some(self.samples[(n - self.start_sample) as usize])
    }
}
