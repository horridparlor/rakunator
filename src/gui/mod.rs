mod app;
mod export_dialog;
mod help_dialog;
mod meter_widget;
mod project_file_dialog;
mod timeline;
mod toolbar;
mod track_view;
mod wave_dialog;

pub use app::RakunatorApp;

/// Height of one track's header + timeline lane row.
pub(crate) const ROW_HEIGHT: f32 = 72.0;
/// Fixed width of the track header column, so every lane's left edge lines
/// up regardless of row content.
pub(crate) const HEADER_WIDTH: f32 = 220.0;
/// Height of the time ruler row above the track list.
pub(crate) const RULER_HEIGHT: f32 = 24.0;
/// Exact vertical gap drawn (with a thin divider line) between track rows.
/// Fixed and exact — not `ui.separator()`'s auto-sized spacing — because
/// the cross-track drag math depends on every row occupying precisely
/// `ROW_HEIGHT + TRACK_ROW_GAP` pixels.
pub(crate) const TRACK_ROW_GAP: f32 = 8.0;
/// Total vertical step from one track row's top to the next one's top.
pub(crate) const TRACK_ROW_STEP: f32 = ROW_HEIGHT + TRACK_ROW_GAP;
