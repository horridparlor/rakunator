use crate::audio_engine::AudioEngine;
use crate::project::{Project, TrackId};

use super::meter_widget;

enum TrackMenuAction {
    MoveUp,
    MoveDown,
    MoveTop,
    MoveBottom,
    MoveBy(i32),
    Duplicate,
    ResetPanVolume,
    Delete,
    SplitToMono,
    MergeWithBelow,
}

/// Draws one track's header controls: editable name, pan (5% steps),
/// volume, mute/solo, its live level meter, and a "..." menu for reordering,
/// duplicating, and deleting the track.
pub fn draw_header(ui: &mut egui::Ui, project: &mut Project, track_id: TrackId, engine: &AudioEngine) {
    let Some(track_index) = project.tracks.iter().position(|t| t.id == track_id) else {
        return;
    };

    // Registered before any of the header's real controls, so it sits
    // underneath them in hit-test order — clicks on the name field, pan
    // slider, etc. still reach those widgets, and only clicks on genuinely
    // empty header space fall through to select the whole track.
    let background_id = egui::Id::new(("track_header_bg", track_id.0));
    let background_response = ui.interact(ui.max_rect(), background_id, egui::Sense::click());
    if project.selected_tracks.contains(&track_id) {
        ui.painter().rect_filled(
            background_response.rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(120, 170, 255, 40),
        );
    }

    let is_stereo = project.tracks[track_index].channels >= 2;
    let below_is_mergeable_mono = project.tracks[track_index].channels == 1
        && project
            .tracks
            .get(track_index + 1)
            .map(|t| t.channels == 1)
            .unwrap_or(false);

    let mut menu_action: Option<TrackMenuAction> = None;
    {
        let track = &mut project.tracks[track_index];

        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut track.name).desired_width(100.0));
                ui.menu_button("...", |ui| {
                    ui.spacing_mut().item_spacing.y += 4.0;
                    if ui.button("Move up").clicked() {
                        menu_action = Some(TrackMenuAction::MoveUp);
                        ui.close();
                    }
                    if ui.button("Move down").clicked() {
                        menu_action = Some(TrackMenuAction::MoveDown);
                        ui.close();
                    }
                    if ui.button("Move to top").clicked() {
                        menu_action = Some(TrackMenuAction::MoveTop);
                        ui.close();
                    }
                    if ui.button("Move to bottom").clicked() {
                        menu_action = Some(TrackMenuAction::MoveBottom);
                        ui.close();
                    }
                    ui.horizontal(|ui| {
                        let move_by_id = egui::Id::new(("move_by", track_id.0));
                        let mut move_by =
                            ui.ctx().data_mut(|d| *d.get_temp_mut_or(move_by_id, 1i32));
                        ui.add(
                            egui::DragValue::new(&mut move_by)
                                .range(-999..=999)
                                .prefix("by "),
                        );
                        ui.ctx().data_mut(|d| d.insert_temp(move_by_id, move_by));
                        if ui.button("Move").clicked() {
                            menu_action = Some(TrackMenuAction::MoveBy(move_by));
                            ui.close();
                        }
                    });
                    ui.separator();
                    if ui.button("Duplicate track").clicked() {
                        menu_action = Some(TrackMenuAction::Duplicate);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Reset pan & volume").clicked() {
                        menu_action = Some(TrackMenuAction::ResetPanVolume);
                        ui.close();
                    }
                    ui.separator();
                    if is_stereo && ui.button("Split to mono").clicked() {
                        menu_action = Some(TrackMenuAction::SplitToMono);
                        ui.close();
                    }
                    if !is_stereo
                        && ui
                            .add_enabled(
                                below_is_mergeable_mono,
                                egui::Button::new("Merge with track below"),
                            )
                            .clicked()
                    {
                        menu_action = Some(TrackMenuAction::MergeWithBelow);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete track").clicked() {
                        menu_action = Some(TrackMenuAction::Delete);
                        ui.close();
                    }
                });
            });

            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::Slider::new(&mut track.pan_percent, -100..=100)
                        .step_by(5.0)
                        .suffix("%")
                        .text("Pan"),
                );
                // Scrolling over the pan slider nudges it by one 5% step
                // per notch, same granularity as dragging it, so it can be
                // adjusted without needing to click-drag the thin handle.
                if response.hovered() {
                    let notches = super::wheel_notches(ui);
                    if notches != 0.0 {
                        let step = (notches.round() as i8) * 5;
                        track.pan_percent = (track.pan_percent + step).clamp(-100, 100);
                    }
                }
            });

            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::Slider::new(&mut track.volume, 0.0..=1.5)
                        .step_by(0.05)
                        .text("Vol"),
                );
                // Scrolling over the volume slider nudges it by one 0.05
                // step per notch, same granularity as dragging it.
                if response.hovered() {
                    let notches = super::wheel_notches(ui);
                    if notches != 0.0 {
                        track.volume = (track.volume + notches.round() * 0.05).clamp(0.0, 1.5);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.toggle_value(&mut track.muted, "M");
                ui.toggle_value(&mut track.soloed, "S");
                let (peak_l, peak_r) = engine.meters.read(track_index);
                meter_widget::draw(ui, peak_l, peak_r);
            });
        });
    }

    match menu_action {
        Some(TrackMenuAction::MoveUp) => project.move_track(track_id, -1),
        Some(TrackMenuAction::MoveDown) => project.move_track(track_id, 1),
        Some(TrackMenuAction::MoveTop) => project.move_track_to_top(track_id),
        Some(TrackMenuAction::MoveBottom) => project.move_track_to_bottom(track_id),
        Some(TrackMenuAction::MoveBy(offset)) => project.move_track(track_id, offset),
        Some(TrackMenuAction::Duplicate) => {
            project.duplicate_track(track_id);
        }
        Some(TrackMenuAction::ResetPanVolume) => {
            project.push_undo();
            if let Some(track) = project.track_mut(track_id) {
                track.pan_percent = 0;
                track.volume = 1.0;
            }
        }
        Some(TrackMenuAction::Delete) => project.remove_track(track_id),
        Some(TrackMenuAction::SplitToMono) => {
            project.split_track_to_mono(track_id);
        }
        Some(TrackMenuAction::MergeWithBelow) => {
            project.merge_track_with_below(track_id);
        }
        None => {}
    }

    if background_response.clicked() {
        let shift = ui.ctx().input(|i| i.modifiers.shift);
        if shift {
            if !project.selected_tracks.remove(&track_id) {
                project.selected_tracks.insert(track_id);
            }
        } else {
            project.selected_tracks.clear();
            project.selected_tracks.insert(track_id);
        }
        project.selection.clear();
    }
}
