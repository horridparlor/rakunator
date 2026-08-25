use std::sync::atomic::{AtomicU32, Ordering};

/// Fixed ceiling on track count for the meter array, sidestepping the need
/// to resize a lock-free structure while tracks are added/removed. The GUI
/// only ever reads indices `0..project.tracks.len()`.
pub const MAX_TRACKS: usize = 64;

/// Lock-free per-track peak level meters (left/right), written by the
/// producer thread every mixed chunk and read by the GUI every frame with
/// no locking at all.
pub struct Meters {
    peak_l: [AtomicU32; MAX_TRACKS],
    peak_r: [AtomicU32; MAX_TRACKS],
}

impl Meters {
    pub fn new() -> Self {
        Meters {
            peak_l: [(); MAX_TRACKS].map(|_| AtomicU32::new(0)),
            peak_r: [(); MAX_TRACKS].map(|_| AtomicU32::new(0)),
        }
    }

    /// Updates track `i`'s meter with this chunk's peak absolute level,
    /// decaying the previous value so the meter animates smoothly instead
    /// of flickering between chunks.
    pub fn update(&self, i: usize, peak_l: f32, peak_r: f32) {
        if i >= MAX_TRACKS {
            return;
        }
        let decayed_l = (f32::from_bits(self.peak_l[i].load(Ordering::Relaxed)) * 0.8).max(peak_l);
        let decayed_r = (f32::from_bits(self.peak_r[i].load(Ordering::Relaxed)) * 0.8).max(peak_r);
        self.peak_l[i].store(decayed_l.to_bits(), Ordering::Relaxed);
        self.peak_r[i].store(decayed_r.to_bits(), Ordering::Relaxed);
    }

    pub fn reset(&self, i: usize) {
        if i >= MAX_TRACKS {
            return;
        }
        self.peak_l[i].store(0, Ordering::Relaxed);
        self.peak_r[i].store(0, Ordering::Relaxed);
    }

    /// Resets every track's meter — for pausing/stopping playback, since
    /// the producer thread that normally decays them towards zero (see
    /// `update`) idles while stopped, otherwise leaving every meter frozen
    /// at whatever it last read while playing.
    pub fn reset_all(&self) {
        for i in 0..MAX_TRACKS {
            self.reset(i);
        }
    }

    pub fn read(&self, i: usize) -> (f32, f32) {
        if i >= MAX_TRACKS {
            return (0.0, 0.0);
        }
        (
            f32::from_bits(self.peak_l[i].load(Ordering::Relaxed)),
            f32::from_bits(self.peak_r[i].load(Ordering::Relaxed)),
        )
    }
}

impl Default for Meters {
    fn default() -> Self {
        Self::new()
    }
}
