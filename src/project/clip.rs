use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClipId(pub u32);

/// A clip places a window of `source` audio on the timeline at
/// `start_sample`. The window (`source_offset`, `length_samples`) is
/// normally the whole buffer, but dragging a clip's edge (Audacity-style
/// trim) narrows it non-destructively — the trimmed-away audio stays in
/// `source` and can be dragged back out, up to the original recording's
/// bounds. `source` is an `Arc` so cut/copy/paste/duplicate stay cheap
/// (no sample data is copied) and so a trim on one duplicate doesn't
/// affect another (mutating it always goes through `Arc::make_mut`,
/// copy-on-write).
///
/// `source_offset`/`length_samples`/`start_sample` are all frame counts
/// (one frame = one sample per channel); `source` itself is interleaved
/// (frame-major: `[L0, R0, L1, R1, ...]` for a stereo clip, or just
/// `[s0, s1, ...]` for mono).
#[derive(Clone)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    pub start_sample: u64,
    channels: u8,
    source: Arc<[f32]>,
    source_offset: u64,
    length_samples: u64,
}

impl Clip {
    /// Builds a mono clip whose entire (untrimmed) audio is `samples`.
    pub fn from_samples(id: ClipId, name: String, start_sample: u64, samples: Vec<f32>) -> Self {
        Self::from_samples_channels(id, name, start_sample, samples, 1)
    }

    /// Builds a clip whose entire (untrimmed) audio is `samples`,
    /// interleaved across `channels` channels (1 = mono, 2 = stereo).
    pub fn from_samples_channels(
        id: ClipId,
        name: String,
        start_sample: u64,
        samples: Vec<f32>,
        channels: u8,
    ) -> Self {
        let channels = channels.max(1);
        let length_samples = samples.len() as u64 / channels as u64;
        Clip {
            id,
            name,
            start_sample,
            channels,
            source: Arc::from(samples),
            source_offset: 0,
            length_samples,
        }
    }

    pub fn channels(&self) -> u8 {
        self.channels
    }

    pub fn len_samples(&self) -> u64 {
        self.length_samples
    }

    pub fn end_sample(&self) -> u64 {
        self.start_sample + self.length_samples
    }

    /// This clip's contribution on `channel` at absolute project sample
    /// (frame) index `n`, or `None` if `n` falls outside the clip's
    /// (post-trim) span. `channel` is clamped to the clip's actual channel
    /// count, so requesting channel 1 on a mono clip harmlessly returns
    /// its only channel.
    pub fn channel_sample_at(&self, n: u64, channel: u8) -> Option<f32> {
        if n < self.start_sample || n >= self.end_sample() {
            return None;
        }
        let frame = self.source_offset + (n - self.start_sample);
        let ch = (channel as u64).min(self.channels as u64 - 1);
        let idx = frame * self.channels as u64 + ch;
        self.source.get(idx as usize).copied()
    }

    /// Mono convenience — channel 0 (a mono clip's only channel). Used by
    /// mono-track mixing/effects, which never touch `channel_sample_at`
    /// directly.
    pub fn sample_at(&self, n: u64) -> Option<f32> {
        self.channel_sample_at(n, 0)
    }

    /// The clip's currently audible sample window (i.e. post-trim) as a
    /// plain interleaved slice — used for waveform drawing and for
    /// "baking" a clip's current state into a standalone buffer (save-to-
    /// disk, split, effects). Baking discards any hidden trimmed-away
    /// audio, which is an accepted simplification: those operations start
    /// a fresh edit history for the resulting clip(s).
    pub fn visible_samples(&self) -> &[f32] {
        let channels = self.channels as usize;
        let start = (self.source_offset as usize * channels).min(self.source.len());
        let end = (start + self.length_samples as usize * channels).min(self.source.len());
        &self.source[start..end]
    }

    /// How many samples of hidden (trimmed-away) audio are available
    /// before the current window — how far the left edge can drag back out.
    /// Exposed for symmetry with `trimmable_after` (e.g. a future "can
    /// still extend this far" UI indicator); not read anywhere yet.
    #[allow(dead_code)]
    pub fn trimmable_before(&self) -> u64 {
        self.source_offset
    }

    /// How many samples of hidden (trimmed-away) audio are available
    /// after the current window — how far the right edge can drag back out.
    pub fn trimmable_after(&self) -> u64 {
        (self.source.len() as u64 / self.channels as u64) - self.source_offset - self.length_samples
    }

    /// Moves the clip's left edge by `delta_samples` (positive shortens the
    /// clip by moving its start later; negative extends it back out, up to
    /// the hidden audio available before the window and the timeline's
    /// start). Keeps at least 1 sample visible.
    pub fn trim_start(&mut self, delta_samples: i64) {
        let max_extend = (-(self.source_offset as i64)).max(-(self.start_sample as i64));
        let max_trim = self.length_samples as i64 - 1;
        let delta = delta_samples.clamp(max_extend, max_trim);
        self.source_offset = (self.source_offset as i64 + delta) as u64;
        self.length_samples = (self.length_samples as i64 - delta) as u64;
        self.start_sample = (self.start_sample as i64 + delta) as u64;
    }

    /// Moves the clip's right edge by `delta_samples` (positive shortens
    /// the clip; negative extends it back out, up to the hidden audio
    /// available after the window). Keeps at least 1 sample visible.
    pub fn trim_end(&mut self, delta_samples: i64) {
        let max_extend = -(self.trimmable_after() as i64);
        let max_trim = self.length_samples as i64 - 1;
        let delta = delta_samples.clamp(max_extend, max_trim);
        self.length_samples = (self.length_samples as i64 - delta) as u64;
    }
}
