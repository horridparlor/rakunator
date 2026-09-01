use super::config::{MP3_BITRATE_KBPS, SAMPLE_RATE_HZ};
use crate::project::ProjectMetadata;
use id3::TagLike;
use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Quality};
use std::path::Path;

/// Encodes `interleaved_stereo` (`[l0, r0, l1, r1, ...]`, each sample in
/// [-1, 1]) to a constant-bitrate stereo MP3 file, then writes `metadata`
/// into it as an ID3v2.4 tag.
pub fn write_mp3(path: &Path, interleaved_stereo: &[f32], metadata: &ProjectMetadata) {
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

    write_tags(path, metadata);
}

/// Writes `metadata` into `path`'s ID3v2.4 tag, replacing whatever tag (if
/// any) is already there.
fn write_tags(path: &Path, metadata: &ProjectMetadata) {
    let mut tag = id3::Tag::new();
    if !metadata.artist_name.is_empty() {
        tag.set_artist(&metadata.artist_name);
    }
    if !metadata.track_title.is_empty() {
        tag.set_title(&metadata.track_title);
    }
    if !metadata.album_title.is_empty() {
        tag.set_album(&metadata.album_title);
    }
    tag.set_track(metadata.track_number);
    tag.set_year(metadata.year as i32);
    if !metadata.genre.is_empty() {
        tag.set_genre(&metadata.genre);
    }
    if !metadata.software.is_empty() {
        tag.set_text("TSSE", &metadata.software);
    }
    if !metadata.comments.is_empty() {
        tag.add_frame(id3::frame::Comment {
            lang: "eng".to_string(),
            description: String::new(),
            text: metadata.comments.clone(),
        });
    }
    tag.write_to_path(path, id3::Version::Id3v24).expect("failed to write mp3 id3 tag");
}