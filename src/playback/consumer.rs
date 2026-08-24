use cpal::traits::DeviceTrait;
use cpal::{Device, OutputCallbackInfo, Stream, StreamConfig};
use ringbuf::traits::Consumer;
use ringbuf::HeapCons;

/// Builds the cpal output stream: the audio callback pops samples the
/// producer thread already generated, padding with silence on underrun.
pub fn build_stream(device: &Device, config: StreamConfig, mut consumer: HeapCons<f32>) -> Stream {
    device
        .build_output_stream(
            config,
            move |data: &mut [f32], _: &OutputCallbackInfo| {
                let filled = consumer.pop_slice(data);
                for sample in &mut data[filled..] {
                    *sample = 0.0;
                }
            },
            |err| eprintln!("stream error: {err}"),
            None,
        )
        .expect("failed to build output stream")
}