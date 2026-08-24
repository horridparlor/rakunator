use crate::audio_engine::AudioEngine;
use crate::project::{Project, TrackId};

use super::meter_widget;

/// Draws one track's header controls: editable name, pan (5% steps),
/// volume, mute/solo, and its live level meter.
pub fn draw_header(ui: &mut egui::Ui, project: &mut Project, track_id: TrackId, engine: &AudioEngine) {
    let Some(track_index) = project.tracks.iter().position(|t| t.id == track_id) else {
        return;
    };

    let mut remove_requested = false;
    {
        let track = &mut project.tracks[track_index];

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut track.name).desired_width(110.0));
                if ui.small_button("✕").clicked() {
                    remove_requested = true;
                }
            });

            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut track.pan_percent, -100..=100)
                        .step_by(5.0)
                        .suffix("%")
                        .text("Pan"),
                );
            });

            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut track.volume, 0.0..=1.5)
                        .step_by(0.05)
                        .text("Vol"),
                );
            });

            ui.horizontal(|ui| {
                ui.toggle_value(&mut track.muted, "M");
                ui.toggle_value(&mut track.soloed, "S");
                let (peak_l, peak_r) = engine.meters.read(track_index);
                meter_widget::draw(ui, peak_l, peak_r);
            });
        });
    }

    if remove_requested {
        project.remove_track(track_id);
    }
}
