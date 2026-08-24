use super::Project;
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
    }
}

fn from_saved(saved: SavedProject) -> Project {
    let mut project = Project::empty(saved.sample_rate_hz);
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
    // Rebuilding a loaded project shouldn't itself be undoable back to
    // an empty one.
    project.clear_undo_history();
    project
}
