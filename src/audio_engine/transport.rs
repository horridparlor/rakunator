use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

const STOPPED: u8 = 0;
const PLAYING: u8 = 1;
const PAUSED: u8 = 2;

/// Playback transport state shared between the GUI thread and the
/// producer/mixer thread. Because clips are pre-rendered sample buffers,
/// mixing at any sample index is a pure, stateless lookup, so Play/Pause/
/// Stop/Seek are just atomic stores — never blocking, safe to call
/// directly from the GUI's per-frame update.
pub struct Transport {
    state: AtomicU8,
    position_samples: AtomicU64,
}

impl Transport {
    pub fn new() -> Self {
        Transport {
            state: AtomicU8::new(STOPPED),
            position_samples: AtomicU64::new(0),
        }
    }

    pub fn play(&self) {
        self.state.store(PLAYING, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.state.store(PAUSED, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.state.store(STOPPED, Ordering::Relaxed);
        self.position_samples.store(0, Ordering::Relaxed);
    }

    pub fn seek(&self, sample_pos: u64) {
        self.position_samples.store(sample_pos, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.state.load(Ordering::Relaxed) == PLAYING
    }

    pub fn position(&self) -> u64 {
        self.position_samples.load(Ordering::Relaxed)
    }

    pub(super) fn advance(&self, frames: u64) {
        self.position_samples.fetch_add(frames, Ordering::Relaxed);
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self::new()
    }
}
