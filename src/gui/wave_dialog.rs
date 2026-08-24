use crate::project::generate::render_waveform_clip;
use crate::project::TrackId;
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
    let state = &mut app.wave_dialog;

    egui::Window::new("Create Wave").open(&mut open).show(ctx, |ui| {
        egui::ComboBox::from_label("Waveform")
            .selected_text(state.waveform.name())
            .show_ui(ui, |ui| {
                for wf in WAVEFORMS {
                    ui.selectable_value(&mut state.waveform, wf, wf.name());
                }
            });

        ui.add(egui::Slider::new(&mut state.frequency_hz, 20.0..=2000.0).text("Frequency (Hz)"));
        ui.add(egui::Slider::new(&mut state.duration_secs, 0.1..=10.0).text("Duration (s)"));

        let target_name = state
            .target_track
            .and_then(|id| track_list.iter().find(|(tid, _)| *tid == id))
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        egui::ComboBox::from_label("Target Track")
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

        if ui.button("Create").clicked() {
            create = true;
        }
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
}
