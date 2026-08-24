use super::timeline::format_time;
use super::RakunatorApp;
use crate::project::reverb::ReverbParams;
use crate::project::stretch::RampParams;
use crate::project::trip_toggler::TripTogglerParams;
use crate::project::{db_to_gain, ClipId, PanToggleDirection, PanToggleParams, RattleParams, TrackId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LastEffect {
    PitchUp,
    PitchDown,
    VolumeUp,
    VolumeDown,
    TempoUp,
    TempoDown,
}

/// Pitch/volume step sizes — Up and Down each have their own independent
/// magnitude (edit via the "Edit steps..." dialog) — plus which effect was
/// used most recently, for the Ctrl+R "repeat last effect" shortcut. Fade
/// in/out have their own dedicated shortcuts, so they're never recorded
/// here.
///
/// Persisted as an app-level settings file (see `settings_persistence`) —
/// every field down to `last_effect` is meaningful to save; the `editing_*`
/// fields and `settings_open` below are just the "Edit steps..." dialog's
/// in-progress scratch state, so they're `#[serde(skip)]`: on load they get
/// the plain `f32`/`bool` default (0.0/false), which is fine since they're
/// always overwritten from the committed fields the moment that dialog is
/// opened, never read before then.
#[derive(Serialize, Deserialize)]
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

    pub tempo_up_step_percent: f32,
    pub tempo_down_step_percent: f32,

    pub reverb_room_size: f32,
    pub reverb_reverberance: f32,
    pub reverb_hf_damping: f32,
    pub reverb_tone_low: f32,
    pub reverb_tone_high: f32,
    pub reverb_wet_gain_db: f32,
    pub reverb_dry_gain_db: f32,
    pub reverb_stereo_width: f32,
    pub reverb_pre_delay_ms: f32,
    pub reverb_wet_only: bool,

    pub echo_delay_seconds: f32,
    pub echo_decay: f32,

    pub distortion_drive_db: f32,
    pub distortion_threshold: f32,

    pub stretch_initial_tempo_percent: f32,
    pub stretch_final_tempo_percent: f32,
    pub stretch_initial_pitch_semitones: f32,
    pub stretch_final_pitch_semitones: f32,

    pub rattle_pitch_up_semitones: f32,
    pub rattle_pitch_down_semitones: f32,
    pub rattle_tempo_x_percent: f32,
    pub rattle_tempo_y_percent: f32,
    pub rattle_fade_in_a_db: f32,
    pub rattle_fade_in_b_db: f32,
    pub rattle_stretch_initial_tempo_percent: f32,
    pub rattle_stretch_final_tempo_percent: f32,
    pub rattle_stretch_initial_pitch_semitones: f32,
    pub rattle_stretch_final_pitch_semitones: f32,
    /// Total [A, B] clips Rattle generates (always an even number of whole
    /// pairs).
    pub rattle_repeat_count: u32,

    /// Pan Toggle's own high/low dB endpoints (the fade-in side ramps
    /// low -> high, the fade-out side high -> low) and which physical
    /// channel gets the fade-in.
    pub pan_toggle_high_db: f32,
    pub pan_toggle_low_db: f32,
    pub pan_toggle_direction: PanToggleDirection,

    pub tt_high_db: f32,
    pub tt_low_db: f32,
    pub tt_super_mode: bool,
    pub tt_detail: f32,
    pub tt_instant_shift: bool,
    pub tt_instant_high_gain_db: f32,
    pub tt_instant_low_gain_db: f32,
    pub tt_instant_high_fade_start_db: f32,
    pub tt_instant_high_fade_end_db: f32,
    pub tt_instant_low_fade_start_db: f32,
    pub tt_instant_low_fade_end_db: f32,
    pub tt_fade_curve_adjust: f32,
    pub tt_start_high: bool,

    #[serde(skip)]
    settings_open: bool,
    #[serde(skip)]
    editing_pitch_up: f32,
    #[serde(skip)]
    editing_pitch_down: f32,
    #[serde(skip)]
    editing_volume_up: f32,
    #[serde(skip)]
    editing_volume_down: f32,
    #[serde(skip)]
    editing_fade_in_a: f32,
    #[serde(skip)]
    editing_fade_in_b: f32,
    #[serde(skip)]
    editing_fade_out_a: f32,
    #[serde(skip)]
    editing_fade_out_b: f32,
    #[serde(skip)]
    editing_fade_toggle_starts_with_in: bool,
    #[serde(skip)]
    editing_tempo_up: f32,
    #[serde(skip)]
    editing_tempo_down: f32,
    #[serde(skip)]
    editing_reverb_room_size: f32,
    #[serde(skip)]
    editing_reverb_reverberance: f32,
    #[serde(skip)]
    editing_reverb_hf_damping: f32,
    #[serde(skip)]
    editing_reverb_tone_low: f32,
    #[serde(skip)]
    editing_reverb_tone_high: f32,
    #[serde(skip)]
    editing_reverb_wet_gain_db: f32,
    #[serde(skip)]
    editing_reverb_dry_gain_db: f32,
    #[serde(skip)]
    editing_reverb_stereo_width: f32,
    #[serde(skip)]
    editing_reverb_pre_delay_ms: f32,
    #[serde(skip)]
    editing_reverb_wet_only: bool,
    #[serde(skip)]
    editing_echo_delay_seconds: f32,
    #[serde(skip)]
    editing_echo_decay: f32,
    #[serde(skip)]
    editing_distortion_drive_db: f32,
    #[serde(skip)]
    editing_distortion_threshold: f32,
    #[serde(skip)]
    editing_stretch_initial_tempo_percent: f32,
    #[serde(skip)]
    editing_stretch_final_tempo_percent: f32,
    #[serde(skip)]
    editing_stretch_initial_pitch_semitones: f32,
    #[serde(skip)]
    editing_stretch_final_pitch_semitones: f32,
    #[serde(skip)]
    editing_rattle_pitch_up_semitones: f32,
    #[serde(skip)]
    editing_rattle_pitch_down_semitones: f32,
    #[serde(skip)]
    editing_rattle_tempo_x_percent: f32,
    #[serde(skip)]
    editing_rattle_tempo_y_percent: f32,
    #[serde(skip)]
    editing_rattle_fade_in_a_db: f32,
    #[serde(skip)]
    editing_rattle_fade_in_b_db: f32,
    #[serde(skip)]
    editing_rattle_stretch_initial_tempo_percent: f32,
    #[serde(skip)]
    editing_rattle_stretch_final_tempo_percent: f32,
    #[serde(skip)]
    editing_rattle_stretch_initial_pitch_semitones: f32,
    #[serde(skip)]
    editing_rattle_stretch_final_pitch_semitones: f32,
    #[serde(skip)]
    editing_rattle_repeat_count: u32,

    #[serde(skip)]
    editing_pan_toggle_high_db: f32,
    #[serde(skip)]
    editing_pan_toggle_low_db: f32,
    #[serde(skip)]
    editing_pan_toggle_direction: PanToggleDirection,

    #[serde(skip)]
    editing_tt_high_db: f32,
    #[serde(skip)]
    editing_tt_low_db: f32,
    #[serde(skip)]
    editing_tt_super_mode: bool,
    #[serde(skip)]
    editing_tt_detail: f32,
    #[serde(skip)]
    editing_tt_instant_shift: bool,
    #[serde(skip)]
    editing_tt_instant_high_gain_db: f32,
    #[serde(skip)]
    editing_tt_instant_low_gain_db: f32,
    #[serde(skip)]
    editing_tt_instant_high_fade_start_db: f32,
    #[serde(skip)]
    editing_tt_instant_high_fade_end_db: f32,
    #[serde(skip)]
    editing_tt_instant_low_fade_start_db: f32,
    #[serde(skip)]
    editing_tt_instant_low_fade_end_db: f32,
    #[serde(skip)]
    editing_tt_fade_curve_adjust: f32,
    #[serde(skip)]
    editing_tt_start_high: bool,
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

            tempo_up_step_percent: 10.0,
            tempo_down_step_percent: 10.0,

            reverb_room_size: 75.0,
            reverb_reverberance: 50.0,
            reverb_hf_damping: 50.0,
            reverb_tone_low: 100.0,
            reverb_tone_high: 100.0,
            reverb_wet_gain_db: -1.0,
            reverb_dry_gain_db: -1.0,
            reverb_stereo_width: 100.0,
            reverb_pre_delay_ms: 10.0,
            reverb_wet_only: false,

            echo_delay_seconds: 1.0,
            echo_decay: 0.5,

            distortion_drive_db: 0.0,
            distortion_threshold: 0.8,

            stretch_initial_tempo_percent: 0.0,
            stretch_final_tempo_percent: 0.0,
            stretch_initial_pitch_semitones: 0.0,
            stretch_final_pitch_semitones: 0.0,

            rattle_pitch_up_semitones: 1.0,
            rattle_pitch_down_semitones: 1.0,
            rattle_tempo_x_percent: 10.0,
            rattle_tempo_y_percent: 10.0,
            rattle_fade_in_a_db: -6.0,
            rattle_fade_in_b_db: 0.0,
            rattle_stretch_initial_tempo_percent: 0.0,
            rattle_stretch_final_tempo_percent: 0.0,
            rattle_stretch_initial_pitch_semitones: 0.0,
            rattle_stretch_final_pitch_semitones: 0.0,
            rattle_repeat_count: 24,

            pan_toggle_high_db: 6.0,
            pan_toggle_low_db: -4.0,
            pan_toggle_direction: PanToggleDirection::Left,

            tt_high_db: 4.0,
            tt_low_db: -4.0,
            tt_super_mode: false,
            tt_detail: 1.0,
            tt_instant_shift: false,
            tt_instant_high_gain_db: 2.0,
            tt_instant_low_gain_db: -6.0,
            tt_instant_high_fade_start_db: 4.0,
            tt_instant_high_fade_end_db: 0.0,
            tt_instant_low_fade_start_db: -4.0,
            tt_instant_low_fade_end_db: 4.0,
            tt_fade_curve_adjust: 0.0,
            tt_start_high: true,

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
            editing_tempo_up: 10.0,
            editing_tempo_down: 10.0,
            editing_reverb_room_size: 75.0,
            editing_reverb_reverberance: 50.0,
            editing_reverb_hf_damping: 50.0,
            editing_reverb_tone_low: 100.0,
            editing_reverb_tone_high: 100.0,
            editing_reverb_wet_gain_db: -1.0,
            editing_reverb_dry_gain_db: -1.0,
            editing_reverb_stereo_width: 100.0,
            editing_reverb_pre_delay_ms: 10.0,
            editing_reverb_wet_only: false,
            editing_echo_delay_seconds: 1.0,
            editing_echo_decay: 0.5,
            editing_distortion_drive_db: 0.0,
            editing_distortion_threshold: 0.8,
            editing_stretch_initial_tempo_percent: 0.0,
            editing_stretch_final_tempo_percent: 0.0,
            editing_stretch_initial_pitch_semitones: 0.0,
            editing_stretch_final_pitch_semitones: 0.0,
            editing_rattle_pitch_up_semitones: 1.0,
            editing_rattle_pitch_down_semitones: 1.0,
            editing_rattle_tempo_x_percent: 10.0,
            editing_rattle_tempo_y_percent: 10.0,
            editing_rattle_fade_in_a_db: -6.0,
            editing_rattle_fade_in_b_db: 0.0,
            editing_rattle_stretch_initial_tempo_percent: 0.0,
            editing_rattle_stretch_final_tempo_percent: 0.0,
            editing_rattle_stretch_initial_pitch_semitones: 0.0,
            editing_rattle_stretch_final_pitch_semitones: 0.0,
            editing_rattle_repeat_count: 24,

            editing_pan_toggle_high_db: 6.0,
            editing_pan_toggle_low_db: -4.0,
            editing_pan_toggle_direction: PanToggleDirection::Left,

            editing_tt_high_db: 4.0,
            editing_tt_low_db: -4.0,
            editing_tt_super_mode: false,
            editing_tt_detail: 1.0,
            editing_tt_instant_shift: false,
            editing_tt_instant_high_gain_db: 2.0,
            editing_tt_instant_low_gain_db: -6.0,
            editing_tt_instant_high_fade_start_db: 4.0,
            editing_tt_instant_high_fade_end_db: 0.0,
            editing_tt_instant_low_fade_start_db: -4.0,
            editing_tt_instant_low_fade_end_db: 4.0,
            editing_tt_fade_curve_adjust: 0.0,
            editing_tt_start_high: true,
        }
    }
}

/// The blue accent tint used for icon-only toolbar buttons' idle "bubble"
/// (Play/Pause/Stop/Record) — the same hue as the app's selection accent,
/// just kept faintly visible at rest instead of only appearing on hover.
const ICON_BUTTON_TINT: egui::Color32 = egui::Color32::from_rgb(120, 170, 255);

pub fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    let recording = app.recording.is_some();
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!recording, |ui| {
            clear_idle_button_frame(ui);
            if text_button(ui, "Add Track").clicked() {
                app.project.lock().unwrap().add_track();
            }
            ui.separator();
            if text_button(ui, "Create Wave...").clicked() {
                app.wave_dialog.open = true;
            }
            if text_button(ui, "Project File...").clicked() {
                app.project_file_dialog.open = true;
            }
            if text_button(ui, "Export Project...").clicked() {
                if let Some(name) = &app.project_name {
                    app.export_dialog.set_file_name(name.clone());
                }
                app.export_dialog.open = true;
            }
            ui.separator();
            draw_effects_menu(ui, app);
            ui.separator();
            if text_button(ui, "Zoom In").clicked() {
                app.timeline.zoom(1.2);
            }
            if text_button(ui, "Zoom Out").clicked() {
                app.timeline.zoom(1.0 / 1.2);
            }
            ui.separator();
            if icon_button(ui, "\u{25b6}").on_hover_text("Play").clicked() {
                app.start_playback();
            }
        });

        // The Record button stays clickable even while recording — it's
        // the only way to stop — so it lives outside the disabled scope
        // that locks the rest of the toolbar during a capture.
        draw_record_button(ui, app);

        ui.add_enabled_ui(!recording, |ui| {
            clear_idle_button_frame(ui);
            if icon_button(ui, "\u{23f8}").on_hover_text("Pause").clicked() {
                app.pause_playback();
            }
            if icon_button(ui, "\u{23f9}").on_hover_text("Stop").clicked() {
                app.engine.stop();
                app.play_start_position = None;
            }
            ui.separator();
            if text_button(ui, "Help").clicked() {
                app.help_open = true;
            }
        });
    });
}

/// Drops the idle-state background/border for every plain button and menu
/// button drawn in `ui` from this point on, so they read as plain text
/// instead of sitting in a "bubble" — hover/press feedback is untouched.
/// `icon_button` layers its own tinted bubble back on top of this per call.
fn clear_idle_button_frame(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
}

/// A plain toolbar button: text only, no idle "bubble" (assumes
/// `clear_idle_button_frame` has already been applied to `ui`).
fn text_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.button(label)
}

/// An icon-only toolbar button (Play/Pause/Stop/Record): keeps its
/// background bubble, tinted blue, with a bit more vertical breathing room
/// around the glyph than a text button gets.
fn icon_button(ui: &mut egui::Ui, content: impl Into<egui::WidgetText>) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.y += 3.0;
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = ICON_BUTTON_TINT.gamma_multiply(0.18);
        visuals.widgets.inactive.bg_fill = ICON_BUTTON_TINT.gamma_multiply(0.18);
        visuals.widgets.hovered.weak_bg_fill = ICON_BUTTON_TINT.gamma_multiply(0.35);
        visuals.widgets.hovered.bg_fill = ICON_BUTTON_TINT.gamma_multiply(0.35);
        visuals.widgets.active.weak_bg_fill = ICON_BUTTON_TINT.gamma_multiply(0.55);
        visuals.widgets.active.bg_fill = ICON_BUTTON_TINT.gamma_multiply(0.55);
        ui.add(egui::Button::new(content))
    })
    .inner
}

/// The Record button: a red dot (Audacity-style, icon-only) that starts
/// capturing the default system microphone on click, and stops it on a
/// second click — its tooltip carries the elapsed time while active since
/// the button itself shows no text.
fn draw_record_button(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    let recording = app.recording.is_some();
    let dot_color = egui::Color32::from_rgb(220, 40, 40);
    let content = egui::RichText::new("\u{25cf}").color(dot_color);

    if recording {
        let elapsed = app
            .record_started_at
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        let tooltip = format!("Stop recording ({})", format_time(elapsed));
        if icon_button(ui, content).on_hover_text(tooltip).clicked() {
            app.stop_recording();
        }
    } else if icon_button(ui, content)
        .on_hover_text("Record from the default microphone")
        .clicked()
    {
        app.start_recording();
    }
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
        LastEffect::TempoUp => {
            let step = app.effects.tempo_up_step_percent;
            apply_to_targets(app, &targets, move |p, id| p.apply_tempo_shift(id, step));
        }
        LastEffect::TempoDown => {
            let step = app.effects.tempo_down_step_percent;
            apply_to_targets(app, &targets, move |p, id| p.apply_tempo_shift(id, -step));
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
        let tempo_up_step = app.effects.tempo_up_step_percent;
        let tempo_down_step = app.effects.tempo_down_step_percent;
        if ui
            .add_enabled(enabled, egui::Button::new(format!("Tempo Up (+{tempo_up_step:.1}%)")))
            .clicked()
        {
            apply_to_targets(app, &targets, move |p, id| p.apply_tempo_shift(id, tempo_up_step));
            app.effects.last_effect = Some(LastEffect::TempoUp);
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new(format!("Tempo Down (-{tempo_down_step:.1}%)")))
            .clicked()
        {
            apply_to_targets(app, &targets, move |p, id| p.apply_tempo_shift(id, -tempo_down_step));
            app.effects.last_effect = Some(LastEffect::TempoDown);
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Give to Speech")).clicked() {
            apply_to_targets(app, &targets, |p, id| p.apply_give_to_speech(id));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Telephone")).clicked() {
            apply_to_targets(app, &targets, |p, id| p.apply_telephone(id));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Autotune")).clicked() {
            apply_to_targets(app, &targets, |p, id| p.apply_autotune(id));
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Reverb")).clicked() {
            let params = reverb_params(&app.effects);
            apply_to_targets(app, &targets, move |p, id| p.apply_reverb(id, &params));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Echo")).clicked() {
            let delay = app.effects.echo_delay_seconds;
            let decay = app.effects.echo_decay;
            apply_to_targets(app, &targets, move |p, id| p.apply_echo(id, delay, decay));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Distortion (Hard Clip)")).clicked() {
            let drive = app.effects.distortion_drive_db;
            let threshold = app.effects.distortion_threshold;
            apply_to_targets(app, &targets, move |p, id| p.apply_hard_clip_distortion(id, drive, threshold));
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Sliding Stretch")).clicked() {
            let params = sliding_stretch_params(&app.effects);
            apply_to_targets(app, &targets, move |p, id| p.apply_sliding_stretch(id, &params));
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Invert")).clicked() {
            apply_to_targets(app, &targets, |p, id| p.apply_invert(id));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Reverse")).clicked() {
            apply_to_targets(app, &targets, |p, id| p.apply_reverse(id));
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Swap Channels"))
            .on_hover_text("Swaps left/right on a stereo clip; no effect on mono clips")
            .clicked()
        {
            apply_to_targets(app, &targets, |p, id| p.apply_swap_channels(id));
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(enabled, egui::Button::new("Pan Toggle"))
            .on_hover_text(
                "Splits a stereo clip into its left/right channels, ramps one side up and the \
                 other down (own high/low dB points and fade direction, in \"Edit steps...\"), \
                 then recombines them; no effect on mono clips.",
            )
            .clicked()
        {
            let params = pan_toggle_params(&app.effects);
            apply_to_targets(app, &targets, move |p, id| p.apply_pan_toggle(id, &params));
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(enabled, egui::Button::new("Rattle"))
            .on_hover_text(
                "Builds pitch/tempo-shifted \"up\" and \"down\" copies of the clip, repeats the \
                 pair 12 times back-to-back, joins them, then applies its own Adjustable Fade \
                 In and Sliding Stretch (own settings below, in \"Edit steps...\").",
            )
            .clicked()
        {
            let params = rattle_params(&app.effects);
            apply_to_targets(app, &targets, move |p, id| p.apply_rattle(id, &params));
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Trip Toggler"))
            .on_hover_text(
                "Finds clear quiet low points in the clip and alternates a fade-down/fade-up \
                 across the resulting segments. Ported from a Python script; when applied to \
                 several targets at once, each successive clip's start High/Low flips from the \
                 last, just like the script alternated across files.",
            )
            .clicked()
        {
            let base_params = trip_toggler_params(&app.effects);
            let mut start_high = base_params.start_high;
            let mut project = app.project.lock().unwrap();
            for &id in &targets {
                let params = TripTogglerParams { start_high, ..base_params.clone() };
                project.apply_trip_toggler(id, &params);
                start_high = !start_high;
            }
            drop(project);
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
            app.effects.editing_tempo_up = app.effects.tempo_up_step_percent;
            app.effects.editing_tempo_down = app.effects.tempo_down_step_percent;
            app.effects.editing_reverb_room_size = app.effects.reverb_room_size;
            app.effects.editing_reverb_reverberance = app.effects.reverb_reverberance;
            app.effects.editing_reverb_hf_damping = app.effects.reverb_hf_damping;
            app.effects.editing_reverb_tone_low = app.effects.reverb_tone_low;
            app.effects.editing_reverb_tone_high = app.effects.reverb_tone_high;
            app.effects.editing_reverb_wet_gain_db = app.effects.reverb_wet_gain_db;
            app.effects.editing_reverb_dry_gain_db = app.effects.reverb_dry_gain_db;
            app.effects.editing_reverb_stereo_width = app.effects.reverb_stereo_width;
            app.effects.editing_reverb_pre_delay_ms = app.effects.reverb_pre_delay_ms;
            app.effects.editing_reverb_wet_only = app.effects.reverb_wet_only;
            app.effects.editing_echo_delay_seconds = app.effects.echo_delay_seconds;
            app.effects.editing_echo_decay = app.effects.echo_decay;
            app.effects.editing_distortion_drive_db = app.effects.distortion_drive_db;
            app.effects.editing_distortion_threshold = app.effects.distortion_threshold;
            app.effects.editing_stretch_initial_tempo_percent = app.effects.stretch_initial_tempo_percent;
            app.effects.editing_stretch_final_tempo_percent = app.effects.stretch_final_tempo_percent;
            app.effects.editing_stretch_initial_pitch_semitones = app.effects.stretch_initial_pitch_semitones;
            app.effects.editing_stretch_final_pitch_semitones = app.effects.stretch_final_pitch_semitones;
            app.effects.editing_rattle_pitch_up_semitones = app.effects.rattle_pitch_up_semitones;
            app.effects.editing_rattle_pitch_down_semitones = app.effects.rattle_pitch_down_semitones;
            app.effects.editing_rattle_tempo_x_percent = app.effects.rattle_tempo_x_percent;
            app.effects.editing_rattle_tempo_y_percent = app.effects.rattle_tempo_y_percent;
            app.effects.editing_rattle_fade_in_a_db = app.effects.rattle_fade_in_a_db;
            app.effects.editing_rattle_fade_in_b_db = app.effects.rattle_fade_in_b_db;
            app.effects.editing_rattle_stretch_initial_tempo_percent =
                app.effects.rattle_stretch_initial_tempo_percent;
            app.effects.editing_rattle_stretch_final_tempo_percent =
                app.effects.rattle_stretch_final_tempo_percent;
            app.effects.editing_rattle_stretch_initial_pitch_semitones =
                app.effects.rattle_stretch_initial_pitch_semitones;
            app.effects.editing_rattle_stretch_final_pitch_semitones =
                app.effects.rattle_stretch_final_pitch_semitones;
            app.effects.editing_rattle_repeat_count = app.effects.rattle_repeat_count;
            app.effects.editing_pan_toggle_high_db = app.effects.pan_toggle_high_db;
            app.effects.editing_pan_toggle_low_db = app.effects.pan_toggle_low_db;
            app.effects.editing_pan_toggle_direction = app.effects.pan_toggle_direction;
            app.effects.editing_tt_high_db = app.effects.tt_high_db;
            app.effects.editing_tt_low_db = app.effects.tt_low_db;
            app.effects.editing_tt_super_mode = app.effects.tt_super_mode;
            app.effects.editing_tt_detail = app.effects.tt_detail;
            app.effects.editing_tt_instant_shift = app.effects.tt_instant_shift;
            app.effects.editing_tt_instant_high_gain_db = app.effects.tt_instant_high_gain_db;
            app.effects.editing_tt_instant_low_gain_db = app.effects.tt_instant_low_gain_db;
            app.effects.editing_tt_instant_high_fade_start_db = app.effects.tt_instant_high_fade_start_db;
            app.effects.editing_tt_instant_high_fade_end_db = app.effects.tt_instant_high_fade_end_db;
            app.effects.editing_tt_instant_low_fade_start_db = app.effects.tt_instant_low_fade_start_db;
            app.effects.editing_tt_instant_low_fade_end_db = app.effects.tt_instant_low_fade_end_db;
            app.effects.editing_tt_fade_curve_adjust = app.effects.tt_fade_curve_adjust;
            app.effects.editing_tt_start_high = app.effects.tt_start_high;
            app.effects.settings_open = true;
            ui.close();
        }
    });
}

/// Builds a `ReverbParams` from the current (committed) Reverb settings in
/// `EffectsState`.
fn reverb_params(effects: &EffectsState) -> ReverbParams {
    ReverbParams {
        room_size: effects.reverb_room_size,
        reverberance: effects.reverb_reverberance,
        hf_damping: effects.reverb_hf_damping,
        tone_low: effects.reverb_tone_low,
        tone_high: effects.reverb_tone_high,
        wet_gain_db: effects.reverb_wet_gain_db,
        dry_gain_db: effects.reverb_dry_gain_db,
        stereo_width: effects.reverb_stereo_width,
        pre_delay_ms: effects.reverb_pre_delay_ms,
        wet_only: effects.reverb_wet_only,
    }
}

/// Builds a `RampParams` from the current (committed) Sliding Stretch
/// settings in `EffectsState`.
fn sliding_stretch_params(effects: &EffectsState) -> RampParams {
    RampParams {
        initial_tempo_percent: effects.stretch_initial_tempo_percent,
        final_tempo_percent: effects.stretch_final_tempo_percent,
        initial_pitch_semitones: effects.stretch_initial_pitch_semitones,
        final_pitch_semitones: effects.stretch_final_pitch_semitones,
    }
}

/// Builds a `RattleParams` from the current (committed) Rattle settings in
/// `EffectsState` — its own Adjustable Fade In / Sliding Stretch values,
/// independent of those effects' regular settings above.
fn rattle_params(effects: &EffectsState) -> RattleParams {
    let fade_in_a = effects.rattle_fade_in_a_db;
    let fade_in_b = effects.rattle_fade_in_b_db;
    RattleParams {
        pitch_up_semitones: effects.rattle_pitch_up_semitones,
        pitch_down_semitones: effects.rattle_pitch_down_semitones,
        tempo_x_percent: effects.rattle_tempo_x_percent,
        tempo_y_percent: effects.rattle_tempo_y_percent,
        fade_in_start_gain: db_to_gain(fade_in_a.min(fade_in_b)),
        fade_in_end_gain: db_to_gain(fade_in_a.max(fade_in_b)),
        stretch: RampParams {
            initial_tempo_percent: effects.rattle_stretch_initial_tempo_percent,
            final_tempo_percent: effects.rattle_stretch_final_tempo_percent,
            initial_pitch_semitones: effects.rattle_stretch_initial_pitch_semitones,
            final_pitch_semitones: effects.rattle_stretch_final_pitch_semitones,
        },
        repeat_count: effects.rattle_repeat_count,
    }
}

/// Builds a `PanToggleParams` from the current (committed) Pan Toggle
/// settings in `EffectsState`.
fn pan_toggle_params(effects: &EffectsState) -> PanToggleParams {
    PanToggleParams {
        high_db: effects.pan_toggle_high_db,
        low_db: effects.pan_toggle_low_db,
        direction: effects.pan_toggle_direction,
    }
}

/// Builds a `TripTogglerParams` from the current (committed) Trip Toggler
/// settings in `EffectsState`.
fn trip_toggler_params(effects: &EffectsState) -> TripTogglerParams {
    TripTogglerParams {
        high_db: effects.tt_high_db,
        low_db: effects.tt_low_db,
        super_mode: effects.tt_super_mode,
        detail: effects.tt_detail,
        instant_shift: effects.tt_instant_shift,
        instant_high_gain_db: effects.tt_instant_high_gain_db,
        instant_low_gain_db: effects.tt_instant_low_gain_db,
        instant_high_fade_start_db: effects.tt_instant_high_fade_start_db,
        instant_high_fade_end_db: effects.tt_instant_high_fade_end_db,
        instant_low_fade_start_db: effects.tt_instant_low_fade_start_db,
        instant_low_fade_end_db: effects.tt_instant_low_fade_end_db,
        fade_curve_adjust: effects.tt_fade_curve_adjust,
        start_high: effects.tt_start_high,
    }
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

    egui::Window::new("Edit Effect Steps").open(&mut open).max_height(600.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
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

        ui.add_space(8.0);
        egui::Grid::new("tempo_steps_grid").num_columns(2).show(ui, |ui| {
            ui.label("Tempo Up (%):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tempo_up).range(0.1..=200.0).speed(0.5));
            ui.end_row();

            ui.label("Tempo Down (%):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tempo_down).range(0.1..=90.0).speed(0.5));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Reverb:");
        egui::Grid::new("reverb_grid").num_columns(2).show(ui, |ui| {
            ui.label("Room Size:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_room_size).range(0.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("Reverberance:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_reverberance).range(0.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("HF Damping:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_hf_damping).range(0.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("Tone Low:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_tone_low).range(0.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("Tone High:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_tone_high).range(0.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("Wet Gain (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_wet_gain_db).range(-60.0..=10.0).speed(0.5));
            ui.end_row();
            ui.label("Dry Gain (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_dry_gain_db).range(-60.0..=10.0).speed(0.5));
            ui.end_row();
            ui.label("Stereo Width:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_stereo_width).range(0.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("Pre-Delay (ms):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_reverb_pre_delay_ms).range(0.0..=500.0).speed(1.0));
            ui.end_row();
            ui.label("Wet Only:");
            ui.checkbox(&mut app.effects.editing_reverb_wet_only, "");
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Echo:");
        egui::Grid::new("echo_grid").num_columns(2).show(ui, |ui| {
            ui.label("Delay time (s):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_echo_delay_seconds).range(0.001..=10.0).speed(0.05));
            ui.end_row();
            ui.label("Decay factor:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_echo_decay).range(0.0..=2.0).speed(0.01));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Distortion (Hard Clip):");
        egui::Grid::new("distortion_grid").num_columns(2).show(ui, |ui| {
            ui.label("Drive (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_distortion_drive_db).range(0.0..=48.0).speed(0.5));
            ui.end_row();
            ui.label("Clip Threshold:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_distortion_threshold).range(0.01..=1.0).speed(0.01));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Sliding Stretch: ramps tempo/pitch from the clip's start to its end.");
        egui::Grid::new("sliding_stretch_grid").num_columns(2).show(ui, |ui| {
            ui.label("Initial Tempo Change (%):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_stretch_initial_tempo_percent).range(-90.0..=500.0).speed(0.5));
            ui.end_row();
            ui.label("Final Tempo Change (%):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_stretch_final_tempo_percent).range(-90.0..=500.0).speed(0.5));
            ui.end_row();
            ui.label("Initial Pitch Shift (semitones):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_stretch_initial_pitch_semitones).range(-24.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Final Pitch Shift (semitones):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_stretch_final_pitch_semitones).range(-24.0..=24.0).speed(0.1));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Rattle (own Adjustable Fade In / Sliding Stretch settings, separate from the ones above):");
        egui::Grid::new("rattle_grid").num_columns(2).show(ui, |ui| {
            ui.label("Pitch Up (semitones):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_pitch_up_semitones).range(0.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Pitch Down (semitones):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_pitch_down_semitones).range(0.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Tempo +x% (first clip):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_tempo_x_percent).range(-90.0..=200.0).speed(0.5));
            ui.end_row();
            ui.label("Tempo -y% (second clip):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_tempo_y_percent).range(-90.0..=200.0).speed(0.5));
            ui.end_row();
            ui.label("Fade In points (dB):");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_fade_in_a_db).range(-60.0..=24.0).speed(0.1));
                ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_fade_in_b_db).range(-60.0..=24.0).speed(0.1));
            });
            ui.end_row();
            ui.label("Sliding Stretch Initial Tempo (%):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_stretch_initial_tempo_percent).range(-90.0..=500.0).speed(0.5));
            ui.end_row();
            ui.label("Sliding Stretch Final Tempo (%):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_stretch_final_tempo_percent).range(-90.0..=500.0).speed(0.5));
            ui.end_row();
            ui.label("Sliding Stretch Initial Pitch (st):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_stretch_initial_pitch_semitones).range(-24.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Sliding Stretch Final Pitch (st):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_stretch_final_pitch_semitones).range(-24.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Repeat Count (A+B clips):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_rattle_repeat_count).range(2..=128).speed(2.0));
            ui.end_row();
        });
        // Always an even number of whole [A, B] pairs, in steps of 2.
        app.effects.editing_rattle_repeat_count = (app.effects.editing_rattle_repeat_count / 2).max(1) * 2;

        ui.add_space(8.0);
        ui.label("Pan Toggle: splits a stereo clip's channels, fades one up and the other down.");
        ui.horizontal(|ui| {
            ui.label("Fade-in side:");
            ui.radio_value(&mut app.effects.editing_pan_toggle_direction, PanToggleDirection::Left, "Left");
            ui.radio_value(&mut app.effects.editing_pan_toggle_direction, PanToggleDirection::Right, "Right");
        });
        egui::Grid::new("pan_toggle_grid").num_columns(2).show(ui, |ui| {
            ui.label("High dB (fade-in end / fade-out start):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_pan_toggle_high_db).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Low dB (fade-in start / fade-out end):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_pan_toggle_low_db).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
        });

        ui.add_space(8.0);
        ui.label("Trip Toggler: finds clear low points and alternates a fade down/up across the segments.");
        ui.horizontal(|ui| {
            ui.label("Detection mode:");
            ui.radio_value(&mut app.effects.editing_tt_super_mode, false, "Basic (between hits)");
            ui.radio_value(&mut app.effects.editing_tt_super_mode, true, "Super (inside a hit's decay)");
        });
        ui.horizontal(|ui| {
            ui.label("Starts:");
            ui.radio_value(&mut app.effects.editing_tt_start_high, true, "High");
            ui.radio_value(&mut app.effects.editing_tt_start_high, false, "Low");
        });
        ui.horizontal(|ui| {
            ui.label("Shift mode:");
            ui.radio_value(&mut app.effects.editing_tt_instant_shift, false, "Gradual (pure fade)");
            ui.radio_value(&mut app.effects.editing_tt_instant_shift, true, "Instant (step + fade)");
        });
        egui::Grid::new("trip_toggler_grid").num_columns(2).show(ui, |ui| {
            ui.label("Detail (detection fine-tune):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tt_detail).range(0.01..=10.0).speed(0.05));
            ui.end_row();
            ui.label("Fade Curve Adjust (-100..100):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tt_fade_curve_adjust).range(-100.0..=100.0).speed(1.0));
            ui.end_row();
            ui.label("Gradual High dB:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tt_high_db).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Gradual Low dB:");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tt_low_db).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Instant High Gain Step (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tt_instant_high_gain_db).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Instant Low Gain Step (dB):");
            ui.add(egui::DragValue::new(&mut app.effects.editing_tt_instant_low_gain_db).range(-60.0..=24.0).speed(0.1));
            ui.end_row();
            ui.label("Instant High Fade (dB):");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut app.effects.editing_tt_instant_high_fade_start_db).range(-60.0..=24.0).speed(0.1));
                ui.add(egui::DragValue::new(&mut app.effects.editing_tt_instant_high_fade_end_db).range(-60.0..=24.0).speed(0.1));
            });
            ui.end_row();
            ui.label("Instant Low Fade (dB):");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut app.effects.editing_tt_instant_low_fade_start_db).range(-60.0..=24.0).speed(0.1));
                ui.add(egui::DragValue::new(&mut app.effects.editing_tt_instant_low_fade_end_db).range(-60.0..=24.0).speed(0.1));
            });
            ui.end_row();
        });
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
        app.effects.tempo_up_step_percent = app.effects.editing_tempo_up;
        app.effects.tempo_down_step_percent = app.effects.editing_tempo_down;
        app.effects.reverb_room_size = app.effects.editing_reverb_room_size;
        app.effects.reverb_reverberance = app.effects.editing_reverb_reverberance;
        app.effects.reverb_hf_damping = app.effects.editing_reverb_hf_damping;
        app.effects.reverb_tone_low = app.effects.editing_reverb_tone_low;
        app.effects.reverb_tone_high = app.effects.editing_reverb_tone_high;
        app.effects.reverb_wet_gain_db = app.effects.editing_reverb_wet_gain_db;
        app.effects.reverb_dry_gain_db = app.effects.editing_reverb_dry_gain_db;
        app.effects.reverb_stereo_width = app.effects.editing_reverb_stereo_width;
        app.effects.reverb_pre_delay_ms = app.effects.editing_reverb_pre_delay_ms;
        app.effects.reverb_wet_only = app.effects.editing_reverb_wet_only;
        app.effects.echo_delay_seconds = app.effects.editing_echo_delay_seconds;
        app.effects.echo_decay = app.effects.editing_echo_decay;
        app.effects.distortion_drive_db = app.effects.editing_distortion_drive_db;
        app.effects.distortion_threshold = app.effects.editing_distortion_threshold;
        app.effects.stretch_initial_tempo_percent = app.effects.editing_stretch_initial_tempo_percent;
        app.effects.stretch_final_tempo_percent = app.effects.editing_stretch_final_tempo_percent;
        app.effects.stretch_initial_pitch_semitones = app.effects.editing_stretch_initial_pitch_semitones;
        app.effects.stretch_final_pitch_semitones = app.effects.editing_stretch_final_pitch_semitones;
        app.effects.rattle_pitch_up_semitones = app.effects.editing_rattle_pitch_up_semitones;
        app.effects.rattle_pitch_down_semitones = app.effects.editing_rattle_pitch_down_semitones;
        app.effects.rattle_tempo_x_percent = app.effects.editing_rattle_tempo_x_percent;
        app.effects.rattle_tempo_y_percent = app.effects.editing_rattle_tempo_y_percent;
        app.effects.rattle_fade_in_a_db = app.effects.editing_rattle_fade_in_a_db;
        app.effects.rattle_fade_in_b_db = app.effects.editing_rattle_fade_in_b_db;
        app.effects.rattle_stretch_initial_tempo_percent =
            app.effects.editing_rattle_stretch_initial_tempo_percent;
        app.effects.rattle_stretch_final_tempo_percent = app.effects.editing_rattle_stretch_final_tempo_percent;
        app.effects.rattle_stretch_initial_pitch_semitones =
            app.effects.editing_rattle_stretch_initial_pitch_semitones;
        app.effects.rattle_stretch_final_pitch_semitones =
            app.effects.editing_rattle_stretch_final_pitch_semitones;
        app.effects.rattle_repeat_count = app.effects.editing_rattle_repeat_count;
        app.effects.pan_toggle_high_db = app.effects.editing_pan_toggle_high_db;
        app.effects.pan_toggle_low_db = app.effects.editing_pan_toggle_low_db;
        app.effects.pan_toggle_direction = app.effects.editing_pan_toggle_direction;
        app.effects.tt_high_db = app.effects.editing_tt_high_db;
        app.effects.tt_low_db = app.effects.editing_tt_low_db;
        app.effects.tt_super_mode = app.effects.editing_tt_super_mode;
        app.effects.tt_detail = app.effects.editing_tt_detail;
        app.effects.tt_instant_shift = app.effects.editing_tt_instant_shift;
        app.effects.tt_instant_high_gain_db = app.effects.editing_tt_instant_high_gain_db;
        app.effects.tt_instant_low_gain_db = app.effects.editing_tt_instant_low_gain_db;
        app.effects.tt_instant_high_fade_start_db = app.effects.editing_tt_instant_high_fade_start_db;
        app.effects.tt_instant_high_fade_end_db = app.effects.editing_tt_instant_high_fade_end_db;
        app.effects.tt_instant_low_fade_start_db = app.effects.editing_tt_instant_low_fade_start_db;
        app.effects.tt_instant_low_fade_end_db = app.effects.editing_tt_instant_low_fade_end_db;
        app.effects.tt_fade_curve_adjust = app.effects.editing_tt_fade_curve_adjust;
        app.effects.tt_start_high = app.effects.editing_tt_start_high;
        app.effects.settings_open = false;
        super::settings_persistence::save_effects_settings(&app.effects);
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
