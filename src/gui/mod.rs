mod app;
mod export_dialog;
mod help_dialog;
mod meter_widget;
mod project_file_dialog;
mod record_monitor;
mod settings_persistence;
mod timeline;
mod toolbar;
mod track_view;
mod wave_dialog;

pub use app::RakunatorApp;

/// Net physical wheel notches scrolled this frame (positive = scrolled
/// up/away), read from raw wheel events rather than `smooth_scroll_delta`.
/// The latter keeps reporting a nonzero, decaying value for several frames
/// after a single flick (it's meant for smooth continuous panning/zooming),
/// which would apply a discrete stepped adjustment — a slider's scroll step,
/// say — many times over for what the user felt as one flick of the wheel.
/// `Point`-unit deltas (trackpads) are normalized by the same 40px-per-line
/// speed egui itself defaults to, so one notch feels the same either way.
pub(crate) fn wheel_notches(ui: &egui::Ui) -> f32 {
    ui.ctx().input(|i| {
        i.events.iter().fold(0.0, |acc, e| {
            let egui::Event::MouseWheel { unit, delta, .. } = e else {
                return acc;
            };
            acc + match unit {
                egui::MouseWheelUnit::Line => delta.y,
                egui::MouseWheelUnit::Point => delta.y / 40.0,
                egui::MouseWheelUnit::Page => delta.y,
            }
        })
    })
}

/// Height of one track's header + timeline lane row. Must be tall enough to
/// fit the header's content (name/menu, Pan, Vol, Mute/Solo+meter rows) —
/// otherwise the header silently overflows past this height (egui grows the
/// row to fit it), while every row-index calculation elsewhere (dragging a
/// clip onto another track, marquee-select, the snap indicator) keeps
/// assuming rows are exactly `TRACK_ROW_STEP` apart, drifting further off
/// with each track and making a barely-moved drag jump to the wrong track.
pub(crate) const ROW_HEIGHT: f32 = 84.0;
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
