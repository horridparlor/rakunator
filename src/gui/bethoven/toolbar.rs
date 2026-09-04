//! Bethoven's own top bar: melody save/switch/export, section tabs +
//! scale/root picker, tempo, play/pause, and the per-selection instrument
//! picker.

use super::{ExportDraft, NewSectionDraft, RakunatorApp};
use crate::bethoven::melody::{self, Melody};
use crate::bethoven::scales;
use crate::bethoven::Instrument;
use crate::project::Project;
use std::collections::HashSet;

pub(super) fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    // A little breathing room between the toolbar's rows, and a left inset
    // so they don't butt right up against the window's edge.
    ui.spacing_mut().item_spacing.y = 8.0;
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.vertical(|ui| {
            // Cloning the `Arc` first (rather than locking `app.project`
            // directly) keeps this guard from borrowing `app` itself, so
            // the closures below can still capture `app` by unique
            // reference to reach `app.bethoven`.
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
            ui.horizontal_wrapped(|ui| {
                draw_note_track_controls(ui, app, &mut project);
            });
        });
    });
    let ctx = ui.ctx().clone();
    draw_new_section_popup(&ctx, app);
    draw_export_popup(&ctx, app);
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
                    project.last_melody_id = Some(id);
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
        project.last_melody_id = Some(id);
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
        .button("Export")
        .on_hover_text(
            "Renders the current section and adds it as a new, fully editable clip on a new track \u{2014} \
             pick which instruments to include first.",
        )
        .clicked()
    {
        open_export_draft(app, project, false);
    }
    if ui
        .button("Export Instruments")
        .on_hover_text(
            "Renders each instrument actually used in the current section onto its own new track \u{2014} \
             pick which ones first.",
        )
        .clicked()
    {
        open_export_draft(app, project, true);
    }

    // Right-aligned within this row (added last, so it claims whatever
    // width the melody controls above didn't use, from the right edge in).
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let fullscreen_label = if app.bethoven.fullscreen { "Restore" } else { "Fullscreen" };
        if ui
            .button(fullscreen_label)
            .on_hover_text("Toggle the Bethoven window between fullscreen and floating/resizable")
            .clicked()
        {
            app.bethoven.fullscreen = !app.bethoven.fullscreen;
        }
    });
}

fn draw_transport_controls(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    let mut bpm = app.bethoven.active_melody(project).map(|m| m.bpm).unwrap_or(120.0);
    let slider = ui.add(egui::Slider::new(&mut bpm, 40.0..=240.0).text("BPM"));
    if slider.drag_started() {
        app.bethoven.record_undo(project);
    }
    if slider.changed() {
        if let Some(melody) = app.bethoven.active_melody_mut(project) {
            melody.bpm = bpm;
        }
        app.bethoven.mark_dirty();
    } else if slider.hovered() {
        let notches = crate::gui::wheel_notches(ui);
        if notches != 0.0 {
            app.bethoven.record_undo(project);
            let new_bpm = (bpm + notches.round()).clamp(40.0, 240.0);
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                melody.bpm = new_bpm;
            }
            app.bethoven.mark_dirty();
        }
    }

    let icon = if app.bethoven.is_playing() { "\u{23f8}" } else { "\u{25b6}" };
    if ui.button(icon).on_hover_text("Play/Pause (Space)").clicked() {
        let sample_rate_hz = app.sample_rate_hz;
        app.bethoven.toggle_playback(sample_rate_hz, project);
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
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                melody.last_section_id = Some(id);
            }
            app.bethoven.selection.clear();
            app.bethoven.renaming_section = None;
            app.bethoven.mark_dirty();
        }
    }
    if ui.button("+ Section").clicked() {
        app.bethoven.new_section_draft = Some(NewSectionDraft::default());
    }
    if let Some(sel_id) = app.bethoven.active_section_id {
        if app.bethoven.renaming_section.is_some() {
            let mut done = false;
            let mut new_name = String::new();
            if let Some(renaming) = app.bethoven.renaming_section.as_mut() {
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
                if let Some(section) = app.bethoven.active_section_mut(project) {
                    section.name = new_name;
                }
                app.bethoven.renaming_section = None;
            }
            if ui.button("Cancel").clicked() {
                app.bethoven.renaming_section = None;
            }
        } else if ui.button("Rename Section").clicked() {
            let current_name = app.bethoven.active_section(project).map(|s| s.name.clone()).unwrap_or_default();
            app.bethoven.renaming_section = Some(current_name);
        }
        if ui.button("Delete Section").clicked() {
            app.bethoven.record_undo(project);
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                melody.remove_section(sel_id);
            }
            app.bethoven.active_section_id = None;
            app.bethoven.selection.clear();
            app.bethoven.renaming_section = None;
            app.bethoven.mark_dirty();
        }
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
            app.bethoven.record_undo(project);
            if let Some(section) = app.bethoven.active_section_mut(project) {
                section.root = new_root;
                section.scale_index = new_scale;
            }
            app.bethoven.mark_dirty();
        }
    }
}

/// The instrument picker works with or without a selection: with one, it
/// (re)assigns the selected notes' instrument; with none, it just changes
/// which instrument new notes get placed with. Either way the button for
/// the currently-active one (the selection's, if it has a single
/// consistent one — otherwise the melody's sticky default) is highlighted.
fn draw_instrument_picker(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    ui.label("Instrument:");
    let active_instrument = app.bethoven.active_melody(project).map(|m| m.default_instrument).unwrap_or(Instrument::Piano);
    for inst in Instrument::ALL {
        let color = super::piano_roll::instrument_color(inst);
        let mut button = egui::Button::new(egui::RichText::new(inst.name()).color(egui::Color32::BLACK)).fill(color);
        if inst == active_instrument {
            button = button.stroke(egui::Stroke::new(2.0, egui::Color32::WHITE));
        }
        if ui.add(button).clicked() {
            let selected: Vec<u32> = app.bethoven.selection.iter().copied().collect();
            if !selected.is_empty() {
                app.bethoven.record_undo(project);
                if let Some(section) = app.bethoven.active_section_mut(project) {
                    section.set_instrument(&selected, inst);
                }
                app.bethoven.mark_dirty();
            }
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                melody.default_instrument = inst;
            }
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
        app.bethoven.record_undo(&project);
        if let Some(melody) = app.bethoven.active_melody_mut(&mut project) {
            let id = melody.add_section(draft.name, draft.root, draft.scale_index, melody::bars_to_ticks(draft.bars));
            melody.last_section_id = Some(id);
            app.bethoven.active_section_id = Some(id);
        }
        app.bethoven.selection.clear();
        app.bethoven.mark_dirty();
    } else if cancel {
        app.bethoven.new_section_draft = None;
    }
}

/// Mute/Solo toggles, one per instrument actually used anywhere in the
/// active melody (across all its sections) — same "M"/"S" toggle style as
/// the main timeline's track headers. Applies to this melody's own preview
/// playback (immediately) and is also the default instrument selection the
/// next time an export dialog is opened, e.g. muting the drums used as a
/// metronome while composing keeps them out of both.
fn draw_note_track_controls(ui: &mut egui::Ui, app: &mut RakunatorApp, project: &mut Project) {
    let used = app.bethoven.active_melody(project).map(|m| m.used_instruments()).unwrap_or_default();
    if used.is_empty() {
        return;
    }
    ui.label("Note Tracks:");
    for inst in used {
        let color = super::piano_roll::instrument_color(inst);
        ui.label(egui::RichText::new(inst.name()).color(color));
        let mut muted = app.bethoven.active_melody(project).is_some_and(|m| m.muted_instruments.contains(&inst));
        let mut soloed = app.bethoven.active_melody(project).is_some_and(|m| m.soloed_instruments.contains(&inst));
        if ui.toggle_value(&mut muted, "M").on_hover_text("Mute this instrument").clicked() {
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                if muted {
                    melody.muted_instruments.insert(inst);
                } else {
                    melody.muted_instruments.remove(&inst);
                }
            }
            app.bethoven.mark_dirty();
        }
        if ui.toggle_value(&mut soloed, "S").on_hover_text("Solo this instrument").clicked() {
            if let Some(melody) = app.bethoven.active_melody_mut(project) {
                if soloed {
                    melody.soloed_instruments.insert(inst);
                } else {
                    melody.soloed_instruments.remove(&inst);
                }
            }
            app.bethoven.mark_dirty();
        }
        ui.add_space(6.0);
    }
}

/// Opens the export instrument-selection popup for either "Export"
/// (`multi_track = false`, one mixed-down track) or "Export Instruments"
/// (`multi_track = true`, one track per instrument) — defaults every
/// used-in-this-section instrument to checked unless it's currently muted
/// (or solo is active elsewhere and it isn't soloed), so the mute/solo row
/// above doubles as a quick way to exclude something (like a metronome
/// drum track) from export too.
fn open_export_draft(app: &mut RakunatorApp, project: &Project, multi_track: bool) {
    let Some(section) = app.bethoven.active_section(project) else { return };
    let used = section.used_instruments();
    if used.is_empty() {
        return;
    }
    let melody = app.bethoven.active_melody(project);
    let checked: HashSet<Instrument> =
        used.iter().copied().filter(|inst| melody.map(|m| m.is_instrument_audible(*inst)).unwrap_or(true)).collect();
    app.bethoven.export_draft = Some(ExportDraft { multi_track, used, checked, focus_export_button: true });
}

/// The instrument-selection popup opened by "Export"/"Export Instruments"
/// above — same focus-the-primary-button-so-Enter-works pattern as the main
/// export dialog's own "Export"/"Overwrite" buttons.
fn draw_export_popup(ctx: &egui::Context, app: &mut RakunatorApp) {
    if app.bethoven.export_draft.is_none() {
        return;
    }
    let multi_track = app.bethoven.export_draft.as_ref().is_some_and(|d| d.multi_track);
    let title = if multi_track { "Export Instruments" } else { "Export" };
    let mut do_export = false;
    let mut cancel = false;
    egui::Window::new(title).collapsible(false).resizable(false).show(ctx, |ui| {
        let Some(draft) = app.bethoven.export_draft.as_mut() else { return };
        ui.label(if draft.multi_track {
            "Each checked instrument becomes its own new track."
        } else {
            "The checked instruments are mixed down onto one new track."
        });
        ui.separator();
        for inst in draft.used.clone() {
            let color = super::piano_roll::instrument_color(inst);
            let mut checked = draft.checked.contains(&inst);
            if ui.checkbox(&mut checked, egui::RichText::new(inst.name()).color(color)).changed() {
                if checked {
                    draft.checked.insert(inst);
                } else {
                    draft.checked.remove(&inst);
                }
            }
        }
        ui.horizontal(|ui| {
            if ui.button("All").clicked() {
                draft.checked = draft.used.iter().copied().collect();
            }
            if ui.button("None").clicked() {
                draft.checked.clear();
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let export_resp = ui.add_enabled(!draft.checked.is_empty(), egui::Button::new("Export"));
            if std::mem::take(&mut draft.focus_export_button) {
                export_resp.request_focus();
            }
            if export_resp.clicked() {
                do_export = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });

    if do_export {
        let Some(draft) = app.bethoven.export_draft.take() else { return };
        perform_export(app, draft);
    } else if cancel {
        app.bethoven.export_draft = None;
    }
}

/// Actually renders and adds the new track(s) for a confirmed export
/// popup — one mixed-down track (checked instruments only) for "Export",
/// or one track per checked instrument for "Export Instruments".
fn perform_export(app: &mut RakunatorApp, draft: ExportDraft) {
    let mut project = app.project.lock().unwrap();
    let melody_name = app.bethoven.active_melody(&project).map(|m| m.name.clone());
    let bpm = app.bethoven.active_melody(&project).map(|m| m.bpm).unwrap_or(120.0);
    let section = app.bethoven.active_section(&project).cloned();
    let (Some(melody_name), Some(section)) = (melody_name, section) else { return };

    let add_track = |project: &mut Project, name: String, samples: Vec<f32>| {
        let track_id = project.add_track();
        if let Some(track) = project.track_mut(track_id) {
            track.name = name.clone();
        }
        project.add_clip_channels(track_id, name, 0, samples, 2);
    };

    if draft.multi_track {
        for inst in draft.used.iter().copied().filter(|i| draft.checked.contains(i)) {
            let samples = melody::render_section_filtered(&section, bpm, app.sample_rate_hz, |i| i == inst);
            let name = format!("{} - {} - {}", melody_name, section.name, inst.name());
            add_track(&mut project, name, samples);
        }
    } else {
        let checked = draft.checked.clone();
        let samples = melody::render_section_filtered(&section, bpm, app.sample_rate_hz, |i| checked.contains(&i));
        let name = format!("{} - {}", melody_name, section.name);
        add_track(&mut project, name, samples);
    }
}
