//! Tests for `audio_engine::mix`'s pure mixing/panning functions — the
//! same logic used by both realtime playback and offline export, so
//! getting this right matters a lot. No real audio device involved.

use rakunator::audio_engine::mix::{is_audible, mix_frame, pan_balance_gains, pan_gains, track_frame, track_frame_stereo};
use rakunator::project::Project;

#[test]
fn pan_gains_center_is_equal_power() {
    let (left, right) = pan_gains(0, 1.0);
    assert!((left - right).abs() < 1e-6);
    // Equal-power panning keeps left^2 + right^2 == volume^2 everywhere.
    assert!((left * left + right * right - 1.0).abs() < 1e-4);
}

#[test]
fn pan_gains_hard_left_and_hard_right() {
    let (left, right) = pan_gains(-100, 1.0);
    assert!(left > 0.99);
    assert!(right < 0.01);

    let (left, right) = pan_gains(100, 1.0);
    assert!(right > 0.99);
    assert!(left < 0.01);
}

#[test]
fn pan_gains_scale_with_volume() {
    let (left, right) = pan_gains(0, 0.5);
    assert!((left - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
    assert!((right - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
}

#[test]
fn mute_always_wins_over_solo() {
    let mut project = Project::new(48_000);
    let track_id = project.tracks[0].id;
    {
        let track = project.track_mut(track_id).unwrap();
        track.muted = true;
        track.soloed = true;
    }
    assert!(!is_audible(&project.tracks[0], true));
}

#[test]
fn solo_suppresses_non_soloed_tracks() {
    let mut project = Project::new(48_000);
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    project.track_mut(track_b).unwrap().soloed = true;

    let any_soloed = project.tracks.iter().any(|t| t.soloed);
    assert!(any_soloed);
    assert!(!is_audible(project.track(track_a).unwrap(), any_soloed));
    assert!(is_audible(project.track(track_b).unwrap(), any_soloed));
}

#[test]
fn with_no_solo_all_unmuted_tracks_are_audible() {
    let project = Project::new(48_000);
    assert!(is_audible(&project.tracks[0], false));
}

#[test]
fn track_frame_sums_overlapping_clips() {
    let mut project = Project::new(48_000);
    let track_id = project.tracks[0].id;
    project.add_clip(track_id, "a".into(), 0, vec![0.3; 10]).unwrap();
    project.add_clip(track_id, "b".into(), 0, vec![0.2; 10]).unwrap();

    let sample = track_frame(&project.tracks[0], 5);
    assert!((sample - 0.5).abs() < 1e-6);
}

#[test]
fn track_frame_is_zero_outside_any_clip() {
    let mut project = Project::new(48_000);
    let track_id = project.tracks[0].id;
    project.add_clip(track_id, "a".into(), 0, vec![1.0; 10]).unwrap();

    assert_eq!(track_frame(&project.tracks[0], 100), 0.0);
}

#[test]
fn mix_frame_excludes_muted_tracks() {
    let mut project = Project::new(48_000);
    let track_a = project.tracks[0].id;
    let track_b = project.add_track();
    // Force mono (tracks default to stereo) so center pan applies the
    // expected equal-power ~0.707 split checked below.
    project.track_mut(track_a).unwrap().channels = 1;
    project.add_clip(track_a, "a".into(), 0, vec![1.0; 10]).unwrap();
    project.add_clip(track_b, "b".into(), 0, vec![1.0; 10]).unwrap();
    project.track_mut(track_b).unwrap().muted = true;

    let (left, right) = mix_frame(&project.tracks, 0);
    // Only track A contributes; at center pan that's ~0.707 each side.
    assert!(left > 0.6 && left < 0.75);
    assert!(right > 0.6 && right < 0.75);
}

#[test]
fn mix_frame_is_silent_with_no_clips() {
    let project = Project::new(48_000);
    let (left, right) = mix_frame(&project.tracks, 0);
    assert_eq!((left, right), (0.0, 0.0));
}

#[test]
fn pan_balance_center_keeps_both_channels_at_full_volume() {
    let (left, right) = pan_balance_gains(0, 1.0);
    assert!((left - 1.0).abs() < 1e-6);
    assert!((right - 1.0).abs() < 1e-6);
}

#[test]
fn pan_balance_hard_right_silences_left_only() {
    let (left, right) = pan_balance_gains(100, 1.0);
    assert!(left < 1e-6);
    assert!((right - 1.0).abs() < 1e-6);
}

#[test]
fn track_frame_stereo_reads_left_and_right_independently() {
    let mut project = Project::new(48_000);
    let track_id = project.tracks[0].id;
    // Interleaved L,R pairs: left is always 0.5, right is always -0.25.
    let interleaved: Vec<f32> = (0..20).map(|i| if i % 2 == 0 { 0.5 } else { -0.25 }).collect();
    project.add_clip_channels(track_id, "stereo".into(), 0, interleaved, 2);

    let (left, right) = track_frame_stereo(&project.tracks[0], 5);
    assert!((left - 0.5).abs() < 1e-6);
    assert!((right - (-0.25)).abs() < 1e-6);
}

#[test]
fn mix_frame_uses_balance_pan_for_stereo_tracks() {
    let mut project = Project::new(48_000);
    let track_id = project.tracks[0].id;
    let interleaved: Vec<f32> = (0..20).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
    project.add_clip_channels(track_id, "stereo".into(), 0, interleaved, 2);
    project.track_mut(track_id).unwrap().pan_percent = 100; // hard right

    let (left, right) = mix_frame(&project.tracks, 0);
    // Panned hard right: left channel is silenced, right channel passes
    // through at full volume (unlike mono's constant-power pan law).
    assert!(left.abs() < 1e-6);
    assert!((right - (-1.0)).abs() < 1e-6);
}
