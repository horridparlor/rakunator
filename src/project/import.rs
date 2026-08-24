use std::path::Path;

/// Reads a WAV file and returns interleaved samples resampled (via simple
/// linear interpolation, per channel) to `target_sample_rate_hz`, along
/// with the channel count actually returned: 1 (mono) or 2 (stereo). A
/// source file with more than 2 channels is downmixed to mono. Returns
/// `None` on read or format errors rather than panicking, since this runs
/// from a live GUI drag-and-drop.
///
/// Only WAV is supported for now — other formats (mp3, flac, ...) would
/// need a dedicated decoder dependency.
pub fn load_wav(path: &Path, target_sample_rate_hz: u32) -> Option<(Vec<f32>, u8)> {
    let mut reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return None;
    }

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 / max)
                .collect()
        }
    };
    if interleaved.is_empty() {
        return None;
    }

    if channels == 2 {
        let left: Vec<f32> = interleaved.iter().step_by(2).copied().collect();
        let right: Vec<f32> = interleaved.iter().skip(1).step_by(2).copied().collect();
        let left = resample_linear(&left, spec.sample_rate, target_sample_rate_hz);
        let right = resample_linear(&right, spec.sample_rate, target_sample_rate_hz);
        let frames = left.len().min(right.len());
        let mut out = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            out.push(left[i]);
            out.push(right[i]);
        }
        return Some((out, 2));
    }

    let mono: Vec<f32> = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    Some((resample_linear(&mono, spec.sample_rate, target_sample_rate_hz), 1))
}

pub(crate) fn resample_linear(input: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if input.is_empty() || from_hz == to_hz {
        return input.to_vec();
    }
    let ratio = from_hz as f64 / to_hz as f64;
    let out_len = ((input.len() as f64) / ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}
