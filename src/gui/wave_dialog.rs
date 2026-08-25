use crate::project::generate::render_waveform_clip;
use crate::project::{import, TrackId};
use crate::waveform::Waveform;
use std::time::Duration;

use super::RakunatorApp;

const WAVEFORMS: [Waveform; 4] = [
    Waveform::Sine,
    Waveform::Square,
    Waveform::Triangle,
    Waveform::Sawtooth,
];

pub struct WaveDialogState {
    pub open: bool,
    waveform: Waveform,
    frequency_hz: f32,
    duration_secs: f32,
    target_track: Option<TrackId>,
}

impl Default for WaveDialogState {
    fn default() -> Self {
        WaveDialogState {
            open: false,
            waveform: Waveform::Sine,
            frequency_hz: 440.0,
            duration_secs: 2.0,
            target_track: None,
        }
    }
}

/// Draws the "Create Wave" modal: pick a waveform shape, frequency,
/// duration and target track, then render it into a new clip at the
/// start of that track.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.wave_dialog.open {
        return;
    }

    let track_list: Vec<(TrackId, String)> = {
        let project = app.project.lock().unwrap();
        project.tracks.iter().map(|t| (t.id, t.name.clone())).collect()
    };
    let target_is_valid = app
        .wave_dialog
        .target_track
        .map(|id| track_list.iter().any(|(tid, _)| *tid == id))
        .unwrap_or(false);
    if !target_is_valid {
        app.wave_dialog.target_track = track_list.first().map(|(id, _)| *id);
    }

    let mut open = true;
    let mut create = false;
    let mut import = false;
    let state = &mut app.wave_dialog;

    egui::Window::new("Create Wave")
        .open(&mut open)
        .resizable(true)
        .frame(super::window_frame(ctx, 1, 1, 1, 1))
        .show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y += 4.0;
        egui::ComboBox::from_label("Waveform")
            .selected_text(state.waveform.name())
            .show_ui(ui, |ui| {
                for wf in WAVEFORMS {
                    ui.selectable_value(&mut state.waveform, wf, wf.name());
                }
            });

        slider_with_scroll(
            ui,
            &mut state.frequency_hz,
            20.0..=2000.0,
            10.0,
            "Frequency (Hz)",
        );
        slider_with_scroll(ui, &mut state.duration_secs, 0.1..=10.0, 0.1, "Duration (s)");

        let target_name = state
            .target_track
            .and_then(|id| track_list.iter().find(|(tid, _)| *tid == id))
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        egui::ComboBox::from_label("Target Track (for \"Create\")")
            .selected_text(target_name)
            .show_ui(ui, |ui| {
                for (id, name) in &track_list {
                    if ui
                        .selectable_label(state.target_track == Some(*id), name.as_str())
                        .clicked()
                    {
                        state.target_track = Some(*id);
                    }
                }
            });

        ui.horizontal(|ui| {
            if ui.button("Create").clicked() {
                create = true;
            }
            ui.separator();
            if ui.button("Import audio file...").clicked() {
                import = true;
            }
        });
        ui.label("(\"Import\" creates a new track. Only .wav import is supported for now.)");

        // This dialog's content is naturally shorter than a manually
        // dragged-taller window: without claiming the leftover space, the
        // window's frame/border snaps back to hug the content every frame
        // instead of visibly growing, making a vertical drag look like it
        // does nothing.
        ui.allocate_space(egui::vec2(0.0, ui.available_height()));
    });

    app.wave_dialog.open = open;

    if create {
        let (waveform, frequency_hz, duration_secs, target_track) = {
            let s = &app.wave_dialog;
            (s.waveform, s.frequency_hz, s.duration_secs, s.target_track)
        };
        if let Some(target) = target_track {
            let mut project = app.project.lock().unwrap();
            let samples = render_waveform_clip(
                waveform,
                frequency_hz,
                Duration::from_secs_f32(duration_secs),
                project.sample_rate_hz,
            );
            let name = format!("{} {}Hz", waveform.name(), frequency_hz as i32);
            project.add_clip(target, name, 0, samples);
        }
        app.wave_dialog.open = false;
    }

    if import {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .pick_file()
        {
            let mut project = app.project.lock().unwrap();
            if let Some((samples, channels)) = import::load_wav(&path, project.sample_rate_hz) {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Imported")
                    .to_string();
                let target = project.add_track();
                if let Some(track) = project.track_mut(target) {
                    track.name = name.clone();
                }
                project.add_clip_channels(target, name, 0, samples, channels);
            } else {
                eprintln!("failed to import {}: not a readable WAV file", path.display());
            }
        }
        app.wave_dialog.open = false;
    }
}

/// A slider that can also be adjusted by scrolling the mouse wheel while
/// hovering over it, in `step` increments per wheel notch.
fn slider_with_scroll(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    label: &str,
) {
    let response = ui.add(egui::Slider::new(value, range.clone()).text(label));
    if response.hovered() {
        let notches = super::wheel_notches(ui);
        if notches != 0.0 {
            *value = (*value + notches.round() * step).clamp(*range.start(), *range.end());
        }
    }
}
