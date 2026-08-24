use ringbuf::traits::Producer;
use ringbuf::HeapProd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const CHUNK_FRAMES: usize = 256;
const MAX_CHANNELS: usize = 32;

/// Spawns the producer thread: generates waveform samples and pushes them
/// into the ring buffer for the consumer (audio callback) to drain.
pub fn spawn(
    mut producer: HeapProd<f32>,
    sample_rate: f32,
    channels: usize,
    frequency_hz: f32,
    waveform: impl Fn(f32) -> f32 + Send + 'static,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut phase = 0f32;
        let phase_step = frequency_hz / sample_rate;

        let mut chunk = [0f32; CHUNK_FRAMES * MAX_CHANNELS];
        let chunk_len = CHUNK_FRAMES * channels;
        let chunk = &mut chunk[..chunk_len];

        while running.load(Ordering::Relaxed) {
            for frame in chunk.chunks_mut(channels) {
                let value = waveform(phase);
                phase = (phase + phase_step) % 1.0;
                for sample in frame.iter_mut() {
                    *sample = value;
                }
            }

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