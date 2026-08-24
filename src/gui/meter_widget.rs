use egui::{Color32, CornerRadius, Rect, Vec2};

/// Draws a compact two-bar (L/R) level meter from live peak values in
/// [0, 1]. Reads a plain (f32, f32) rather than the `Meters` struct
/// directly, so this widget stays decoupled from the audio engine.
pub fn draw(ui: &mut egui::Ui, peak_l: f32, peak_r: f32) {
    let size = Vec2::new(48.0, 20.0);
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();

    let bar_height = (rect.height() - 2.0) / 2.0;
    draw_bar(
        painter,
        Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), bar_height)),
        peak_l,
    );
    draw_bar(
        painter,
        Rect::from_min_size(
            rect.left_top() + Vec2::new(0.0, bar_height + 2.0),
            Vec2::new(rect.width(), bar_height),
        ),
        peak_r,
    );
}

fn draw_bar(painter: &egui::Painter, rect: Rect, level: f32) {
    painter.rect_filled(rect, CornerRadius::from(2.0), Color32::from_gray(30));
    let level = level.clamp(0.0, 1.0);
    if level <= 0.0 {
        return;
    }
    let fill_width = rect.width() * level;
    let fill_rect =
        Rect::from_min_size(rect.left_top(), Vec2::new(fill_width, rect.height()));
    let color = if level > 0.9 {
        Color32::from_rgb(220, 60, 60)
    } else if level > 0.7 {
        Color32::from_rgb(230, 200, 60)
    } else {
        Color32::from_rgb(80, 200, 120)
    };
    painter.rect_filled(fill_rect, CornerRadius::from(2.0), color);
}
