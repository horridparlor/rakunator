pub mod clip;
pub mod dynamics;
pub mod eq;
pub mod generate;
pub mod import;
pub mod metadata;
pub mod noise_reduction;
pub mod pan_toggle;
pub mod persistence;
pub mod reverb;
pub mod stretch;
pub mod track;
pub mod trip_toggler;

pub use clip::{Clip, ClipId};
pub use metadata::ProjectMetadata;
pub use pan_toggle::{PanToggleDirection, PanToggleParams};
pub use track::{Track, TrackId};

use std::collections::{HashMap, HashSet};

/// Converts a decibel level to a linear amplitude gain (0 dB = 1.0), for
/// the Effects menu's adjustable fade start/end points.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// The classic "tape speed" pitch shift: resampling `source` (interleaved,
/// `channels` channels) by a factor of `2^(semitones/12)` changes both its
/// pitch and its playback duration together. A true pitch-preserving shift
/// would need a phase vocoder or similar time-stretching algorithm (see
/// `stretch::apply_time_pitch_ramp`, used by the Tempo/Sliding
/// Stretch/Rattle effects instead).
fn resample_for_pitch(source: &[f32], channels: usize, semitones: f32) -> Vec<f32> {
    if channels == 0 || source.is_empty() {
        return Vec::new();
    }
    let frames = source.len() / channels;
    let ratio = 2f32.powf(semitones / 12.0);
    let new_frames = ((frames as f32) / ratio).round().max(1.0) as usize;
    let mut resampled = vec![0.0f32; new_frames * channels];
    // Clamped to the last valid frame rather than defaulting to 0.0 past
    // the end — otherwise the interpolation between the last real frame
    // and an out-of-bounds "phantom" 0.0 fades the tail toward silence
    // (most visible when lengthening, e.g. a pitch-down shift, where
    // `idx + 1` frequently lands one frame past the source's end).
    let frame_sample =
        |frame: usize, ch: usize| -> f32 { source[frame.min(frames.saturating_sub(1)) * channels + ch] };
    for ch in 0..channels {
        for i in 0..new_frames {
            let src_pos = i as f32 * ratio;
            let idx = src_pos.floor() as usize;
            let frac = src_pos - idx as f32;
            let a = frame_sample(idx, ch);
            let b = frame_sample(idx + 1, ch);
            resampled[i * channels + ch] = a + (b - a) * frac;
        }
    }
    resampled
}

/// Appends `variant` (interleaved, `channels` channels) onto `combined`,
/// summing the last `overlap_frames` frames already in `combined` with the
/// first `overlap_frames` frames of `variant` instead of placing them back
/// to back — see `Project::apply_rattle`, the only caller. `overlap_frames`
/// is clamped to whichever of the two buffers is shorter, so a very short
/// variant can't underflow `combined`'s length.
fn append_with_overlap_add(combined: &mut Vec<f32>, variant: &[f32], channels: usize, overlap_frames: usize) {
    if combined.is_empty() || channels == 0 {
        combined.extend_from_slice(variant);
        return;
    }
    let combined_frames = combined.len() / channels;
    let variant_frames = variant.len() / channels;
    let overlap = overlap_frames.min(combined_frames).min(variant_frames);
    let overlap_start = combined.len() - overlap * channels;
    for (dst, src) in combined[overlap_start..].iter_mut().zip(&variant[..overlap * channels]) {
        *dst += src;
    }
    combined.extend_from_slice(&variant[overlap * channels..]);
}

/// Converts interleaved `samples` (`from` channels) to `to` channels: mono
/// is duplicated to both channels when going 1 -> 2, and averaged down to
/// one channel when going 2 -> 1. A no-op when `from == to`.
fn convert_channel_count(samples: Vec<f32>, from: u8, to: u8) -> Vec<f32> {
    if from == to {
        return samples;
    }
    match (from, to) {
        (1, 2) => samples.into_iter().flat_map(|s| [s, s]).collect(),
        (2, 1) => samples.as_chunks::<2>().0.iter().map(|f| (f[0] + f[1]) / 2.0).collect(),
        _ => samples,
    }
}

/// One clipboard entry, positioned relative to the copied group's anchor
/// (its earliest clip's track index and start sample) so a multi-clip
/// paste preserves the clips' relative layout.
#[derive(Clone)]
struct ClipboardEntry {
    track_offset: i64,
    start_offset: i64,
    clip: Clip,
}

/// The persistent content an undo/redo step restores — deliberately just
/// the track/clip content, not ephemeral state like selection or the
/// clipboard.
#[derive(Clone)]
struct ProjectSnapshot {
    sample_rate_hz: u32,
    tracks: Vec<Track>,
    next_track_id: u32,
    next_clip_id: u32,
}

/// Undo history is capped so it can't grow without bound over a long
/// editing session.
const MAX_UNDO_HISTORY: usize = 50;

/// The whole editable project. Shared between the GUI thread and the audio
/// engine's mixer thread behind `Arc<Mutex<Project>>` — see
/// `audio_engine::AudioEngine`.
pub struct Project {
    pub sample_rate_hz: u32,
    pub tracks: Vec<Track>,
    /// Export tag metadata (artist/title/album/etc.) — not touched by
    /// undo/redo, same as `selection`/`selected_tracks` below.
    pub metadata: ProjectMetadata,
    next_track_id: u32,
    next_clip_id: u32,
    /// Multi-clip selection (Shift+click toggles membership; a plain click
    /// replaces it with just that clip).
    pub selection: HashSet<ClipId>,
    /// Set by clicking a track header's background (outside its
    /// controls; Ctrl+click toggles a track, Shift+click range-selects);
    /// the Effects menu applies to every clip on every selected track when
    /// this is non-empty, taking priority over `selection`.
    pub selected_tracks: HashSet<TrackId>,
    /// The last plain- or Ctrl-clicked track header, i.e. the fixed end of
    /// a Shift+click range-select — not itself part of the persisted
    /// project state, just ephemeral UI interaction state kept alongside
    /// `selected_tracks`.
    pub track_selection_anchor: Option<TrackId>,
    clipboard: Vec<ClipboardEntry>,
    undo_stack: Vec<ProjectSnapshot>,
    redo_stack: Vec<ProjectSnapshot>,
}

/// Cloning a project (e.g. to export or save a snapshot) never carries its
/// undo/redo history along — that history belongs to the live editing
/// session, not to a point-in-time copy of its content.
impl Clone for Project {
    fn clone(&self) -> Self {
        Project {
            sample_rate_hz: self.sample_rate_hz,
            tracks: self.tracks.clone(),
            metadata: self.metadata.clone(),
            next_track_id: self.next_track_id,
            next_clip_id: self.next_clip_id,
            selection: self.selection.clone(),
            selected_tracks: self.selected_tracks.clone(),
            track_selection_anchor: self.track_selection_anchor,
            clipboard: self.clipboard.clone(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }
}

/// Settings for the Rattle effect — its own Adjustable Fade In and Sliding
/// Stretch values, independent of those effects' regular Effects-menu
/// settings (see `Project::apply_rattle`).
pub struct RattleParams {
    pub pitch_up_semitones: f32,
    pub pitch_down_semitones: f32,
    pub tempo_x_percent: f32,
    pub tempo_y_percent: f32,
    pub fade_in_start_gain: f32,
    pub fade_in_end_gain: f32,
    pub stretch: stretch::RampParams,
    /// Total [A, B] clips generated (always rounded down to an even number
    /// of whole pairs — see `Project::apply_rattle`).
    pub repeat_count: u32,
}

/// Removes each channel's DC offset (subtracts its mean), then scales the
/// whole (channel-linked) buffer so its peak lands at `peak_target_db`.
/// Used by `Project::apply_autotune`'s Normalize stage.
fn normalize_in_place(samples: &mut [f32], channels: usize, peak_target_db: f32) {
    if channels == 0 || samples.is_empty() {
        return;
    }
    let frames = samples.len() / channels;
    for ch in 0..channels {
        let mean: f32 = (0..frames).map(|f| samples[f * channels + ch]).sum::<f32>() / frames as f32;
        for f in 0..frames {
            samples[f * channels + ch] -= mean;
        }
    }
    let peak = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    if peak > 1e-9 {
        let gain = db_to_gain(peak_target_db) / peak;
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

impl Project {
    pub fn new(sample_rate_hz: u32) -> Self {
        let mut project = Self::empty(sample_rate_hz);
        project.add_track();
        // `add_track` records an undo step, but a freshly-constructed
        // project shouldn't itself be "undoable" back to zero tracks.
        project.clear_undo_history();
        project
    }

    /// Discards all undo/redo history — used right after building a
    /// project from scratch or from a save file, so that construction's
    /// own steps (adding the default track, rebuilding saved tracks/clips)
    /// aren't themselves undoable back to an empty project.
    pub(crate) fn clear_undo_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// A project with no tracks at all, used as the starting point when
    /// rebuilding a project loaded from disk (which supplies its own tracks).
    pub(crate) fn empty(sample_rate_hz: u32) -> Self {
        Project {
            sample_rate_hz,
            tracks: Vec::new(),
            metadata: ProjectMetadata::default(),
            next_track_id: 0,
            next_clip_id: 0,
            selection: HashSet::new(),
            selected_tracks: HashSet::new(),
            track_selection_anchor: None,
            clipboard: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Whether every track in the project has zero clips — used to tell a
    /// freshly-opened, never-touched project apart from one with actual
    /// audio in it (e.g. so Ctrl+L can fall back to "load last project"
    /// only when there's nothing here yet to mute).
    pub fn is_empty(&self) -> bool {
        self.tracks.iter().all(|t| t.clips.is_empty())
    }

    fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            sample_rate_hz: self.sample_rate_hz,
            tracks: self.tracks.clone(),
            next_track_id: self.next_track_id,
            next_clip_id: self.next_clip_id,
        }
    }

    fn restore(&mut self, snap: ProjectSnapshot) {
        self.sample_rate_hz = snap.sample_rate_hz;
        self.tracks = snap.tracks;
        self.next_track_id = snap.next_track_id;
        self.next_clip_id = snap.next_clip_id;
        self.selection.clear();
        self.selected_tracks.clear();
    }

    /// Records the current content for undo. Called automatically at the
    /// start of most mutating edits — cut/paste/duplicate/split/move/
    /// trim/effects/track add-remove-reorder — so callers don't need to
    /// remember to do it themselves. `pub(crate)` only for the handful of
    /// UI-side mutations that don't go through a `Project` method (e.g.
    /// resetting a track's pan/volume sliders directly).
    pub(crate) fn push_undo(&mut self) {
        self.undo_stack.push(self.snapshot());
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Undoes the most recent edit, if any (Ctrl+Z).
    pub fn undo(&mut self) {
        if let Some(snap) = self.undo_stack.pop() {
            self.redo_stack.push(self.snapshot());
            self.restore(snap);
        }
    }

    /// Re-applies the most recently undone edit, if any (Ctrl+Shift+Z).
    pub fn redo(&mut self) {
        if let Some(snap) = self.redo_stack.pop() {
            self.undo_stack.push(self.snapshot());
            self.restore(snap);
        }
    }

    pub fn add_track(&mut self) -> TrackId {
        self.push_undo();
        let id = TrackId(self.next_track_id);
        self.next_track_id += 1;
        let name = format!("Track {}", self.tracks.len() + 1);
        self.tracks.push(Track::new(id, name));
        id
    }

    pub fn remove_track(&mut self, id: TrackId) {
        self.push_undo();
        self.tracks.retain(|t| t.id != id);
        self.selected_tracks.remove(&id);
    }

    /// Moves a track by `offset` positions in the track list (negative =
    /// up/earlier, positive = down/later), clamped to the list's bounds.
    pub fn move_track(&mut self, id: TrackId, offset: i32) {
        let Some(pos) = self.tracks.iter().position(|t| t.id == id) else {
            return;
        };
        let new_pos = (pos as i64 + offset as i64).clamp(0, self.tracks.len() as i64 - 1) as usize;
        if new_pos != pos {
            self.push_undo();
            let track = self.tracks.remove(pos);
            self.tracks.insert(new_pos, track);
        }
    }

    pub fn move_track_to_top(&mut self, id: TrackId) {
        if let Some(pos) = self.tracks.iter().position(|t| t.id == id) {
            self.push_undo();
            let track = self.tracks.remove(pos);
            self.tracks.insert(0, track);
        }
    }

    pub fn move_track_to_bottom(&mut self, id: TrackId) {
        if let Some(pos) = self.tracks.iter().position(|t| t.id == id) {
            self.push_undo();
            let track = self.tracks.remove(pos);
            self.tracks.push(track);
        }
    }

    /// Duplicates a track (name, controls, and all clips) as a new track
    /// directly below the original, with fresh track/clip ids.
    pub fn duplicate_track(&mut self, id: TrackId) -> Option<TrackId> {
        let pos = self.tracks.iter().position(|t| t.id == id)?;
        self.push_undo();
        let mut cloned = self.tracks[pos].clone();
        cloned.id = TrackId(self.next_track_id);
        self.next_track_id += 1;
        cloned.name = format!("{} copy", cloned.name);
        for clip in &mut cloned.clips {
            clip.id = self.alloc_clip_id();
        }
        let new_id = cloned.id;
        self.tracks.insert(pos + 1, cloned);
        Some(new_id)
    }

    /// Splits a stereo track into two new mono tracks ("<name> L" / "<name>
    /// R"), replacing the original in place. Bakes all of the original
    /// track's clips down into one continuous clip per side (matching how
    /// `join_clips`/`merge_track_with_below` already bake overlapping
    /// clips into a single buffer). Returns the new (left, right) track
    /// ids, or `None` if the track isn't stereo.
    pub fn split_track_to_mono(&mut self, track_id: TrackId) -> Option<(TrackId, TrackId)> {
        let pos = self.tracks.iter().position(|t| t.id == track_id)?;
        if self.tracks[pos].channels < 2 {
            return None;
        }
        let total_frames = self.tracks[pos].clips.iter().map(|c| c.end_sample()).max().unwrap_or(0);

        self.push_undo();
        let original = self.tracks.remove(pos);

        let mut left = Track::new(TrackId(self.next_track_id), format!("{} L", original.name));
        self.next_track_id += 1;
        let mut right = Track::new(TrackId(self.next_track_id), format!("{} R", original.name));
        self.next_track_id += 1;
        for t in [&mut left, &mut right] {
            t.pan_percent = original.pan_percent;
            t.volume = original.volume;
            t.muted = original.muted;
            t.soloed = original.soloed;
            t.channels = 1;
        }

        if total_frames > 0 {
            let frames = total_frames as usize;
            let mut left_samples = vec![0.0f32; frames];
            let mut right_samples = vec![0.0f32; frames];
            for n in 0..total_frames {
                for c in &original.clips {
                    if let Some(l) = c.channel_sample_at(n, 0) {
                        left_samples[n as usize] += l;
                    }
                    if let Some(r) = c.channel_sample_at(n, 1) {
                        right_samples[n as usize] += r;
                    }
                }
            }
            let left_id = self.alloc_clip_id();
            left.clips.push(Clip::from_samples(left_id, left.name.clone(), 0, left_samples));
            let right_id = self.alloc_clip_id();
            right.clips.push(Clip::from_samples(right_id, right.name.clone(), 0, right_samples));
        }

        let left_id = left.id;
        let right_id = right.id;
        self.tracks.insert(pos, right);
        self.tracks.insert(pos, left);
        self.selected_tracks.clear();
        Some((left_id, right_id))
    }

    /// Merges `track_id` (mono) with the mono track directly below it into
    /// one new stereo track (this track becomes the left channel, the one
    /// below becomes the right), replacing both. Bakes both tracks' clips
    /// down into one continuous stereo clip. Returns the new track id, or
    /// `None` if there's no track below, or either isn't mono.
    pub fn merge_track_with_below(&mut self, track_id: TrackId) -> Option<TrackId> {
        let pos = self.tracks.iter().position(|t| t.id == track_id)?;
        if pos + 1 >= self.tracks.len() {
            return None;
        }
        if self.tracks[pos].channels != 1 || self.tracks[pos + 1].channels != 1 {
            return None;
        }

        let top_end = self.tracks[pos].clips.iter().map(|c| c.end_sample()).max().unwrap_or(0);
        let below_end = self.tracks[pos + 1].clips.iter().map(|c| c.end_sample()).max().unwrap_or(0);
        let total_frames = top_end.max(below_end);

        self.push_undo();
        let below = self.tracks.remove(pos + 1);
        let top = self.tracks.remove(pos);

        let mut merged = Track::new(TrackId(self.next_track_id), top.name.clone());
        self.next_track_id += 1;
        merged.pan_percent = top.pan_percent;
        merged.volume = top.volume;
        merged.muted = top.muted;
        merged.soloed = top.soloed;
        merged.channels = 2;

        if total_frames > 0 {
            let frames = total_frames as usize;
            let mut interleaved = vec![0.0f32; frames * 2];
            for n in 0..total_frames {
                let l: f32 = top.clips.iter().filter_map(|c| c.sample_at(n)).sum();
                let r: f32 = below.clips.iter().filter_map(|c| c.sample_at(n)).sum();
                interleaved[n as usize * 2] = l.clamp(-1.0, 1.0);
                interleaved[n as usize * 2 + 1] = r.clamp(-1.0, 1.0);
            }
            let clip_id = self.alloc_clip_id();
            merged
                .clips
                .push(Clip::from_samples_channels(clip_id, merged.name.clone(), 0, interleaved, 2));
        }

        let merged_id = merged.id;
        self.tracks.insert(pos, merged);
        self.selected_tracks.clear();
        Some(merged_id)
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    fn track_index(&self, id: TrackId) -> Option<usize> {
        self.tracks.iter().position(|t| t.id == id)
    }

    fn alloc_clip_id(&mut self) -> ClipId {
        let id = ClipId(self.next_clip_id);
        self.next_clip_id += 1;
        id
    }

    /// Which clips the Effects menu / fade shortcuts should apply to:
    /// every clip on every selected track (clicking a track header's
    /// background selects it, Shift+click adds/removes another) if any
    /// are selected, otherwise the current multi-clip `selection`.
    pub fn effect_targets(&self) -> Vec<ClipId> {
        if !self.selected_tracks.is_empty() {
            self.selected_tracks
                .iter()
                .flat_map(|&track_id| self.clip_ids_on_track(track_id))
                .collect()
        } else {
            self.selection.iter().copied().collect()
        }
    }

    /// Which track a fresh recording should land on, if any existing one is
    /// a clear fit — checked in priority order:
    /// 1. The single selected track (clicking a track header's background).
    /// 2. The track owning the current clip selection, if that selection is
    ///    all on one track.
    /// 3. `last_click_track` — the track most recently clicked in the
    ///    timeline, including a plain click on empty lane space that just
    ///    repositions the playhead (see `TimelineState::last_click`) and
    ///    doesn't set either selection above — this is how most people
    ///    actually "pick" a track to record onto.
    /// 4. With none of those available, the last track, if it's empty.
    ///
    /// `None` means the caller should create a fresh track instead
    /// (multiple tracks/clips selected across tracks, or nothing to go on
    /// and the last track already has clips on it).
    pub fn recording_target_track(&self, last_click_track: Option<TrackId>) -> Option<TrackId> {
        let mut selected_tracks = self.selected_tracks.iter();
        match (selected_tracks.next(), selected_tracks.next()) {
            (Some(&id), None) => return Some(id),
            (Some(_), Some(_)) => return None,
            _ => {}
        }

        if !self.selection.is_empty() {
            let mut clip_tracks = self.selection.iter().filter_map(|&id| self.find_clip_track(id));
            return match clip_tracks.next() {
                Some(first) if clip_tracks.all(|t| t == first) => Some(first),
                _ => None,
            };
        }

        if let Some(id) = last_click_track.filter(|id| self.track(*id).is_some()) {
            return Some(id);
        }

        self.tracks.last().filter(|t| t.clips.is_empty()).map(|t| t.id)
    }

    /// Finds which track currently owns `clip_id`, if any.
    pub fn find_clip_track(&self, clip_id: ClipId) -> Option<TrackId> {
        self.tracks
            .iter()
            .find(|t| t.clips.iter().any(|c| c.id == clip_id))
            .map(|t| t.id)
    }

    /// Every clip id belonging to `track_id`, in no particular order.
    pub fn clip_ids_on_track(&self, track_id: TrackId) -> Vec<ClipId> {
        self.track(track_id)
            .map(|t| t.clips.iter().map(|c| c.id).collect())
            .unwrap_or_default()
    }

    /// Adds a new mono clip built from raw samples (e.g. from the "Create
    /// Wave" dialog) onto `track_id` at `start_sample`.
    pub fn add_clip(
        &mut self,
        track_id: TrackId,
        name: String,
        start_sample: u64,
        samples: Vec<f32>,
    ) -> Option<ClipId> {
        self.add_clip_channels(track_id, name, start_sample, samples, 1)
    }

    /// Adds a new clip built from raw interleaved `samples` (`source_channels`
    /// channels) onto `track_id` at `start_sample` — used by file import,
    /// which may supply stereo audio. An empty target track's channel count
    /// is raised to `source_channels` if the clip is wider (e.g. a stereo
    /// import onto a fresh stereo-default track stays stereo, and a mono
    /// clip is duplicated to both channels rather than downgrading the
    /// track); a track that already has clips keeps its existing channel
    /// count, and the incoming audio is converted to match (mono duplicated
    /// to both channels, or stereo downmixed to mono) so every clip on a
    /// track always shares its channel count.
    pub fn add_clip_channels(
        &mut self,
        track_id: TrackId,
        name: String,
        start_sample: u64,
        samples: Vec<f32>,
        source_channels: u8,
    ) -> Option<ClipId> {
        self.push_undo();
        let id = self.alloc_clip_id();
        let track = self.track_mut(track_id)?;
        let source_channels = source_channels.max(1);
        if track.clips.is_empty() {
            track.channels = track.channels.max(source_channels);
        }
        let target_channels = track.channels;
        let samples = convert_channel_count(samples, source_channels, target_channels);
        let clip = Clip::from_samples_channels(id, name, start_sample, samples, target_channels);
        track.clips.push(clip);
        Some(id)
    }

    /// Moves an existing clip to a new position, possibly on a different
    /// track. Used by the timeline's clip-drag interaction.
    pub fn move_clip(&mut self, clip_id: ClipId, target_track: TrackId, new_start: u64) {
        self.push_undo();
        self.move_clip_inner(clip_id, target_track, new_start);
    }

    fn move_clip_inner(&mut self, clip_id: ClipId, target_track: TrackId, new_start: u64) {
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

    /// Moves `clip_id` to `(target_track, new_start)` — same as `move_clip`
    /// — then shifts every clip in `riders` by the same time delta on its
    /// own track, so dragging one clip in a multi-selection carries the
    /// rest of the selection along with it. One undo step for the whole
    /// group.
    pub fn move_clip_group(&mut self, clip_id: ClipId, target_track: TrackId, new_start: u64, riders: &[ClipId]) {
        let Some(origin_track) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(current_start) = self
            .track(origin_track)
            .and_then(|t| t.clips.iter().find(|c| c.id == clip_id))
            .map(|c| c.start_sample)
        else {
            return;
        };
        let delta = new_start as i64 - current_start as i64;
        self.push_undo();
        self.move_clip_inner(clip_id, target_track, new_start);
        for &id in riders {
            let Some(track_id) = self.find_clip_track(id) else { continue };
            let Some(start) = self.track(track_id).and_then(|t| t.clips.iter().find(|c| c.id == id)).map(|c| c.start_sample)
            else {
                continue;
            };
            let new_start = (start as i64 + delta).max(0) as u64;
            self.move_clip_inner(id, track_id, new_start);
        }
    }

    /// Trims a clip's left edge by `delta_samples` (positive shortens it).
    pub fn trim_clip_start(&mut self, clip_id: ClipId, delta_samples: i64) {
        self.push_undo();
        self.trim_clip_start_inner(clip_id, delta_samples);
    }

    fn trim_clip_start_inner(&mut self, clip_id: ClipId, delta_samples: i64) {
        if let Some(track_id) = self.find_clip_track(clip_id)
            && let Some(track) = self.track_mut(track_id)
                && let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) {
                    clip.trim_start(delta_samples);
                }
    }

    /// Trims `clip_id`'s left edge by `delta_samples` — same as
    /// `trim_clip_start` — then trims every clip in `riders` by the same
    /// amount, so dragging one clip's edge in a multi-selection trims the
    /// rest of the selection along with it. One undo step for the whole
    /// group.
    pub fn trim_clip_start_group(&mut self, clip_id: ClipId, delta_samples: i64, riders: &[ClipId]) {
        self.push_undo();
        self.trim_clip_start_inner(clip_id, delta_samples);
        for &id in riders {
            self.trim_clip_start_inner(id, delta_samples);
        }
    }

    /// Trims a clip's right edge by `delta_samples` (positive shortens it).
    pub fn trim_clip_end(&mut self, clip_id: ClipId, delta_samples: i64) {
        self.push_undo();
        self.trim_clip_end_inner(clip_id, delta_samples);
    }

    fn trim_clip_end_inner(&mut self, clip_id: ClipId, delta_samples: i64) {
        if let Some(track_id) = self.find_clip_track(clip_id)
            && let Some(track) = self.track_mut(track_id)
                && let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) {
                    clip.trim_end(delta_samples);
                }
    }

    /// Trims `clip_id`'s right edge by `delta_samples` — same as
    /// `trim_clip_end` — then trims every clip in `riders` by the same
    /// amount. One undo step for the whole group.
    pub fn trim_clip_end_group(&mut self, clip_id: ClipId, delta_samples: i64, riders: &[ClipId]) {
        self.push_undo();
        self.trim_clip_end_inner(clip_id, delta_samples);
        for &id in riders {
            self.trim_clip_end_inner(id, delta_samples);
        }
    }

    /// Builds clipboard entries for `clip_ids`, anchored to the earliest
    /// clip's track index and start sample so a later paste preserves
    /// their relative layout across tracks and time.
    fn build_clipboard(&self, clip_ids: &[ClipId]) -> Vec<ClipboardEntry> {
        let mut found: Vec<(usize, Clip)> = Vec::new();
        for &id in clip_ids {
            let Some(track_id) = self.find_clip_track(id) else { continue };
            let Some(track_index) = self.track_index(track_id) else { continue };
            let Some(clip) = self.track(track_id).and_then(|t| t.clips.iter().find(|c| c.id == id))
            else {
                continue;
            };
            found.push((track_index, clip.clone()));
        }
        let Some(anchor_track) = found.iter().map(|(idx, _)| *idx).min() else {
            return Vec::new();
        };
        let Some(anchor_start) = found.iter().map(|(_, c)| c.start_sample).min() else {
            return Vec::new();
        };
        found
            .into_iter()
            .map(|(track_index, clip)| ClipboardEntry {
                track_offset: track_index as i64 - anchor_track as i64,
                start_offset: clip.start_sample as i64 - anchor_start as i64,
                clip,
            })
            .collect()
    }

    /// Copies `clip_ids` into the clipboard, leaving them in place.
    pub fn copy_clips(&mut self, clip_ids: &[ClipId]) {
        let entries = self.build_clipboard(clip_ids);
        if !entries.is_empty() {
            self.clipboard = entries;
        }
    }

    /// Removes `clip_ids` from their tracks and stashes them in the
    /// clipboard.
    pub fn cut_clips(&mut self, clip_ids: &[ClipId]) {
        let entries = self.build_clipboard(clip_ids);
        if entries.is_empty() {
            return;
        }
        self.push_undo();
        self.clipboard = entries;
        for &id in clip_ids {
            if let Some(track_id) = self.find_clip_track(id)
                && let Some(track) = self.track_mut(track_id) {
                    track.clips.retain(|c| c.id != id);
                }
            // Otherwise a cut clip stays in `selection` under a now-dead
            // id — invisible (nothing on screen is drawn selected), but
            // still non-empty, so a later Left/Right/Shift+Left/Right
            // reads it as "a clip is selected", tries to nudge/jump that
            // (nonexistent) clip via `find_clip_track`, and silently does
            // nothing instead of falling back to moving the playhead.
            self.selection.remove(&id);
        }
    }

    /// Removes `clip_ids` from their tracks without touching the clipboard.
    pub fn delete_clips(&mut self, clip_ids: &[ClipId]) {
        if clip_ids.is_empty() {
            return;
        }
        self.push_undo();
        for &id in clip_ids {
            if let Some(track_id) = self.find_clip_track(id)
                && let Some(track) = self.track_mut(track_id) {
                    track.clips.retain(|c| c.id != id);
                }
            // See the matching comment in `cut_clips`.
            self.selection.remove(&id);
        }
    }

    /// Pastes the clipboard onto `target_track` at `start_sample`,
    /// preserving the copied clips' relative track and time offsets.
    /// Returns the newly created clip ids.
    pub fn paste(&mut self, target_track: TrackId, start_sample: u64) -> Vec<ClipId> {
        let Some(target_index) = self.track_index(target_track) else {
            return Vec::new();
        };
        if self.clipboard.is_empty() {
            return Vec::new();
        }
        self.push_undo();
        let entries = self.clipboard.clone();
        let mut new_ids = Vec::new();
        for entry in entries {
            let track_index =
                (target_index as i64 + entry.track_offset).clamp(0, self.tracks.len() as i64 - 1) as usize;
            let new_start = (start_sample as i64 + entry.start_offset).max(0) as u64;
            let mut clip = entry.clip;
            clip.id = self.alloc_clip_id();
            clip.start_sample = new_start;
            let id = clip.id;
            self.tracks[track_index].clips.push(clip);
            new_ids.push(id);
        }
        new_ids
    }

    /// Duplicates a clip onto `target_track` at `start_sample`, without
    /// touching the clipboard. Used by explicit Duplicate actions and by
    /// Ctrl/Cmd-drag-to-copy in the timeline. Pushes one undo step per
    /// call — use `duplicate_selection` (one undo step for the whole
    /// batch) rather than calling this in a loop.
    pub fn duplicate_clip(
        &mut self,
        clip_id: ClipId,
        target_track: TrackId,
        start_sample: u64,
    ) -> Option<ClipId> {
        self.push_undo();
        self.duplicate_clip_inner(clip_id, target_track, start_sample)
    }

    fn duplicate_clip_inner(
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

    /// Duplicates every clip in `clip_ids` in place on its own track,
    /// immediately after itself. Used by the multi-select Duplicate
    /// shortcut/action. One undo step for the whole batch.
    pub fn duplicate_selection(&mut self, clip_ids: &[ClipId]) {
        if clip_ids.is_empty() {
            return;
        }
        self.push_undo();
        for &id in clip_ids {
            let Some(track_id) = self.find_clip_track(id) else { continue };
            let end_sample = self
                .track(track_id)
                .and_then(|t| t.clips.iter().find(|c| c.id == id))
                .map(|c| c.end_sample());
            if let Some(end_sample) = end_sample {
                self.duplicate_clip_inner(id, track_id, end_sample);
            }
        }
    }

    /// Joins every selected clip that shares a track into one clip
    /// spanning from the earliest one's start to the latest one's end
    /// (silence fills any gap between them; where they overlap, the
    /// later clip — by start position — wins). Clips on different tracks
    /// are joined independently, one result per track. Returns the new
    /// clip id(s).
    pub fn join_clips(&mut self, clip_ids: &[ClipId]) -> Vec<ClipId> {
        let mut by_track: HashMap<TrackId, Vec<ClipId>> = HashMap::new();
        for &id in clip_ids {
            if let Some(track_id) = self.find_clip_track(id) {
                by_track.entry(track_id).or_default().push(id);
            }
        }
        let mut new_ids = Vec::new();
        for (track_id, ids) in by_track {
            if ids.len() < 2 {
                continue;
            }
            if let Some(new_id) = self.join_clips_on_track(track_id, &ids) {
                new_ids.push(new_id);
            }
        }
        new_ids
    }

    fn join_clips_on_track(&mut self, track_id: TrackId, clip_ids: &[ClipId]) -> Option<ClipId> {
        let (name, min_start, buffer, channels) = {
            let track = self.track(track_id)?;
            let mut clips: Vec<&Clip> = track.clips.iter().filter(|c| clip_ids.contains(&c.id)).collect();
            if clips.len() < 2 {
                return None;
            }
            clips.sort_by_key(|c| c.start_sample);
            let channels = track.channels.max(1) as usize;
            let min_start = clips.first()?.start_sample;
            let max_end = clips.iter().map(|c| c.end_sample()).max()?;
            let mut buffer = vec![0.0f32; (max_end - min_start) as usize * channels];
            for c in &clips {
                let offset = (c.start_sample - min_start) as usize * channels;
                let visible = c.visible_samples();
                buffer[offset..offset + visible.len()].copy_from_slice(visible);
            }
            let name = clips.first()?.name.clone();
            (name, min_start, buffer, channels)
        };

        self.push_undo();
        let new_id = self.alloc_clip_id();
        let track = self.track_mut(track_id)?;
        track.clips.retain(|c| !clip_ids.contains(&c.id));
        track
            .clips
            .push(Clip::from_samples_channels(new_id, name, min_start, buffer, channels as u8));
        Some(new_id)
    }

    /// Splits a clip into two at `at_sample` (an absolute project sample
    /// index). Returns the two new clip ids, or `None` if the split point
    /// isn't strictly inside the clip. Bakes the clip's current trim state
    /// into both halves (any hidden trimmed-away audio is discarded).
    pub fn split_clip(&mut self, clip_id: ClipId, at_sample: u64) -> Option<(ClipId, ClipId)> {
        self.push_undo();
        self.split_clip_no_undo(clip_id, at_sample)
    }

    /// Same as `split_clip`, minus the undo checkpoint — for callers (e.g.
    /// `select_range`) that need to perform several splits as one undoable
    /// step instead of one per split.
    fn split_clip_no_undo(&mut self, clip_id: ClipId, at_sample: u64) -> Option<(ClipId, ClipId)> {
        let track_id = self.find_clip_track(clip_id)?;

        let split_index = {
            let track = self.track(track_id)?;
            let clip = track.clips.iter().find(|c| c.id == clip_id)?;
            if at_sample <= clip.start_sample || at_sample >= clip.end_sample() {
                return None;
            }
            (at_sample - clip.start_sample) as usize
        };

        let first_id = self.alloc_clip_id();
        let second_id = self.alloc_clip_id();

        let track = self.track_mut(track_id)?;
        let pos = track.clips.iter().position(|c| c.id == clip_id)?;
        let original = track.clips.remove(pos);
        let channels = original.channels();
        let raw_split = split_index * channels as usize;
        let visible = original.visible_samples();
        let start_sample = original.start_sample;
        let name = original.name.clone();

        track.clips.push(Clip::from_samples_channels(
            first_id,
            format!("{name} (1)"),
            start_sample,
            visible[..raw_split].to_vec(),
            channels,
        ));
        track.clips.push(Clip::from_samples_channels(
            second_id,
            format!("{name} (2)"),
            start_sample + split_index as u64,
            visible[raw_split..].to_vec(),
            channels,
        ));

        // If the clip being split was itself selected, hand that selection
        // off to both halves — otherwise it's left referring to a now-dead
        // id (same hazard as an un-migrated cut/delete; see the comment in
        // `cut_clips`), and `select_range`'s callers rely on this to carry
        // a still-active whole-clip selection through a boundary split.
        if self.selection.remove(&clip_id) {
            self.selection.insert(first_id);
            self.selection.insert(second_id);
        }

        Some((first_id, second_id))
    }

    /// Commits a timeline "paint" range selection (see `timeline::draw_lane`)
    /// spanning `[lo_sample, hi_sample)` across `track_ids`: every clip on
    /// those tracks that only partly overlaps the range is split at
    /// whichever of its edges fall inside the range, so the range lands on
    /// exact clip boundaries, then every clip (whole or freshly split) that
    /// now exactly occupies the range on its track becomes the new
    /// selection — letting the ordinary clip-based cut/delete/effect flows
    /// act on just that sub-range instead of whole clips. A no-op if the
    /// range is empty.
    pub fn select_range(&mut self, track_ids: &[TrackId], lo_sample: u64, hi_sample: u64) {
        if lo_sample >= hi_sample {
            return;
        }
        self.push_undo();
        let mut selected = HashSet::new();
        for &track_id in track_ids {
            for clip_id in self.clip_ids_on_track(track_id) {
                let Some((start, end)) = self
                    .track(track_id)
                    .and_then(|t| t.clips.iter().find(|c| c.id == clip_id))
                    .map(|c| (c.start_sample, c.end_sample()))
                else {
                    continue;
                };
                let lo = lo_sample.max(start);
                let hi = hi_sample.min(end);
                if lo >= hi {
                    continue;
                }
                let mut working = clip_id;
                if lo > start {
                    let Some((_, second)) = self.split_clip_no_undo(working, lo) else {
                        continue;
                    };
                    working = second;
                }
                if hi < end {
                    let Some((first, _)) = self.split_clip_no_undo(working, hi) else {
                        continue;
                    };
                    working = first;
                }
                selected.insert(working);
            }
        }
        self.selection = selected;
        self.selected_tracks.clear();
    }

    /// Multiplies a clip's (visible) samples by `factor` in place (e.g.
    /// +1dB/-1dB volume steps from the Effects menu). Destructive: bakes
    /// the clip's current trim state into a fresh buffer.
    pub fn apply_gain(&mut self, clip_id: ClipId, factor: f32) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let samples: Vec<f32> = clip
            .visible_samples()
            .iter()
            .map(|s| (s * factor).clamp(-1.0, 1.0))
            .collect();
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// Cuts -4 dB across ~2 kHz-5 kHz — a "give to speech" dip that tames
    /// harshness in that presence range. Destructive: bakes the clip's
    /// current trim state into a fresh buffer.
    pub fn apply_give_to_speech(&mut self, clip_id: ClipId) {
        let stage = eq::give_to_speech_stage(self.sample_rate_hz as f32);
        self.apply_biquad_stages(clip_id, &[stage]);
    }

    /// Audacity-style "Telephone" bandpass EQ — see `eq::telephone_stages`.
    /// Destructive: bakes the clip's current trim state into a fresh
    /// buffer.
    pub fn apply_telephone(&mut self, clip_id: ClipId) {
        let stages = eq::telephone_stages(self.sample_rate_hz as f32);
        self.apply_biquad_stages(clip_id, &stages);
    }

    /// Runs a clip's (visible) samples through a cascade of biquad EQ
    /// stages in place. Destructive: bakes the clip's current trim state
    /// into a fresh buffer.
    fn apply_biquad_stages(&mut self, clip_id: ClipId, stages: &[eq::Biquad]) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let mut samples = clip.visible_samples().to_vec();
        eq::apply_cascade(stages, &mut samples, channels as usize);
        for s in &mut samples {
            *s = s.clamp(-1.0, 1.0);
        }
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// Runs the full Audacity "Autotune" macro chain: blind noise
    /// reduction, the exported Filter Curve EQ, normalize, compressor,
    /// limiter, then reverb. See the individual stage modules
    /// (`noise_reduction`, `eq`, `dynamics`, `reverb`) for how each is
    /// approximated — Audacity's own internals aren't available to match
    /// bit-for-bit, so this reproduces the same signal-chain shape and
    /// parameter values using standard equivalents. Destructive: bakes the
    /// clip's current trim state into a fresh buffer.
    pub fn apply_autotune(&mut self, clip_id: ClipId) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let sample_rate = self.sample_rate_hz as f32;
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let ch = channels as usize;
        let mut samples = clip.visible_samples().to_vec();

        noise_reduction::reduce_noise(&mut samples, ch);

        let kernel = eq::design_filter_curve_fir(&eq::AUTOTUNE_FILTER_CURVE, 8191, sample_rate);
        eq::convolve_fir(&kernel, &mut samples, ch);

        normalize_in_place(&mut samples, ch, -1.0);

        dynamics::process(
            &mut samples,
            ch,
            sample_rate,
            &dynamics::DynamicsParams {
                threshold_db: -18.0,
                ratio: 3.5,
                knee_width_db: 6.0,
                attack_ms: 5.0,
                release_ms: 120.0,
                lookahead_ms: 1.0,
                makeup_gain_db: 3.0,
            },
        );

        // Limiter: threshold=-5dB, makeup_target=-1dB — modeled as boosting
        // by the (target - threshold) headroom, then hard-limiting (very
        // high ratio) so nothing exceeds that target ceiling.
        dynamics::process(
            &mut samples,
            ch,
            sample_rate,
            &dynamics::DynamicsParams {
                threshold_db: -5.0,
                ratio: 1000.0,
                knee_width_db: 2.0,
                attack_ms: 1.0,
                release_ms: 20.0,
                lookahead_ms: 1.0,
                makeup_gain_db: -1.0 - -5.0,
            },
        );

        reverb::apply(
            &mut samples,
            ch,
            sample_rate,
            &reverb::ReverbParams {
                room_size: 25.0,
                reverberance: 15.0,
                hf_damping: 70.0,
                tone_low: 100.0,
                tone_high: 20.0,
                wet_gain_db: -8.0,
                dry_gain_db: 0.0,
                stereo_width: 100.0,
                pre_delay_ms: 10.0,
                wet_only: false,
            },
        );

        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// A Freeverb-style reverb (see the `reverb` module) with the same
    /// controls as Audacity's built-in Reverb effect. Destructive: bakes
    /// the clip's current trim state into a fresh buffer.
    pub fn apply_reverb(&mut self, clip_id: ClipId, params: &reverb::ReverbParams) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let sample_rate = self.sample_rate_hz as f32;
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let mut samples = clip.visible_samples().to_vec();
        reverb::apply(&mut samples, channels as usize, sample_rate, params);
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// Audacity-style Echo: `output[n] = input[n] + decay * output[n -
    /// delay]` — a recursive (feedback) repeat, so a `decay` at or above
    /// 1.0 will build up rather than fade out, exactly as in Audacity.
    /// Destructive: bakes the clip's current trim state into a fresh
    /// buffer.
    pub fn apply_echo(&mut self, clip_id: ClipId, delay_seconds: f32, decay: f32) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let sample_rate = self.sample_rate_hz as f32;
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels() as usize;
        let mut samples = clip.visible_samples().to_vec();
        let delay_samples = ((delay_seconds.max(0.0)) * sample_rate).round() as usize;
        if delay_samples > 0 && channels > 0 {
            let frames = samples.len() / channels;
            for frame in delay_samples..frames {
                for c in 0..channels {
                    let echoed = samples[(frame - delay_samples) * channels + c] * decay;
                    samples[frame * channels + c] = (samples[frame * channels + c] + echoed).clamp(-1.0, 1.0);
                }
            }
        }
        *clip = Clip::from_samples_channels(
            clip.id,
            clip.name.clone(),
            clip.start_sample,
            samples,
            channels as u8,
        );
    }

    /// Audacity-style hard-clipping distortion: boosts by `drive_db` then
    /// hard-clips anything beyond `+-threshold` back to the threshold.
    /// Destructive: bakes the clip's current trim state into a fresh
    /// buffer.
    pub fn apply_hard_clip_distortion(&mut self, clip_id: ClipId, drive_db: f32, threshold: f32) {
        let threshold = threshold.clamp(0.01, 1.0);
        self.apply_sample_transform(clip_id, |s| {
            (s * db_to_gain(drive_db)).clamp(-threshold, threshold)
        });
    }

    /// Inverts phase: multiplies every sample by -1. Destructive: bakes
    /// the clip's current trim state into a fresh buffer.
    pub fn apply_invert(&mut self, clip_id: ClipId) {
        self.apply_sample_transform(clip_id, |s| -s);
    }

    /// Runs a clip's (visible) samples through a per-sample transform `f`
    /// in place. Destructive: bakes the clip's current trim state into a
    /// fresh buffer.
    fn apply_sample_transform(&mut self, clip_id: ClipId, f: impl Fn(f32) -> f32) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let samples: Vec<f32> = clip.visible_samples().iter().map(|&s| f(s)).collect();
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// Reverses the clip's audio in time (frame order flipped; each
    /// frame's channels stay together). Destructive: bakes the clip's
    /// current trim state into a fresh buffer.
    pub fn apply_reverse(&mut self, clip_id: ClipId) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels() as usize;
        let mut samples = clip.visible_samples().to_vec();
        let frames = samples.len() / channels;
        for frame in 0..frames / 2 {
            let other = frames - 1 - frame;
            for c in 0..channels {
                samples.swap(frame * channels + c, other * channels + c);
            }
        }
        *clip = Clip::from_samples_channels(
            clip.id,
            clip.name.clone(),
            clip.start_sample,
            samples,
            channels as u8,
        );
    }

    /// Swaps the left/right channels of a stereo clip (a no-op on mono
    /// clips). Destructive: bakes the clip's current trim state into a
    /// fresh buffer.
    pub fn apply_swap_channels(&mut self, clip_id: ClipId) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        if channels < 2 {
            return;
        }
        let channels = channels as usize;
        let mut samples = clip.visible_samples().to_vec();
        let frames = samples.len() / channels;
        for frame in 0..frames {
            samples.swap(frame * channels, frame * channels + 1);
        }
        *clip = Clip::from_samples_channels(
            clip.id,
            clip.name.clone(),
            clip.start_sample,
            samples,
            channels as u8,
        );
    }

    /// Splits a stereo clip apart into its left/right channels, ramps one
    /// side's gain up (fading in) and the other down (fading out) linearly
    /// across the clip's duration, then recombines them back into the
    /// stereo output — see the `pan_toggle` module. A no-op on mono clips.
    /// Destructive: bakes the clip's current trim state into a fresh
    /// buffer.
    pub fn apply_pan_toggle(&mut self, clip_id: ClipId, params: &pan_toggle::PanToggleParams) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(track) = self.track(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter().find(|c| c.id == clip_id) else {
            return;
        };
        if clip.channels() != 2 {
            return;
        }
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let mut samples = clip.visible_samples().to_vec();
        pan_toggle::apply(&mut samples, channels as usize, params);
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// A port of "trip-toggler.py": finds clear low points in the clip and
    /// alternates a fade-down and a fade-up across the resulting segments
    /// — see the `trip_toggler` module for the algorithm and what's
    /// intentionally left out of this port (batch/file-level features that
    /// don't apply to a single selection). Destructive: bakes the clip's
    /// current trim state into a fresh buffer.
    pub fn apply_trip_toggler(&mut self, clip_id: ClipId, params: &trip_toggler::TripTogglerParams) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let sample_rate = self.sample_rate_hz as f32;
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let mut samples = clip.visible_samples().to_vec();
        trip_toggler::apply(&mut samples, channels as usize, sample_rate, params);
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, samples, channels);
    }

    /// A constant-ratio Tempo Up/Down: WSOLA time-stretch by `tempo_percent`
    /// (Audacity's "Change Tempo" convention: `+10.0` finishes 10% faster,
    /// i.e. shorter) with pitch left untouched. Destructive: bakes the
    /// clip's current trim state into a fresh buffer, and its length
    /// generally changes.
    pub fn apply_tempo_shift(&mut self, clip_id: ClipId, tempo_percent: f32) {
        let params = stretch::RampParams {
            initial_tempo_percent: tempo_percent,
            final_tempo_percent: tempo_percent,
            initial_pitch_semitones: 0.0,
            final_pitch_semitones: 0.0,
        };
        self.apply_stretch_ramp(clip_id, &params);
    }

    /// Audacity-style Sliding Stretch: tempo and pitch each ramp linearly
    /// from an initial value (clip start) to a final value (clip end) — see
    /// `stretch::apply_time_pitch_ramp`. Destructive: bakes the clip's
    /// current trim state into a fresh buffer, and its length generally
    /// changes.
    pub fn apply_sliding_stretch(&mut self, clip_id: ClipId, params: &stretch::RampParams) {
        self.apply_stretch_ramp(clip_id, params);
    }

    fn apply_stretch_ramp(&mut self, clip_id: ClipId, params: &stretch::RampParams) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let sample_rate = self.sample_rate_hz as f32;
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels();
        let source = clip.visible_samples().to_vec();
        let result = stretch::apply_time_pitch_ramp(&source, channels as usize, sample_rate, params);
        *clip = Clip::from_samples_channels(clip.id, clip.name.clone(), clip.start_sample, result, channels);
    }

    /// Builds two pitch/tempo-shifted variants of a clip ("A": pitched up
    /// and sped/slowed by `tempo_x_percent`; "B": pitched down and
    /// sped/slowed by `-tempo_y_percent`), lays out `params.repeat_count / 2`
    /// [A, B] repetitions starting right after the original clip ends (same
    /// convention as
    /// `duplicate_selection` — the copy goes right after the source, not on
    /// top of it) into one new clip, then applies its own Adjustable Fade
    /// In and its own Sliding Stretch (independent settings from those
    /// effects' regular Effects-menu entries). Destructive/generative: the
    /// original clip is left in place; this adds a new clip alongside it
    /// (undoable, like any other clip-adding operation).
    ///
    /// Each repetition is overlap-added onto the previous by
    /// `stretch::synth_hop_frames` rather than placed strictly back-to-back
    /// — `apply_time_pitch_ramp`'s output is only a valid reconstruction
    /// away from its very first/last `synth_hop_frames`, which are each a
    /// lone, un-summed Hann ramp (see that function's docs); a cold splice
    /// between two independent outputs would leave an audible silent gap
    /// there, and summing the overlap reconstructs the same constant-unity
    /// envelope a single continuous call would have produced.
    pub fn apply_rattle(&mut self, clip_id: ClipId, params: &RattleParams) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let sample_rate = self.sample_rate_hz as f32;

        let Some((channels, base_start, original_samples)) = self
            .track(track_id)
            .and_then(|t| t.clips.iter().find(|c| c.id == clip_id))
            .map(|c| (c.channels(), c.end_sample(), c.visible_samples().to_vec()))
        else {
            return;
        };
        let ch = channels as usize;

        let a_pitched = resample_for_pitch(&original_samples, ch, params.pitch_up_semitones);
        let a_final = stretch::apply_time_pitch_ramp(
            &a_pitched,
            ch,
            sample_rate,
            &stretch::RampParams {
                initial_tempo_percent: params.tempo_x_percent,
                final_tempo_percent: params.tempo_x_percent,
                initial_pitch_semitones: 0.0,
                final_pitch_semitones: 0.0,
            },
        );

        let b_pitched = resample_for_pitch(&original_samples, ch, -params.pitch_down_semitones);
        let b_final = stretch::apply_time_pitch_ramp(
            &b_pitched,
            ch,
            sample_rate,
            &stretch::RampParams {
                initial_tempo_percent: -params.tempo_y_percent,
                final_tempo_percent: -params.tempo_y_percent,
                initial_pitch_semitones: 0.0,
                final_pitch_semitones: 0.0,
            },
        );

        let overlap_frames = stretch::synth_hop_frames(sample_rate);
        let variants = [&a_final, &b_final];
        let pairs = (params.repeat_count / 2).max(1);
        let mut combined: Vec<f32> = Vec::new();
        for _ in 0..pairs {
            for variant in &variants {
                append_with_overlap_add(&mut combined, variant, ch, overlap_frames);
            }
        }

        self.push_undo();
        let combined_id = self.alloc_clip_id();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        track
            .clips
            .push(Clip::from_samples_channels(combined_id, "Rattle".to_string(), base_start, combined, channels));

        self.apply_adjustable_fade(combined_id, params.fade_in_start_gain, params.fade_in_end_gain);
        self.apply_sliding_stretch(combined_id, &params.stretch);
    }

    /// A constant-ratio Pitch Up/Down: WSOLA pitch shift by `semitones`
    /// (positive = up, negative = down) with tempo left untouched — the
    /// same `stretch::apply_time_pitch_ramp` engine `apply_tempo_shift`
    /// uses, just with the tempo/pitch roles swapped, so pitch and tempo
    /// are shifted fully independently of each other (Audacity-style)
    /// rather than the old "tape speed" resample trick, which changed
    /// both together. Destructive: bakes the clip's current trim state
    /// into a fresh buffer; its length is preserved (mod WSOLA's usual
    /// rounding).
    pub fn apply_pitch_shift(&mut self, clip_id: ClipId, semitones: f32) {
        let params = stretch::RampParams {
            initial_tempo_percent: 0.0,
            final_tempo_percent: 0.0,
            initial_pitch_semitones: semitones,
            final_pitch_semitones: semitones,
        };
        self.apply_stretch_ramp(clip_id, &params);
    }

    /// Silences samples in `[from_sample, to_sample)` (absolute project
    /// sample indices) within a clip, e.g. for muting a selected portion
    /// of it (Ctrl+L). Destructive: bakes the clip's current trim state
    /// into a fresh buffer.
    pub fn mute_range(&mut self, clip_id: ClipId, from_sample: u64, to_sample: u64) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter().find(|c| c.id == clip_id) else {
            return;
        };
        let clip_start = clip.start_sample;
        let clip_end = clip.end_sample();
        let from = from_sample.max(clip_start).saturating_sub(clip_start);
        let to = to_sample.min(clip_end).saturating_sub(clip_start);
        if from >= to {
            return;
        }
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels() as usize;
        let mut samples = clip.visible_samples().to_vec();
        let frames = samples.len() / channels;
        let to = (to as usize).min(frames);
        for frame in from as usize..to {
            for ch in 0..channels {
                samples[frame * channels + ch] = 0.0;
            }
        }
        *clip = Clip::from_samples_channels(
            clip.id,
            clip.name.clone(),
            clip.start_sample,
            samples,
            channels as u8,
        );
    }

    pub fn apply_fade_in(&mut self, clip_id: ClipId) {
        self.apply_fade_with_gains(clip_id, 0.0, 1.0);
    }

    pub fn apply_fade_out(&mut self, clip_id: ClipId) {
        self.apply_fade_with_gains(clip_id, 1.0, 0.0);
    }

    /// A fade with user-configurable start/end levels (e.g. from the
    /// "Adjustable Fade In/Out" effects, which let the levels be dB
    /// values in either order — the caller is responsible for sorting
    /// them by direction before converting to linear gain and calling
    /// this).
    pub fn apply_adjustable_fade(&mut self, clip_id: ClipId, start_gain: f32, end_gain: f32) {
        self.apply_fade_with_gains(clip_id, start_gain, end_gain);
    }

    /// Applies a linear ramp from `start_gain` to `end_gain` across the
    /// clip's entire (visible) duration.
    fn apply_fade_with_gains(&mut self, clip_id: ClipId, start_gain: f32, end_gain: f32) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels() as usize;
        let mut samples = clip.visible_samples().to_vec();
        let frames = samples.len() / channels;
        let last = (frames.max(1) - 1).max(1) as f32;
        for frame in 0..frames {
            let t = frame as f32 / last;
            let gain = start_gain + t * (end_gain - start_gain);
            for ch in 0..channels {
                samples[frame * channels + ch] *= gain;
            }
        }
        *clip = Clip::from_samples_channels(
            clip.id,
            clip.name.clone(),
            clip.start_sample,
            samples,
            channels as u8,
        );
    }

    /// Fades just `[from_sample, to_sample)` (absolute project sample
    /// indices) within a clip — e.g. fading in only the start of a clip,
    /// or a held-mouse-selected portion of it. `overall_lo`/`overall_hi`
    /// is the full span of the (possibly multi-clip) selection the fade
    /// belongs to: the ramp's 0..1 progress is computed against that whole
    /// span, not just this one clip's slice of it, so a fade spanning
    /// several clips ramps smoothly across all of them instead of
    /// restarting at each clip boundary.
    pub fn apply_fade_in_range(
        &mut self,
        clip_id: ClipId,
        from_sample: u64,
        to_sample: u64,
        overall_lo: u64,
        overall_hi: u64,
    ) {
        self.apply_fade_range_with_gains(clip_id, from_sample, to_sample, overall_lo, overall_hi, 0.0, 1.0);
    }

    pub fn apply_fade_out_range(
        &mut self,
        clip_id: ClipId,
        from_sample: u64,
        to_sample: u64,
        overall_lo: u64,
        overall_hi: u64,
    ) {
        self.apply_fade_range_with_gains(clip_id, from_sample, to_sample, overall_lo, overall_hi, 1.0, 0.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_fade_range_with_gains(
        &mut self,
        clip_id: ClipId,
        from_sample: u64,
        to_sample: u64,
        overall_lo: u64,
        overall_hi: u64,
        start_gain: f32,
        end_gain: f32,
    ) {
        let Some(track_id) = self.find_clip_track(clip_id) else {
            return;
        };
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter().find(|c| c.id == clip_id) else {
            return;
        };
        let clip_start = clip.start_sample;
        let clip_end = clip.end_sample();
        let from = from_sample.max(clip_start).saturating_sub(clip_start) as usize;
        let to = to_sample.min(clip_end).saturating_sub(clip_start) as usize;
        if from >= to {
            return;
        }
        self.push_undo();
        let Some(track) = self.track_mut(track_id) else {
            return;
        };
        let Some(clip) = track.clips.iter_mut().find(|c| c.id == clip_id) else {
            return;
        };
        let channels = clip.channels() as usize;
        let mut samples = clip.visible_samples().to_vec();
        let frames = samples.len() / channels;
        let to = to.min(frames);
        let span = overall_hi.saturating_sub(overall_lo).max(1) as f32;
        for frame in from..to {
            let absolute_sample = clip_start + frame as u64;
            let t = (absolute_sample.saturating_sub(overall_lo) as f32 / span).clamp(0.0, 1.0);
            let gain = start_gain + t * (end_gain - start_gain);
            for ch in 0..channels {
                samples[frame * channels + ch] *= gain;
            }
        }
        *clip = Clip::from_samples_channels(
            clip.id,
            clip.name.clone(),
            clip.start_sample,
            samples,
            channels as u8,
        );
    }
}
