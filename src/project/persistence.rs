use super::{Project, ProjectMetadata};
use crate::bethoven::Melody;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// On-disk shape of a `.raku` project file. Deliberately separate from the
/// live `Project`/`Track`/`Clip` types: it only carries persistent content
/// (no selection/clipboard/id-counter state), and keeps the runtime types
/// free of serde coupling.
#[derive(Serialize, Deserialize)]
struct SavedProject {
    sample_rate_hz: u32,
    tracks: Vec<SavedTrack>,
    /// Absent from `.raku` files saved before export metadata existed —
    /// `SavedMetadata::default()` matches `ProjectMetadata::default()` in
    /// that case.
    #[serde(default)]
    metadata: SavedMetadata,
    /// Absent from `.raku` files saved before Bethoven existed.
    #[serde(default)]
    melodies: Vec<Melody>,
    /// Absent from `.raku` files saved before this existed.
    #[serde(default)]
    last_melody_id: Option<u32>,
}

/// On-disk shape of `ProjectMetadata` — see `to_saved`/`from_saved` for the
/// conversion. Kept separate from the runtime type for the same reason as
/// `SavedProject`/`Project`.
#[derive(Serialize, Deserialize)]
struct SavedMetadata {
    #[serde(default)]
    export_file_name: String,
    artist_name: String,
    track_title: String,
    album_title: String,
    track_number: u32,
    year: u32,
    genre: String,
    comments: String,
    software: String,
}

impl Default for SavedMetadata {
    fn default() -> Self {
        let m = ProjectMetadata::default();
        SavedMetadata {
            export_file_name: m.export_file_name,
            artist_name: m.artist_name,
            track_title: m.track_title,
            album_title: m.album_title,
            track_number: m.track_number,
            year: m.year,
            genre: m.genre,
            comments: m.comments,
            software: m.software,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedTrack {
    name: String,
    pan_percent: i8,
    volume: f32,
    muted: bool,
    soloed: bool,
    #[serde(default = "default_channels")]
    channels: u8,
    clips: Vec<SavedClip>,
}

#[derive(Serialize, Deserialize)]
struct SavedClip {
    name: String,
    start_sample: u64,
    #[serde(default = "default_channels")]
    channels: u8,
    samples: Vec<f32>,
}

/// Older `.raku` files predate stereo support and have no `channels`
/// field — they're always mono.
fn default_channels() -> u8 {
    1
}

pub fn save_project(project: &Project, path: &Path) -> Result<(), String> {
    let saved = to_saved(project);
    let json = serde_json::to_string(&saved).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_project(path: &Path) -> Result<Project, String> {
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let saved: SavedProject = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(from_saved(saved))
}

fn to_saved(project: &Project) -> SavedProject {
    SavedProject {
        sample_rate_hz: project.sample_rate_hz,
        metadata: SavedMetadata {
            export_file_name: project.metadata.export_file_name.clone(),
            artist_name: project.metadata.artist_name.clone(),
            track_title: project.metadata.track_title.clone(),
            album_title: project.metadata.album_title.clone(),
            track_number: project.metadata.track_number,
            year: project.metadata.year,
            genre: project.metadata.genre.clone(),
            comments: project.metadata.comments.clone(),
            software: project.metadata.software.clone(),
        },
        tracks: project
            .tracks
            .iter()
            .map(|t| SavedTrack {
                name: t.name.clone(),
                pan_percent: t.pan_percent,
                volume: t.volume,
                muted: t.muted,
                soloed: t.soloed,
                channels: t.channels,
                clips: t
                    .clips
                    .iter()
                    .map(|c| SavedClip {
                        name: c.name.clone(),
                        start_sample: c.start_sample,
                        channels: c.channels(),
                        samples: c.visible_samples().to_vec(),
                    })
                    .collect(),
            })
            .collect(),
        melodies: project.melodies.clone(),
        last_melody_id: project.last_melody_id,
    }
}

fn from_saved(saved: SavedProject) -> Project {
    let mut project = Project::empty(saved.sample_rate_hz);
    project.metadata = ProjectMetadata {
        export_file_name: saved.metadata.export_file_name,
        artist_name: saved.metadata.artist_name,
        track_title: saved.metadata.track_title,
        album_title: saved.metadata.album_title,
        track_number: saved.metadata.track_number,
        year: saved.metadata.year,
        genre: saved.metadata.genre,
        comments: saved.metadata.comments,
        software: saved.metadata.software,
    };
    for saved_track in saved.tracks {
        let track_id = project.add_track();
        if let Some(track) = project.track_mut(track_id) {
            track.name = saved_track.name;
            track.pan_percent = saved_track.pan_percent;
            track.volume = saved_track.volume;
            track.muted = saved_track.muted;
            track.soloed = saved_track.soloed;
            track.channels = saved_track.channels.max(1);
        }
        for saved_clip in saved_track.clips {
            project.add_clip_channels(
                track_id,
                saved_clip.name,
                saved_clip.start_sample,
                saved_clip.samples,
                saved_clip.channels.max(1),
            );
        }
    }
    project.melodies = saved.melodies;
    project.last_melody_id = saved.last_melody_id;
    // Rebuilding a loaded project shouldn't itself be undoable back to
    // an empty one.
    project.clear_undo_history();
    project
}
