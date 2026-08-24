pub mod consumer;
pub mod producer;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::Split;
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const RING_BUFFER_FRAMES: usize = 4096;

/// Plays `frequency_hz` of the given waveform for `duration` on the default
/// output device. `waveform` maps a phase in [0, 1) to a sample in [-1, 1].
pub fn play_wave(frequency_hz: f32, duration: Duration, waveform: impl Fn(f32) -> f32 + Send + 'static) {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let config = device
        .default_output_config()
        .expect("no default output config");

    let sample_rate = config.sample_rate() as f32;
    let channels = config.channels() as usize;
    let stream_config: cpal::StreamConfig = config.into();

    let rb = HeapRb::<f32>::new(RING_BUFFER_FRAMES * channels);
    let (producer, consumer_half) = rb.split();

    let stream = consumer::build_stream(&device, stream_config, consumer_half);
    stream.play().expect("failed to play stream");

    let running = Arc::new(AtomicBool::new(true));
    let producer_handle = producer::spawn(
        producer,
        sample_rate,
        channels,
        frequency_hz,
        waveform,
        Arc::clone(&running),
    );

    thread::sleep(duration);
    running.store(false, Ordering::Relaxed);
    producer_handle.join().expect("producer thread panicked");
}