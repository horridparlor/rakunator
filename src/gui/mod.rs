mod app;
mod export_dialog;
mod meter_widget;
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
