use crate::audio_engine::recorder::{to_project_format, Recorder};
use crate::audio_engine::AudioEngine;
use crate::project::{import, ClipId, Project, TrackId};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{
    export_dialog, export_dialog::ExportDialogState, help_dialog, project_file_dialog,
    project_file_dialog::ProjectFileDialogState, settings_persistence, timeline,
    timeline::TimelineState, toast, toolbar, toolbar::EffectsState, track_view, wave_dialog,
    wave_dialog::WaveDialogState, HEADER_WIDTH, ROW_HEIGHT, RULER_HEIGHT, TRACK_ROW_GAP,
    TRACK_ROW_STEP,
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
    /// Live filter text for the Help window's search box — matched
    /// case-insensitively against each row's action/description.
    pub(super) help_search: String,
    /// Where playback was sitting when it was last started; pausing jumps
    /// back here, so tapping Space previews repeatedly from the same spot.
    pub(super) play_start_position: Option<u64>,
    /// Base name of the last project file saved/loaded (no directory or
    /// extension); the Export dialog defaults its filename to this.
    pub(super) project_name: Option<String>,
    /// The in-progress microphone capture, if the Record button is active.
    /// Its presence locks the rest of the UI (see `ui`) so edits can't race
    /// with the clip that's growing live on `record_target_track`.
    pub(super) recording: Option<Recorder>,
    /// When the current recording started, for the toolbar's elapsed-time
    /// readout.
    pub(super) record_started_at: Option<std::time::Instant>,
    /// Where the playhead was sitting when recording started — the live
    /// clip grows from here up to the current playhead, the real clip gets
    /// baked in starting here once recording stops, and the playhead
    /// returns here at that point too.
    pub(super) record_start_sample: Option<u64>,
    /// The track the live recording is landing on: the single track that
    /// was selected when recording started, or a freshly created one —
    /// decided (and, if new, created) in `start_recording`, so the track
    /// already exists in place from the very first frame of the capture.
    pub(super) record_target_track: Option<TrackId>,
    /// The timeline's horizontal scroll position when recording started —
    /// restored once recording stops, since the view auto-follows the live
    /// capture as it grows (see `timeline::draw_lane`) and would otherwise
    /// end up wherever that follow scroll last left it.
    pub(super) record_scroll_start: Option<f32>,
    /// Mono downmix of everything captured so far this take, accumulated
    /// frame by frame from `Recorder::samples_since` (see `ui`) and drawn
    /// live on `record_target_track`'s lane in place of the real clip,
    /// which only gets created once recording stops.
    pub(super) record_preview_samples: Vec<f32>,
    /// How many raw (device-native, interleaved) samples of the current
    /// take have already been folded into `record_preview_samples`, so each
    /// frame only downmixes what's new instead of re-copying the whole take.
    pub(super) record_preview_read_len: usize,
    /// When the input last clipped, so the live recording waveform's hot
    /// color stays visible for a moment instead of flashing for a single
    /// frame.
    pub(super) record_clip_flash: Option<std::time::Instant>,
    /// A brief confirmation message (e.g. "Project saved") and when it was
    /// shown, faded out and cleared by `toast::draw` a couple of seconds
    /// later.
    pub(super) toast: Option<(String, std::time::Instant)>,
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
            effects: settings_persistence::load_effects_settings(),
            help_open: false,
            help_search: String::new(),
            play_start_position: None,
            project_name: None,
            recording: None,
            record_started_at: None,
            record_start_sample: None,
            record_target_track: None,
            record_scroll_start: None,
            record_preview_samples: Vec::new(),
            record_preview_read_len: 0,
            record_clip_flash: None,
            toast: None,
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

    /// Opens the default microphone and starts capturing, from wherever the
    /// playhead currently sits. Also starts playback, so the timeline
    /// scrolls and existing tracks are audible as a click/backing reference
    /// while recording. Lands on an existing track when one is a clear fit
    /// (see `Project::recording_target_track` — the selected track, the
    /// clip selection's track, or the track last clicked in the timeline);
    /// otherwise a new track is created immediately, so there's somewhere
    /// for the live waveform to grow in place from the first frame. Does
    /// nothing if already recording, or if there's no input device
    /// available.
    pub(super) fn start_recording(&mut self) {
        if self.recording.is_some() {
            return;
        }
        match Recorder::start() {
            Some(recorder) => {
                self.recording = Some(recorder);
                self.record_started_at = Some(std::time::Instant::now());
                self.record_start_sample = Some(self.engine.position());
                self.record_scroll_start = Some(self.timeline.scroll_x_samples);
                self.record_preview_samples.clear();
                self.record_preview_read_len = 0;

                let last_click_track = self.timeline.last_click.map(|(track, _)| track);
                let mut project = self.project.lock().unwrap();
                self.record_target_track = Some(match project.recording_target_track(last_click_track) {
                    Some(id) => id,
                    None => {
                        let name = format!("Recording {}", project.tracks.len() + 1);
                        let id = project.add_track();
                        if let Some(track) = project.track_mut(id) {
                            track.name = name;
                        }
                        id
                    }
                });
                drop(project);

                self.start_playback();
            }
            None => eprintln!("recording failed: no microphone/input device available"),
        }
    }

    /// Stops capturing and playback, returns the playhead to where
    /// recording started, and bakes whatever was recorded into a clip at
    /// that position on `record_target_track` (created up front in
    /// `start_recording`), converted to the project's sample rate and
    /// channel layout (see `to_project_format`). Does nothing if not
    /// currently recording.
    pub(super) fn stop_recording(&mut self) {
        let Some(recorder) = self.recording.take() else {
            return;
        };
        self.record_started_at = None;
        self.record_clip_flash = None;
        self.record_preview_samples.clear();
        self.record_preview_read_len = 0;
        if let Some(scroll) = self.record_scroll_start.take() {
            self.timeline.scroll_x_samples = scroll;
        }
        let start_sample = self.record_start_sample.take().unwrap_or(0);
        let target_track = self.record_target_track.take();
        self.engine.pause();
        self.engine.seek(start_sample);
        self.play_start_position = None;
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

        let Some(target) = target_track.filter(|id| project.track(*id).is_some()) else {
            return;
        };
        let clip_index = project.track(target).map_or(0, |t| t.clips.len());
        let name = format!("Recording {}", clip_index + 1);
        project.add_clip_channels(target, name, start_sample, samples, out_channels);
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
        handle_record_shortcut(ui, self);
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

        // Colors the live recording waveform drawn on its track's lane
        // below, the same way the old standalone monitor strip's color did:
        // normal accent blue, amber once it's hot, red once it's actually
        // clipped (latched for a moment past the last clipped sample so a
        // single hot sample doesn't just flash past in one frame).
        let mut record_color = egui::Color32::from_rgb(120, 170, 255);
        if let Some(recorder) = self.recording.as_ref() {
            if recorder.take_clipped() {
                self.record_clip_flash = Some(std::time::Instant::now());
            }
            let clipping_recently = self
                .record_clip_flash
                .is_some_and(|t| t.elapsed() < Duration::from_millis(1200));
            record_color = if clipping_recently {
                egui::Color32::from_rgb(220, 60, 60)
            } else if recorder.peak_level() > 0.85 {
                egui::Color32::from_rgb(230, 200, 60)
            } else {
                record_color
            };

            // Fold whatever's been captured since the last frame into the
            // running mono preview, only consuming whole frames (`channels`
            // samples at a time) so a partial trailing frame is picked back
            // up next time instead of permanently shifting the channel
            // alignment of everything downmixed after it.
            let (new_raw, total) = recorder.samples_since(self.record_preview_read_len);
            let channels = recorder.channels.max(1);
            let usable = (new_raw.len() / channels) * channels;
            self.record_preview_samples.extend(
                new_raw[..usable]
                    .chunks_exact(channels)
                    .map(|frame| frame.iter().sum::<f32>() / channels as f32),
            );
            self.record_preview_read_len = total - (new_raw.len() - usable);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_enabled_ui(!recording, |ui| {
                let project = &self.project;
                let engine = &self.engine;
                let timeline_state = &mut self.timeline;
                let sample_rate_hz = self.sample_rate_hz;
                let playhead = engine.position();
                let record_target_track = self.record_target_track;
                let record_start_sample = self.record_start_sample.unwrap_or(0);
                let record_preview_samples = &self.record_preview_samples;

                // Every clip edge in the project, used by the timeline's
                // move/trim snapping (and click-to-seek snapping — see
                // `timeline::snap_click`) so clips can align to each
                // other's start/end points across tracks. Computed once up
                // front, ahead of the ruler, so both it and the lanes below
                // share the same snapshot.
                let snap_targets: Vec<u64> = {
                    let project = project.lock().unwrap();
                    project
                        .tracks
                        .iter()
                        .flat_map(|t| &t.clips)
                        .flat_map(|c| [c.start_sample, c.end_sample()])
                        .collect()
                };
                let content_end_sample = snap_targets.iter().copied().max().unwrap_or(0);

                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::Vec2::new(HEADER_WIDTH, RULER_HEIGHT), |_ui| {});
                    timeline::draw_ruler(ui, timeline_state, playhead, sample_rate_hz, engine, &snap_targets);
                });

                timeline_state.clear_snap_indicator();
                let mut track_count = 0usize;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut project = project.lock().unwrap();
                    timeline_state.tracks_top_y = ui.cursor().top();

                    let track_ids: Vec<_> = project.tracks.iter().map(|t| t.id).collect();
                    track_count = track_ids.len();
                    for track_id in &track_ids {
                        let live_recording = (record_target_track == Some(*track_id)).then_some(timeline::LiveRecording {
                            start_sample: record_start_sample,
                            samples: record_preview_samples.as_slice(),
                            color: record_color,
                        });
                        ui.horizontal(|ui| {
                            ui.allocate_ui(egui::Vec2::new(HEADER_WIDTH, ROW_HEIGHT), |ui| {
                                track_view::draw_header(ui, &mut project, *track_id, engine, timeline_state);
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
                                live_recording.as_ref(),
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
        let toast_active = toast::draw(ui.ctx(), self);

        if self.engine.is_playing() || recording || toast_active || self.timeline.click_snap_flash_active() {
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

/// Plain "R" starts recording; while recording, plain "R" or Space stops it
/// (Space also doubles as play/pause when not recording, handled below in
/// `handle_shortcuts`). Runs unconditionally, even while recording, unlike
/// the rest of the shortcuts which are locked out during a capture.
fn handle_record_shortcut(ui: &egui::Ui, app: &mut RakunatorApp) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (r, space) = ui.ctx().input(|i| {
        (
            !i.modifiers.any() && i.key_pressed(egui::Key::R),
            !i.modifiers.any() && i.key_pressed(egui::Key::Space),
        )
    });
    if app.recording.is_some() {
        if r || space {
            app.stop_recording();
            // `recording` flips to false for the rest of this frame, so
            // without this, `handle_shortcuts` would see the same Space (or
            // plain R) key press right below and immediately toggle
            // playback back on — consume both so stopping a recording never
            // also resumes playback in the same frame.
            ui.ctx().input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key { key: egui::Key::R | egui::Key::Space, pressed: true, .. }
                    )
                });
            });
        }
    } else if r {
        app.start_recording();
    }
}

/// How far Left/Right nudge selected clips (or the playhead) per key press,
/// in *screen pixels* at the timeline's current zoom — rather than a fixed
/// sample/time amount, so it stays a small, precise nudge when zoomed in
/// (where a fixed time amount would span many pixels and make it
/// impossible to land on a specific spot) and scales up proportionally
/// when zoomed out. Shift+Left/Right jump to the very start/end instead of
/// nudging.
const NUDGE_PIXELS: f32 = 4.0;

/// The nudge step in samples for the timeline's current zoom level (see
/// `NUDGE_PIXELS`) — always at least 1 sample so a key press never does
/// nothing, however far zoomed in.
fn nudge_samples(app: &RakunatorApp) -> i64 {
    ((NUDGE_PIXELS / app.timeline.px_per_sample).round() as i64).max(1)
}

/// Ctrl/Cmd+X/C/V/D cut/copy/paste/duplicate the selected clip(s) — or, if
/// one or more tracks are selected instead, Ctrl+D duplicates each of them
/// directly below itself (see `Project::duplicate_track`); Ctrl+F /
/// Ctrl+Shift+F fade the effect targets in/out; Ctrl+L mutes them; Ctrl+N
/// adds a new track; Ctrl+M toggles the window between maximized and
/// restored; Ctrl+Escape quits the application; Left/Right nudges the
/// selected clip(s) in time; plain Space toggles play/pause (resuming from
/// wherever it was paused); plain S splits the selected clip(s) at the
/// playhead. All keyboard handling is skipped while a text field (e.g. a
/// track name) has focus, so typing a space or an "s" doesn't hijack the
/// transport.
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

    let toggle_fullscreen = ui.ctx().input(|i| i.key_pressed(egui::Key::F11));
    if toggle_fullscreen {
        let is_fullscreen = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
    }

    let toggle_maximized = ui
        .ctx()
        .input(|i| i.modifiers.command && i.key_pressed(egui::Key::M));
    if toggle_maximized {
        let is_maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
    }

    let quit = ui
        .ctx()
        .input(|i| i.modifiers.command && i.key_pressed(egui::Key::Escape));
    if quit {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // Closes every currently-open dialog (Create Wave, Project File,
    // Export Project, Edit Effect Steps, Help) — whichever happen to be
    // open, since more than one can be up at once.
    let close_dialogs = ui.ctx().input(|i| i.modifiers.command && i.key_pressed(egui::Key::W));
    if close_dialogs {
        app.wave_dialog.open = false;
        app.project_file_dialog.open = false;
        app.export_dialog.open = false;
        app.effects.close_settings();
        app.help_open = false;
    }

    if undo {
        app.project.lock().unwrap().undo();
    }
    if redo {
        app.project.lock().unwrap().redo();
    }

    let add_track = ui
        .ctx()
        .input(|i| i.modifiers.command && i.key_pressed(egui::Key::N));
    if add_track {
        app.project.lock().unwrap().add_track();
    }

    let (save, save_as) = ui.ctx().input(|i| {
        (
            i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S),
            i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S),
        )
    });
    if save {
        project_file_dialog::save_current(app);
    } else if save_as {
        project_file_dialog::save_as(app);
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

    if duplicate && !project.selected_tracks.is_empty() {
        let track_ids: Vec<TrackId> = project.selected_tracks.iter().copied().collect();
        for id in track_ids {
            project.duplicate_track(id);
        }
        return;
    }

    let selected_ids: Vec<ClipId> = project.selection.iter().copied().collect();
    if selected_ids.is_empty() {
        // With no clip selected, these move the playhead itself instead of
        // a clip that isn't there.
        if nudge_left || nudge_right {
            let nudge = nudge_samples(app);
            let current_pos = app.engine.position();
            let delta = if nudge_left { -nudge } else { nudge };
            let naive_new_pos = (current_pos as i64 + delta).max(0) as u64;
            // Stop exactly at the nearest clip edge instead of stepping
            // past it in one nudge, mirroring the selected-clip nudge
            // below — otherwise a step bigger than the remaining distance
            // silently skips over a clip boundary instead of docking
            // against it.
            let edges: Vec<u64> = project
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .flat_map(|c| [c.start_sample, c.end_sample()])
                .collect();
            let blocking_edge = if nudge_right {
                edges.iter().copied().filter(|&e| e > current_pos && e <= naive_new_pos).min()
            } else {
                edges.iter().copied().filter(|&e| e < current_pos && e >= naive_new_pos).max()
            };
            let new_pos = blocking_edge.unwrap_or(naive_new_pos);
            app.engine.seek(new_pos);
            if let Some(edge) = blocking_edge {
                app.timeline.flash_snap(edge);
            }
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
        let nudge = nudge_samples(app);
        let delta = if nudge_left { -nudge } else { nudge };
        for id in &selected_ids {
            if let Some(track_id) = project.find_clip_track(*id) {
                let current = project
                    .track(track_id)
                    .and_then(|t| t.clips.iter().find(|c| c.id == *id))
                    .map(|c| (c.start_sample, c.len_samples()));
                if let Some((current_start, len)) = current {
                    let naive_new_start = (current_start as i64 + delta).max(0) as u64;
                    // Stop exactly at the nearest other clip's edge on this
                    // track instead of nudging straight through/past it in
                    // one step — otherwise a nudge step bigger than the
                    // remaining gap silently creates (or worsens) an
                    // overlap instead of docking against it.
                    let other_edges: Vec<u64> = project
                        .track(track_id)
                        .map(|t| {
                            t.clips
                                .iter()
                                .filter(|c| c.id != *id)
                                .flat_map(|c| [c.start_sample, c.end_sample()])
                                .collect()
                        })
                        .unwrap_or_default();
                    let blocking_edge = if nudge_right {
                        let naive_new_end = naive_new_start + len;
                        let current_end = current_start + len;
                        other_edges.iter().copied().filter(|&e| e > current_end && e <= naive_new_end).min()
                    } else {
                        other_edges.iter().copied().filter(|&e| e < current_start && e >= naive_new_start).max()
                    };
                    let new_start = match blocking_edge {
                        Some(edge) if nudge_right => edge.saturating_sub(len),
                        Some(edge) => edge,
                        None => naive_new_start,
                    };
                    project.move_clip(*id, track_id, new_start);
                    // Same yellow flash as a click snapping to an edge, so
                    // stopping here (instead of nudging straight through)
                    // reads as deliberate, not like the key press did
                    // nothing.
                    if let Some(edge) = blocking_edge {
                        app.timeline.flash_snap(edge);
                    }
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
