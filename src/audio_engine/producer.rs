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
                for (frame_idx, frame) in chunk.chunks_mut(channels).enumerate() {
                    let n = start + frame_idx as u64;
                    let (left, right) = mix_frame_with_meters(&project, n, &meters);
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

/// Mixes every audible track at sample index `n`, updating each track's
/// live meter with its gained peak contribution along the way. Bottoms out
/// in the same `is_audible`/`pan_gains`/`track_frame` primitives as
/// `mix::mix_frame`, so realtime playback can't drift from offline export.
fn mix_frame_with_meters(project: &Project, n: u64, meters: &Meters) -> (f32, f32) {
    let any_soloed = project.tracks.iter().any(|t| t.soloed);
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    for (i, track) in project.tracks.iter().enumerate() {
        if !mix::is_audible(track, any_soloed) {
            meters.reset(i);
            continue;
        }
        let mono = mix::track_frame(track, n);
        let (left_gain, right_gain) = mix::pan_gains(track.pan_percent, track.volume);
        let left_sample = mono * left_gain;
        let right_sample = mono * right_gain;
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
