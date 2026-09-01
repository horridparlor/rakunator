use super::config::{SAMPLE_RATE_HZ, WAV_BITS_PER_SAMPLE};
use crate::project::ProjectMetadata;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Writes `interleaved_stereo` (`[l0, r0, l1, r1, ...]`, each sample in
/// [-1, 1]) as signed 32-bit PCM WAV, then appends `metadata` as a
/// `LIST`/`INFO` chunk.
pub fn write_wav(path: &Path, interleaved_stereo: &[f32], metadata: &ProjectMetadata) {
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

    append_info_chunk(path, metadata);
}

/// Appends a `LIST`/`INFO` chunk carrying `metadata` onto an already
/// finalized WAV file, then patches the RIFF header's overall size field to
/// account for it — `hound` has no support for writing this chunk itself.
fn append_info_chunk(path: &Path, metadata: &ProjectMetadata) {
    let mut tags: Vec<(&[u8; 4], &str)> = Vec::new();
    if !metadata.track_title.is_empty() {
        tags.push((b"INAM", &metadata.track_title));
    }
    if !metadata.artist_name.is_empty() {
        tags.push((b"IART", &metadata.artist_name));
    }
    if !metadata.album_title.is_empty() {
        tags.push((b"IPRD", &metadata.album_title));
    }
    if !metadata.genre.is_empty() {
        tags.push((b"IGNR", &metadata.genre));
    }
    if !metadata.comments.is_empty() {
        tags.push((b"ICMT", &metadata.comments));
    }
    if !metadata.software.is_empty() {
        tags.push((b"ISFT", &metadata.software));
    }
    let year = metadata.year.to_string();
    tags.push((b"ICRD", &year));
    let track_number = metadata.track_number.to_string();
    tags.push((b"ITRK", &track_number));

    let mut info_body = Vec::new();
    info_body.extend_from_slice(b"INFO");
    for (id, value) in &tags {
        // Each INFO sub-chunk's text is NUL-terminated, and the whole
        // sub-chunk is padded to an even length (standard RIFF chunk
        // alignment) — hence the `+ 1` for the terminator and the
        // conditional extra padding byte below.
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        info_body.extend_from_slice(*id);
        info_body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        info_body.extend_from_slice(&data);
        if data.len() % 2 != 0 {
            info_body.push(0);
        }
    }

    let mut list_chunk = Vec::new();
    list_chunk.extend_from_slice(b"LIST");
    list_chunk.extend_from_slice(&(info_body.len() as u32).to_le_bytes());
    list_chunk.extend_from_slice(&info_body);

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("failed to reopen wav file for tagging");
    let file_len = file.metadata().expect("failed to stat wav file").len();
    file.seek(SeekFrom::Start(file_len)).expect("failed to seek to end of wav file");
    file.write_all(&list_chunk).expect("failed to append wav LIST chunk");

    let mut riff_size = [0u8; 4];
    file.seek(SeekFrom::Start(4)).expect("failed to seek to wav RIFF size field");
    file.read_exact(&mut riff_size).expect("failed to read wav RIFF size field");
    let new_riff_size = u32::from_le_bytes(riff_size) + list_chunk.len() as u32;
    file.seek(SeekFrom::Start(4)).expect("failed to seek to wav RIFF size field");
    file.write_all(&new_riff_size.to_le_bytes()).expect("failed to patch wav RIFF size field");
}