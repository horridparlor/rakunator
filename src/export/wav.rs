use super::config::{SAMPLE_RATE_HZ, WAV_BITS_PER_SAMPLE};
use std::path::Path;

/// Writes `interleaved_stereo` (`[l0, r0, l1, r1, ...]`, each sample in
/// [-1, 1]) as signed 32-bit PCM WAV.
pub fn write_wav(path: &Path, interleaved_stereo: &[f32]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE_HZ,
        bits_per_sample: WAV_BITS_PER_SAMPLE,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec).expect("failed to create wav file");
    for &sample in interleaved_stereo {
        let value = (sample.clamp(-1.0, 1.0) * i32::MAX as f32) as i32;
        writer.write_sample(value).expect("failed to write wav sample");
    }
    writer.finalize().expect("failed to finalize wav file");
}