//! Bethoven's own top bar: melody save/switch/export, section tabs +
//! scale/root picker, tempo, play/pause, and the per-selection instrument
//! picker.

use super::{NewSectionDraft, RakunatorApp};
use crate::bethoven::melody::{self, Melody};
use crate::bethoven::scales;
use crate::bethoven::Instrument;
use crate::project::Project;

pub(super) fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    {
        // Cloning the `Arc` first (rather than locking `app.project`
        // directly) keeps this guard from borrowing `app` itself, so the
        // closures below can still capture `app` by unique reference to
        // reach `app.bethoven`.
        let project_arc = std::sync::Arc::clone(&app.project);
        let mut project = project_arc.lock().unwrap();
        ui.horizontal_wrapped(|ui| {
            draw_melody_controls(ui, app, &mut project);
        });
        ui.horizontal(|ui| {
            draw_transport_controls(ui, app, &mut project);
        });
        ui.horizontal_wrapped(|ui| {
            draw_section_tabs(ui, app, &mut project);
        });
        ui.horizontal_wrapped(|ui| {
            draw_instrument_picker(ui, app, &mut project);
        });
    }
    let ctx = ui.ctx().clone();
    draw_new_section_popup(&ctx, app);
}

fn draw_melody_controls(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    ui.label("Melody:");
    let current_name = app.bethoven.active_melody(project).map(|m| m.name.clone()).unwrap_or_default();
    egui::ComboBox::from_id_salt("bethoven_melody_combo")
        .selected_text(current_name.clone())
        .show_ui(ui, |ui| {
            let ids: Vec<(u32, String)> = project.melodies.iter().map(|m| (m.id, m.name.clone())).collect();
            for (id, name) in ids {
                if ui.selectable_label(app.bethoven.active_melody_id == Some(id), name).clicked() {
                    app.bethoven.active_melody_id = Some(id);
                    app.bethoven.active_section_id = None;
                    app.bethoven.selection.clear();
                    app.bethoven.mark_dirty();
                }
            }
        });

    if ui.button("New").clicked() {
        let id = project.next_melody_id();
        let name = format!("Melody {}", project.melodies.len() + 1);
        project.melodies.push(Melody::new(id, name));
        app.bethoven.active_melody_id = Some(id);
        app.bethoven.active_section_id = None;
        app.bethoven.selection.clear();
        app.bethoven.mark_dirty();
    }

    if app.bethoven.renaming_melody.is_some() {
        let mut done = false;
        let mut new_name = String::new();
        if let Some(renaming) = app.bethoven.renaming_melody.as_mut() {
            let resp = ui.text_edit_singleline(renaming);
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                done = true;
            }
            new_name = renaming.clone();
        }
        if ui.button("OK").clicked() {
            done = true;
        }
        if done {
            if let Some(m) = app.bethoven.active_melody_mut(project) {
                m.name = new_name;
            }
            app.bethoven.renaming_melody = None;
        }
        if ui.button("Cancel").clicked() {
            app.bethoven.renaming_melody = None;
        }
    } else if ui.button("Rename").clicked() {
        app.bethoven.renaming_melody = Some(current_name);
    }

    if ui.button("Delete").clicked()
        && let Some(id) = app.bethoven.active_melody_id
    {
        project.melodies.retain(|m| m.id != id);
        app.bethoven.active_melody_id = None;
        app.bethoven.active_section_id = None;
        app.bethoven.selection.clear();
        app.bethoven.mark_dirty();
    }

    if ui
        .button("Export to Project Track")
        .on_hover_text("Renders every section in order and adds it as a new, fully editable clip on a new track.")
        .clicked()
        && let Some(melody) = app.bethoven.active_melody(project)
    {
        let samples = melody::render_melody(melody, app.sample_rate_hz);
        let name = melody.name.clone();
        let track_id = project.add_track();
        if let Some(track) = project.track_mut(track_id) {
            track.name = name.clone();
        }
        project.add_clip_channels(track_id, name, 0, samples, 2);
    }
}

fn draw_transport_controls(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    let mut bpm = app.bethoven.active_melody(project).map(|m| m.bpm).unwrap_or(120.0);
    let slider = ui.add(egui::Slider::new(&mut bpm, 40.0..=240.0).text("BPM"));
    if slider.changed() {
        if let Some(melody) = app.bethoven.active_melody_mut(project) {
            melody.bpm = bpm;
        }
        app.bethoven.mark_dirty();
    } else if slider.hovered() {
        let notches = crate::gui::wheel_notches(ui);
        if notches != 0.0 {
            let new_bpm = (bpm + notches.round()).clamp(40.0, 240.0);
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                melody.bpm = new_bpm;
            }
            app.bethoven.mark_dirty();
        }
    }

    let icon = if app.bethoven.is_playing() { "\u{23f8}" } else { "\u{25b6}" };
    if ui.button(icon).on_hover_text("Play/Pause (Space)").clicked() {
        app.bethoven.toggle_playback();
    }
}

fn draw_section_tabs(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    ui.label("Section:");
    let sections: Vec<(u32, String)> = app
        .bethoven
        .active_melody(project)
        .map(|m| m.sections.iter().map(|s| (s.id, s.name.clone())).collect())
        .unwrap_or_default();
    for (id, name) in sections {
        if ui.selectable_label(app.bethoven.active_section_id == Some(id), name).clicked() {
            app.bethoven.active_section_id = Some(id);
            app.bethoven.selection.clear();
            app.bethoven.mark_dirty();
        }
    }
    if ui.button("+ Section").clicked() {
        app.bethoven.new_section_draft = Some(NewSectionDraft::default());
    }
    if let Some(sel_id) = app.bethoven.active_section_id
        && ui.button("Delete Section").clicked()
    {
        if let Some(melody) = app.bethoven.active_melody_mut(project) {
            melody.remove_section(sel_id);
        }
        app.bethoven.active_section_id = None;
        app.bethoven.selection.clear();
        app.bethoven.mark_dirty();
    }

    if let Some((root, scale_index)) = app.bethoven.active_section(project).map(|s| (s.root, s.scale_index)) {
        ui.separator();
        let mut new_root = root;
        egui::ComboBox::from_id_salt("bethoven_root_combo")
            .selected_text(scales::ROOT_NAMES[root as usize])
            .show_ui(ui, |ui| {
                for (i, name) in scales::ROOT_NAMES.iter().enumerate() {
                    if ui.selectable_label(root as usize == i, *name).clicked() {
                        new_root = i as u8;
                    }
                }
            });
        let mut new_scale = scale_index;
        egui::ComboBox::from_id_salt("bethoven_scale_combo")
            .selected_text(scales::SCALES[scale_index].0)
            .show_ui(ui, |ui| {
                for (i, (name, _)) in scales::SCALES.iter().enumerate() {
                    if ui.selectable_label(scale_index == i, *name).clicked() {
                        new_scale = i;
                    }
                }
            });
        if new_root != root || new_scale != scale_index {
            if let Some(section) = app.bethoven.active_section_mut(project) {
                section.root = new_root;
                section.scale_index = new_scale;
            }
            app.bethoven.mark_dirty();
        }
    }
}

fn draw_instrument_picker(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    ui.label("Set instrument:");
    let enabled = !app.bethoven.selection.is_empty();
    for inst in Instrument::ALL {
        let color = super::piano_roll::instrument_color(inst);
        let button = egui::Button::new(egui::RichText::new(inst.name()).color(egui::Color32::BLACK)).fill(color);
        if ui.add_enabled(enabled, button).clicked() {
            let ids: Vec<u32> = app.bethoven.selection.iter().copied().collect();
            if let Some(section) = app.bethoven.active_section_mut(project) {
                section.set_instrument(&ids, inst);
            }
            app.bethoven.mark_dirty();
        }
    }
}

/// A small floating window collecting a new section's name/length/scale
/// before it's actually created — opened by "+ Section" above.
fn draw_new_section_popup(ctx: &egui::Context, app: &mut RakunatorApp) {
    if app.bethoven.new_section_draft.is_none() {
        return;
    }
    let mut create = false;
    let mut cancel = false;
    egui::Window::new("New Section").collapsible(false).resizable(false).show(ctx, |ui| {
        let Some(draft) = app.bethoven.new_section_draft.as_mut() else { return };
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut draft.name);
        });
        ui.add(egui::Slider::new(&mut draft.bars, 1..=64).text("Bars"));
        egui::ComboBox::from_label("Root")
            .selected_text(scales::ROOT_NAMES[draft.root as usize])
            .show_ui(ui, |ui| {
                for (i, name) in scales::ROOT_NAMES.iter().enumerate() {
                    if ui.selectable_label(draft.root as usize == i, *name).clicked() {
                        draft.root = i as u8;
                    }
                }
            });
        egui::ComboBox::from_label("Scale")
            .selected_text(scales::SCALES[draft.scale_index].0)
            .show_ui(ui, |ui| {
                for (i, (name, _)) in scales::SCALES.iter().enumerate() {
                    if ui.selectable_label(draft.scale_index == i, *name).clicked() {
                        draft.scale_index = i;
                    }
                }
            });
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                create = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });

    if create {
        let Some(draft) = app.bethoven.new_section_draft.take() else { return };
        let mut project = app.project.lock().unwrap();
        if let Some(melody) = app.bethoven.active_melody_mut(&mut project) {
            let id = melody.add_section(draft.name, draft.root, draft.scale_index, melody::bars_to_ticks(draft.bars));
            app.bethoven.active_section_id = Some(id);
        }
        app.bethoven.selection.clear();
        app.bethoven.mark_dirty();
    } else if cancel {
        app.bethoven.new_section_draft = None;
    }
}
