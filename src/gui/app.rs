use crate::audio_engine::AudioEngine;
use crate::project::Project;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{
    export_dialog, export_dialog::ExportDialogState, timeline, timeline::TimelineState, toolbar,
    track_view, wave_dialog, wave_dialog::WaveDialogState, HEADER_WIDTH, ROW_HEIGHT,
};

pub struct RakunatorApp {
    pub(super) project: Arc<Mutex<Project>>,
    pub(super) engine: AudioEngine,
    pub(super) wave_dialog: WaveDialogState,
    pub(super) export_dialog: ExportDialogState,
    pub(super) timeline: TimelineState,
}

impl RakunatorApp {
    pub fn new() -> Self {
        let project = Arc::new(Mutex::new(Project::new(48_000)));
        let engine = AudioEngine::start(Arc::clone(&project));
        RakunatorApp {
            project,
            engine,
            wave_dialog: WaveDialogState::default(),
            export_dialog: ExportDialogState::default(),
            timeline: TimelineState::default(),
        }
    }
}

impl eframe::App for RakunatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        handle_shortcuts(ui, self);

        egui::Panel::top("toolbar").show(ui, |ui| {
            toolbar::draw(ui, self);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let project = &self.project;
            let engine = &self.engine;
            let timeline_state = &mut self.timeline;

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut project = project.lock().unwrap();
                timeline_state.tracks_top_y = ui.cursor().top();
                let playhead = engine.position();

                let track_ids: Vec<_> = project.tracks.iter().map(|t| t.id).collect();
                for track_id in &track_ids {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::Vec2::new(HEADER_WIDTH, ROW_HEIGHT), |ui| {
                            track_view::draw_header(ui, &mut project, *track_id, engine);
                        });
                        timeline::draw_lane(
                            ui,
                            &mut project,
                            *track_id,
                            &track_ids,
                            timeline_state,
                            playhead,
                            engine,
                        );
                    });
                }
            });
        });

        wave_dialog::draw(ui.ctx(), self);
        export_dialog::draw(ui.ctx(), self);

        if self.engine.is_playing() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}

/// Ctrl/Cmd+X/C/V/D cut/copy/paste/duplicate the currently selected clip.
/// Paste targets the last clicked timeline position; duplicate places the
/// copy immediately after the original on the same track.
fn handle_shortcuts(ui: &egui::Ui, app: &mut RakunatorApp) {
    let (cut, copy, paste, duplicate) = ui.ctx().input(|i| {
        (
            i.modifiers.command && i.key_pressed(egui::Key::X),
            i.modifiers.command && i.key_pressed(egui::Key::C),
            i.modifiers.command && i.key_pressed(egui::Key::V),
            i.modifiers.command && i.key_pressed(egui::Key::D),
        )
    });
    if !(cut || copy || paste || duplicate) {
        return;
    }

    let mut project = app.project.lock().unwrap();

    if paste {
        let fallback = project.tracks.first().map(|t| t.id).map(|t| (t, 0));
        if let Some((track, pos)) = app.timeline.last_click.or(fallback) {
            project.paste(track, pos);
        }
        return;
    }

    let Some(selected) = project.selection else {
        return;
    };

    if cut {
        project.cut_clip(selected);
    } else if copy {
        project.copy_clip(selected);
    } else if duplicate
        && let Some(origin_track) = project.find_clip_track(selected) {
            let end_sample = project
                .track(origin_track)
                .and_then(|t| t.clips.iter().find(|c| c.id == selected))
                .map(|c| c.end_sample());
            if let Some(new_start) = end_sample {
                project.duplicate_clip(selected, origin_track, new_start);
            }
        }
}
