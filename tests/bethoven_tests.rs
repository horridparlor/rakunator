//! Black-box tests for the Bethoven piano-roll composer's data model,
//! exercised through the crate's public API (mirrors `project_tests.rs`'s
//! and `persistence_tests.rs`'s headless style — no GUI/audio device).
//! Most unit-level coverage (scale tables, note mutators, instrument
//! synthesis) lives alongside the code in `#[cfg(test)]` modules under
//! `src/bethoven/`; this file covers what only the public API can exercise
//! end-to-end, chiefly `.raku` persistence.

use rakunator::bethoven::melody::{self, PPQ};
use rakunator::bethoven::scales;
use rakunator::bethoven::{Instrument, Melody};
use rakunator::project::persistence::{load_project, save_project};
use rakunator::project::Project;

#[test]
fn scale_table_has_43_types_and_valid_pitch_classes() {
    assert_eq!(scales::SCALES.len(), 43);
    for index in 0..scales::SCALES.len() {
        for root in 0..12u8 {
            let pcs = scales::pitch_classes(root, index);
            // The root itself is always a member of its own scale.
            assert!(pcs[root as usize], "root {root} missing from scale {index}");
        }
    }
}

#[test]
fn melody_starts_with_one_section_and_sane_defaults() {
    let melody = Melody::new(0, "Test".to_string());
    assert_eq!(melody.sections.len(), 1);
    assert_eq!(melody.default_note_length_ticks, PPQ);
    assert_eq!(melody.default_instrument, Instrument::Piano);
    assert_eq!(melody.bpm, 120.0);
}

#[test]
fn project_round_trips_melodies_through_raku_file() {
    let mut project = Project::new(48_000);
    let mut melody = Melody::new(project.next_melody_id(), "My Song".to_string());
    melody.bpm = 140.0;
    let section_id = melody.sections[0].id;
    let section = melody.section_mut(section_id).unwrap();
    let note_id = section.add_note(64, 0, PPQ, Instrument::Guitar);
    section.adjust_gain(&[note_id], -0.3);
    section.adjust_pan(&[note_id], -25);
    project.melodies.push(melody);

    let path = std::env::temp_dir().join(format!("rakunator_bethoven_test_{}.raku", std::process::id()));
    save_project(&project, &path).expect("save should succeed");

    let loaded = load_project(&path).expect("load should succeed");
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.melodies.len(), 1);
    let loaded_melody = &loaded.melodies[0];
    assert_eq!(loaded_melody.name, "My Song");
    assert_eq!(loaded_melody.bpm, 140.0);
    assert_eq!(loaded_melody.sections.len(), 1);
    let loaded_section = &loaded_melody.sections[0];
    assert_eq!(loaded_section.notes.len(), 1);
    let loaded_note = &loaded_section.notes[0];
    assert_eq!(loaded_note.pitch, 64);
    assert_eq!(loaded_note.instrument, Instrument::Guitar);
    assert!((loaded_note.gain - 0.7).abs() < 1e-6);
    assert_eq!(loaded_note.pan, -25);
}

#[test]
fn loading_an_old_raku_file_with_no_melodies_field_still_works() {
    // Simulates a `.raku` file saved before Bethoven existed: no "melodies"
    // key at all in the JSON.
    let path = std::env::temp_dir().join(format!("rakunator_bethoven_legacy_{}.raku", std::process::id()));
    let legacy_json = r#"{"sample_rate_hz":48000,"tracks":[]}"#;
    std::fs::write(&path, legacy_json).expect("write should succeed");

    let loaded = load_project(&path).expect("load should succeed even without a melodies field");
    let _ = std::fs::remove_file(&path);

    assert!(loaded.melodies.is_empty());
    assert_eq!(loaded.last_melody_id, None);
}

#[test]
fn project_round_trips_which_melody_and_section_were_last_open() {
    let mut project = Project::new(48_000);
    let melody_a = Melody::new(project.next_melody_id(), "A".to_string());
    project.melodies.push(melody_a);

    let mut melody_b = Melody::new(project.next_melody_id(), "B".to_string());
    let second_section = melody_b.add_section("Second".to_string(), 0, 0, melody::bars_to_ticks(4));
    melody_b.last_section_id = Some(second_section);
    let melody_b_id = melody_b.id;
    project.melodies.push(melody_b);
    project.last_melody_id = Some(melody_b_id);

    let path = std::env::temp_dir().join(format!("rakunator_bethoven_last_open_{}.raku", std::process::id()));
    save_project(&project, &path).expect("save should succeed");
    let loaded = load_project(&path).expect("load should succeed");
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.last_melody_id, Some(melody_b_id));
    let loaded_b = loaded.melodies.iter().find(|m| m.id == melody_b_id).unwrap();
    assert_eq!(loaded_b.last_section_id, Some(second_section));
}

#[test]
fn project_round_trips_muted_and_soloed_instruments() {
    let mut project = Project::new(48_000);
    let mut melody = Melody::new(project.next_melody_id(), "M".to_string());
    melody.muted_instruments.insert(Instrument::Drum);
    melody.soloed_instruments.insert(Instrument::Piano);
    let melody_id = melody.id;
    project.melodies.push(melody);

    let path = std::env::temp_dir().join(format!("rakunator_bethoven_mute_solo_{}.raku", std::process::id()));
    save_project(&project, &path).expect("save should succeed");
    let loaded = load_project(&path).expect("load should succeed");
    let _ = std::fs::remove_file(&path);

    let loaded_melody = loaded.melodies.iter().find(|m| m.id == melody_id).unwrap();
    assert!(loaded_melody.muted_instruments.contains(&Instrument::Drum));
    assert!(loaded_melody.soloed_instruments.contains(&Instrument::Piano));
}

#[test]
fn loading_an_old_raku_file_with_no_mute_solo_fields_still_works() {
    // Simulates a melody saved before mute/solo existed: no
    // "muted_instruments"/"soloed_instruments" keys in the JSON.
    let path = std::env::temp_dir().join(format!("rakunator_bethoven_legacy_mute_solo_{}.raku", std::process::id()));
    let legacy_json = r#"{"sample_rate_hz":48000,"tracks":[],"melodies":[{"id":0,"name":"M","bpm":120.0,"sections":[],"default_note_length_ticks":96,"default_instrument":"Piano"}]}"#;
    std::fs::write(&path, legacy_json).expect("write should succeed");

    let loaded = load_project(&path).expect("load should succeed even without mute/solo fields");
    let _ = std::fs::remove_file(&path);

    assert!(loaded.melodies[0].muted_instruments.is_empty());
    assert!(loaded.melodies[0].soloed_instruments.is_empty());
}

#[test]
fn export_to_project_track_produces_a_playable_clip() {
    let mut project = Project::new(48_000);
    let mut melody = Melody::new(0, "Export Me".to_string());
    let section_id = melody.sections[0].id;
    melody.section_mut(section_id).unwrap().length_ticks = melody::bars_to_ticks(1);
    melody.section_mut(section_id).unwrap().add_note(60, 0, PPQ, Instrument::Bass);

    let samples = melody::render_melody(&melody, project.sample_rate_hz);
    assert!(!samples.is_empty());
    assert!(samples.iter().any(|&s| s != 0.0));

    let track = project.add_track();
    let clip_id = project.add_clip_channels(track, melody.name.clone(), 0, samples, 2).unwrap();
    let added_clip = project.track(track).unwrap().clips.iter().find(|c| c.id == clip_id).unwrap();
    assert!(added_clip.len_samples() > 0);
}
