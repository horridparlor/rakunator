use crate::project::Track;
use std::f32::consts::FRAC_PI_4;

/// Whether `track` should be heard given whether any track in the project
/// is soloed. Mute always wins; if any track is soloed, only soloed tracks
/// are audible.
pub fn is_audible(track: &Track, any_soloed: bool) -> bool {
    if track.muted {
        return false;
    }
    if any_soloed && !track.soloed {
        return false;
    }
    true
}

/// Converts a mono track's pan (-100..=100) and linear volume into a
/// constant-power (left, right) gain pair. At pan 0 both channels get
/// volume * ~0.707 (equal power); at -100/+100 all power goes to one side.
pub fn pan_gains(pan_percent: i8, volume: f32) -> (f32, f32) {
    let pan = (pan_percent as f32 / 100.0).clamp(-1.0, 1.0);
    let theta = (pan + 1.0) * FRAC_PI_4; // maps -1..1 -> 0..PI/2
    (volume * theta.cos(), volume * theta.sin())
}

/// Converts a stereo track's pan (-100..=100, acting as a left/right
/// balance control rather than a mono-to-stereo placement) and linear
/// volume into a (left, right) gain pair. At pan 0 both channels get full
/// `volume`; panning right attenuates the left channel and vice versa —
/// there's no crossfeed between channels, matching how Audacity's stereo
/// track pan behaves.
pub fn pan_balance_gains(pan_percent: i8, volume: f32) -> (f32, f32) {
    let pan = (pan_percent as f32 / 100.0).clamp(-1.0, 1.0);
    let left = volume * (1.0 - pan.max(0.0));
    let right = volume * (1.0 + pan.min(0.0));
    (left, right)
}

/// A single mono track's raw contribution at absolute sample index `n`,
/// before pan/volume are applied.
pub fn track_frame(track: &Track, n: u64) -> f32 {
    track.clips.iter().filter_map(|c| c.sample_at(n)).sum()
}

/// A single stereo track's raw (left, right) contribution at absolute
/// sample index `n`, before pan/volume are applied.
pub fn track_frame_stereo(track: &Track, n: u64) -> (f32, f32) {
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    for clip in &track.clips {
        left += clip.channel_sample_at(n, 0).unwrap_or(0.0);
        right += clip.channel_sample_at(n, 1).unwrap_or(0.0);
    }
    (left, right)
}

/// Sums every audible track's contribution at sample index `n` into one
/// stereo frame. Pure and stateless, so it's used identically by the
/// realtime mixer and the offline export renderer — they can't drift.
pub fn mix_frame(tracks: &[Track], n: u64) -> (f32, f32) {
    let any_soloed = tracks.iter().any(|t| t.soloed);
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    for track in tracks {
        if !is_audible(track, any_soloed) {
            continue;
        }
        if track.channels >= 2 {
            let (l, r) = track_frame_stereo(track, n);
            let (left_gain, right_gain) = pan_balance_gains(track.pan_percent, track.volume);
            left += l * left_gain;
            right += r * right_gain;
        } else {
            let (left_gain, right_gain) = pan_gains(track.pan_percent, track.volume);
            let mono = track_frame(track, n);
            left += mono * left_gain;
            right += mono * right_gain;
        }
    }
    (left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0))
}
