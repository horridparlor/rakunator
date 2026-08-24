use super::config::{MP3_BITRATE_KBPS, SAMPLE_RATE_HZ};
use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Quality};
use std::path::Path;

/// Encodes `interleaved_stereo` (`[l0, r0, l1, r1, ...]`, each sample in
/// [-1, 1]) to a constant-bitrate stereo MP3 file.
pub fn write_mp3(path: &Path, interleaved_stereo: &[f32]) {
    let bitrate = match MP3_BITRATE_KBPS {
        320 => Bitrate::Kbps320,
        other => panic!("unsupported mp3 bitrate: {other}kbps"),
    };

    let mut encoder = Builder::new()
        .expect("failed to create lame encoder builder")
        .with_num_channels(2)
        .expect("failed to set channels")
        .with_sample_rate(SAMPLE_RATE_HZ)
        .expect("failed to set sample rate")
        .with_brate(bitrate)
        .expect("failed to set bitrate")
        .with_quality(Quality::Best)
        .expect("failed to set quality")
        .build()
        .expect("failed to initialize lame encoder");

    // mp3lame_encoder wants deinterleaved left/right slices.
    let to_i16 = |sample: f32| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
    let (left, right): (Vec<i16>, Vec<i16>) = interleaved_stereo
        .as_chunks::<2>().0.iter()
        .map(|frame| (to_i16(frame[0]), to_i16(frame[1])))
        .unzip();

    // encode_to_vec/flush_to_vec write into the Vec's spare capacity, so it
    // must be reserved upfront or LAME writes into a zero-length buffer.
    let mut mp3_out = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(left.len()));
    encoder
        .encode_to_vec(DualPcm { left: &left, right: &right }, &mut mp3_out)
        .expect("failed to encode mp3");
    mp3_out.reserve(7200);
    encoder
        .flush_to_vec::<FlushNoGap>(&mut mp3_out)
        .expect("failed to flush mp3 encoder");

    std::fs::write(path, mp3_out).expect("failed to write mp3 file");
}