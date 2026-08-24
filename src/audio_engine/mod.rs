pub mod consumer;
pub mod meter;
pub mod mix;
pub mod producer;
pub mod recorder;
pub mod transport;

use crate::project::Project;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use meter::Meters;
use ringbuf::traits::Split;
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use transport::Transport;

const RING_BUFFER_FRAMES: usize = 4096;

/// Owns the persistent cpal output stream and the producer/mixer thread
/// for the app's lifetime, built once at startup rather than per play
/// click. Transport (play/pause/stop/seek) and per-track meters are cheap
/// atomics, safe to poll from the GUI thread every frame.
pub struct AudioEngine {
    // Kept alive so the underlying cpal stream (and its OS audio callback)
    // keeps running; never read directly.
    _stream: cpal::Stream,
    running: Arc<AtomicBool>,
    pub transport: Arc<Transport>,
    pub meters: Arc<Meters>,
}

impl AudioEngine {
    pub fn start(project: Arc<Mutex<Project>>) -> AudioEngine {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("no output device available");
        let config = device
            .default_output_config()
            .expect("no default output config");

        let channels = config.channels() as usize;
        let stream_config: cpal::StreamConfig = config.into();

        let rb = HeapRb::<f32>::new(RING_BUFFER_FRAMES * channels);
        let (producer, consumer_half) = rb.split();

        let stream = consumer::build_stream(&device, stream_config, consumer_half);
        stream.play().expect("failed to play stream");

        let running = Arc::new(AtomicBool::new(true));
        let transport = Arc::new(Transport::new());
        let meters = Arc::new(Meters::new());

        producer::spawn(
            producer,
            channels,
            project,
            Arc::clone(&transport),
            Arc::clone(&meters),
            Arc::clone(&running),
        );

        AudioEngine {
            _stream: stream,
            running,
            transport,
            meters,
        }
    }

    pub fn play(&self) {
        self.transport.play();
    }

    pub fn pause(&self) {
        self.transport.pause();
    }

    pub fn stop(&self) {
        self.transport.stop();
    }

    pub fn seek(&self, sample_pos: u64) {
        self.transport.seek(sample_pos);
    }

    pub fn is_playing(&self) -> bool {
        self.transport.is_playing()
    }

    pub fn position(&self) -> u64 {
        self.transport.position()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
