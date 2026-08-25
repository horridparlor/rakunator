/// Rakunator's window/taskbar icon, built procedurally as raw RGBA pixels —
/// no image asset or extra dependency needed for something this simple: a
/// rounded dark-blue square with a lighter-blue diamond centered in it (the
/// lighter blue matches the app's accent color, see `modern_visuals` in
/// `app.rs`).
pub fn app_icon() -> egui::IconData {
    const SIZE: usize = 64;
    const DARK_BLUE: [f32; 3] = [22.0, 40.0, 92.0];
    const LIGHT_BLUE: [f32; 3] = [120.0, 170.0, 255.0];
    const CORNER_RADIUS: f32 = SIZE as f32 * 0.2;
    const DIAMOND_HALF: f32 = SIZE as f32 * 0.32;

    let half = SIZE as f32 / 2.0;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let square_coverage = rounded_square_coverage(px, py, half, CORNER_RADIUS);
            if square_coverage <= 0.0 {
                continue;
            }

            // Diamond edge (L1 distance from center, in a basis rotated 45°
            // from the square's), blended into the background color rather
            // than just alpha-masked, so its edge antialiases too.
            let diamond_d = (px - half).abs() + (py - half).abs() - DIAMOND_HALF;
            let diamond_mix = (0.5 - diamond_d).clamp(0.0, 1.0);

            let idx = (y * SIZE + x) * 4;
            for c in 0..3 {
                rgba[idx + c] = (DARK_BLUE[c] + (LIGHT_BLUE[c] - DARK_BLUE[c]) * diamond_mix).round() as u8;
            }
            rgba[idx + 3] = (square_coverage * 255.0).round() as u8;
        }
    }

    egui::IconData { rgba, width: SIZE as u32, height: SIZE as u32 }
}

/// 0.0 outside a rounded square of half-width `half` centered on the image,
/// 1.0 fully inside, with roughly a 1px soft edge for antialiasing — the
/// standard rounded-box signed-distance formula (Inigo Quilez), turned into
/// coverage.
fn rounded_square_coverage(px: f32, py: f32, half: f32, radius: f32) -> f32 {
    let qx = (px - half).abs() - (half - radius);
    let qy = (py - half).abs() - (half - radius);
    let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
    (0.5 - d).clamp(0.0, 1.0)
}
