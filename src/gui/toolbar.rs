use super::RakunatorApp;
use crate::project::{db_to_gain, ClipId, TrackId};

#[derive(Clone, Copy, PartialEq)]
pub enum LastEffect {
    PitchUp,
    PitchDown,
    VolumeUp,
    VolumeDown,
}

/// Pitch/volume step sizes — Up and Down each have their own independent
/// magnitude (edit via the "Edit steps..." dialog) — plus which effect was
/// used most recently, for the Ctrl+R "repeat last effect" shortcut. Fade
/// in/out have their own dedicated shortcuts, so they're never recorded
/// here.
pub struct EffectsState {
    pub pitch_up_step: f32,
    pub pitch_down_step: f32,
    pub volume_up_step_db: f32,
    pub volume_down_step_db: f32,
    /// The two dB endpoints for "Adjustable Fade In" — order doesn't
    /// matter, whichever is lower is treated as the quiet (start) end.
    pub fade_in_point_a_db: f32,
    pub fade_in_point_b_db: f32,
    /// The two dB endpoints for "Adjustable Fade Out" — order doesn't
    /// matter, whichever is higher is treated as the loud (start) end.
    pub fade_out_point_a_db: f32,
    pub fade_out_point_b_db: f32,
    /// For the "Fade Toggle" track effect: whether the earliest clip (by
    /// start position) on each selected track gets the adjustable fade-in
    /// (true) or the adjustable fade-out (false); clips alternate from
    /// there.
    pub fade_toggle_starts_with_in: bool,
    pub last_effect: Option<LastEffect>,
    settings_open: bool,
    editing_pitch_up: f32,
    editing_pitch_down: f32,
    editing_volume_up: f32,
    editing_volume_down: f32,
    editing_fade_in_a: f32,
    editing_fade_in_b: f32,
    editing_fade_out_a: f32,
    editing_fade_out_b: f32,
    editing_fade_toggle_starts_with_in: bool,
}

impl Default for EffectsState {
    fn default() -> Self {
        EffectsState {
            pitch_up_step: 1.0,
            pitch_down_step: 1.0,
            volume_up_step_db: 1.0,
            volume_down_step_db: 1.0,
            fade_in_point_a_db: -6.0,
            fade_in_point_b_db: 0.0,
            fade_out_point_a_db: 0.0,
            fade_out_point_b_db: -6.0,
            fade_toggle_starts_with_in: true,
            last_effect: None,
            settings_open: false,
            editing_pitch_up: 1.0,
            editing_pitch_down: 1.0,
            editing_volume_up: 1.0,
            editing_volume_down: 1.0,
            editing_fade_in_a: -6.0,
            editing_fade_in_b: 0.0,
            editing_fade_out_a: 0.0,
            editing_fade_out_b: -6.0,
            editing_fade_toggle_starts_with_in: true,
        }
    }
}

pub fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    ui.horizontal(|ui| {
        if ui.button("Add Track").clicked() {
            app.project.lock().unwrap().add_track();
        }
        ui.separator();
        if ui.button("Create Wave...").clicked() {
            app.wave_dialog.open = true;
        }
        if ui.button("Project File...").clicked() {
            app.project_file_dialog.open = true;
        }
        if ui.button("Export Project...").clicked() {
            if let Some(name) = &app.project_name {
                app.export_dialog.set_file_name(name.clone());
            }
            app.export_dialog.open = true;
        }
        ui.separator();
        draw_effects_menu(ui, app);
        ui.separator();
        if ui.button("Zoom In").clicked() {
            app.timeline.zoom(1.2);
        }
        if ui.button("Zoom Out").clicked() {
            app.timeline.zoom(1.0 / 1.2);
        }
        ui.separator();
        if ui.button("Play").clicked() {
            app.start_playback();
        }
        if ui.button("Pause").clicked() {
            app.pause_playback();
        }
        if ui.button("Stop").clicked() {
            app.engine.stop();
            app.play_start_position = None;
        }
        ui.separator();
        if ui.button("Help").clicked() {
            app.help_open = true;
        }
    });
}

/// Applies whichever of Pitch Up/Down or Volume Up/Down was last used (at
/// its currently configured step) to the current effect targets. Used by
/// the Ctrl+R "repeat last effect" shortcut.
pub fn repeat_last_effect(app: &mut RakunatorApp) {
    let Some(last) = app.effects.last_effect else {
        return;
    };
    let targets = app.project.lock().unwrap().effect_targets();
    match last {
        LastEffect::PitchUp => {
            let step = app.effects.pitch_up_step;
            apply_to_targets(app, &targets, move |p, id| p.apply_pitch_shift(id, step));
        }
        LastEffect::PitchDown => {
            let step = app.effects.pitch_down_step;
            apply_to_targets(app, &targets, move |p, id| p.apply_pitch_shift(id, -step));
        }
        LastEffect::VolumeUp => {
            let factor = 10f32.powf(app.effects.volume_up_step_db / 20.0);
            apply_to_targets(app, &targets, move |p, id| p.apply_gain(id, factor));
        }
        LastEffect::VolumeDown => {
            let factor = 10f32.powf(-app.effects.volume_down_step_db / 20.0);
            apply_to_targets(app, &targets, move |p, id| p.apply_gain(id, factor));
        }
    }
}

/// Pitch/volume/fade effects applied destructively to whichever clips are
/// targeted: every clip on the selected track (clicking a track header's
/// empty space selects the whole track), or the current multi-clip
/// selection otherwise — see `Project::effect_targets`. "Edit steps..."
/// opens a small dialog to change the Pitch/Volume step sizes (Up and
/// Down independently). Pitch shift here is the classic "tape speed"
/// trick (resample the clip), which changes duration along with pitch —
/// a true pitch-preserving shift would need a phase vocoder or similar,
/// which is out of scope for now.
fn draw_effects_menu(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    ui.menu_button("Effects", |ui| {
        let targets = app.project.lock().unwrap().effect_targets();
        let enabled = !targets.is_empty();
        let pitch_up_step = app.effects.pitch_up_step;
        let pitch_down_step = app.effects.pitch_down_step;
        let volume_up_step = app.effects.volume_up_step_db;
        let volume_down_step = app.effects.volume_down_step_db;

        if ui
            .add_enabled(enabled, egui::Button::new(format!("Pitch Up (+{pitch_up_step:.1} semitone)")))
            .clicked()
        {
            apply_to_targets(app, &targets, move |p, id| p.apply_pitch_shift(id, pitch_up_step));
            app.effects.last_effect = Some(LastEffect::PitchUp);
            ui.close();
        }
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("Pitch Down (-{pitch_down_step:.1} semitone)")),
            )
            .clicked()
        {
            apply_to_targets(app, &targets, move |p, id| p.apply_pitch_shift(id, -pitch_down_step));
            app.effects.last_effect = Some(LastEffect::PitchDown);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(enabled, egui::Button::new(format!("Volume Up (+{volume_up_step:.1} dB)")))
            .clicked()
        {
            let factor = 10f32.powf(volume_up_step / 20.0);
            apply_to_targets(app, &targets, move |p, id| p.apply_gain(id, factor));
            app.effects.last_effect = Some(LastEffect::VolumeUp);
            ui.close();
        }
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("Volume Down (-{volume_down_step:.1} dB)")),
            )
            .clicked()
        {
            let factor = 10f32.powf(-volume_down_step / 20.0);
            apply_to_targets(app, &targets, move |p, id| p.apply_gain(id, factor));
            app.effects.last_effect = Some(LastEffect::VolumeDown);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(enabled, egui::Button::new("Fade In (Ctrl+F)"))
            .clicked()
        {
            apply_to_targets(app, &targets, |p, id| p.apply_fade_in(id));
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Fade Out (Ctrl+Shift+F)"))
            .clicked()
        {
            apply_to_targets(app, &targets, |p, id| p.apply_fade_out(id));
            ui.close();
        }
        ui.separator();
        let fade_in_a = app.effects.fade_in_point_a_db;
        let fade_in_b = app.effects.fade_in_point_b_db;
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!(
                    "Adjustable Fade In ({:.1} dB \u{2192} {:.1} dB)",
                    fade_in_a.min(fade_in_b),
                    fade_in_a.max(fade_in_b)
                )),
            )
            .clicked()
        {
            let start_gain = db_to_gain(fade_in_a.min(fade_in_b));
            let end_gain = db_to_gain(fade_in_a.max(fade_in_b));
            apply_to_targets(app, &targets, move |p, id| p.apply_adjustable_fade(id, start_gain, end_gain));
            ui.close();
        }
        let fade_out_a = app.effects.fade_out_point_a_db;
        let fade_out_b = app.effects.fade_out_point_b_db;
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!(
                    "Adjustable Fade Out ({:.1} dB \u{2192} {:.1} dB)",
                    fade_out_a.max(fade_out_b),
                    fade_out_a.min(fade_out_b)
                )),
            )
            .clicked()
        {
            let start_gain = db_to_gain(fade_out_a.max(fade_out_b));
            let end_gain = db_to_gain(fade_out_a.min(fade_out_b));
            apply_to_targets(app, &targets, move |p, id| p.apply_adjustable_fade(id, start_gain, end_gain));
            ui.close();
        }
        ui.separator();
        let track_targets_enabled = !app.project.lock().unwrap().selected_tracks.is_empty();
        if ui
            .add_enabled(track_targets_enabled, egui::Button::new("Fade Toggle (alternating in/out)"))
            .on_hover_text(
                "For each selected track, alternates adjustable fade-in and adjustable \
                 fade-out across its clips in timeline order.",
            )
            .clicked()
        {
            apply_fade_toggle(app);
            ui.close();
        }
        ui.separator();
        if ui.button("Edit steps...").clicked() {
            app.effects.editing_pitch_up = app.effects.pitch_up_step;
            app.effects.editing_pitch_down = app.effects.pitch_down_step;
            app.effects.editing_volume_up = app.effects.volume_up_step_db;
            app.effects.editing_volume_down = app.effects.volume_down_step_db;
            app.effects.editing_fade_in_a = app.effects.fade_in_point_a_db;
            app.effects.editing_fade_in_b = app.effects.fade_in_point_b_db;
            app.effects.editing_fade_out_a = app.effects.fade_out_point_a_db;
            app.effects.editing_fade_out_b = app.effects.fade_out_point_b_db;
            app.effects.editing_fade_toggle_starts_with_in = app.effects.fade_toggle_starts_with_in;
            app.effects.settings_open = true;
            ui.close();
        }
    });
}

/// For each selected track, sorts its clips by `start_sample` and applies
/// the adjustable fade-in/fade-out alternately (which one starts is set by
/// `EffectsState::fade_toggle_starts_with_in`, editable in "Edit steps...").
fn apply_fade_toggle(app: &mut RakunatorApp) {
    let fade_in_a = app.effects.fade_in_point_a_db;
    let fade_in_b = app.effects.fade_in_point_b_db;
    let fade_in_start = db_to_gain(fade_in_a.min(fade_in_b));
    let fade_in_end = db_to_gain(fade_in_a.max(fade_in_b));

    let fade_out_a = app.effects.fade_out_point_a_db;
    let fade_out_b = app.effects.fade_out_point_b_db;
    let fade_out_start = db_to_gain(fade_out_a.max(fade_out_b));
    let fade_out_end = db_to_gain(fade_out_a.min(fade_out_b));

    let starts_with_in = app.effects.fade_toggle_starts_with_in;

    let mut project = app.project.lock().unwrap();
    let track_ids: Vec<TrackId> = project.selected_tracks.iter().copied().collect();
    for track_id in track_ids {
        let Some(track) = project.track(track_id) else {
            continue;
        };
        let mut clip_ids: Vec<ClipId> = track.clips.iter().map(|c| c.id).collect();
        clip_ids.sort_by_key(|id| track.clips.iter().find(|c| c.id == *id).unwrap().start_sample);

        for (i, clip_id) in clip_ids.into_iter().enumerate() {
            let use_fade_in = if starts_with_in { i % 2 == 0 } else { i % 2 == 1 };
            if use_fade_in {
                project.apply_adjustable_fade(clip_id, fade_in_start, fade_in_end);
            } else {
                project.apply_adjustable_fade(clip_id, fade_out_start, fade_out_end);
            }
        }
    }
}

/// Draws the "Edit steps..." modal for the Pitch/Volume effect step sizes
/// (Up and Down set independently), with OK/Cancel — a real dialog rather
/// than a right-click popup, since right-clicking a button nested inside
/// an already-open menu doesn't reliably open a second, nested popup in
/// egui.
pub fn draw_effects_settings_dialog(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.effects.settings_open {
        return;
    }

    let mut open = true;
    let mut ok = false;
    let mut cancel = false;

    egui::Window::new("Edit Effect Steps").open(&mut open).show(ctx, |ui| {
        egui::Grid::new("effect_steps_grid").num_columns(2).show(ui, |ui| {
            ui.label("Pitch Up (semitones):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_pitch_up).range(0.1..=12.0).speed(0.1));
            ui.end_row();

            ui.label("Pitch Down (semitones):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_pitch_down).range(0.1..=12.0).speed(0.1));
            ui.end_row();

            ui.label("Volume Up (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_volume_up).range(0.1..=24.0).speed(0.1));
            ui.end_row();

            ui.label("Volume Down (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_volume_down).range(0.1..=24.0).speed(0.1));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Adjustable fades: two dB points, in either order — the effect works out which is louder/quieter.");
        egui::Grid::new("fade_points_grid").num_columns(3).show(ui, |ui| {
            ui.label("Fade In points (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_fade_in_a).range(-60.0..=24.0).speed(0.1));
            ui.add(egui::DragValue::new(&mut app.effects.editing_fade_in_b).range(-60.0..=24.0).speed(0.1));
            ui.end_row();

            ui.label("Fade Out points (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_fade_out_a).range(-60.0..=24.0).speed(0.1));
            ui.add(egui::DragValue::new(&mut app.effects.editing_fade_out_b).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Fade Toggle: which comes first on each selected track's earliest clip?");
        ui.horizontal(|ui| {
            ui.radio_value(&mut app.effects.editing_fade_toggle_starts_with_in, true, "Fade In first");
            ui.radio_value(&mut app.effects.editing_fade_toggle_starts_with_in, false, "Fade Out first");
        });

        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });

    if ok {
        app.effects.pitch_up_step = app.effects.editing_pitch_up;
        app.effects.pitch_down_step = app.effects.editing_pitch_down;
        app.effects.volume_up_step_db = app.effects.editing_volume_up;
        app.effects.volume_down_step_db = app.effects.editing_volume_down;
        app.effects.fade_in_point_a_db = app.effects.editing_fade_in_a;
        app.effects.fade_in_point_b_db = app.effects.editing_fade_in_b;
        app.effects.fade_out_point_a_db = app.effects.editing_fade_out_a;
        app.effects.fade_out_point_b_db = app.effects.editing_fade_out_b;
        app.effects.fade_toggle_starts_with_in = app.effects.editing_fade_toggle_starts_with_in;
        app.effects.settings_open = false;
    } else if cancel || !open {
        app.effects.settings_open = false;
    }
}

fn apply_to_targets(app: &RakunatorApp, targets: &[ClipId], f: impl Fn(&mut crate::project::Project, ClipId)) {
    let mut project = app.project.lock().unwrap();
    for &id in targets {
        f(&mut project, id);
    }
}
