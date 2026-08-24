//! Tests for `.raku` project save/load round-tripping.

use rakunator::project::persistence::{load_project, save_project};
use rakunator::project::Project;

#[test]
fn save_then_load_round_trips_tracks_and_clips() {
    let mut project = Project::new(48_000);
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().name = "Vocals".to_string();
    project.track_mut(track).unwrap().pan_percent = -20;
    project.add_clip(track, "take 1".into(), 100, vec![0.25, 0.5, -0.25, -0.5]).unwrap();

    let path = std::env::temp_dir().join(format!("rakunator_test_{}.raku", std::process::id()));
    save_project(&project, &path).expect("save should succeed");

    let loaded = load_project(&path).expect("load should succeed");
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.tracks.len(), 1);
    assert_eq!(loaded.tracks[0].name, "Vocals");
    assert_eq!(loaded.tracks[0].pan_percent, -20);
    assert_eq!(loaded.tracks[0].clips.len(), 1);
    let clip = &loaded.tracks[0].clips[0];
    assert_eq!(clip.start_sample, 100);
    assert_eq!(clip.visible_samples(), &[0.25, 0.5, -0.25, -0.5]);
}

#[test]
fn loading_a_project_does_not_leave_it_undoable_to_empty() {
    let mut project = Project::new(48_000);
    let track = project.tracks[0].id;
    project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();

    let path = std::env::temp_dir().join(format!("rakunator_test_undo_{}.raku", std::process::id()));
    save_project(&project, &path).expect("save should succeed");

    let mut loaded = load_project(&path).expect("load should succeed");
    let _ = std::fs::remove_file(&path);

    let clip_count_before = loaded.tracks[0].clips.len();
    loaded.undo();
    assert_eq!(loaded.tracks[0].clips.len(), clip_count_before);
}
