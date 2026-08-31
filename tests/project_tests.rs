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
fn select_range_carves_out_the_middle_of_a_clip() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();

    project.select_range(&[track], 30, 70);

    let clips = &project.track(track).unwrap().clips;
    assert_eq!(clips.len(), 3);
    assert_eq!(project.selection.len(), 1);
    let selected_id = *project.selection.iter().next().unwrap();
    let selected = clips.iter().find(|c| c.id == selected_id).unwrap();
    assert_eq!(selected.start_sample, 30);
    assert_eq!(selected.len_samples(), 40);
}

#[test]
fn select_range_keeps_a_fully_covered_clip_whole() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 10, vec![1.0; 20]).unwrap();

    project.select_range(&[track], 0, 100);

    let clips = &project.track(track).unwrap().clips;
    assert_eq!(clips.len(), 1);
    assert_eq!(project.selection, std::collections::HashSet::from([clip_id]));
}

#[test]
fn select_range_spans_every_given_track() {
    let mut project = project_with_one_track();
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    project.add_clip(track_a, "a".into(), 0, vec![1.0; 100]).unwrap();
    project.add_clip(track_b, "b".into(), 0, vec![1.0; 100]).unwrap();

    project.select_range(&[track_a, track_b], 20, 80);

    assert_eq!(project.selection.len(), 2);
    for id in &project.selection {
        let track_id = project.find_clip_track(*id).unwrap();
        let clip = project.track(track_id).unwrap().clips.iter().find(|c| c.id == *id).unwrap();
        assert_eq!(clip.start_sample, 20);
        assert_eq!(clip.len_samples(), 60);
    }
}

#[test]
fn select_range_ignores_tracks_outside_the_given_list() {
    let mut project = project_with_one_track();
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    project.add_clip(track_b, "b".into(), 0, vec![1.0; 100]).unwrap();

    project.select_range(&[track_a], 0, 100);

    assert!(project.selection.is_empty());
    assert_eq!(project.track(track_b).unwrap().clips.len(), 1);
}

#[test]
fn select_range_undoes_as_a_single_step() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();

    project.select_range(&[track], 30, 70);
    assert_eq!(project.track(track).unwrap().clips.len(), 3);

    project.undo();

    let clips = &project.track(track).unwrap().clips;
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].start_sample, 0);
    assert_eq!(clips[0].len_samples(), 100);
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
fn cut_clips_drops_the_cut_ids_from_selection() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();
    project.selection.insert(clip_id);

    project.cut_clips(&[clip_id]);

    assert!(project.selection.is_empty());
}

#[test]
fn delete_clips_drops_the_deleted_ids_from_selection() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 10]).unwrap();
    project.selection.insert(clip_id);

    project.delete_clips(&[clip_id]);

    assert!(project.selection.is_empty());
}

#[test]
fn splitting_a_selected_clip_hands_the_selection_to_both_halves() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![1.0; 100]).unwrap();
    project.selection.insert(clip_id);

    let (first, second) = project.split_clip(clip_id, 40).unwrap();

    assert_eq!(project.selection, std::collections::HashSet::from([first, second]));
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
    project.track_mut(track).unwrap().channels = 1;
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
    project.track_mut(track).unwrap().channels = 1;
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
    project.track_mut(track).unwrap().channels = 1;
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
    // Force the track to mono (tracks default to stereo) so a stereo clip
    // added afterwards has to be downmixed to match.
    project.track_mut(track_id).unwrap().channels = 1;
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
    // Tracks default to stereo; force both to mono, as merge requires.
    project.track_mut(top_id).unwrap().channels = 1;
    project.track_mut(bottom_id).unwrap().channels = 1;
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

fn sine_samples(sample_rate: u32, frames: usize, freq: f32) -> Vec<f32> {
    (0..frames)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
        .collect()
}

#[test]
fn invert_flips_the_sign_of_every_sample() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![0.5, -0.25, 0.0]).unwrap();

    project.apply_invert(clip_id);

    let samples = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(samples, vec![-0.5, 0.25, 0.0]);
}

#[test]
fn reverse_flips_frame_order_keeping_channels_together() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 2;
    let clip_id = project.add_clip_channels(track, "c".into(), 0, vec![1.0, -1.0, 2.0, -2.0, 3.0, -3.0], 2).unwrap();

    project.apply_reverse(clip_id);

    let samples = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(samples, vec![3.0, -3.0, 2.0, -2.0, 1.0, -1.0]);
}

#[test]
fn swap_channels_swaps_left_and_right() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    let clip_id = project.add_clip_channels(track, "c".into(), 0, vec![0.1, 0.9, 0.2, 0.8], 2).unwrap();

    project.apply_swap_channels(clip_id);

    let samples = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(samples, vec![0.9, 0.1, 0.8, 0.2]);
}

#[test]
fn swap_channels_is_a_no_op_on_mono() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![0.1, 0.2, 0.3]).unwrap();

    project.apply_swap_channels(clip_id);

    let samples = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(samples, vec![0.1, 0.2, 0.3]);
}

#[test]
fn echo_adds_a_delayed_decayed_repeat() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let mut samples = vec![0.0f32; 10];
    samples[0] = 1.0;
    let clip_id = project.add_clip(track, "c".into(), 0, samples).unwrap();

    // Sample rate is 48_000, so a 1-sample delay needs an absurdly small
    // time — instead pick a delay in seconds that lands exactly on sample
    // index 3 at this project's sample rate.
    let delay_seconds = 3.0 / 48_000.0;
    project.apply_echo(clip_id, delay_seconds, 0.5);

    let out = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert!((out[0] - 1.0).abs() < 1e-6);
    assert!((out[3] - 0.5).abs() < 1e-6, "expected the decayed echo at sample 3, got {}", out[3]);
}

#[test]
fn hard_clip_distortion_clamps_to_the_threshold() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let clip_id = project.add_clip(track, "c".into(), 0, vec![0.9, -0.9, 0.1]).unwrap();

    project.apply_hard_clip_distortion(clip_id, 12.0, 0.5);

    let out = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert!((out[0] - 0.5).abs() < 1e-6, "loud positive sample should clip to the threshold, got {}", out[0]);
    assert!((out[1] - -0.5).abs() < 1e-6, "loud negative sample should clip to -threshold, got {}", out[1]);
    assert!(out[2].abs() <= 0.5 + 1e-6);
}

#[test]
fn tempo_shift_changes_clip_length_without_crashing() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let samples = sine_samples(48_000, 48_000 / 2, 220.0); // 0.5s
    let clip_id = project.add_clip(track, "c".into(), 0, samples).unwrap();
    let before_len = project.track(track).unwrap().clips[0].len_samples();

    project.apply_tempo_shift(clip_id, 50.0); // 50% faster -> shorter

    let after_len = project.track(track).unwrap().clips[0].len_samples();
    assert!(after_len < before_len, "speeding up tempo should shorten the clip: {before_len} -> {after_len}");
}

#[test]
fn sliding_stretch_ramps_tempo_and_pitch_without_crashing() {
    use rakunator::project::stretch::RampParams;

    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let samples = sine_samples(48_000, 48_000 / 2, 220.0);
    let clip_id = project.add_clip(track, "c".into(), 0, samples).unwrap();

    project.apply_sliding_stretch(
        clip_id,
        &RampParams {
            initial_tempo_percent: -20.0,
            final_tempo_percent: 20.0,
            initial_pitch_semitones: -2.0,
            final_pitch_semitones: 2.0,
        },
    );

    let clip = &project.track(track).unwrap().clips[0];
    assert!(clip.len_samples() > 0);
    assert!(clip.visible_samples().iter().all(|s| s.is_finite()));
}

#[test]
fn reverb_leaves_a_tail_after_the_clip_s_own_content_would_have_ended() {
    use rakunator::project::reverb::ReverbParams;

    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let mut samples = vec![0.0f32; 8000];
    samples[0] = 1.0;
    let clip_id = project.add_clip(track, "c".into(), 0, samples).unwrap();

    project.apply_reverb(clip_id, &ReverbParams::default());

    let out = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(out.len(), 8000);
    assert!(out[4000..8000].iter().any(|&s| s.abs() > 1e-4), "expected an audible reverb tail");
}

#[test]
fn give_to_speech_and_telephone_run_without_crashing() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let clip_a = project.add_clip(track, "a".into(), 0, sine_samples(48_000, 4096, 1000.0)).unwrap();
    let clip_b = project.add_clip(track, "b".into(), 5000, sine_samples(48_000, 4096, 1000.0)).unwrap();

    project.apply_give_to_speech(clip_a);
    project.apply_telephone(clip_b);

    assert!(project.track(track).unwrap().clips[0].visible_samples().iter().all(|s| s.is_finite()));
    assert!(project.track(track).unwrap().clips[1].visible_samples().iter().all(|s| s.is_finite()));
}

#[test]
fn autotune_runs_the_full_chain_without_crashing() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let samples = sine_samples(48_000, 48_000, 440.0); // 1s
    let clip_id = project.add_clip(track, "c".into(), 0, samples).unwrap();

    project.apply_autotune(clip_id);

    let out = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(out.len(), 48_000);
    assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
}

#[test]
fn rattle_builds_24_variants_and_joins_them_into_one_clip_after_the_original() {
    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;
    let samples = sine_samples(48_000, 4800, 220.0); // 0.1s
    let clip_id = project.add_clip(track, "c".into(), 0, samples).unwrap();

    let params = rakunator::project::RattleParams {
        pitch_up_semitones: 1.0,
        pitch_down_semitones: 1.0,
        tempo_x_percent: 10.0,
        tempo_y_percent: 10.0,
        fade_in_start_gain: 0.0,
        fade_in_end_gain: 1.0,
        stretch: rakunator::project::stretch::RampParams {
            initial_tempo_percent: 0.0,
            final_tempo_percent: 0.0,
            initial_pitch_semitones: 0.0,
            final_pitch_semitones: 0.0,
        },
        repeat_count: 24,
    };
    project.apply_rattle(clip_id, &params);

    let clips = &project.track(track).unwrap().clips;
    // The original clip, plus one joined clip built from the 24 variants.
    assert_eq!(clips.len(), 2);
    let original = clips.iter().find(|c| c.id == clip_id).unwrap();
    let joined = clips.iter().find(|c| c.id != clip_id).unwrap();
    assert!(joined.start_sample >= original.end_sample());
    assert!(joined.len_samples() > original.len_samples() * 20);
    assert!(joined.visible_samples().iter().all(|s| s.is_finite()));
}

#[test]
fn trip_toggler_runs_on_a_clip_and_stays_in_range() {
    use rakunator::project::trip_toggler::TripTogglerParams;

    let mut project = project_with_one_track();
    let track = project.tracks[0].id;
    project.track_mut(track).unwrap().channels = 1;

    // Two short decaying "hits" separated by silence, so there's a clear
    // low point in between for the effect to find.
    let mut samples = Vec::new();
    for _ in 0..2 {
        for i in 0..4800usize {
            let t = i as f32 / 48_000.0;
            let envelope = (-3.0 * i as f32 / 4800.0).exp();
            samples.push(envelope * (2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }
        samples.extend(std::iter::repeat_n(0.0, 4800));
    }
    let clip_id = project.add_clip(track, "c".into(), 0, samples.clone()).unwrap();

    project.apply_trip_toggler(clip_id, &TripTogglerParams::default());

    let out = project.track(track).unwrap().clips[0].visible_samples().to_vec();
    assert_eq!(out.len(), samples.len());
    assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    assert!(out.iter().zip(samples.iter()).any(|(a, b)| (a - b).abs() > 1e-6));
}
