pub mod clip;
pub mod generate;
pub mod track;

pub use clip::{Clip, ClipId};
pub use track::{Track, TrackId};

use std::sync::Arc;

/// The whole editable project. Shared between the GUI thread and the audio
/// engine's mixer thread behind `Arc<Mutex<Project>>` — see
/// `audio_engine::AudioEngine`.
#[derive(Clone)]
pub struct Project {
    pub sample_rate_hz: u32,
    pub tracks: Vec<Track>,
    next_track_id: u32,
    next_clip_id: u32,
    pub selection: Option<ClipId>,
    pub clipboard: Option<Clip>,
}

impl Project {
    pub fn new(sample_rate_hz: u32) -> Self {
        let mut project = Project {
            sample_rate_hz,
            tracks: Vec::new(),
            next_track_id: 0,
            next_clip_id: 0,
            selection: None,
            clipboard: None,
        };
        project.add_track();
        project
    }

    pub fn add_track(&mut self) -> TrackId {
        let id = TrackId(self.next_track_id);
        self.next_track_id += 1;
        let name = format!("Track {}", self.tracks.len() + 1);
        self.tracks.push(Track::new(id, name));
        id
    }

    pub fn remove_track(&mut self, id: TrackId) {
        self.tracks.retain(|t| t.id != id);
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    fn alloc_clip_id(&mut self) -> ClipId {
        let id = ClipId(self.next_clip_id);
        self.next_clip_id += 1;
        id
    }

    /// Finds which track currently owns `clip_id`, if any.
    pub fn find_clip_track(&self, clip_id: ClipId) -> Option<TrackId> {
        self.tracks
            .iter()
            .find(|t| t.clips.iter().any(|c| c.id == clip_id))
            .map(|t| t.id)
    }

    /// Adds a new clip built from raw samples (e.g. from the "Create Wave"
    /// dialog) onto `track_id` at `start_sample`.
    pub fn add_clip(
        &mut self,
        track_id: TrackId,
        name: String,
        start_sample: u64,
        samples: Vec<f32>,
    ) -> Option<ClipId> {
        let id = self.alloc_clip_id();
        let clip = Clip {
            id,
            name,
            start_sample,
            samples: Arc::from(samples),
        };
        let track = self.track_mut(track_id)?;
        track.clips.push(clip);
        Some(id)
    }

    /// Moves an existing clip to a new position, possibly on a different
    /// track. Used by the timeline's clip-drag interaction.
    pub fn move_clip(&mut self, clip_id: ClipId, target_track: TrackId, new_start: u64) {
        let Some(origin_track) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(origin) = self.track_mut(origin_track) else {
            return;
        };
        let Some(pos) = origin.clips.iter().position(|c| c.id == clip_id) else {
            return;
        };
        let mut clip = origin.clips.remove(pos);
        clip.start_sample = new_start;
        if let Some(target) = self.track_mut(target_track) {
            target.clips.push(clip);
        } else if let Some(origin) = self.track_mut(origin_track) {
            // Target vanished (e.g. removed mid-drag) — put it back.
            origin.clips.push(clip);
        }
    }

    /// Removes the selected clip and stashes a copy in the clipboard.
    pub fn cut_clip(&mut self, clip_id: ClipId) {
        let Some(origin_track) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(origin) = self.track_mut(origin_track) else {
            return;
        };
        let Some(pos) = origin.clips.iter().position(|c| c.id == clip_id) else {
            return;
        };
        self.clipboard = Some(origin.clips.remove(pos));
    }

    /// Copies the selected clip into the clipboard, leaving it in place.
    pub fn copy_clip(&mut self, clip_id: ClipId) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(track) = self.track(track_id) else {
            return;
        };
        if let Some(clip) = track.clips.iter().find(|c| c.id == clip_id) {
            self.clipboard = Some(clip.clone());
        }
    }

    /// Pastes the clipboard's clip onto `target_track` at `start_sample`.
    pub fn paste(&mut self, target_track: TrackId, start_sample: u64) -> Option<ClipId> {
        let mut clip = self.clipboard.clone()?;
        clip.id = self.alloc_clip_id();
        clip.start_sample = start_sample;
        let track = self.track_mut(target_track)?;
        let id = clip.id;
        track.clips.push(clip);
        Some(id)
    }

    /// Duplicates a clip onto `target_track` at `start_sample`, without
    /// touching the clipboard. Used by explicit Duplicate actions and by
    /// Ctrl/Cmd-drag-to-copy in the timeline.
    pub fn duplicate_clip(
        &mut self,
        clip_id: ClipId,
        target_track: TrackId,
        start_sample: u64,
    ) -> Option<ClipId> {
        let origin_track = self.find_clip_track(clip_id)?;
        let mut clip = self.track(origin_track)?.clips.iter().find(|c| c.id == clip_id)?.clone();
        clip.id = self.alloc_clip_id();
        clip.start_sample = start_sample;
        let track = self.track_mut(target_track)?;
        let id = clip.id;
        track.clips.push(clip);
        Some(id)
    }
}
