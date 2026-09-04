//! The note/section/melody data model — plain data plus pure mutators, with
//! no egui dependency, so it's fully headless-testable the same way
//! `project::Project`'s mutators are (see `tests/bethoven_tests.rs`).

use super::instrument::{render_note, Instrument};
use crate::audio_engine::mix::pan_gains;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

/// Ticks per quarter note — fine enough grain for 16th-note snapping
/// without needing floating point tick positions.
pub const PPQ: u32 = 96;
pub const BEATS_PER_BAR: u32 = 4;
/// Grid/snap resolution: a 16th note. This is only the *default* — the
/// piano roll's drag-move/resize snaps to progressively finer subdivisions
/// of this once zoomed in close enough (see `piano_roll::grid_step_ticks`),
/// which is why the model's own floor on a note's length is `MIN_NOTE_TICKS`
/// (a single tick), not this.
pub const GRID_TICKS: u32 = PPQ / 4;
/// The shortest a note can ever be — one tick, i.e. 1/96th of a quarter
/// note — just enough to keep a note from collapsing to zero/negative
/// length; not a musical grid step in its own right.
pub const MIN_NOTE_TICKS: u32 = 1;
pub const DEFAULT_SECTION_BARS: u32 = 8;

/// Rounds `ticks` down to the nearest grid step (never below one step).
pub fn snap_ticks(ticks: i64) -> u32 {
    let step = GRID_TICKS as i64;
    ((ticks.max(step) / step) * step) as u32
}

pub fn bars_to_ticks(bars: u32) -> u32 {
    bars * BEATS_PER_BAR * PPQ
}

pub fn ticks_to_samples(ticks: u32, bpm: f32, sample_rate_hz: u32) -> u64 {
    let seconds_per_beat = 60.0 / bpm.max(1.0);
    let beats = ticks as f64 / PPQ as f64;
    (beats * seconds_per_beat as f64 * sample_rate_hz as f64).round() as u64
}

/// Inverse of `ticks_to_samples` — used to draw the preview playhead at its
/// corresponding tick position in the piano roll.
pub fn samples_to_ticks(samples: u64, bpm: f32, sample_rate_hz: u32) -> u32 {
    let seconds_per_beat = 60.0 / bpm.max(1.0) as f64;
    let seconds = samples as f64 / sample_rate_hz as f64;
    let beats = seconds / seconds_per_beat;
    (beats * PPQ as f64).round().max(0.0) as u32
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: u32,
    /// MIDI-style note number (60 = C4).
    pub pitch: u8,
    pub start_tick: u32,
    pub length_ticks: u32,
    pub instrument: Instrument,
    /// Linear gain, 0.0..=1.0.
    pub gain: f32,
    /// -100 (full left) ..= 100 (full right).
    pub pan: i8,
}

impl Note {
    pub fn end_tick(&self) -> u32 {
        self.start_tick + self.length_ticks
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: u32,
    pub name: String,
    /// Root pitch class, 0..=11 (see `scales::ROOT_NAMES`).
    pub root: u8,
    /// Index into `scales::SCALES`.
    pub scale_index: usize,
    pub length_ticks: u32,
    pub notes: Vec<Note>,
}

fn next_id(existing: impl Iterator<Item = u32>) -> u32 {
    existing.max().map(|m| m + 1).unwrap_or(0)
}

impl Section {
    pub fn new(id: u32, name: String, root: u8, scale_index: usize, length_ticks: u32) -> Self {
        Section { id, name, root, scale_index, length_ticks, notes: Vec::new() }
    }

    pub fn note(&self, id: u32) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn add_note(&mut self, pitch: u8, start_tick: u32, length_ticks: u32, instrument: Instrument) -> u32 {
        let id = next_id(self.notes.iter().map(|n| n.id));
        self.notes.push(Note {
            id,
            pitch,
            start_tick,
            length_ticks: length_ticks.max(MIN_NOTE_TICKS),
            instrument,
            gain: 1.0,
            pan: 0,
        });
        id
    }

    pub fn delete_notes(&mut self, ids: &[u32]) {
        self.notes.retain(|n| !ids.contains(&n.id));
    }

    /// Moves every note in `ids` by `delta_ticks`/`delta_pitch` together —
    /// used for both single- and multi-note drag, so the whole selection
    /// stays in the same relative shape.
    pub fn move_notes(&mut self, ids: &[u32], delta_ticks: i64, delta_pitch: i32) {
        for note in self.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
            note.start_tick = (note.start_tick as i64 + delta_ticks).max(0) as u32;
            note.pitch = (note.pitch as i32 + delta_pitch).clamp(0, 127) as u8;
        }
    }

    /// Like `move_notes`, but the vertical move is expressed as a number of
    /// *visible rows* (as shown in the piano roll, i.e. `visible_rows`,
    /// already filtered to the section's scale, highest pitch first) rather
    /// than raw semitones — since adjacent scale rows aren't always a
    /// semitone apart, moving by a flat semitone delta can land a note on a
    /// pitch that isn't a member of `visible_rows` at all, making it
    /// silently disappear from the roll (while still sounding, since
    /// playback doesn't filter by scale). `delta_rows` follows the same
    /// sign convention as `move_notes`'s `delta_pitch`: positive moves to a
    /// higher pitch (i.e. toward the *start* of `visible_rows`). A note
    /// whose current pitch isn't in `visible_rows` (e.g. left over from
    /// before the section's scale was changed) is moved in time only — its
    /// pitch is left alone rather than guessed at.
    pub fn move_notes_by_row(&mut self, ids: &[u32], delta_ticks: i64, delta_rows: i32, visible_rows: &[u8]) {
        for note in self.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
            note.start_tick = (note.start_tick as i64 + delta_ticks).max(0) as u32;
            if delta_rows != 0
                && let Some(idx) = visible_rows.iter().position(|&p| p == note.pitch)
            {
                let new_idx = (idx as i32 - delta_rows).clamp(0, visible_rows.len() as i32 - 1) as usize;
                note.pitch = visible_rows[new_idx];
            }
        }
    }

    /// Resizes one note by moving its start and/or end independently
    /// (dragging the left edge changes `start_tick` while keeping the same
    /// end; dragging the right edge only changes the length) — callers pass
    /// the new absolute start/length, already snapped to the grid.
    pub fn resize_note(&mut self, id: u32, new_start_tick: u32, new_length_ticks: u32) {
        if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
            note.start_tick = new_start_tick;
            note.length_ticks = new_length_ticks.max(MIN_NOTE_TICKS);
        }
    }

    /// Duplicates every note in `ids`, shifted by `tick_offset`, returning
    /// the new notes' ids (which become the new selection).
    pub fn duplicate_notes(&mut self, ids: &[u32], tick_offset: i64) -> Vec<u32> {
        let mut next = next_id(self.notes.iter().map(|n| n.id));
        let mut new_ids = Vec::new();
        let copies: Vec<Note> = self
            .notes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(|n| {
                let mut copy = n.clone();
                copy.id = next;
                copy.start_tick = (copy.start_tick as i64 + tick_offset).max(0) as u32;
                new_ids.push(next);
                next += 1;
                copy
            })
            .collect();
        self.notes.extend(copies);
        new_ids
    }

    pub fn set_instrument(&mut self, ids: &[u32], instrument: Instrument) {
        for note in self.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
            note.instrument = instrument;
        }
    }

    pub fn adjust_gain(&mut self, ids: &[u32], delta: f32) {
        for note in self.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
            note.gain = (note.gain + delta).clamp(0.0, 1.0);
        }
    }

    pub fn adjust_pan(&mut self, ids: &[u32], delta: i32) {
        for note in self.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
            note.pan = (note.pan as i32 + delta).clamp(-100, 100) as i8;
        }
    }

    pub fn reset_gain_pan(&mut self, ids: &[u32]) {
        for note in self.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
            note.gain = 1.0;
            note.pan = 0;
        }
    }

    /// Which instruments have at least one note in this section, in
    /// `Instrument::ALL` order — used to populate the export dialogs'
    /// instrument checklists ("actually used" rather than all 13).
    pub fn used_instruments(&self) -> Vec<Instrument> {
        Instrument::ALL.iter().copied().filter(|inst| self.notes.iter().any(|n| n.instrument == *inst)).collect()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Melody {
    pub id: u32,
    pub name: String,
    pub bpm: f32,
    pub sections: Vec<Section>,
    /// The "sticky" default applied to newly placed notes — copied from a
    /// note via Alt+D.
    pub default_note_length_ticks: u32,
    pub default_instrument: Instrument,
    /// Which section was open in the piano roll when this melody was last
    /// worked on — restored the next time the project is opened, instead
    /// of always landing back on the first section. Absent from `.raku`
    /// files saved before this existed.
    #[serde(default)]
    pub last_section_id: Option<u32>,
    /// Instruments muted for this melody's own preview playback and
    /// export — e.g. muting the drums used as a metronome while composing
    /// so they're silent in playback and unchecked by default when
    /// exporting. Overridden by `soloed_instruments` whenever that's
    /// non-empty (see `is_instrument_audible`). Absent from `.raku` files
    /// saved before this existed.
    #[serde(default)]
    pub muted_instruments: HashSet<Instrument>,
    /// Instruments soloed for this melody — when non-empty, only these
    /// play/export regardless of `muted_instruments`.
    #[serde(default)]
    pub soloed_instruments: HashSet<Instrument>,
}

impl Melody {
    pub fn new(id: u32, name: String) -> Self {
        let mut melody = Melody {
            id,
            name,
            bpm: 120.0,
            sections: Vec::new(),
            default_note_length_ticks: PPQ,
            default_instrument: Instrument::Piano,
            last_section_id: None,
            muted_instruments: HashSet::new(),
            soloed_instruments: HashSet::new(),
        };
        let id = melody.add_section("Section 1".to_string(), 0, 0, bars_to_ticks(DEFAULT_SECTION_BARS));
        melody.last_section_id = Some(id);
        melody
    }

    pub fn add_section(&mut self, name: String, root: u8, scale_index: usize, length_ticks: u32) -> u32 {
        let id = next_id(self.sections.iter().map(|s| s.id));
        self.sections.push(Section::new(id, name, root, scale_index, length_ticks.max(GRID_TICKS)));
        id
    }

    pub fn remove_section(&mut self, id: u32) {
        self.sections.retain(|s| s.id != id);
    }

    pub fn section(&self, id: u32) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    pub fn section_mut(&mut self, id: u32) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.id == id)
    }

    /// Whether `instrument` should be heard/exported right now: soloing
    /// anything makes it (and only it/its solo-siblings) exclusively
    /// audible, otherwise it's audible unless individually muted.
    pub fn is_instrument_audible(&self, instrument: Instrument) -> bool {
        if !self.soloed_instruments.is_empty() {
            self.soloed_instruments.contains(&instrument)
        } else {
            !self.muted_instruments.contains(&instrument)
        }
    }

    /// Which instruments have at least one note anywhere in this melody
    /// (across all sections), in `Instrument::ALL` order — populates the
    /// mute/solo "Note Tracks" row, which applies melody-wide.
    pub fn used_instruments(&self) -> Vec<Instrument> {
        Instrument::ALL
            .iter()
            .copied()
            .filter(|inst| self.sections.iter().any(|s| s.notes.iter().any(|n| n.instrument == *inst)))
            .collect()
    }
}

/// Renders every note in `section` into one stereo (interleaved) buffer
/// spanning exactly `section.length_ticks` at `bpm`. Notes are placed at
/// their tick offset and panned via `pan_gains` (the same constant-power law
/// the main mixer uses for mono tracks); anything hanging past the
/// section's end is simply not written.
pub fn render_section(section: &Section, bpm: f32, sample_rate_hz: u32) -> Vec<f32> {
    render_section_filtered(section, bpm, sample_rate_hz, |_| true)
}

/// Like `render_section`, but only notes whose instrument passes `include`
/// are rendered — used for melody-wide mute/solo during preview playback,
/// and for the export dialogs' per-instrument checklists.
pub fn render_section_filtered(
    section: &Section,
    bpm: f32,
    sample_rate_hz: u32,
    include: impl Fn(Instrument) -> bool,
) -> Vec<f32> {
    let total_samples = ticks_to_samples(section.length_ticks, bpm, sample_rate_hz) as usize;
    let mut buf = vec![0.0f32; total_samples * 2];
    for note in section.notes.iter().filter(|n| include(n.instrument)) {
        let start = ticks_to_samples(note.start_tick, bpm, sample_rate_hz) as usize;
        let dur_samples = ticks_to_samples(note.length_ticks, bpm, sample_rate_hz).max(1);
        let duration = Duration::from_secs_f64(dur_samples as f64 / sample_rate_hz as f64);
        let mono = render_note(note.instrument, note.pitch, duration, note.gain, sample_rate_hz);
        let (left_gain, right_gain) = pan_gains(note.pan, 1.0);
        for (i, &s) in mono.iter().enumerate() {
            let idx = start + i;
            if idx >= total_samples {
                break;
            }
            buf[idx * 2] += s * left_gain;
            buf[idx * 2 + 1] += s * right_gain;
        }
    }
    for s in buf.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }
    buf
}

/// Renders the whole melody: every section's mixdown, concatenated in
/// order.
pub fn render_melody(melody: &Melody, sample_rate_hz: u32) -> Vec<f32> {
    let mut out = Vec::new();
    for section in &melody.sections {
        out.extend(render_section(section, melody.bpm, sample_rate_hz));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_melody_marks_its_own_section_as_last_open() {
        let melody = Melody::new(0, "Test".into());
        assert_eq!(melody.last_section_id, Some(melody.sections[0].id));
    }

    #[test]
    fn add_move_resize_delete_notes() {
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(4));
        let id = section.add_note(60, 0, PPQ, Instrument::Piano);
        assert_eq!(section.notes.len(), 1);

        section.move_notes(&[id], PPQ as i64, 2);
        assert_eq!(section.note(id).unwrap().start_tick, PPQ);
        assert_eq!(section.note(id).unwrap().pitch, 62);

        section.resize_note(id, PPQ, PPQ * 2);
        assert_eq!(section.note(id).unwrap().length_ticks, PPQ * 2);

        section.delete_notes(&[id]);
        assert!(section.notes.is_empty());
    }

    #[test]
    fn resize_and_add_note_only_floor_at_a_single_tick_not_a_16th_note() {
        // The piano roll's own drag snapping (`piano_roll::grid_step_ticks`)
        // is what normally keeps a note at least a 16th note long at low
        // zoom — the model itself must allow shorter, otherwise zooming in
        // to resize a note below a 16th note gets silently clamped back up.
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(4));
        let id = section.add_note(60, 0, PPQ, Instrument::Piano);
        section.resize_note(id, 0, 3);
        assert_eq!(section.note(id).unwrap().length_ticks, 3);

        let short_id = section.add_note(60, PPQ, 3, Instrument::Piano);
        assert_eq!(section.note(short_id).unwrap().length_ticks, 3);

        section.resize_note(id, 0, 0);
        assert_eq!(section.note(id).unwrap().length_ticks, MIN_NOTE_TICKS);
    }

    #[test]
    fn move_notes_by_row_stays_on_a_visible_scale_row() {
        // C Major rows near middle C: ... A3(57) B3(59) C4(60) D4(62) E4(64) ...
        let visible_rows: Vec<u8> = vec![64, 62, 60, 59, 57]; // high-to-low, as drawn
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(4));
        let id = section.add_note(60, 0, PPQ, Instrument::Piano); // C4, row index 2

        // Moving up (positive = higher pitch) one row should land exactly
        // on D4 (62), not C4+1=61 (which isn't in the scale and would
        // otherwise vanish from the roll).
        section.move_notes_by_row(&[id], 0, 1, &visible_rows);
        assert_eq!(section.note(id).unwrap().pitch, 62);

        // Moving down two rows from there lands on B3 (59).
        section.move_notes_by_row(&[id], 0, -2, &visible_rows);
        assert_eq!(section.note(id).unwrap().pitch, 59);

        // Clamped at the edges of the visible range instead of leaving it.
        section.move_notes_by_row(&[id], 0, -100, &visible_rows);
        assert_eq!(section.note(id).unwrap().pitch, 57);
    }

    #[test]
    fn move_notes_by_row_leaves_off_scale_pitch_untouched() {
        let visible_rows: Vec<u8> = vec![64, 62, 60, 59, 57];
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(4));
        let id = section.add_note(61, 0, PPQ, Instrument::Piano); // not in visible_rows
        section.move_notes_by_row(&[id], PPQ as i64, -1, &visible_rows);
        assert_eq!(section.note(id).unwrap().pitch, 61);
        assert_eq!(section.note(id).unwrap().start_tick, PPQ);
    }

    #[test]
    fn duplicate_notes_get_fresh_ids_and_offset() {
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(4));
        let id = section.add_note(60, 0, PPQ, Instrument::Piano);
        let new_ids = section.duplicate_notes(&[id], PPQ as i64);
        assert_eq!(new_ids.len(), 1);
        assert_ne!(new_ids[0], id);
        assert_eq!(section.note(new_ids[0]).unwrap().start_tick, PPQ);
        assert_eq!(section.notes.len(), 2);
    }

    #[test]
    fn gain_pan_adjust_clamp_and_reset() {
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(4));
        let id = section.add_note(60, 0, PPQ, Instrument::Piano);
        section.adjust_gain(&[id], -2.0);
        assert_eq!(section.note(id).unwrap().gain, 0.0);
        section.adjust_pan(&[id], 500);
        assert_eq!(section.note(id).unwrap().pan, 100);
        section.reset_gain_pan(&[id]);
        assert_eq!(section.note(id).unwrap().gain, 1.0);
        assert_eq!(section.note(id).unwrap().pan, 0);
    }

    #[test]
    fn ticks_to_samples_scales_with_bpm() {
        // At 120 BPM, one quarter note (PPQ ticks) is 0.5s.
        let samples = ticks_to_samples(PPQ, 120.0, 48_000);
        assert_eq!(samples, 24_000);
        // At 60 BPM, one quarter note is 1s.
        let samples = ticks_to_samples(PPQ, 60.0, 48_000);
        assert_eq!(samples, 48_000);
    }

    #[test]
    fn render_section_length_and_silence() {
        let section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(1));
        let buf = render_section(&section, 120.0, 48_000);
        let expected_frames = ticks_to_samples(bars_to_ticks(1), 120.0, 48_000) as usize;
        assert_eq!(buf.len(), expected_frames * 2);
        assert!(buf.iter().all(|&s| s == 0.0), "empty section should render silence");
    }

    #[test]
    fn render_section_with_note_is_not_silent() {
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(1));
        section.add_note(60, 0, PPQ, Instrument::Piano);
        let buf = render_section(&section, 120.0, 48_000);
        assert!(buf.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn used_instruments_lists_only_instruments_with_notes() {
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(1));
        section.add_note(60, 0, PPQ, Instrument::Piano);
        section.add_note(40, PPQ, PPQ, Instrument::Drum);
        let used = section.used_instruments();
        assert_eq!(used, vec![Instrument::Piano, Instrument::Drum]);

        let mut melody = Melody::new(0, "M".into());
        let melody_section_id = melody.sections[0].id;
        *melody.section_mut(melody_section_id).unwrap() = section;
        assert_eq!(melody.used_instruments(), vec![Instrument::Piano, Instrument::Drum]);
    }

    #[test]
    fn is_instrument_audible_respects_mute_and_solo() {
        let mut melody = Melody::new(0, "M".into());
        assert!(melody.is_instrument_audible(Instrument::Drum));

        melody.muted_instruments.insert(Instrument::Drum);
        assert!(!melody.is_instrument_audible(Instrument::Drum));
        assert!(melody.is_instrument_audible(Instrument::Piano));

        // Soloing Piano overrides Drum's mute state entirely: only the
        // soloed instrument is audible, regardless of what's muted.
        melody.soloed_instruments.insert(Instrument::Piano);
        assert!(melody.is_instrument_audible(Instrument::Piano));
        assert!(!melody.is_instrument_audible(Instrument::Guitar));
    }

    #[test]
    fn render_section_filtered_excludes_unwanted_instruments() {
        let mut section = Section::new(0, "S".into(), 0, 0, bars_to_ticks(1));
        section.add_note(60, 0, PPQ, Instrument::Piano);
        section.add_note(60, 0, PPQ, Instrument::Drum);

        let piano_only = render_section_filtered(&section, 120.0, 48_000, |i| i == Instrument::Piano);
        let piano_alone = {
            let mut s = Section::new(0, "S".into(), 0, 0, bars_to_ticks(1));
            s.add_note(60, 0, PPQ, Instrument::Piano);
            render_section(&s, 120.0, 48_000)
        };
        assert_eq!(piano_only, piano_alone);

        let none = render_section_filtered(&section, 120.0, 48_000, |_| false);
        assert!(none.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn render_melody_concatenates_sections() {
        let mut melody = Melody::new(0, "M".into());
        melody.sections[0].length_ticks = bars_to_ticks(1);
        let second = melody.add_section("Section 2".into(), 0, 0, bars_to_ticks(1));
        melody.section_mut(second).unwrap().length_ticks = bars_to_ticks(1);
        let buf = render_melody(&melody, 48_000);
        let one_bar_frames = ticks_to_samples(bars_to_ticks(1), melody.bpm, 48_000) as usize;
        assert_eq!(buf.len(), one_bar_frames * 2 * 2);
    }
}
