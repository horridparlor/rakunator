use crate::audio_engine::recorder::{to_project_format, Recorder};
use crate::audio_engine::AudioEngine;
use crate::project::{import, ClipId, Project, TrackId};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{
    export_dialog, export_dialog::ExportDialogState, help_dialog, project_file_dialog,
    project_file_dialog::ProjectFileDialogState, timeline, timeline::TimelineState, toolbar,
    toolbar::EffectsState, track_view, wave_dialog, wave_dialog::WaveDialogState, HEADER_WIDTH,
    ROW_HEIGHT, RULER_HEIGHT, TRACK_ROW_GAP, TRACK_ROW_STEP,
};

pub struct RakunatorApp {
    pub(super) project: Arc<Mutex<Project>>,
    pub(super) engine: AudioEngine,
    pub(super) sample_rate_hz: u32,
    pub(super) wave_dialog: WaveDialogState,
    pub(super) export_dialog: ExportDialogState,
    pub(super) project_file_dialog: ProjectFileDialogState,
    pub(super) timeline: TimelineState,
    pub(super) effects: EffectsState,
    pub(super) help_open: bool,
    /// Where playback was sitting when it was last started; pausing jumps
    /// back here, so tapping Space previews repeatedly from the same spot.
    pub(super) play_start_position: Option<u64>,
    /// Base name of the last project file saved/loaded (no directory or
    /// extension); the Export dialog defaults its filename to this.
    pub(super) project_name: Option<String>,
    /// The in-progress microphone capture, if the Record button is active.
    /// Its presence locks the rest of the UI (see `ui`) so edits can't race
    /// with the new track/clip that appears once recording stops.
    pub(super) recording: Option<Recorder>,
    /// When the current recording started, for the toolbar's elapsed-time
    /// readout.
    pub(super) record_started_at: Option<std::time::Instant>,
}

impl RakunatorApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        // egui reserves Ctrl/Cmd+scroll to zoom the whole UI; disable that
        // so our own Ctrl+scroll (zoom the timeline) actually receives the
        // scroll delta instead of egui swallowing it first.
        cc.egui_ctx
            .options_mut(|o| o.input_options.zoom_modifier = egui::Modifiers::NONE);
        cc.egui_ctx.set_visuals(modern_visuals());

        let sample_rate_hz = 48_000;
        let project = Arc::new(Mutex::new(Project::new(sample_rate_hz)));
        let engine = AudioEngine::start(Arc::clone(&project));
        RakunatorApp {
            project,
            engine,
            sample_rate_hz,
            wave_dialog: WaveDialogState::default(),
            export_dialog: ExportDialogState::default(),
            project_file_dialog: ProjectFileDialogState::default(),
            timeline: TimelineState::default(),
            effects: EffectsState::default(),
            help_open: false,
            play_start_position: None,
            project_name: None,
            recording: None,
            record_started_at: None,
        }
    }

    /// Starts playback from wherever the playhead currently is, remembering
    /// that spot so `pause_playback` can jump back to it.
    pub(super) fn start_playback(&mut self) {
        self.play_start_position = Some(self.engine.position());
        self.engine.play();
    }

    /// Pauses playback and returns the playhead to where it was when
    /// `start_playback` was last called.
    pub(super) fn pause_playback(&mut self) {
        self.engine.pause();
        if let Some(pos) = self.play_start_position.take() {
            self.engine.seek(pos);
        }
    }

    /// Opens the default microphone and starts capturing. Does nothing if
    /// already recording, or if there's no input device available.
    pub(super) fn start_recording(&mut self) {
        if self.recording.is_some() {
            return;
        }
        match Recorder::start() {
            Some(recorder) => {
                self.recording = Some(recorder);
                self.record_started_at = Some(std::time::Instant::now());
            }
            None => eprintln!("recording failed: no microphone/input device available"),
        }
    }

    /// Stops capturing and bakes whatever was recorded into a fresh track,
    /// named after how long it ran, converted to the project's sample rate
    /// and channel layout (see `to_project_format`). Does nothing if not
    /// currently recording.
    pub(super) fn stop_recording(&mut self) {
        let Some(recorder) = self.recording.take() else {
            return;
        };
        self.record_started_at = None;
        let channels = recorder.channels;
        let device_rate = recorder.sample_rate_hz;
        let raw = recorder.stop();
        if raw.is_empty() {
            return;
        }

        let mut project = self.project.lock().unwrap();
        let (samples, out_channels) = to_project_format(&raw, channels, device_rate, project.sample_rate_hz);
        if samples.is_empty() {
            return;
        }
        let name = format!("Recording {}", project.tracks.len() + 1);
        let target = project.add_track();
        if let Some(track) = project.track_mut(target) {
            track.name = name.clone();
        }
        project.add_clip_channels(target, name, 0, samples, out_channels);
    }
}

/// A dark theme with a consistent accent color (the same blue used
/// throughout the timeline for selection/snap/ghost highlights), applied
/// app-wide for a more cohesive look.
fn modern_visuals() -> egui::Visuals {
    let accent = egui::Color32::from_rgb(120, 170, 255);
    let mut visuals = egui::Visuals::dark();

    visuals.selection.bg_fill = accent;
    visuals.selection.stroke.color = egui::Color32::from_rgb(20, 22, 28);
    visuals.hyperlink_color = accent;

    visuals.widgets.hovered.bg_fill = accent.gamma_multiply(0.35);
    visuals.widgets.hovered.weak_bg_fill = accent.gamma_multiply(0.25);
    visuals.widgets.hovered.bg_stroke.color = accent;
    visuals.widgets.active.bg_fill = accent.gamma_multiply(0.55);
    visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.45);
    visuals.widgets.active.bg_stroke.color = accent;

    let rounding = egui::CornerRadius::same(6);
    visuals.window_corner_radius = rounding;
    visuals.menu_corner_radius = rounding;
    for style in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        style.corner_radius = rounding;
    }

    visuals.panel_fill = egui::Color32::from_rgb(24, 26, 32);
    visuals.window_fill = egui::Color32::from_rgb(28, 30, 36);
    visuals.extreme_bg_color = egui::Color32::from_rgb(18, 19, 24);

    visuals
}

impl eframe::App for RakunatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let recording = self.recording.is_some();

        if !recording {
            handle_shortcuts(ui, self);
            handle_dropped_files(ui, self);
        }

        egui::Panel::top("toolbar")
            .frame(egui::Frame::default().inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 6,
                bottom: 6,
            }))
            .show(ui, |ui| {
                toolbar::draw(ui, self);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_enabled_ui(!recording, |ui| {
                let project = &self.project;
                let engine = &self.engine;
                let timeline_state = &mut self.timeline;
                let sample_rate_hz = self.sample_rate_hz;
                let playhead = engine.position();

                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::Vec2::new(HEADER_WIDTH, RULER_HEIGHT), |_ui| {});
                    timeline::draw_ruler(ui, timeline_state, playhead, sample_rate_hz, engine);
                });

                timeline_state.clear_snap_indicator();
                let mut track_count = 0usize;
                let mut content_end_sample = 0u64;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut project = project.lock().unwrap();
                    timeline_state.tracks_top_y = ui.cursor().top();

                    // Every clip edge in the project, used by the timeline's
                    // move/trim snapping so clips can align to each other's
                    // start/end points across tracks.
                    let snap_targets: Vec<u64> = project
                        .tracks
                        .iter()
                        .flat_map(|t| &t.clips)
                        .flat_map(|c| [c.start_sample, c.end_sample()])
                        .collect();
                    content_end_sample = snap_targets.iter().copied().max().unwrap_or(0);

                    let track_ids: Vec<_> = project.tracks.iter().map(|t| t.id).collect();
                    track_count = track_ids.len();
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
                                &snap_targets,
                                timeline_state,
                                playhead,
                                engine,
                            );
                        });
                        draw_row_gap(ui);
                    }
                });

                draw_marquee_overlay(ui, timeline_state);
                draw_snap_indicator(ui, timeline_state, track_count);

                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::Vec2::new(HEADER_WIDTH, timeline::SCROLLBAR_HEIGHT), |_ui| {});
                    timeline::draw_horizontal_scrollbar(ui, timeline_state, content_end_sample, sample_rate_hz);
                });
            });
        });

        if !recording {
            wave_dialog::draw(ui.ctx(), self);
            export_dialog::draw(ui.ctx(), self);
            project_file_dialog::draw(ui.ctx(), self);
            toolbar::draw_effects_settings_dialog(ui.ctx(), self);
        }
        help_dialog::draw(ui.ctx(), self);

        if self.engine.is_playing() || recording {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}

/// Draws the live Shift+drag marquee-selection rectangle, if one is active.
fn draw_marquee_overlay(ui: &egui::Ui, timeline: &TimelineState) {
    let Some(anchor) = timeline.marquee_anchor() else {
        return;
    };
    let Some(current) = ui.ctx().pointer_interact_pos() else {
        return;
    };
    let rect = egui::Rect::from_two_pos(anchor, current);
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(120, 170, 255, 40));
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 170, 255)),
        egui::StrokeKind::Middle,
    );
}

/// Draws a yellow vertical line spanning every track row at the sample
/// position a clip is currently snapped to while being moved/trimmed, so
/// it's clear which other clip's edge it just aligned with.
fn draw_snap_indicator(ui: &egui::Ui, timeline: &TimelineState, track_count: usize) {
    let Some(sample) = timeline.snap_indicator() else {
        return;
    };
    let Some(x) = timeline.x_for_sample(sample) else {
        return;
    };
    let top = timeline.tracks_top_y;
    let bottom = top + track_count as f32 * TRACK_ROW_STEP;
    ui.painter().line_segment(
        [egui::pos2(x, top), egui::pos2(x, bottom)],
        egui::Stroke::new(2.0, egui::Color32::from_rgb(230, 200, 40)),
    );
}

/// Draws the fixed-height gap (with a centered divider line) between two
/// track rows. Deliberately not `ui.separator()` — its auto-sized spacing
/// would throw off the cross-track drag math in `timeline.rs`, which
/// assumes every row occupies exactly `TRACK_ROW_STEP` pixels.
fn draw_row_gap(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), TRACK_ROW_GAP), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
}

/// How far Left/Right nudge selected clips per key press.
const NUDGE_SECONDS: f32 = 0.05;

/// Ctrl/Cmd+X/C/V/D cut/copy/paste/duplicate the selected clip(s); Ctrl+F /
/// Ctrl+Shift+F fade the effect targets in/out; Ctrl+L mutes them;
/// Left/Right nudges the selected clip(s) in time; plain Space toggles
/// play/pause (resuming from wherever it was paused); plain S splits the
/// selected clip(s) at the playhead. All keyboard handling is skipped while
/// a text field (e.g. a track name) has focus, so typing a space or an "s"
/// doesn't hijack the transport.
fn handle_shortcuts(ui: &egui::Ui, app: &mut RakunatorApp) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }

    // egui-winit intercepts Ctrl/Cmd+X/C/V for its own text clipboard
    // integration: it emits `Event::Cut`/`Copy`/`Paste` instead of a normal
    // `Key::X/C/V` press, so `key_pressed(Key::X)` etc. would never fire —
    // we have to look for those events specifically instead.
    let (cut, copy, paste) = ui.ctx().input(|i| {
        (
            i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
            i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
            i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
        )
    });

    #[allow(clippy::type_complexity)]
    let (
        space,
        duplicate,
        split,
        join,
        nudge_left,
        nudge_right,
        jump_start,
        jump_end,
        fade_in,
        fade_out,
        mute_range,
        repeat_effect,
        undo,
        redo,
        delete,
    ) = ui.ctx().input(|i| {
        (
            i.key_pressed(egui::Key::Space),
            i.modifiers.command && i.key_pressed(egui::Key::D),
            !i.modifiers.any() && i.key_pressed(egui::Key::S),
            i.modifiers.command && i.key_pressed(egui::Key::J),
            !i.modifiers.shift && i.key_pressed(egui::Key::ArrowLeft),
            !i.modifiers.shift && i.key_pressed(egui::Key::ArrowRight),
            i.modifiers.shift && i.key_pressed(egui::Key::ArrowLeft),
            i.modifiers.shift && i.key_pressed(egui::Key::ArrowRight),
            i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::F),
            i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::F),
            i.modifiers.command && i.key_pressed(egui::Key::L),
            i.modifiers.command && i.key_pressed(egui::Key::R),
            i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
            i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z),
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
        )
    });

    if undo {
        app.project.lock().unwrap().undo();
    }
    if redo {
        app.project.lock().unwrap().redo();
    }

    if repeat_effect {
        toolbar::repeat_last_effect(app);
    }

    if space {
        if app.engine.is_playing() {
            app.pause_playback();
        } else {
            app.start_playback();
        }
    }

    if join {
        let selected_ids: Vec<ClipId> = app.project.lock().unwrap().selection.iter().copied().collect();
        if selected_ids.len() >= 2 {
            let mut project = app.project.lock().unwrap();
            let new_ids = project.join_clips(&selected_ids);
            if !new_ids.is_empty() {
                project.selection = new_ids.into_iter().collect();
                project.selected_tracks.clear();
            }
        }
    }

    if fade_in || fade_out {
        let mut project = app.project.lock().unwrap();
        let targets = project.effect_targets();
        for id in targets {
            if fade_in {
                project.apply_fade_in(id);
            } else {
                project.apply_fade_out(id);
            }
        }
    }

    if mute_range {
        let mut project = app.project.lock().unwrap();
        let targets = project.effect_targets();
        for id in targets {
            project.mute_range(id, 0, u64::MAX);
        }
    }

    if !(cut || copy || paste || duplicate || split || nudge_left || nudge_right || jump_start || jump_end || delete)
    {
        return;
    }

    let mut project = app.project.lock().unwrap();

    if paste {
        let fallback = project.tracks.first().map(|t| t.id).map(|t| (t, 0));
        if let Some((track, pos)) = app.timeline.last_click.or(fallback) {
            let new_ids = project.paste(track, pos);
            if !new_ids.is_empty() {
                project.selection = new_ids.into_iter().collect();
                project.selected_tracks.clear();
            }
        }
        return;
    }

    if delete && !project.selected_tracks.is_empty() {
        let track_ids: Vec<TrackId> = project.selected_tracks.iter().copied().collect();
        for id in track_ids {
            project.remove_track(id);
        }
        return;
    }

    let selected_ids: Vec<ClipId> = project.selection.iter().copied().collect();
    if selected_ids.is_empty() {
        // With no clip selected, these move the playhead itself instead of
        // a clip that isn't there.
        if nudge_left || nudge_right {
            let nudge = (app.sample_rate_hz as f32 * NUDGE_SECONDS) as i64;
            let delta = if nudge_left { -nudge } else { nudge };
            let new_pos = (app.engine.position() as i64 + delta).max(0) as u64;
            app.engine.seek(new_pos);
        } else if jump_start {
            app.engine.seek(0);
        } else if jump_end {
            let content_end = project
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .map(|c| c.end_sample())
                .max()
                .unwrap_or(0);
            app.engine.seek(content_end);
        }
        return;
    }

    if cut {
        project.cut_clips(&selected_ids);
    } else if delete {
        project.delete_clips(&selected_ids);
    } else if copy {
        project.copy_clips(&selected_ids);
    } else if duplicate {
        project.duplicate_selection(&selected_ids);
    } else if split {
        let playhead = app.engine.position();
        for id in &selected_ids {
            project.split_clip(*id, playhead);
        }
    } else if nudge_left || nudge_right {
        let nudge = (app.sample_rate_hz as f32 * NUDGE_SECONDS) as i64;
        let delta = if nudge_left { -nudge } else { nudge };
        for id in &selected_ids {
            if let Some(track_id) = project.find_clip_track(*id) {
                let current_start = project
                    .track(track_id)
                    .and_then(|t| t.clips.iter().find(|c| c.id == *id))
                    .map(|c| c.start_sample);
                if let Some(current_start) = current_start {
                    let new_start = (current_start as i64 + delta).max(0) as u64;
                    project.move_clip(*id, track_id, new_start);
                }
            }
        }
    } else if jump_start {
        // Shift+Left: jump the selected clip(s) to the very beginning.
        for id in &selected_ids {
            if let Some(track_id) = project.find_clip_track(*id) {
                project.move_clip(*id, track_id, 0);
            }
        }
    } else if jump_end {
        // Shift+Right: jump the selected clip(s) to right after the end
        // of the project's last clip.
        let content_end = project
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| c.end_sample())
            .max()
            .unwrap_or(0);
        for id in &selected_ids {
            if let Some(track_id) = project.find_clip_track(*id) {
                project.move_clip(*id, track_id, content_end);
            }
        }
    }
}

/// Imports any WAV files dropped onto the window, one new track per file.
/// Other formats are silently skipped for now (only WAV import is
/// supported without adding a dedicated audio-decoder dependency).
fn handle_dropped_files(ui: &egui::Ui, app: &mut RakunatorApp) {
    let dropped = ui.ctx().input(|i| i.raw.dropped_files.clone());
    if dropped.is_empty() {
        return;
    }
    eprintln!("dropped {} file(s):", dropped.len());

    let mut project = app.project.lock().unwrap();
    for file in dropped {
        let path = file.path();
        eprintln!("  {}", path.display());
        let is_wav = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("wav"))
            .unwrap_or(false);
        if !is_wav {
            eprintln!("    skipped: only .wav import is supported right now");
            continue;
        }
        let Some((samples, channels)) = import::load_wav(path, project.sample_rate_hz) else {
            eprintln!("    failed to read as WAV");
            continue;
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported")
            .to_string();
        let track_id = project.add_track();
        if let Some(track) = project.track_mut(track_id) {
            track.name = name.clone();
        }
        project.add_clip_channels(track_id, name, 0, samples, channels);
    }
}
