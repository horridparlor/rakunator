pub mod config;
mod mp3;
mod wav;

use crate::audio_engine::mix;
use crate::project::Project;
use config::ExportDir;
use std::path::PathBuf;

/// Renders the full multi-track mixdown (respecting each track's volume,
/// pan, mute and solo) and writes it to `<export dir>/<base_name>.wav` and
/// `.mp3`. Uses the same `mix::mix_frame` the realtime engine uses for
/// playback, so what you hear during playback and what gets exported can't
/// drift apart.
pub fn export_project(project: &Project, base_name: &str) {
    let sample_count = project
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .map(|c| c.end_sample())
        .max()
        .unwrap_or(0);

    let mut interleaved = Vec::with_capacity(sample_count as usize * 2);
    for n in 0..sample_count {
        let (left, right) = mix::mix_frame(&project.tracks, n);
        interleaved.push(left);
        interleaved.push(right);
    }

    let (wav_path, mp3_path) = export_paths(base_name);

    wav::write_wav(&wav_path, &interleaved);
    mp3::write_mp3(&mp3_path, &interleaved);

    println!("wrote {}", wav_path.display());
    println!("wrote {}", mp3_path.display());
}

/// The exact `.wav`/`.mp3` paths `export_project` would write for
/// `base_name` — exposed so the GUI can check whether either already
/// exists (and confirm before overwriting) without duplicating the
/// sanitizing/directory-resolution logic.
pub fn export_paths(base_name: &str) -> (PathBuf, PathBuf) {
    let base_name = sanitize_file_name(base_name);
    let export_dir = resolve_export_dir();
    (export_dir.join(format!("{base_name}.wav")), export_dir.join(format!("{base_name}.mp3")))
}

/// Strips path separators (and trims whitespace) from a user-typed export
/// name, so it can't accidentally write outside the export directory; falls
/// back to "mixdown" if that leaves nothing.
fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .filter(|c| !matches!(c, '/' | '\\'))
        .collect();
    if cleaned.is_empty() {
        "mixdown".to_string()
    } else {
        cleaned
    }
}

fn resolve_export_dir() -> PathBuf {
    match config::EXPORT_DIR {
        ExportDir::Downloads => {
            dirs::download_dir().expect("could not determine Downloads directory")
        }
        ExportDir::Path(path) => PathBuf::from(path),
    }
}
