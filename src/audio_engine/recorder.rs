use crate::project::import::resample_linear;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{InputCallbackInfo, Stream, StreamConfig};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// ~2 seconds of headroom at a typical 2-channel 48kHz input, so the
/// collector thread comfortably drains the callback without the ring
/// buffer ever filling up.
const RING_BUFFER_FRAMES: usize = 48_000 * 2;

/// Captures the default system microphone into an in-memory buffer.
/// `start` opens the input device and begins capturing immediately;
/// `stop` closes the stream and returns everything captured, still at the
/// device's native channel count and sample rate (see
/// `to_project_format` to convert it for a `Project`).
pub struct Recorder {
    stream: Stream,
    collecting: Arc<AtomicBool>,
    collector: Option<JoinHandle<()>>,
    buffer: Arc<Mutex<Vec<f32>>>,
    /// Decaying peak input level (0.0-1.0+), updated directly in the audio
    /// callback so the GUI can poll it every frame without touching the
    /// (potentially large, mutex-guarded) capture buffer.
    peak: Arc<AtomicU32>,
    /// Latched by the callback whenever a sample hits/exceeds full scale;
    /// the GUI reads-and-clears it each frame to drive a clip warning.
    clipped: Arc<AtomicBool>,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

impl Recorder {
    /// Opens the default input device and starts capturing. Returns `None`
    /// if there's no input device, or it can't be opened — the caller
    /// should surface that as "no microphone available" rather than panic,
    /// since this runs from a live GUI button press.
    pub fn start() -> Option<Recorder> {
        let host = cpal::default_host();
        let device = host.default_input_device()?;
        let config = device.default_input_config().ok()?;
        let channels = config.channels() as usize;
        let sample_rate_hz = config.sample_rate();
        let stream_config: StreamConfig = config.into();

        let rb = HeapRb::<f32>::new(RING_BUFFER_FRAMES * channels);
        let (mut producer, mut consumer) = rb.split();

        let peak = Arc::new(AtomicU32::new(0));
        let clipped = Arc::new(AtomicBool::new(false));
        let stream = {
            let peak = Arc::clone(&peak);
            let clipped = Arc::clone(&clipped);
            device
                .build_input_stream(
                    stream_config,
                    move |data: &[f32], _: &InputCallbackInfo| {
                        let block_peak = data.iter().fold(0f32, |m, s| m.max(s.abs()));
                        if block_peak >= 0.999 {
                            clipped.store(true, Ordering::Relaxed);
                        }
                        let prev = f32::from_bits(peak.load(Ordering::Relaxed));
                        peak.store((prev * 0.9).max(block_peak).to_bits(), Ordering::Relaxed);
                        producer.push_slice(data);
                    },
                    |err| eprintln!("input stream error: {err}"),
                    None,
                )
                .ok()?
        };
        stream.play().ok()?;

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let collecting = Arc::new(AtomicBool::new(true));
        let collector = {
            let buffer = Arc::clone(&buffer);
            let collecting = Arc::clone(&collecting);
            thread::spawn(move || {
                let mut chunk = [0f32; 4096];
                loop {
                    let n = consumer.pop_slice(&mut chunk);
                    if n > 0 {
                        buffer.lock().unwrap().extend_from_slice(&chunk[..n]);
                    } else if collecting.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(5));
                    } else {
                        // Stopped and the ring buffer is fully drained.
                        break;
                    }
                }
            })
        };

        Some(Recorder {
            stream,
            collecting,
            collector: Some(collector),
            buffer,
            peak,
            clipped,
            channels,
            sample_rate_hz,
        })
    }

    /// Current decaying peak input level, roughly 0.0-1.0 (can exceed 1.0
    /// briefly on a hot signal before the decay catches up).
    pub fn peak_level(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed))
    }

    /// True if any sample captured since the last call hit/exceeded full
    /// scale. Clears the flag, so call this once per frame.
    pub fn take_clipped(&self) -> bool {
        self.clipped.swap(false, Ordering::Relaxed)
    }

    /// A copy of roughly the last `max_samples` interleaved samples
    /// captured so far (fewer if less has been recorded), for a live
    /// waveform preview while recording.
    pub fn recent_samples(&self, max_samples: usize) -> Vec<f32> {
        let buffer = self.buffer.lock().unwrap();
        let start = buffer.len().saturating_sub(max_samples);
        buffer[start..].to_vec()
    }

    /// Every sample captured from index `from` (into the interleaved
    /// capture buffer) onward, plus the buffer's current total length —
    /// lets a caller accumulate a live waveform incrementally, frame by
    /// frame, without re-copying already-read samples each time the way
    /// `recent_samples` would.
    pub fn samples_since(&self, from: usize) -> (Vec<f32>, usize) {
        let buffer = self.buffer.lock().unwrap();
        let from = from.min(buffer.len());
        (buffer[from..].to_vec(), buffer.len())
    }

    /// Stops capturing and returns every sample recorded, interleaved at
    /// `self.channels` channels and `self.sample_rate_hz`.
    pub fn stop(self) -> Vec<f32> {
        // Stop the input callback first so no more data is produced, then
        // let the collector thread drain whatever's still in the ring
        // buffer before joining it.
        drop(self.stream);
        self.collecting.store(false, Ordering::Relaxed);
        if let Some(handle) = self.collector {
            let _ = handle.join();
        }
        Arc::try_unwrap(self.buffer)
            .map(|m| m.into_inner().unwrap())
            .unwrap_or_default()
    }
}

/// Converts a raw interleaved capture (`channels` channels at `from_hz`)
/// into a `Project`-ready buffer: downmixed to mono if the device captured
/// more than 2 channels, resampled per channel to `to_hz`. Mirrors
/// `import::load_wav`'s channel handling so recorded and imported audio
/// behave the same way once they're on a track. Returns the converted
/// samples and the channel count (1 or 2).
pub fn to_project_format(raw: &[f32], channels: usize, from_hz: u32, to_hz: u32) -> (Vec<f32>, u8) {
    if channels == 0 || raw.is_empty() {
        return (Vec::new(), 1);
    }

    if channels == 2 {
        let left: Vec<f32> = raw.iter().step_by(2).copied().collect();
        let right: Vec<f32> = raw.iter().skip(1).step_by(2).copied().collect();
        let left = resample_linear(&left, from_hz, to_hz);
        let right = resample_linear(&right, from_hz, to_hz);
        let frames = left.len().min(right.len());
        let mut out = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            out.push(left[i]);
            out.push(right[i]);
        }
        return (out, 2);
    }

    let mono: Vec<f32> = raw
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    (resample_linear(&mono, from_hz, to_hz), 1)
}
