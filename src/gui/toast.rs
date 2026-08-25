use super::RakunatorApp;
use std::time::{Duration, Instant};

/// How long a toast stays fully visible before it starts fading.
const VISIBLE: Duration = Duration::from_millis(1600);
/// How long the fade-out itself takes, once it starts.
const FADE: Duration = Duration::from_millis(400);

/// Shows `message` as a small overlay near the bottom of the window for a
/// couple of seconds, then fades it out — for quick confirmations (Ctrl+S,
/// loading a project) that don't need a whole dialog to acknowledge.
pub fn show(app: &mut RakunatorApp, message: impl Into<String>) {
    app.toast = Some((message.into(), Instant::now()));
}

/// Draws the current toast, if any (clearing it once it's fully faded).
/// Returns whether a toast is still visible, so the caller knows to keep
/// requesting repaints for the fade animation.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) -> bool {
    let Some((message, shown_at)) = app.toast.clone() else {
        return false;
    };
    let elapsed = shown_at.elapsed();
    if elapsed >= VISIBLE + FADE {
        app.toast = None;
        return false;
    }

    let alpha = if elapsed < VISIBLE {
        1.0
    } else {
        1.0 - (elapsed - VISIBLE).as_secs_f32() / FADE.as_secs_f32()
    };

    egui::Area::new(egui::Id::new("toast"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -24.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(egui::Color32::from_rgba_unmultiplied(30, 32, 38, (235.0 * alpha) as u8))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    // `.extend()` keeps this to a single line regardless of
                    // how long the message is (e.g. a long project file
                    // name) — the frame around it just grows to fit, rather
                    // than wrapping onto a second line.
                    ui.add(
                        egui::Label::new(egui::RichText::new(message.as_str()).color(
                            egui::Color32::from_rgba_unmultiplied(235, 235, 240, (255.0 * alpha) as u8),
                        ))
                        .extend(),
                    );
                });
        });

    true
}
