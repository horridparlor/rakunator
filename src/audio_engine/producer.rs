use super::meter::Meters;
use super::mix;
use super::transport::Transport;
use crate::project::Project;
use ringbuf::traits::Producer;
use ringbuf::HeapProd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const CHUNK_FRAMES: usize = 256;
const MAX_CHANNELS: usize = 32;

/// Spawns the producer/mixer thread: while the transport is playing, mixes
/// every audible track's clips at the current playback position into
/// interleaved device-channel frames and pushes them into the ring buffer
/// for the consumer (audio callback) to drain. While stopped/paused, it
/// idles so the ring buffer drains and the consumer zero-fills.
pub fn spawn(
    mut producer: HeapProd<f32>,
    channels: usize,
    project: Arc<Mutex<Project>>,
    transport: Arc<Transport>,
    meters: Arc<Meters>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0f32; CHUNK_FRAMES * MAX_CHANNELS];
        let chunk_len = CHUNK_FRAMES * channels;
        let chunk = &mut chunk[..chunk_len];

        while running.load(Ordering::Relaxed) {
            if !transport.is_playing() {
                thread::sleep(Duration::from_millis(5));
                continue;
            }

            {
                let project = project.lock().unwrap();
                let start = transport.position();
                // Audibility and pan/volume gains can't change mid-chunk
                // (the project lock is held for the whole chunk), so
                // resolve them once per track here rather than redoing an
                // `any_soloed` scan and, for mono tracks, a `cos`/`sin`
                // pair per sample — that used to happen CHUNK_FRAMES times
                // per chunk regardless.
                let voices = resolve_voices(&project);
                for (frame_idx, frame) in chunk.chunks_mut(channels).enumerate() {
                    let n = start + frame_idx as u64;
                    let (left, right) = mix_frame_with_meters(&project, &voices, n, &meters);
                    write_frame(frame, left, right);
                }
            }
            transport.advance(CHUNK_FRAMES as u64);

            let mut remaining = &chunk[..];
            while !remaining.is_empty() && running.load(Ordering::Relaxed) {
                let pushed = producer.push_slice(remaining);
                remaining = &remaining[pushed..];
                if pushed == 0 {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
    })
}

/// A track's resolved-once-per-chunk mix state: whether it's audible right
/// now and its pan/volume gain pair, so the per-frame mix loop never
/// recomputes an `any_soloed` scan or a gain (`cos`/`sin`, for a mono
/// track) that can't have changed since the chunk started.
struct Voice {
    audible: bool,
    left_gain: f32,
    right_gain: f32,
}

/// Resolves every track's `Voice` for the chunk about to be mixed. Bottoms
/// out in the same `is_audible`/`pan_gains`/`pan_balance_gains` primitives
/// as `mix::mix_frame`, so realtime playback can't drift from offline
/// export.
fn resolve_voices(project: &Project) -> Vec<Voice> {
    let any_soloed = project.tracks.iter().any(|t| t.soloed);
    project
        .tracks
        .iter()
        .map(|track| {
            let audible = mix::is_audible(track, any_soloed);
            let (left_gain, right_gain) = if track.channels >= 2 {
                mix::pan_balance_gains(track.pan_percent, track.volume)
            } else {
                mix::pan_gains(track.pan_percent, track.volume)
            };
            Voice { audible, left_gain, right_gain }
        })
        .collect()
}

/// Mixes every audible track at sample index `n`, using `voices` (this
/// chunk's already-resolved audibility/gains) and updating each track's
/// live meter with its gained peak contribution along the way. Bottoms out
/// in the same `track_frame`/`track_frame_stereo` primitives as
/// `mix::mix_frame` (including its `channels >= 2` branch — a stereo
/// track's actual left/right content, not just its left channel duplicated
/// per `pan_gains`), so realtime playback can't drift from offline export.
fn mix_frame_with_meters(project: &Project, voices: &[Voice], n: u64, meters: &Meters) -> (f32, f32) {
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    for (i, track) in project.tracks.iter().enumerate() {
        let voice = &voices[i];
        if !voice.audible {
            meters.reset(i);
            continue;
        }
        let (left_sample, right_sample) = if track.channels >= 2 {
            let (l, r) = mix::track_frame_stereo(track, n);
            (l * voice.left_gain, r * voice.right_gain)
        } else {
            let mono = mix::track_frame(track, n);
            (mono * voice.left_gain, mono * voice.right_gain)
        };
        left += left_sample;
        right += right_sample;
        meters.update(i, left_sample.abs(), right_sample.abs());
    }
    (left.clamp(-1.0, 1.0), right.clamp(-1.0, 1.0))
}

/// Adapts a mixed stereo pair to the device's actual output channel count.
fn write_frame(frame: &mut [f32], left: f32, right: f32) {
    match frame.len() {
        0 => {}
        1 => frame[0] = (left + right) * 0.5,
        2 => {
            frame[0] = left;
            frame[1] = right;
        }
        n => {
            frame[0] = left;
            frame[1] = right;
            for sample in &mut frame[2..n] {
                *sample = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;

    /// Regression test for a divergence from `mix::mix_frame`: this
    /// function used to always call the mono-only `track_frame`/`pan_gains`
    /// pair regardless of `track.channels`, so a stereo track's right
    /// channel was never even read during realtime playback — silently
    /// collapsing every stereo track to its left channel, panned, even
    /// though offline export (which goes through `mix::mix_frame` directly)
    /// rendered it correctly. Panned hard right, a stereo clip with
    /// different left/right content must pass the right channel through
    /// unchanged, not the left channel's `pan_gains`-panned copy.
    #[test]
    fn mix_frame_with_meters_uses_real_stereo_content_not_just_left_channel() {
        let mut project = Project::new(48_000);
        let track_id = project.tracks[0].id;
        let interleaved: Vec<f32> = (0..20).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        project.add_clip_channels(track_id, "stereo".into(), 0, interleaved, 2);
        project.track_mut(track_id).unwrap().pan_percent = 100; // hard right

        let meters = Meters::new();
        let voices = resolve_voices(&project);
        let (left, right) = mix_frame_with_meters(&project, &voices, 0, &meters);
        assert!(left.abs() < 1e-6, "hard-right pan should silence the left channel, got {left}");
        assert!((right - (-1.0)).abs() < 1e-6, "right channel should pass its actual content through, got {right}");
    }
}
