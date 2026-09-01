use chrono::Datelike;

/// Export tag metadata for a project — the fields shown in the "Export
/// Project" dialog and written into the exported `.wav`/`.mp3` as file
/// tags. Persisted as part of the `.raku` project file (see
/// `persistence::SavedMetadata`) so it stays the same the next time the
/// same project is exported, rather than resetting to these defaults every
/// time.
#[derive(Clone)]
pub struct ProjectMetadata {
    /// Base name (no directory or extension) the "Export Project" dialog's
    /// "Name:" field is prefilled with. Empty means "not set yet" — the
    /// dialog falls back to the project's current file name for display,
    /// but that fallback is never written back in here on its own; it only
    /// becomes permanent once the user actually exports.
    pub export_file_name: String,
    pub artist_name: String,
    /// Empty means "not set yet" — the Export dialog falls back to
    /// whatever the project's current file name is for display, but that
    /// fallback is never written back in here on its own; it only becomes
    /// permanent once the user actually exports.
    pub track_title: String,
    pub album_title: String,
    pub track_number: u32,
    pub year: u32,
    pub genre: String,
    pub comments: String,
    pub software: String,
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        ProjectMetadata {
            export_file_name: String::new(),
            artist_name: "Rakuel".to_string(),
            track_title: String::new(),
            album_title: String::new(),
            track_number: 1,
            year: chrono::Local::now().year().max(0) as u32,
            genre: "Gangsta Rap".to_string(),
            comments: String::new(),
            software: "Rakunator".to_string(),
        }
    }
}
