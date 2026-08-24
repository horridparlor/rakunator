use super::timeline;
use crate::audio_engine::recorder::Recorder;

/// Height of the live recording waveform strip.
const HEIGHT: f32 = 64.0;
/// How many seconds of recent audio to keep visible, so a hot passage
/// doesn't scroll out of view before it's noticed.
const PREVIEW_SECONDS: f32 = 4.0;

/// Draws a live, scrolling waveform of what's being captured right now,
/// downmixed to mono, colored by how hot the signal is — normal accent
/// blue, amber past -3 dBFS-ish, red once it's actually clipped — so
/// clipping is visible at a glance without waiting for the take to finish.
/// `clipping` latches red for a moment after the last clipped sample (see
/// `RakunatorApp::record_clip_flash`), since a single hot sample can flash
/// past in one frame otherwise.
pub fn draw(ui: &mut egui::Ui, recorder: &Recorder, clipping: bool) {
    let channels = recorder.channels.max(1);
    let max_samples = (recorder.sample_rate_hz as f32 * PREVIEW_SECONDS) as usize * channels;
    let raw = recorder.recent_samples(max_samples);
    let mono: Vec<f32> = raw
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    let peak = recorder.peak_level();

    let size = egui::Vec2::new(ui.available_width(), HEIGHT);
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, egui::CornerRadius::from(4.0), egui::Color32::from_rgb(18, 19, 24));

    let wave_color = if clipping {
        egui::Color32::from_rgb(220, 60, 60)
    } else if peak > 0.85 {
        egui::Color32::from_rgb(230, 200, 60)
    } else {
        egui::Color32::from_rgb(120, 170, 255)
    };
    timeline::draw_waveform(painter, rect, &mono, wave_color, 1.0);

    painter.rect_stroke(
        rect,
        egui::CornerRadius::from(4.0),
        egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 74, 82)),
        egui::StrokeKind::Middle,
    );

    if clipping {
        painter.text(
            rect.right_top() + egui::Vec2::new(-6.0, 4.0),
            egui::Align2::RIGHT_TOP,
            "CLIPPING",
            egui::FontId::proportional(14.0),
            egui::Color32::from_rgb(220, 60, 60),
        );
    }
}
