use super::toolbar::EffectsState;
use std::path::PathBuf;

/// `~/.config/rakunator/effects_settings.json` (platform equivalent via
/// `dirs::config_dir`) — the Effects "Edit steps..." values, persisted
/// independently of any project file so they survive across every project
/// and every restart.
fn settings_path() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("rakunator");
    Some(dir.join("effects_settings.json"))
}

/// Loads the persisted Effects settings at startup. Any failure (no config
/// dir, no file yet, unreadable, unparsable) just falls back to
/// `EffectsState::default()` silently — an app-level settings file is a
/// convenience, not something worth blocking startup over.
pub fn load_effects_settings() -> EffectsState {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

/// Persists the current Effects settings — called once the "Edit steps..."
/// dialog's OK is clicked. Failures (no writable config dir, etc.) are
/// silently ignored, same reasoning as `load_effects_settings`.
pub fn save_effects_settings(effects: &EffectsState) {
    let Some(path) = settings_path() else { return };
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(effects) {
        let _ = std::fs::write(path, json);
    }
}
