//! The note/section/melody data model — plain data plus pure mutators, with
//! no egui dependency, so it's fully headless-testable the same way
//! `project::Project`'s mutators are (see `tests/bethoven_tests.rs`).

use super::instrument::{render_note, Instrument};
use crate::audio_engine::mix::pan_gains;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Ticks per quarter note — fine enough grain for 16th-note snapping
/// without needing floating point tick positions.
pub const PPQ: u32 = 96;
pub const BEATS_PER_BAR: u32 = 4;
/// Grid/snap resolution: a 16th note.
pub const GRID_TICKS: u32 = PPQ / 4;
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
            length_ticks: length_ticks.max(GRID_TICKS),
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

    /// Resizes one note by moving its start and/or end independently
    /// (dragging the left edge changes `start_tick` while keeping the same
    /// end; dragging the right edge only changes the length) — callers pass
    /// the new absolute start/length, already snapped to the grid.
    pub fn resize_note(&mut self, id: u32, new_start_tick: u32, new_length_ticks: u32) {
        if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
            note.start_tick = new_start_tick;
            note.length_ticks = new_length_ticks.max(GRID_TICKS);
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
        };
        melody.add_section("Section 1".to_string(), 0, 0, bars_to_ticks(DEFAULT_SECTION_BARS));
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
}

/// Renders every note in `section` into one stereo (interleaved) buffer
/// spanning exactly `section.length_ticks` at `bpm`. Notes are placed at
/// their tick offset and panned via `pan_gains` (the same constant-power law
/// the main mixer uses for mono tracks); anything hanging past the
/// section's end is simply not written.
pub fn render_section(section: &Section, bpm: f32, sample_rate_hz: u32) -> Vec<f32> {
    let total_samples = ticks_to_samples(section.length_ticks, bpm, sample_rate_hz) as usize;
    let mut buf = vec![0.0f32; total_samples * 2];
    for note in &section.notes {
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
/// order — used both by "Export to Project Track" and (for the active
/// section alone) Bethoven's own preview playback.
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
