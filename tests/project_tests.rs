//! Integration tests for the core `project` editing model: tracks, clips,
//! cut/copy/paste/duplicate/split/join/trim, effects, and undo/redo.
//! Deliberately does not touch `audio_engine::AudioEngine` (opens a real
//! output device) or `gui` (needs a window) — those aren't hermetic/CI-safe.

use rakunator::project::Project;

fn project_with_one_track() -> Project {
    Project::new(48_000)
}

#[test]
fn new_project_starts_with_one_track() {
    let project = project_with_one_track();
    assert_eq!(project.tracks.len(), 1);
    assert_eq!(project.tracks[0].name, "Track 1");
}

#[test]
fn add_track_appends_and_names_sequentially() {
    let mut project = project_with_one_track();
    let second = project.add_track();
    assert_eq!(project.tracks.len(), 2);
    assert_eq!(project.track(second).unwrap().name, "Track 2");
}

#[test]
fn remove_track_drops_it() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.remove_track(track);
    assert!(project.tracks.is_empty());
}

#[test]
fn move_track_reorders_the_list() {
    let mut project = project_with_one_track();
    let first = project.tracks[0].id;
    let second = project.add_track();
    project.move_track(first, 1);
    assert_eq!(project.tracks[0].id, second);
    assert_eq!(project.tracks[1].id, first);
}

#[test]
fn move_track_to_top_and_bottom() {
    let mut project = project_with_one_track();
    let first = project.tracks[0].id;
    let second = project.add_track();
    let third = project.add_track();

    project.move_track_to_top(third);
    assert_eq!(project.tracks[0].id, third);

    project.move_track_to_bottom(third);
    assert_eq!(project.tracks.last().unwrap().id, third);
    assert_eq!(project.tracks[0].id, first);
    assert_eq!(project.tracks[1].id, second);
}

#[test]
fn duplicate_track_copies_clips_with_fresh_ids() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();

    let dup_track = project.duplicate_track(track).unwrap();

    assert_ne!(dup_track, track);
    let dup_clip = project.track(dup_track).unwrap().clips[0].id;
    assert_ne!(dup_clip, clip_id);
}

#[test]
fn add_clip_reports_correct_position_and_length() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 5, vec![0.0; 100]).unwrap();

    let clip = project.track(track).unwrap().clips.iter().find(|c| c.id == clip_id).unwrap();
    assert_eq!(clip.start_sample, 5);
    assert_eq!(clip.len_samples(), 100);
    assert_eq!(clip.end_sample(), 105);
}

#[test]
fn move_clip_updates_position_and_can_change_track() {
    let mut project = project_with_one_track();
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    let clip_id = project.add_clip(track_a, "c".into(), 0, vec![1.0; 10]).unwrap();

    project.move_clip(clip_id, track_b, 50);

    assert!(project.track(track_a).unwrap().clips.is_empty());
    let moved = &project.track(track_b).unwrap().clips[0];
    assert_eq!(moved.start_sample, 50);
}

#[test]
fn split_clip_produces_two_contiguous_pieces() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();

    let (first, second) = project.split_clip(clip_id, 40).unwrap();

    let clips = &project.track(track).unwrap().clips;
    let c1 = clips.iter().find(|c| c.id == first).unwrap();
    let c2 = clips.iter().find(|c| c.id == second).unwrap();
    assert_eq!(c1.start_sample, 0);
    assert_eq!(c1.len_samples(), 40);
    assert_eq!(c2.start_sample, 40);
    assert_eq!(c2.len_samples(), 60);
}

#[test]
fn split_clip_rejects_points_at_or_outside_its_edges() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();

    assert!(project.split_clip(clip_id, 0).is_none());
    assert!(project.split_clip(clip_id, 100).is_none());
    assert!(project.split_clip(clip_id, 200).is_none());
}

#[test]
fn cut_then_paste_round_trips_a_clip() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 10, vec![1.0; 20]).unwrap();

    project.cut_clips(&[clip_id]);
    assert!(project.track(track).unwrap().clips.is_empty());

    let pasted = project.paste(track, 100);
    assert_eq!(pasted.len(), 1);
    let clip = &project.track(track).unwrap().clips[0];
    assert_eq!(clip.start_sample, 100);
    assert_eq!(clip.len_samples(), 20);
}

#[test]
fn copy_paste_preserves_relative_layout_across_tracks() {
    let mut project = project_with_one_track();
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    let clip_a = project.add_clip(track_a, "a".into(), 0, vec![1.0; 10]).unwrap();
    let clip_b = project.add_clip(track_b, "b".into(), 20, vec![1.0; 10]).unwrap();

    project.copy_clips(&[clip_a, clip_b]);
    let new_ids = project.paste(track_a, 100);

    assert_eq!(new_ids.len(), 2);
    let mut starts: Vec<u64> = project
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter(|c| new_ids.contains(&c.id))
        .map(|c| c.start_sample)
        .collect();
    starts.sort();
    // Anchor clip lands at 100; the other keeps its +20 relative offset.
    assert_eq!(starts, vec![100, 120]);
}

#[test]
fn duplicate_clip_creates_an_independent_copy() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();

    let dup_id = project.duplicate_clip(clip_id, track, 50).unwrap();

    assert_ne!(dup_id, clip_id);
    assert_eq!(project.track(track).unwrap().clips.len(), 2);
    let dup = project.track(track).unwrap().clips.iter().find(|c| c.id == dup_id).unwrap();
    assert_eq!(dup.start_sample, 50);
}

#[test]
fn duplicate_selection_duplicates_every_clip_once() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let a = project.add_clip(track, "a".into(), 0, vec![1.0; 10]).unwrap();
    let b = project.add_clip(track, "b".into(), 100, vec![1.0; 10]).unwrap();

    project.duplicate_selection(&[a, b]);

    assert_eq!(project.track(track).unwrap().clips.len(), 4);
}

#[test]
fn join_clips_spans_from_earliest_start_to_latest_end() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let a = project.add_clip(track, "a".into(), 0, vec![1.0; 10]).unwrap();
    let b = project.add_clip(track, "b".into(), 20, vec![0.5; 10]).unwrap();

    let new_ids = project.join_clips(&[a, b]);

    assert_eq!(new_ids.len(), 1);
    let clips = &project.track(track).unwrap().clips;
    assert_eq!(clips.len(), 1);
    let joined = &clips[0];
    assert_eq!(joined.start_sample, 0);
    assert_eq!(joined.len_samples(), 30);
}

#[test]
fn join_clips_does_nothing_for_a_single_clip() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let a = project.add_clip(track, "a".into(), 0, vec![1.0; 10]).unwrap();

    let new_ids = project.join_clips(&[a]);

    assert!(new_ids.is_empty());
    assert_eq!(project.track(track).unwrap().clips.len(), 1);
}

#[test]
fn trim_start_shortens_and_advances_the_start() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 100, vec![1.0; 100]).unwrap();

    project.trim_clip_start(clip_id, 20);

    let clip = &project.track(track).unwrap().clips[0];
    assert_eq!(clip.start_sample, 120);
    assert_eq!(clip.len_samples(), 80);
}

#[test]
fn trim_end_shortens_from_the_right() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();

    project.trim_clip_end(clip_id, 20);

    let clip = &project.track(track).unwrap().clips[0];
    assert_eq!(clip.start_sample, 0);
    assert_eq!(clip.len_samples(), 80);
}

#[test]
fn trimmed_audio_can_be_dragged_back_out() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();

    project.trim_clip_start(clip_id, 20);
    project.trim_clip_start(clip_id, -20);

    let clip = &project.track(track).unwrap().clips[0];
    assert_eq!(clip.start_sample, 0);
    assert_eq!(clip.len_samples(), 100);
}

#[test]
fn mute_range_silences_only_the_given_span() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();

    project.mute_range(clip_id, 3, 6);

    let clip = &project.track(track).unwrap().clips[0];
    let samples = clip.visible_samples();
    assert_eq!(samples[0..3], [1.0, 1.0, 1.0]);
    assert_eq!(samples[3..6], [0.0, 0.0, 0.0]);
    assert_eq!(samples[6..10], [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn apply_gain_scales_and_clamps_samples() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![0.5; 4]).unwrap();

    project.apply_gain(clip_id, 3.0); // 0.5 * 3.0 = 1.5, should clamp to 1.0

    let clip = &project.track(track).unwrap().clips[0];
    for &s in clip.visible_samples() {
        assert!((s - 1.0).abs() < 1e-6);
    }
}

#[test]
fn fade_in_ramps_from_silence_to_full_and_is_monotonic() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 5]).unwrap();

    project.apply_fade_in(clip_id);

    let clip = &project.track(track).unwrap().clips[0];
    let samples = clip.visible_samples();
    assert!(samples[0].abs() < 1e-6);
    assert!((samples[4] - 1.0).abs() < 1e-6);
    for pair in samples.windows(2) {
        assert!(pair[1] + 1e-6 >= pair[0]);
    }
}

#[test]
fn fade_out_ramps_from_full_to_silence() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 5]).unwrap();

    project.apply_fade_out(clip_id);

    let clip = &project.track(track).unwrap().clips[0];
    let samples = clip.visible_samples();
    assert!((samples[0] - 1.0).abs() < 1e-6);
    assert!(samples[4].abs() < 1e-6);
}

#[test]
fn ranged_fade_only_touches_the_given_span_using_the_overall_ramp() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();

    // Fade covers samples [0, 10) overall, but we only apply it to [5, 10)
    // here — those samples should land in the back half of the ramp
    // (>= 0.5), and the untouched front half should stay at full volume.
    project.apply_fade_in_range(clip_id, 5, 10, 0, 10);

    let clip = &project.track(track).unwrap().clips[0];
    let samples = clip.visible_samples();
    for &s in &samples[0..5] {
        assert!((s - 1.0).abs() < 1e-6);
    }
    for &s in &samples[5..10] {
        assert!(s >= 0.5 - 1e-6);
    }
}

#[test]
fn undo_reverts_the_last_edit_and_redo_reapplies_it() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();
    assert_eq!(project.track(track).unwrap().clips.len(), 1);

    project.undo();
    assert_eq!(project.track(track).unwrap().clips.len(), 0);

    project.redo();
    assert_eq!(project.track(track).unwrap().clips.len(), 1);
}

#[test]
fn undo_with_nothing_to_undo_is_a_no_op() {
    let mut project = project_with_one_track();
    let track_count_before = project.tracks.len();
    project.undo();
    assert_eq!(project.tracks.len(), track_count_before);
}

#[test]
fn effect_targets_prefers_selected_tracks_over_clip_selection() {
    let mut project = project_with_one_track();
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    let clip_a = project.add_clip(track_a, "a".into(), 0, vec![1.0; 10]).unwrap();
    let clip_b = project.add_clip(track_b, "b".into(), 0, vec![1.0; 10]).unwrap();

    project.selection.insert(clip_a);
    assert_eq!(project.effect_targets(), vec![clip_a]);

    project.selected_tracks.insert(track_b);
    assert_eq!(project.effect_targets(), vec![clip_b]);
}

#[test]
fn effect_targets_can_span_multiple_selected_tracks() {
    let mut project = project_with_one_track();
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    let clip_a = project.add_clip(track_a, "a".into(), 0, vec![1.0; 10]).unwrap();
    let clip_b = project.add_clip(track_b, "b".into(), 0, vec![1.0; 10]).unwrap();

    project.selected_tracks.insert(track_a);
    project.selected_tracks.insert(track_b);

    let mut targets = project.effect_targets();
    targets.sort_by_key(|id| id.0);
    let mut expected = vec![clip_a, clip_b];
    expected.sort_by_key(|id| id.0);
    assert_eq!(targets, expected);
}

#[test]
fn cloning_a_project_does_not_carry_over_undo_history() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();

    let mut cloned = project.clone();
    // The clone has no undo history of its own, so undo is a no-op even
    // though the original project could still undo the add_clip above.
    let clips_before = cloned.track(track).unwrap().clips.len();
    cloned.undo();
    assert_eq!(cloned.track(track).unwrap().clips.len(), clips_before);
}

#[test]
fn add_clip_channels_sets_track_channel_count_from_first_clip() {
    let mut project = project_with_one_track();
    let track_id = project.tracks[0].id;
    project.add_clip_channels(track_id, "stereo".into(), 0, vec![0.1, 0.2, 0.3, 0.4], 2);

    let track = project.track(track_id).unwrap();
    assert_eq!(track.channels, 2);
    assert_eq!(track.clips[0].channels(), 2);
    assert_eq!(track.clips[0].len_samples(), 2);
}

#[test]
fn add_clip_channels_converts_mismatched_input_to_the_track_s_existing_channel_count() {
    let mut project = project_with_one_track();
    let track_id = project.tracks[0].id;
    // First clip fixes the track at mono.
    project.add_clip_channels(track_id, "mono".into(), 0, vec![1.0; 4], 1);
    // A stereo clip added afterwards is downmixed to match.
    project.add_clip_channels(track_id, "stereo".into(), 10, vec![1.0, 0.0, 1.0, 0.0], 2);

    let track = project.track(track_id).unwrap();
    assert_eq!(track.channels, 1);
    let second = &track.clips[1];
    assert_eq!(second.channels(), 1);
    assert_eq!(second.visible_samples(), &[0.5, 0.5]);
}

#[test]
fn effects_preserve_a_clip_s_channel_count() {
    let mut project = project_with_one_track();
    let track_id = project.tracks[0].id;
    let id = project
        .add_clip_channels(track_id, "stereo".into(), 0, vec![0.5, -0.5, 0.5, -0.5], 2)
        .unwrap();

    project.apply_gain(id, 0.5);
    assert_eq!(project.track(track_id).unwrap().clips[0].channels(), 2);

    project.apply_fade_in(id);
    assert_eq!(project.track(track_id).unwrap().clips[0].channels(), 2);

    project.mute_range(id, 0, 1);
    assert_eq!(project.track(track_id).unwrap().clips[0].channels(), 2);
}

#[test]
fn split_track_to_mono_creates_two_mono_tracks_and_removes_the_original() {
    let mut project = project_with_one_track();
    let track_id = project.tracks[0].id;
    project.add_clip_channels(track_id, "stereo".into(), 0, vec![0.4, -0.2, 0.4, -0.2], 2);

    let (left_id, right_id) = project.split_track_to_mono(track_id).unwrap();

    assert!(project.track(track_id).is_none());
    let left = project.track(left_id).unwrap();
    let right = project.track(right_id).unwrap();
    assert_eq!(left.channels, 1);
    assert_eq!(right.channels, 1);
    assert_eq!(left.clips[0].visible_samples(), &[0.4, 0.4]);
    assert_eq!(right.clips[0].visible_samples(), &[-0.2, -0.2]);
}

#[test]
fn merge_track_with_below_creates_one_stereo_track() {
    let mut project = project_with_one_track();
    let top_id = project.tracks[0].id;
    let bottom_id = project.add_track();
    project.add_clip(top_id, "l".into(), 0, vec![0.6, 0.6]).unwrap();
    project.add_clip(bottom_id, "r".into(), 0, vec![-0.3, -0.3]).unwrap();

    let merged_id = project.merge_track_with_below(top_id).unwrap();

    assert!(project.track(top_id).is_none());
    assert!(project.track(bottom_id).is_none());
    let merged = project.track(merged_id).unwrap();
    assert_eq!(merged.channels, 2);
    assert_eq!(merged.clips[0].channels(), 2);
    assert_eq!(merged.clips[0].visible_samples(), &[0.6, -0.3, 0.6, -0.3]);
}
