use super::timeline::format_time;
use super::{export_dialog, toast, RakunatorApp};
use crate::project::reverb::ReverbParams;
use crate::project::stretch::RampParams;
use crate::project::trip_toggler::TripTogglerParams;
use crate::project::{db_to_gain, ClipId, PanToggleDirection, PanToggleParams, RattleParams, TrackId};
use serde::{Deserialize, Serialize};

/// Each variant carries the exact step value that was actually used —
/// whichever value was in the quick-edit dialog's field when OK was
/// clicked, whether or not "Update steps" was checked — so Ctrl+R repeats
/// that same value even when it was a one-off edit never committed as the
/// new default.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LastEffect {
    PitchUp(f32),
    PitchDown(f32),
    VolumeUp(f32),
    VolumeDown(f32),
    TempoUp(f32),
    TempoDown(f32),
}

/// Which effect's "quick edit" modal (see `draw_active_effect_dialog`) is
/// currently open, if any — set when an effect with tweakable step values
/// is clicked in the Effects menu, instead of that click applying the
/// effect immediately.
#[derive(Clone, Copy, PartialEq)]
pub enum ActiveEffectDialog {
    PitchUp,
    PitchDown,
    VolumeUp,
    VolumeDown,
    AdjustableFadeIn,
    AdjustableFadeOut,
    FadeToggle,
    TempoUp,
    TempoDown,
    Reverb,
    Echo,
    Distortion,
    SlidingStretch,
    PanToggle,
    Rattle,
    TripToggler,
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
    /// The "Edit Effect Steps" dialog's own search box (filters which
    /// sections are shown) — separate from `RakunatorApp::help_search`,
    /// which is the Help window's. Like that one, this persists across
    /// opens/closes rather than resetting, so re-opening the dialog keeps
    /// whatever you last searched for.
    #[serde(skip)]
    settings_search: String,
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

    /// Which effect's "quick edit" modal is currently open, if any — see
    /// `ActiveEffectDialog`.
    #[serde(skip)]
    active_effect_dialog: Option<ActiveEffectDialog>,
    /// The quick-edit modal's "Update steps" checkbox: whether OK should
    /// also commit its `editing_*` value(s) back as the new defaults
    /// (persisted), on top of applying the effect. Reset to unchecked
    /// every time a modal opens — deliberately not sticky across effects
    /// or re-openings, so remembering a default is always an explicit,
    /// per-edit choice.
    #[serde(skip)]
    remember_as_default: bool,
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
            rattle_tempo_x_percent: 20.0,
            rattle_tempo_y_percent: 20.0,
            rattle_fade_in_a_db: -6.0,
            rattle_fade_in_b_db: 0.0,
            rattle_stretch_initial_tempo_percent: 50.0,
            rattle_stretch_final_tempo_percent: 250.0,
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
            settings_search: String::new(),
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
            editing_rattle_tempo_x_percent: 20.0,
            editing_rattle_tempo_y_percent: 20.0,
            editing_rattle_fade_in_a_db: -6.0,
            editing_rattle_fade_in_b_db: 0.0,
            editing_rattle_stretch_initial_tempo_percent: 50.0,
            editing_rattle_stretch_final_tempo_percent: 250.0,
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

            active_effect_dialog: None,
            remember_as_default: false,
        }
    }
}

impl EffectsState {
    /// Resets every `editing_*` scratch field (the "Edit Effect Steps"
    /// dialog's in-progress values) back to `EffectsState::default()`,
    /// without touching the already-committed settings — mirrors the
    /// committed-\>editing copy done when the dialog is opened, just sourced
    /// from the defaults instead. OK still has to be clicked afterwards to
    /// actually apply the reset (Cancel discards it, same as any other edit).
    fn reset_editing_to_defaults(&mut self) {
        let d = EffectsState::default();
        self.editing_pitch_up = d.pitch_up_step;
        self.editing_pitch_down = d.pitch_down_step;
        self.editing_volume_up = d.volume_up_step_db;
        self.editing_volume_down = d.volume_down_step_db;
        self.editing_fade_in_a = d.fade_in_point_a_db;
        self.editing_fade_in_b = d.fade_in_point_b_db;
        self.editing_fade_out_a = d.fade_out_point_a_db;
        self.editing_fade_out_b = d.fade_out_point_b_db;
        self.editing_fade_toggle_starts_with_in = d.fade_toggle_starts_with_in;
        self.editing_tempo_up = d.tempo_up_step_percent;
        self.editing_tempo_down = d.tempo_down_step_percent;
        self.editing_reverb_room_size = d.reverb_room_size;
        self.editing_reverb_reverberance = d.reverb_reverberance;
        self.editing_reverb_hf_damping = d.reverb_hf_damping;
        self.editing_reverb_tone_low = d.reverb_tone_low;
        self.editing_reverb_tone_high = d.reverb_tone_high;
        self.editing_reverb_wet_gain_db = d.reverb_wet_gain_db;
        self.editing_reverb_dry_gain_db = d.reverb_dry_gain_db;
        self.editing_reverb_stereo_width = d.reverb_stereo_width;
        self.editing_reverb_pre_delay_ms = d.reverb_pre_delay_ms;
        self.editing_reverb_wet_only = d.reverb_wet_only;
        self.editing_echo_delay_seconds = d.echo_delay_seconds;
        self.editing_echo_decay = d.echo_decay;
        self.editing_distortion_drive_db = d.distortion_drive_db;
        self.editing_distortion_threshold = d.distortion_threshold;
        self.editing_stretch_initial_tempo_percent = d.stretch_initial_tempo_percent;
        self.editing_stretch_final_tempo_percent = d.stretch_final_tempo_percent;
        self.editing_stretch_initial_pitch_semitones = d.stretch_initial_pitch_semitones;
        self.editing_stretch_final_pitch_semitones = d.stretch_final_pitch_semitones;
        self.editing_rattle_pitch_up_semitones = d.rattle_pitch_up_semitones;
        self.editing_rattle_pitch_down_semitones = d.rattle_pitch_down_semitones;
        self.editing_rattle_tempo_x_percent = d.rattle_tempo_x_percent;
        self.editing_rattle_tempo_y_percent = d.rattle_tempo_y_percent;
        self.editing_rattle_fade_in_a_db = d.rattle_fade_in_a_db;
        self.editing_rattle_fade_in_b_db = d.rattle_fade_in_b_db;
        self.editing_rattle_stretch_initial_tempo_percent = d.rattle_stretch_initial_tempo_percent;
        self.editing_rattle_stretch_final_tempo_percent = d.rattle_stretch_final_tempo_percent;
        self.editing_rattle_stretch_initial_pitch_semitones = d.rattle_stretch_initial_pitch_semitones;
        self.editing_rattle_stretch_final_pitch_semitones = d.rattle_stretch_final_pitch_semitones;
        self.editing_rattle_repeat_count = d.rattle_repeat_count;
        self.editing_pan_toggle_high_db = d.pan_toggle_high_db;
        self.editing_pan_toggle_low_db = d.pan_toggle_low_db;
        self.editing_pan_toggle_direction = d.pan_toggle_direction;
        self.editing_tt_high_db = d.tt_high_db;
        self.editing_tt_low_db = d.tt_low_db;
        self.editing_tt_super_mode = d.tt_super_mode;
        self.editing_tt_detail = d.tt_detail;
        self.editing_tt_instant_shift = d.tt_instant_shift;
        self.editing_tt_instant_high_gain_db = d.tt_instant_high_gain_db;
        self.editing_tt_instant_low_gain_db = d.tt_instant_low_gain_db;
        self.editing_tt_instant_high_fade_start_db = d.tt_instant_high_fade_start_db;
        self.editing_tt_instant_high_fade_end_db = d.tt_instant_high_fade_end_db;
        self.editing_tt_instant_low_fade_start_db = d.tt_instant_low_fade_start_db;
        self.editing_tt_instant_low_fade_end_db = d.tt_instant_low_fade_end_db;
        self.editing_tt_fade_curve_adjust = d.tt_fade_curve_adjust;
        self.editing_tt_start_high = d.tt_start_high;
    }

    /// Closes the "Edit Effect Steps" dialog without applying any pending
    /// edits — `settings_open` is private to this module, so the global
    /// Ctrl+W "close open dialogs" shortcut in `app.rs` goes through this
    /// instead of touching the field directly.
    pub(super) fn close_settings(&mut self) {
        self.settings_open = false;
    }

    /// Closes the effect "quick edit" modal (see `ActiveEffectDialog`)
    /// without applying it — same reasoning as `close_settings` above, for
    /// the global Ctrl+W shortcut.
    pub(super) fn close_active_effect_dialog(&mut self) {
        self.active_effect_dialog = None;
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
                export_dialog::open(app);
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

/// Applies whichever of Pitch Up/Down or Volume Up/Down was last used, at
/// the exact value that was actually used at the time (see `LastEffect`),
/// to the current effect targets. Used by the Ctrl+R "repeat last effect"
/// shortcut.
pub fn repeat_last_effect(app: &mut RakunatorApp) {
    let Some(last) = app.effects.last_effect else {
        return;
    };
    let targets = app.project.lock().unwrap().effect_targets();
    match last {
        LastEffect::PitchUp(step) => {
            apply_to_targets("Pitch Up", app, &targets, move |p, id| p.apply_pitch_shift(id, step));
        }
        LastEffect::PitchDown(step) => {
            apply_to_targets("Pitch Down", app, &targets, move |p, id| p.apply_pitch_shift(id, -step));
        }
        LastEffect::VolumeUp(step_db) => {
            let factor = 10f32.powf(step_db / 20.0);
            apply_to_targets("Volume Up", app, &targets, move |p, id| p.apply_gain(id, factor));
        }
        LastEffect::VolumeDown(step_db) => {
            let factor = 10f32.powf(-step_db / 20.0);
            apply_to_targets("Volume Down", app, &targets, move |p, id| p.apply_gain(id, factor));
        }
        LastEffect::TempoUp(step) => {
            apply_to_targets("Tempo Up", app, &targets, move |p, id| p.apply_tempo_shift(id, step));
        }
        LastEffect::TempoDown(step) => {
            apply_to_targets("Tempo Down", app, &targets, move |p, id| p.apply_tempo_shift(id, -step));
        }
    }
}

/// Pitch/volume/fade effects applied destructively to whichever clips are
/// targeted: every clip on the selected track (clicking a track header's
/// empty space selects the whole track), or the current multi-clip
/// selection otherwise — see `Project::effect_targets`. "Edit steps..."
/// opens a small dialog to change the Pitch/Volume step sizes (Up and
/// Down independently). Pitch shift preserves tempo/duration (WSOLA, see
/// `Project::apply_pitch_shift`), the same way Tempo Up/Down preserves
/// pitch — the two are fully independent of each other, Audacity-style.
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
            app.effects.editing_pitch_up = app.effects.pitch_up_step;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::PitchUp);
            ui.close();
        }
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("Pitch Down (-{pitch_down_step:.1} semitone)")),
            )
            .clicked()
        {
            app.effects.editing_pitch_down = app.effects.pitch_down_step;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::PitchDown);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(enabled, egui::Button::new(format!("Volume Up (+{volume_up_step:.1} dB)")))
            .clicked()
        {
            app.effects.editing_volume_up = app.effects.volume_up_step_db;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::VolumeUp);
            ui.close();
        }
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(format!("Volume Down (-{volume_down_step:.1} dB)")),
            )
            .clicked()
        {
            app.effects.editing_volume_down = app.effects.volume_down_step_db;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::VolumeDown);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(enabled, egui::Button::new("Fade In (Ctrl+F)"))
            .clicked()
        {
            apply_to_targets("Fade In", app, &targets, |p, id| p.apply_fade_in(id));
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Fade Out (Ctrl+Shift+F)"))
            .clicked()
        {
            apply_to_targets("Fade Out", app, &targets, |p, id| p.apply_fade_out(id));
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
            app.effects.editing_fade_in_a = app.effects.fade_in_point_a_db;
            app.effects.editing_fade_in_b = app.effects.fade_in_point_b_db;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::AdjustableFadeIn);
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
            app.effects.editing_fade_out_a = app.effects.fade_out_point_a_db;
            app.effects.editing_fade_out_b = app.effects.fade_out_point_b_db;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::AdjustableFadeOut);
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
            app.effects.editing_fade_toggle_starts_with_in = app.effects.fade_toggle_starts_with_in;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::FadeToggle);
            ui.close();
        }
        ui.separator();
        let tempo_up_step = app.effects.tempo_up_step_percent;
        let tempo_down_step = app.effects.tempo_down_step_percent;
        if ui
            .add_enabled(enabled, egui::Button::new(format!("Tempo Up (+{tempo_up_step:.1}%)")))
            .clicked()
        {
            app.effects.editing_tempo_up = app.effects.tempo_up_step_percent;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::TempoUp);
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new(format!("Tempo Down (-{tempo_down_step:.1}%)")))
            .clicked()
        {
            app.effects.editing_tempo_down = app.effects.tempo_down_step_percent;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::TempoDown);
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Give to Speech")).clicked() {
            apply_to_targets("Give to Speech", app, &targets, |p, id| p.apply_give_to_speech(id));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Telephone")).clicked() {
            apply_to_targets("Telephone", app, &targets, |p, id| p.apply_telephone(id));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Autotune")).clicked() {
            apply_to_targets("Autotune", app, &targets, |p, id| p.apply_autotune(id));
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Reverb")).clicked() {
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
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::Reverb);
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Echo")).clicked() {
            app.effects.editing_echo_delay_seconds = app.effects.echo_delay_seconds;
            app.effects.editing_echo_decay = app.effects.echo_decay;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::Echo);
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Distortion (Hard Clip)")).clicked() {
            app.effects.editing_distortion_drive_db = app.effects.distortion_drive_db;
            app.effects.editing_distortion_threshold = app.effects.distortion_threshold;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::Distortion);
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Sliding Stretch")).clicked() {
            app.effects.editing_stretch_initial_tempo_percent = app.effects.stretch_initial_tempo_percent;
            app.effects.editing_stretch_final_tempo_percent = app.effects.stretch_final_tempo_percent;
            app.effects.editing_stretch_initial_pitch_semitones = app.effects.stretch_initial_pitch_semitones;
            app.effects.editing_stretch_final_pitch_semitones = app.effects.stretch_final_pitch_semitones;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::SlidingStretch);
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(enabled, egui::Button::new("Invert")).clicked() {
            apply_to_targets("Invert", app, &targets, |p, id| p.apply_invert(id));
            ui.close();
        }
        if ui.add_enabled(enabled, egui::Button::new("Reverse")).clicked() {
            apply_to_targets("Reverse", app, &targets, |p, id| p.apply_reverse(id));
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Swap Channels"))
            .on_hover_text("Swaps left/right on a stereo clip; no effect on mono clips")
            .clicked()
        {
            apply_to_targets("Swap Channels", app, &targets, |p, id| p.apply_swap_channels(id));
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
            app.effects.editing_pan_toggle_high_db = app.effects.pan_toggle_high_db;
            app.effects.editing_pan_toggle_low_db = app.effects.pan_toggle_low_db;
            app.effects.editing_pan_toggle_direction = app.effects.pan_toggle_direction;
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::PanToggle);
            ui.close();
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Rattle"))
            .on_hover_text(
                "Builds pitch/tempo-shifted \"up\" and \"down\" copies of the clip, repeats the \
                 pair back-to-back (Repeat Count, in \"Edit steps...\"), joins them, then applies \
                 its own Adjustable Fade In and Sliding Stretch (own settings below, in \"Edit \
                 steps...\").",
            )
            .clicked()
        {
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
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::Rattle);
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
            app.effects.remember_as_default = false;
            app.effects.active_effect_dialog = Some(ActiveEffectDialog::TripToggler);
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

/// Builds a `ReverbParams` from the quick-edit/"Edit Effect Steps" dialogs'
/// in-progress (`editing_*`) Reverb settings in `EffectsState`.
fn reverb_params(effects: &EffectsState) -> ReverbParams {
    ReverbParams {
        room_size: effects.editing_reverb_room_size,
        reverberance: effects.editing_reverb_reverberance,
        hf_damping: effects.editing_reverb_hf_damping,
        tone_low: effects.editing_reverb_tone_low,
        tone_high: effects.editing_reverb_tone_high,
        wet_gain_db: effects.editing_reverb_wet_gain_db,
        dry_gain_db: effects.editing_reverb_dry_gain_db,
        stereo_width: effects.editing_reverb_stereo_width,
        pre_delay_ms: effects.editing_reverb_pre_delay_ms,
        wet_only: effects.editing_reverb_wet_only,
    }
}

/// Builds a `RampParams` from the quick-edit/"Edit Effect Steps" dialogs'
/// in-progress (`editing_*`) Sliding Stretch settings in `EffectsState`.
fn sliding_stretch_params(effects: &EffectsState) -> RampParams {
    RampParams {
        initial_tempo_percent: effects.editing_stretch_initial_tempo_percent,
        final_tempo_percent: effects.editing_stretch_final_tempo_percent,
        initial_pitch_semitones: effects.editing_stretch_initial_pitch_semitones,
        final_pitch_semitones: effects.editing_stretch_final_pitch_semitones,
    }
}

/// Builds a `RattleParams` from the quick-edit/"Edit Effect Steps" dialogs'
/// in-progress (`editing_*`) Rattle settings in `EffectsState` — its own
/// Adjustable Fade In / Sliding Stretch values, independent of those
/// effects' regular settings above.
fn rattle_params(effects: &EffectsState) -> RattleParams {
    let fade_in_a = effects.editing_rattle_fade_in_a_db;
    let fade_in_b = effects.editing_rattle_fade_in_b_db;
    RattleParams {
        pitch_up_semitones: effects.editing_rattle_pitch_up_semitones,
        pitch_down_semitones: effects.editing_rattle_pitch_down_semitones,
        tempo_x_percent: effects.editing_rattle_tempo_x_percent,
        tempo_y_percent: effects.editing_rattle_tempo_y_percent,
        fade_in_start_gain: db_to_gain(fade_in_a.min(fade_in_b)),
        fade_in_end_gain: db_to_gain(fade_in_a.max(fade_in_b)),
        stretch: RampParams {
            initial_tempo_percent: effects.editing_rattle_stretch_initial_tempo_percent,
            final_tempo_percent: effects.editing_rattle_stretch_final_tempo_percent,
            initial_pitch_semitones: effects.editing_rattle_stretch_initial_pitch_semitones,
            final_pitch_semitones: effects.editing_rattle_stretch_final_pitch_semitones,
        },
        repeat_count: effects.editing_rattle_repeat_count,
    }
}

/// Builds a `PanToggleParams` from the quick-edit/"Edit Effect Steps"
/// dialogs' in-progress (`editing_*`) Pan Toggle settings in `EffectsState`.
fn pan_toggle_params(effects: &EffectsState) -> PanToggleParams {
    PanToggleParams {
        high_db: effects.editing_pan_toggle_high_db,
        low_db: effects.editing_pan_toggle_low_db,
        direction: effects.editing_pan_toggle_direction,
    }
}

/// Builds a `TripTogglerParams` from the quick-edit/"Edit Effect Steps"
/// dialogs' in-progress (`editing_*`) Trip Toggler settings in
/// `EffectsState`.
fn trip_toggler_params(effects: &EffectsState) -> TripTogglerParams {
    TripTogglerParams {
        high_db: effects.editing_tt_high_db,
        low_db: effects.editing_tt_low_db,
        super_mode: effects.editing_tt_super_mode,
        detail: effects.editing_tt_detail,
        instant_shift: effects.editing_tt_instant_shift,
        instant_high_gain_db: effects.editing_tt_instant_high_gain_db,
        instant_low_gain_db: effects.editing_tt_instant_low_gain_db,
        instant_high_fade_start_db: effects.editing_tt_instant_high_fade_start_db,
        instant_high_fade_end_db: effects.editing_tt_instant_high_fade_end_db,
        instant_low_fade_start_db: effects.editing_tt_instant_low_fade_start_db,
        instant_low_fade_end_db: effects.editing_tt_instant_low_fade_end_db,
        fade_curve_adjust: effects.editing_tt_fade_curve_adjust,
        start_high: effects.editing_tt_start_high,
    }
}

/// For each selected track, sorts its clips by `start_sample` and applies
/// the adjustable fade-in/fade-out alternately — which one starts is given
/// by `starts_with_in` (the quick-edit dialog's in-progress value; see
/// `EffectsState::fade_toggle_starts_with_in` for the committed default,
/// editable in "Edit steps...").
fn apply_fade_toggle(app: &mut RakunatorApp, starts_with_in: bool) {
    let fade_in_a = app.effects.fade_in_point_a_db;
    let fade_in_b = app.effects.fade_in_point_b_db;
    let fade_in_start = db_to_gain(fade_in_a.min(fade_in_b));
    let fade_in_end = db_to_gain(fade_in_a.max(fade_in_b));

    let fade_out_a = app.effects.fade_out_point_a_db;
    let fade_out_b = app.effects.fade_out_point_b_db;
    let fade_out_start = db_to_gain(fade_out_a.max(fade_out_b));
    let fade_out_end = db_to_gain(fade_out_a.min(fade_out_b));

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

/// The title of the effect currently open in the quick-edit dialog (see
/// `draw_active_effect_dialog`) — also used as that `egui::Window`'s id.
fn active_effect_dialog_title(dialog: ActiveEffectDialog) -> &'static str {
    match dialog {
        ActiveEffectDialog::PitchUp => "Pitch Up",
        ActiveEffectDialog::PitchDown => "Pitch Down",
        ActiveEffectDialog::VolumeUp => "Volume Up",
        ActiveEffectDialog::VolumeDown => "Volume Down",
        ActiveEffectDialog::AdjustableFadeIn => "Adjustable Fade In",
        ActiveEffectDialog::AdjustableFadeOut => "Adjustable Fade Out",
        ActiveEffectDialog::FadeToggle => "Fade Toggle",
        ActiveEffectDialog::TempoUp => "Tempo Up",
        ActiveEffectDialog::TempoDown => "Tempo Down",
        ActiveEffectDialog::Reverb => "Reverb",
        ActiveEffectDialog::Echo => "Echo",
        ActiveEffectDialog::Distortion => "Distortion (Hard Clip)",
        ActiveEffectDialog::SlidingStretch => "Sliding Stretch",
        ActiveEffectDialog::PanToggle => "Pan Toggle",
        ActiveEffectDialog::Rattle => "Rattle",
        ActiveEffectDialog::TripToggler => "Trip Toggler",
    }
}

/// Records `effect` as the one Ctrl+R should repeat, and persists it
/// immediately — unlike the committed step defaults (only saved when
/// "Update steps" is checked), the last-used effect is meant to survive a
/// restart unconditionally, so every application of it writes straight
/// through rather than waiting for some other save to carry it along.
fn set_last_effect(app: &mut RakunatorApp, effect: LastEffect) {
    app.effects.last_effect = Some(effect);
    super::settings_persistence::save_effects_settings(&app.effects);
}

/// Applies the effect currently open in the quick-edit dialog, using its
/// `editing_*` value(s) — the same logic each effect's Effects-menu button
/// used to run directly on click, before clicking started opening this
/// dialog (pre-filled from the committed values) instead.
fn apply_active_effect(app: &mut RakunatorApp, dialog: ActiveEffectDialog) {
    let targets = app.project.lock().unwrap().effect_targets();
    match dialog {
        ActiveEffectDialog::PitchUp => {
            let step = app.effects.editing_pitch_up;
            apply_to_targets("Pitch Up", app, &targets, move |p, id| p.apply_pitch_shift(id, step));
            set_last_effect(app, LastEffect::PitchUp(step));
        }
        ActiveEffectDialog::PitchDown => {
            let step = app.effects.editing_pitch_down;
            apply_to_targets("Pitch Down", app, &targets, move |p, id| p.apply_pitch_shift(id, -step));
            set_last_effect(app, LastEffect::PitchDown(step));
        }
        ActiveEffectDialog::VolumeUp => {
            let step_db = app.effects.editing_volume_up;
            let factor = 10f32.powf(step_db / 20.0);
            apply_to_targets("Volume Up", app, &targets, move |p, id| p.apply_gain(id, factor));
            set_last_effect(app, LastEffect::VolumeUp(step_db));
        }
        ActiveEffectDialog::VolumeDown => {
            let step_db = app.effects.editing_volume_down;
            let factor = 10f32.powf(-step_db / 20.0);
            apply_to_targets("Volume Down", app, &targets, move |p, id| p.apply_gain(id, factor));
            set_last_effect(app, LastEffect::VolumeDown(step_db));
        }
        ActiveEffectDialog::AdjustableFadeIn => {
            let a = app.effects.editing_fade_in_a;
            let b = app.effects.editing_fade_in_b;
            let start_gain = db_to_gain(a.min(b));
            let end_gain = db_to_gain(a.max(b));
            apply_to_targets("Adjustable Fade In", app, &targets, move |p, id| p.apply_adjustable_fade(id, start_gain, end_gain));
        }
        ActiveEffectDialog::AdjustableFadeOut => {
            let a = app.effects.editing_fade_out_a;
            let b = app.effects.editing_fade_out_b;
            let start_gain = db_to_gain(a.max(b));
            let end_gain = db_to_gain(a.min(b));
            apply_to_targets("Adjustable Fade Out", app, &targets, move |p, id| p.apply_adjustable_fade(id, start_gain, end_gain));
        }
        ActiveEffectDialog::FadeToggle => {
            apply_fade_toggle(app, app.effects.editing_fade_toggle_starts_with_in);
        }
        ActiveEffectDialog::TempoUp => {
            let step = app.effects.editing_tempo_up;
            apply_to_targets("Tempo Up", app, &targets, move |p, id| p.apply_tempo_shift(id, step));
            set_last_effect(app, LastEffect::TempoUp(step));
        }
        ActiveEffectDialog::TempoDown => {
            let step = app.effects.editing_tempo_down;
            apply_to_targets("Tempo Down", app, &targets, move |p, id| p.apply_tempo_shift(id, -step));
            set_last_effect(app, LastEffect::TempoDown(step));
        }
        ActiveEffectDialog::Reverb => {
            let params = reverb_params(&app.effects);
            apply_to_targets("Reverb", app, &targets, move |p, id| p.apply_reverb(id, &params));
        }
        ActiveEffectDialog::Echo => {
            let delay = app.effects.editing_echo_delay_seconds;
            let decay = app.effects.editing_echo_decay;
            apply_to_targets("Echo", app, &targets, move |p, id| p.apply_echo(id, delay, decay));
        }
        ActiveEffectDialog::Distortion => {
            let drive = app.effects.editing_distortion_drive_db;
            let threshold = app.effects.editing_distortion_threshold;
            apply_to_targets("Hard Clip Distortion", app, &targets, move |p, id| p.apply_hard_clip_distortion(id, drive, threshold));
        }
        ActiveEffectDialog::SlidingStretch => {
            let params = sliding_stretch_params(&app.effects);
            apply_to_targets("Sliding Stretch", app, &targets, move |p, id| p.apply_sliding_stretch(id, &params));
        }
        ActiveEffectDialog::PanToggle => {
            let params = pan_toggle_params(&app.effects);
            apply_to_targets("Pan Toggle", app, &targets, move |p, id| p.apply_pan_toggle(id, &params));
        }
        ActiveEffectDialog::Rattle => {
            let params = rattle_params(&app.effects);
            apply_to_targets("Rattle", app, &targets, move |p, id| p.apply_rattle(id, &params));
        }
        ActiveEffectDialog::TripToggler => {
            let base_params = trip_toggler_params(&app.effects);
            let mut start_high = base_params.start_high;
            let mut project = app.project.lock().unwrap();
            for &id in &targets {
                let params = TripTogglerParams { start_high, ..base_params.clone() };
                project.apply_trip_toggler(id, &params);
                start_high = !start_high;
            }
        }
    }
}

/// Copies the quick-edit dialog's `editing_*` value(s) for `dialog` back
/// into the matching committed field(s) in `EffectsState` — same effect as
/// editing them via "Edit steps..." and clicking its OK. Only run when the
/// quick-edit dialog's "Update steps" checkbox is checked at OK; the
/// caller is responsible for persisting afterwards.
fn commit_effect_defaults(app: &mut RakunatorApp, dialog: ActiveEffectDialog) {
    match dialog {
        ActiveEffectDialog::PitchUp => app.effects.pitch_up_step = app.effects.editing_pitch_up,
        ActiveEffectDialog::PitchDown => app.effects.pitch_down_step = app.effects.editing_pitch_down,
        ActiveEffectDialog::VolumeUp => app.effects.volume_up_step_db = app.effects.editing_volume_up,
        ActiveEffectDialog::VolumeDown => app.effects.volume_down_step_db = app.effects.editing_volume_down,
        ActiveEffectDialog::AdjustableFadeIn => {
            app.effects.fade_in_point_a_db = app.effects.editing_fade_in_a;
            app.effects.fade_in_point_b_db = app.effects.editing_fade_in_b;
        }
        ActiveEffectDialog::AdjustableFadeOut => {
            app.effects.fade_out_point_a_db = app.effects.editing_fade_out_a;
            app.effects.fade_out_point_b_db = app.effects.editing_fade_out_b;
        }
        ActiveEffectDialog::FadeToggle => {
            app.effects.fade_toggle_starts_with_in = app.effects.editing_fade_toggle_starts_with_in;
        }
        ActiveEffectDialog::TempoUp => app.effects.tempo_up_step_percent = app.effects.editing_tempo_up,
        ActiveEffectDialog::TempoDown => app.effects.tempo_down_step_percent = app.effects.editing_tempo_down,
        ActiveEffectDialog::Reverb => {
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
        }
        ActiveEffectDialog::Echo => {
            app.effects.echo_delay_seconds = app.effects.editing_echo_delay_seconds;
            app.effects.echo_decay = app.effects.editing_echo_decay;
        }
        ActiveEffectDialog::Distortion => {
            app.effects.distortion_drive_db = app.effects.editing_distortion_drive_db;
            app.effects.distortion_threshold = app.effects.editing_distortion_threshold;
        }
        ActiveEffectDialog::SlidingStretch => {
            app.effects.stretch_initial_tempo_percent = app.effects.editing_stretch_initial_tempo_percent;
            app.effects.stretch_final_tempo_percent = app.effects.editing_stretch_final_tempo_percent;
            app.effects.stretch_initial_pitch_semitones = app.effects.editing_stretch_initial_pitch_semitones;
            app.effects.stretch_final_pitch_semitones = app.effects.editing_stretch_final_pitch_semitones;
        }
        ActiveEffectDialog::PanToggle => {
            app.effects.pan_toggle_high_db = app.effects.editing_pan_toggle_high_db;
            app.effects.pan_toggle_low_db = app.effects.editing_pan_toggle_low_db;
            app.effects.pan_toggle_direction = app.effects.editing_pan_toggle_direction;
        }
        ActiveEffectDialog::Rattle => {
            app.effects.rattle_pitch_up_semitones = app.effects.editing_rattle_pitch_up_semitones;
            app.effects.rattle_pitch_down_semitones = app.effects.editing_rattle_pitch_down_semitones;
            app.effects.rattle_tempo_x_percent = app.effects.editing_rattle_tempo_x_percent;
            app.effects.rattle_tempo_y_percent = app.effects.editing_rattle_tempo_y_percent;
            app.effects.rattle_fade_in_a_db = app.effects.editing_rattle_fade_in_a_db;
            app.effects.rattle_fade_in_b_db = app.effects.editing_rattle_fade_in_b_db;
            app.effects.rattle_stretch_initial_tempo_percent =
                app.effects.editing_rattle_stretch_initial_tempo_percent;
            app.effects.rattle_stretch_final_tempo_percent =
                app.effects.editing_rattle_stretch_final_tempo_percent;
            app.effects.rattle_stretch_initial_pitch_semitones =
                app.effects.editing_rattle_stretch_initial_pitch_semitones;
            app.effects.rattle_stretch_final_pitch_semitones =
                app.effects.editing_rattle_stretch_final_pitch_semitones;
            app.effects.rattle_repeat_count = app.effects.editing_rattle_repeat_count;
        }
        ActiveEffectDialog::TripToggler => {
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
        }
    }
}

/// Draws the "quick edit" modal that opens when clicking an effect with
/// tweakable step values in the Effects menu (see `draw_effects_menu`),
/// instead of that click applying the effect immediately at its currently
/// committed values. Pre-filled from those committed values via the same
/// `editing_*` scratch fields the full "Edit Effect Steps" dialog uses,
/// editable here, then applied on OK at whatever the field(s) end up
/// holding. The "Update steps" checkbox additionally commits the edited
/// value(s) back as the new defaults (persisted) — unchecked by default,
/// so a one-off tweak here doesn't silently change the defaults unless
/// asked.
pub fn draw_active_effect_dialog(ctx: &egui::Context, app: &mut RakunatorApp) {
    let Some(dialog) = app.effects.active_effect_dialog else {
        return;
    };

    let mut open = true;
    let mut ok = false;
    let mut cancel = false;

    egui::Window::new(active_effect_dialog_title(dialog))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .frame(super::window_frame(ctx, 1, 1, 1, 1))
        .show(ctx, |ui| {
            match dialog {
                ActiveEffectDialog::PitchUp => {
                    egui::Grid::new("quick_pitch_up_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Pitch Up (semitones):");
                        drag_value_scroll(ui, &mut app.effects.editing_pitch_up, 0.1..=12.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::PitchDown => {
                    egui::Grid::new("quick_pitch_down_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Pitch Down (semitones):");
                        drag_value_scroll(ui, &mut app.effects.editing_pitch_down, 0.1..=12.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::VolumeUp => {
                    egui::Grid::new("quick_volume_up_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Volume Up (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_volume_up, 0.1..=24.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::VolumeDown => {
                    egui::Grid::new("quick_volume_down_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Volume Down (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_volume_down, 0.1..=24.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::AdjustableFadeIn => {
                    ui.label("Two dB points, in either order — the effect works out which is louder/quieter.");
                    egui::Grid::new("quick_fade_in_grid").num_columns(3).show(ui, |ui| {
                        ui.label("Fade In points (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_fade_in_a, -60.0..=24.0, 0.1);
                        drag_value_scroll(ui, &mut app.effects.editing_fade_in_b, -60.0..=24.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::AdjustableFadeOut => {
                    ui.label("Two dB points, in either order — the effect works out which is louder/quieter.");
                    egui::Grid::new("quick_fade_out_grid").num_columns(3).show(ui, |ui| {
                        ui.label("Fade Out points (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_fade_out_a, -60.0..=24.0, 0.1);
                        drag_value_scroll(ui, &mut app.effects.editing_fade_out_b, -60.0..=24.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::FadeToggle => {
                    ui.label("Which comes first on each selected track's earliest clip?");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.effects.editing_fade_toggle_starts_with_in, true, "Fade In first");
                        ui.radio_value(&mut app.effects.editing_fade_toggle_starts_with_in, false, "Fade Out first");
                    });
                }
                ActiveEffectDialog::TempoUp => {
                    egui::Grid::new("quick_tempo_up_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Tempo Up (%):");
                        drag_value_scroll(ui, &mut app.effects.editing_tempo_up, 0.1..=200.0, 0.5);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::TempoDown => {
                    egui::Grid::new("quick_tempo_down_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Tempo Down (%):");
                        drag_value_scroll(ui, &mut app.effects.editing_tempo_down, 0.1..=90.0, 0.5);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::Reverb => {
                    egui::Grid::new("quick_reverb_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Room Size:");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_room_size, 0.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("Reverberance:");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_reverberance, 0.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("HF Damping:");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_hf_damping, 0.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("Tone Low:");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_tone_low, 0.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("Tone High:");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_tone_high, 0.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("Wet Gain (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_wet_gain_db, -60.0..=10.0, 0.5);
                        ui.end_row();
                        ui.label("Dry Gain (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_dry_gain_db, -60.0..=10.0, 0.5);
                        ui.end_row();
                        ui.label("Stereo Width:");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_stereo_width, 0.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("Pre-Delay (ms):");
                        drag_value_scroll(ui, &mut app.effects.editing_reverb_pre_delay_ms, 0.0..=500.0, 1.0);
                        ui.end_row();
                        ui.label("Wet Only:");
                        ui.checkbox(&mut app.effects.editing_reverb_wet_only, "");
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::Echo => {
                    egui::Grid::new("quick_echo_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Delay time (s):");
                        drag_value_scroll(ui, &mut app.effects.editing_echo_delay_seconds, 0.001..=10.0, 0.05);
                        ui.end_row();
                        ui.label("Decay factor:");
                        drag_value_scroll(ui, &mut app.effects.editing_echo_decay, 0.0..=2.0, 0.01);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::Distortion => {
                    egui::Grid::new("quick_distortion_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Drive (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_distortion_drive_db, 0.0..=48.0, 0.5);
                        ui.end_row();
                        ui.label("Clip Threshold:");
                        drag_value_scroll(ui, &mut app.effects.editing_distortion_threshold, 0.01..=1.0, 0.01);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::SlidingStretch => {
                    ui.label("Ramps tempo/pitch from the clip's start to its end.");
                    egui::Grid::new("quick_sliding_stretch_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Initial Tempo Change (%):");
                        drag_value_scroll(ui, &mut app.effects.editing_stretch_initial_tempo_percent, -90.0..=500.0, 0.5);
                        ui.end_row();
                        ui.label("Final Tempo Change (%):");
                        drag_value_scroll(ui, &mut app.effects.editing_stretch_final_tempo_percent, -90.0..=500.0, 0.5);
                        ui.end_row();
                        ui.label("Initial Pitch Shift (semitones):");
                        drag_value_scroll(ui, &mut app.effects.editing_stretch_initial_pitch_semitones, -24.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Final Pitch Shift (semitones):");
                        drag_value_scroll(ui, &mut app.effects.editing_stretch_final_pitch_semitones, -24.0..=24.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::PanToggle => {
                    ui.label("Splits a stereo clip's channels, fades one up and the other down.");
                    ui.horizontal(|ui| {
                        ui.label("Fade-in side:");
                        ui.radio_value(&mut app.effects.editing_pan_toggle_direction, PanToggleDirection::Left, "Left");
                        ui.radio_value(&mut app.effects.editing_pan_toggle_direction, PanToggleDirection::Right, "Right");
                    });
                    egui::Grid::new("quick_pan_toggle_grid").num_columns(2).show(ui, |ui| {
                        ui.label("High dB (fade-in end / fade-out start):");
                        drag_value_scroll(ui, &mut app.effects.editing_pan_toggle_high_db, -60.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Low dB (fade-in start / fade-out end):");
                        drag_value_scroll(ui, &mut app.effects.editing_pan_toggle_low_db, -60.0..=24.0, 0.1);
                        ui.end_row();
                    });
                }
                ActiveEffectDialog::Rattle => {
                    ui.label("Own Adjustable Fade In / Sliding Stretch settings, separate from the ones above.");
                    egui::Grid::new("quick_rattle_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Pitch Up (semitones):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_pitch_up_semitones, 0.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Pitch Down (semitones):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_pitch_down_semitones, 0.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Tempo +x% (first clip):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_tempo_x_percent, -90.0..=200.0, 0.5);
                        ui.end_row();
                        ui.label("Tempo -y% (second clip):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_tempo_y_percent, -90.0..=200.0, 0.5);
                        ui.end_row();
                        ui.label("Fade In points (dB):");
                        ui.horizontal(|ui| {
                            drag_value_scroll(ui, &mut app.effects.editing_rattle_fade_in_a_db, -60.0..=24.0, 0.1);
                            drag_value_scroll(ui, &mut app.effects.editing_rattle_fade_in_b_db, -60.0..=24.0, 0.1);
                        });
                        ui.end_row();
                        ui.label("Sliding Stretch Initial Tempo (%):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_initial_tempo_percent, -90.0..=500.0, 0.5);
                        ui.end_row();
                        ui.label("Sliding Stretch Final Tempo (%):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_final_tempo_percent, -90.0..=500.0, 0.5);
                        ui.end_row();
                        ui.label("Sliding Stretch Initial Pitch (st):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_initial_pitch_semitones, -24.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Sliding Stretch Final Pitch (st):");
                        drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_final_pitch_semitones, -24.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Repeat Count (A+B clips):");
                        drag_value_scroll_u32(ui, &mut app.effects.editing_rattle_repeat_count, 2..=128, 2.0);
                        ui.end_row();
                    });
                    app.effects.editing_rattle_repeat_count = (app.effects.editing_rattle_repeat_count / 2).max(1) * 2;
                }
                ActiveEffectDialog::TripToggler => {
                    ui.label("Finds clear low points and alternates a fade down/up across the segments.");
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
                    egui::Grid::new("quick_trip_toggler_grid").num_columns(2).show(ui, |ui| {
                        ui.label("Detail (detection fine-tune):");
                        drag_value_scroll(ui, &mut app.effects.editing_tt_detail, 0.01..=10.0, 0.05);
                        ui.end_row();
                        ui.label("Fade Curve Adjust (-100..100):");
                        drag_value_scroll(ui, &mut app.effects.editing_tt_fade_curve_adjust, -100.0..=100.0, 1.0);
                        ui.end_row();
                        ui.label("Gradual High dB:");
                        drag_value_scroll(ui, &mut app.effects.editing_tt_high_db, -60.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Gradual Low dB:");
                        drag_value_scroll(ui, &mut app.effects.editing_tt_low_db, -60.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Instant High Gain Step (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_tt_instant_high_gain_db, -60.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Instant Low Gain Step (dB):");
                        drag_value_scroll(ui, &mut app.effects.editing_tt_instant_low_gain_db, -60.0..=24.0, 0.1);
                        ui.end_row();
                        ui.label("Instant High Fade (dB):");
                        ui.horizontal(|ui| {
                            drag_value_scroll(ui, &mut app.effects.editing_tt_instant_high_fade_start_db, -60.0..=24.0, 0.1);
                            drag_value_scroll(ui, &mut app.effects.editing_tt_instant_high_fade_end_db, -60.0..=24.0, 0.1);
                        });
                        ui.end_row();
                        ui.label("Instant Low Fade (dB):");
                        ui.horizontal(|ui| {
                            drag_value_scroll(ui, &mut app.effects.editing_tt_instant_low_fade_start_db, -60.0..=24.0, 0.1);
                            drag_value_scroll(ui, &mut app.effects.editing_tt_instant_low_fade_end_db, -60.0..=24.0, 0.1);
                        });
                        ui.end_row();
                    });
                }
            }

            ui.add_space(12.0);
            ui.checkbox(&mut app.effects.remember_as_default, "Update steps (remember these values as the default)");
            ui.add_space(8.0);
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
        apply_active_effect(app, dialog);
        if app.effects.remember_as_default {
            commit_effect_defaults(app, dialog);
            super::settings_persistence::save_effects_settings(&app.effects);
        }
        app.effects.active_effect_dialog = None;
    } else if cancel || !open {
        app.effects.active_effect_dialog = None;
    }
}

/// Draws a bold "subtitle" heading followed by a separator line, marking
/// the start of one effect's settings group in the "Edit Effect Steps"
/// dialog — the extra vertical space above it (well beyond a plain
/// `add_space`) is what keeps a long, otherwise unbroken column of dozens
/// of fields visually sorted into which effect each one belongs to.
fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(18.0);
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.separator();
    ui.add_space(4.0);
}

/// Draws a `DragValue` and, if the pointer ends up hovering it, applies
/// one `step`-sized nudge per scroll-wheel notch (clamped to `range`) —
/// the same scroll-to-adjust convenience the Pan/Vol sliders already have
/// (see `track_view::draw_header`), just for the "Edit Effect Steps"
/// dialog's `DragValue`s instead of a `Slider`.
fn drag_value_scroll(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32) {
    let response = ui.add(egui::DragValue::new(value).range(range.clone()).speed(step));
    if response.hovered() {
        let notches = super::wheel_notches(ui);
        if notches != 0.0 {
            *value = (*value + notches.round() * step).clamp(*range.start(), *range.end());
        }
        // `wheel_notches` only zeroes the scroll delta on frames where it
        // saw a fresh wheel event; egui smooths scrolling over several
        // frames afterward, and that leftover momentum would otherwise
        // reach the "Edit Effect Steps" dialog's surrounding `ScrollArea`
        // and scroll the whole dialog out from under the field being
        // adjusted. Swallow it every frame this field is hovered, not just
        // the frames with a fresh notch.
        ui.ctx().input_mut(|i| i.smooth_scroll_delta.y = 0.0);
    }
}

/// Integer counterpart of `drag_value_scroll`, for `editing_rattle_repeat_count`
/// (the only non-float field among these `DragValue`s).
fn drag_value_scroll_u32(ui: &mut egui::Ui, value: &mut u32, range: std::ops::RangeInclusive<u32>, step: f32) {
    let response = ui.add(egui::DragValue::new(value).range(range.clone()).speed(step));
    if response.hovered() {
        let notches = super::wheel_notches(ui);
        if notches != 0.0 {
            let delta = (notches.round() * step) as i64;
            let nudged = (*value as i64 + delta).clamp(*range.start() as i64, *range.end() as i64);
            *value = nudged as u32;
        }
        // See the matching comment in `drag_value_scroll` — swallow any
        // leftover scroll momentum every frame this field is hovered so it
        // never bleeds through to the dialog's surrounding `ScrollArea`.
        ui.ctx().input_mut(|i| i.smooth_scroll_delta.y = 0.0);
    }
}

/// Draws the "Edit steps..." modal for every effect's tweakable settings
/// (step sizes, dB points, Reverb/Echo/Rattle/etc. parameters), with
/// OK/Cancel/Reset to Defaults — a real dialog rather than a right-click
/// popup, since right-clicking a button nested inside an already-open menu
/// doesn't reliably open a second, nested popup in egui.
pub fn draw_effects_settings_dialog(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.effects.settings_open {
        return;
    }

    let mut open = true;
    let mut ok = false;
    let mut cancel = false;

    egui::Window::new("Edit Effect Steps")
        .open(&mut open)
        .default_width(520.0)
        .default_height(600.0)
        .resizable(true)
        .frame(super::window_frame(ctx, 1, 1, 1, 0))
        .show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label("Search:");
            // Right-to-left so the Clear button claims its width first, and
            // the text edit fills exactly what's left — see the matching
            // comment in `help_dialog::draw` for why the naive left-to-right
            // order (text edit first, `desired_width(INFINITY)`) overflows
            // the row by one button's width and misaligns it under the
            // window's own close button.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !app.effects.settings_search.is_empty()
                    && ui.button("\u{2715}").on_hover_text("Clear").clicked()
                {
                    app.effects.settings_search.clear();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut app.effects.settings_search)
                        .hint_text("effect or setting name...")
                        .desired_width(ui.available_width()),
                );
            });
        });
        ui.add_space(4.0);

        let query = app.effects.settings_search.trim().to_lowercase();
        // Whether `title`'s section should be drawn: an empty query always
        // shows everything; otherwise the section title matching (which, as
        // in Help, shows every field under it regardless of their own
        // labels) or any of that section's own field labels matching. A
        // pure function of `query` (no shared mutable state) so it can be
        // called both here, to precompute whether *anything* matched, and
        // again per-section below as each one's visibility gate — a
        // `FnMut` version that updated a shared "did anything match" flag
        // as a side effect doesn't borrow-check once it's also captured by
        // the `ScrollArea` closure below, since that closure would need to
        // hold the mutable borrow live for its entire body.
        let show_section = |title: &str, field_labels: &[&str]| -> bool {
            query.is_empty()
                || title.to_lowercase().contains(&query)
                || field_labels.iter().any(|l| l.to_lowercase().contains(&query))
        };
        let any_section_shown = show_section(
            "Pitch & Volume",
            &["Pitch Up (semitones)", "Pitch Down (semitones)", "Volume Up (dB)", "Volume Down (dB)"],
        ) || show_section("Adjustable Fades", &["Fade In points (dB)", "Fade Out points (dB)"])
            || show_section("Fade Toggle", &["Fade In first", "Fade Out first"])
            || show_section("Tempo Steps", &["Tempo Up (%)", "Tempo Down (%)"])
            || show_section(
                "Reverb",
                &[
                    "Room Size", "Reverberance", "HF Damping", "Tone Low", "Tone High", "Wet Gain (dB)",
                    "Dry Gain (dB)", "Stereo Width", "Pre-Delay (ms)", "Wet Only",
                ],
            )
            || show_section("Echo", &["Delay time (s)", "Decay factor"])
            || show_section("Distortion (Hard Clip)", &["Drive (dB)", "Clip Threshold"])
            || show_section(
                "Sliding Stretch",
                &["Initial Tempo Change (%)", "Final Tempo Change (%)", "Initial Pitch Shift (semitones)", "Final Pitch Shift (semitones)"],
            )
            || show_section(
                "Rattle",
                &[
                    "Pitch Up (semitones)", "Pitch Down (semitones)", "Tempo +x% (first clip)", "Tempo -y% (second clip)",
                    "Fade In points (dB)", "Sliding Stretch Initial Tempo (%)", "Sliding Stretch Final Tempo (%)",
                    "Sliding Stretch Initial Pitch (st)", "Sliding Stretch Final Pitch (st)", "Repeat Count (A+B clips)",
                ],
            )
            || show_section("Pan Toggle", &["Fade-in side", "High dB (fade-in end / fade-out start)", "Low dB (fade-in start / fade-out end)"])
            || show_section(
                "Trip Toggler",
                &[
                    "Detection mode", "Starts", "Shift mode", "Detail (detection fine-tune)", "Fade Curve Adjust (-100..100)",
                    "Gradual High dB", "Gradual Low dB", "Instant High Gain Step (dB)", "Instant Low Gain Step (dB)",
                    "Instant High Fade (dB)", "Instant Low Fade (dB)",
                ],
            );

        // Leaves room below for the OK/Cancel/Reset row and its surrounding
        // spacing (see further down) rather than a fixed constant, so
        // dragging the window taller actually grows the scrollable list
        // instead of just adding dead space under a size-capped `ScrollArea`.
        const BOTTOM_CONTROLS_RESERVED_HEIGHT: f32 = 56.0;
        let list_height = (ui.available_height() - BOTTOM_CONTROLS_RESERVED_HEIGHT).max(120.0);
        egui::ScrollArea::vertical().max_height(list_height).show(ui, |ui| {
        if show_section(
            "Pitch & Volume",
            &["Pitch Up (semitones)", "Pitch Down (semitones)", "Volume Up (dB)", "Volume Down (dB)"],
        ) {
        section_header(ui, "Pitch & Volume");
        egui::Grid::new("effect_steps_grid").num_columns(2).show(ui, |ui| {
            ui.label("Pitch Up (semitones):");
            drag_value_scroll(ui, &mut app.effects.editing_pitch_up, 0.1..=12.0, 0.1);
            ui.end_row();

            ui.label("Pitch Down (semitones):");
            drag_value_scroll(ui, &mut app.effects.editing_pitch_down, 0.1..=12.0, 0.1);
            ui.end_row();

            ui.label("Volume Up (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_volume_up, 0.1..=24.0, 0.1);
            ui.end_row();

            ui.label("Volume Down (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_volume_down, 0.1..=24.0, 0.1);
            ui.end_row();
        });
        }

        if show_section("Adjustable Fades", &["Fade In points (dB)", "Fade Out points (dB)"]) {
        section_header(ui, "Adjustable Fades");
        ui.label("Two dB points, in either order — the effect works out which is louder/quieter.");
        egui::Grid::new("fade_points_grid").num_columns(3).show(ui, |ui| {
            ui.label("Fade In points (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_fade_in_a, -60.0..=24.0, 0.1);
            drag_value_scroll(ui, &mut app.effects.editing_fade_in_b, -60.0..=24.0, 0.1);
            ui.end_row();

            ui.label("Fade Out points (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_fade_out_a, -60.0..=24.0, 0.1);
            drag_value_scroll(ui, &mut app.effects.editing_fade_out_b, -60.0..=24.0, 0.1);
            ui.end_row();
        });
        }

        if show_section("Fade Toggle", &["Fade In first", "Fade Out first"]) {
        section_header(ui, "Fade Toggle");
        ui.label("Which comes first on each selected track's earliest clip?");
        ui.horizontal(|ui| {
            ui.radio_value(&mut app.effects.editing_fade_toggle_starts_with_in, true, "Fade In first");
            ui.radio_value(&mut app.effects.editing_fade_toggle_starts_with_in, false, "Fade Out first");
        });
        }

        if show_section("Tempo Steps", &["Tempo Up (%)", "Tempo Down (%)"]) {
        section_header(ui, "Tempo Steps");
        egui::Grid::new("tempo_steps_grid").num_columns(2).show(ui, |ui| {
            ui.label("Tempo Up (%):");
            drag_value_scroll(ui, &mut app.effects.editing_tempo_up, 0.1..=200.0, 0.5);
            ui.end_row();

            ui.label("Tempo Down (%):");
            drag_value_scroll(ui, &mut app.effects.editing_tempo_down, 0.1..=90.0, 0.5);
            ui.end_row();
        });
        }

        if show_section(
            "Reverb",
            &[
                "Room Size", "Reverberance", "HF Damping", "Tone Low", "Tone High", "Wet Gain (dB)",
                "Dry Gain (dB)", "Stereo Width", "Pre-Delay (ms)", "Wet Only",
            ],
        ) {
        section_header(ui, "Reverb");
        egui::Grid::new("reverb_grid").num_columns(2).show(ui, |ui| {
            ui.label("Room Size:");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_room_size, 0.0..=100.0, 1.0);
            ui.end_row();
            ui.label("Reverberance:");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_reverberance, 0.0..=100.0, 1.0);
            ui.end_row();
            ui.label("HF Damping:");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_hf_damping, 0.0..=100.0, 1.0);
            ui.end_row();
            ui.label("Tone Low:");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_tone_low, 0.0..=100.0, 1.0);
            ui.end_row();
            ui.label("Tone High:");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_tone_high, 0.0..=100.0, 1.0);
            ui.end_row();
            ui.label("Wet Gain (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_wet_gain_db, -60.0..=10.0, 0.5);
            ui.end_row();
            ui.label("Dry Gain (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_dry_gain_db, -60.0..=10.0, 0.5);
            ui.end_row();
            ui.label("Stereo Width:");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_stereo_width, 0.0..=100.0, 1.0);
            ui.end_row();
            ui.label("Pre-Delay (ms):");
            drag_value_scroll(ui, &mut app.effects.editing_reverb_pre_delay_ms, 0.0..=500.0, 1.0);
            ui.end_row();
            ui.label("Wet Only:");
            ui.checkbox(&mut app.effects.editing_reverb_wet_only, "");
            ui.end_row();
        });
        }

        if show_section("Echo", &["Delay time (s)", "Decay factor"]) {
        section_header(ui, "Echo");
        egui::Grid::new("echo_grid").num_columns(2).show(ui, |ui| {
            ui.label("Delay time (s):");
            drag_value_scroll(ui, &mut app.effects.editing_echo_delay_seconds, 0.001..=10.0, 0.05);
            ui.end_row();
            ui.label("Decay factor:");
            drag_value_scroll(ui, &mut app.effects.editing_echo_decay, 0.0..=2.0, 0.01);
            ui.end_row();
        });
        }

        if show_section("Distortion (Hard Clip)", &["Drive (dB)", "Clip Threshold"]) {
        section_header(ui, "Distortion (Hard Clip)");
        egui::Grid::new("distortion_grid").num_columns(2).show(ui, |ui| {
            ui.label("Drive (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_distortion_drive_db, 0.0..=48.0, 0.5);
            ui.end_row();
            ui.label("Clip Threshold:");
            drag_value_scroll(ui, &mut app.effects.editing_distortion_threshold, 0.01..=1.0, 0.01);
            ui.end_row();
        });
        }

        if show_section(
            "Sliding Stretch",
            &["Initial Tempo Change (%)", "Final Tempo Change (%)", "Initial Pitch Shift (semitones)", "Final Pitch Shift (semitones)"],
        ) {
        section_header(ui, "Sliding Stretch");
        ui.label("Ramps tempo/pitch from the clip's start to its end.");
        egui::Grid::new("sliding_stretch_grid").num_columns(2).show(ui, |ui| {
            ui.label("Initial Tempo Change (%):");
            drag_value_scroll(ui, &mut app.effects.editing_stretch_initial_tempo_percent, -90.0..=500.0, 0.5);
            ui.end_row();
            ui.label("Final Tempo Change (%):");
            drag_value_scroll(ui, &mut app.effects.editing_stretch_final_tempo_percent, -90.0..=500.0, 0.5);
            ui.end_row();
            ui.label("Initial Pitch Shift (semitones):");
            drag_value_scroll(ui, &mut app.effects.editing_stretch_initial_pitch_semitones, -24.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Final Pitch Shift (semitones):");
            drag_value_scroll(ui, &mut app.effects.editing_stretch_final_pitch_semitones, -24.0..=24.0, 0.1);
            ui.end_row();
        });
        }

        if show_section(
            "Rattle",
            &[
                "Pitch Up (semitones)", "Pitch Down (semitones)", "Tempo +x% (first clip)", "Tempo -y% (second clip)",
                "Fade In points (dB)", "Sliding Stretch Initial Tempo (%)", "Sliding Stretch Final Tempo (%)",
                "Sliding Stretch Initial Pitch (st)", "Sliding Stretch Final Pitch (st)", "Repeat Count (A+B clips)",
            ],
        ) {
        section_header(ui, "Rattle");
        ui.label("Own Adjustable Fade In / Sliding Stretch settings, separate from the ones above.");
        egui::Grid::new("rattle_grid").num_columns(2).show(ui, |ui| {
            ui.label("Pitch Up (semitones):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_pitch_up_semitones, 0.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Pitch Down (semitones):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_pitch_down_semitones, 0.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Tempo +x% (first clip):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_tempo_x_percent, -90.0..=200.0, 0.5);
            ui.end_row();
            ui.label("Tempo -y% (second clip):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_tempo_y_percent, -90.0..=200.0, 0.5);
            ui.end_row();
            ui.label("Fade In points (dB):");
            ui.horizontal(|ui| {
                drag_value_scroll(ui, &mut app.effects.editing_rattle_fade_in_a_db, -60.0..=24.0, 0.1);
                drag_value_scroll(ui, &mut app.effects.editing_rattle_fade_in_b_db, -60.0..=24.0, 0.1);
            });
            ui.end_row();
            ui.label("Sliding Stretch Initial Tempo (%):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_initial_tempo_percent, -90.0..=500.0, 0.5);
            ui.end_row();
            ui.label("Sliding Stretch Final Tempo (%):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_final_tempo_percent, -90.0..=500.0, 0.5);
            ui.end_row();
            ui.label("Sliding Stretch Initial Pitch (st):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_initial_pitch_semitones, -24.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Sliding Stretch Final Pitch (st):");
            drag_value_scroll(ui, &mut app.effects.editing_rattle_stretch_final_pitch_semitones, -24.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Repeat Count (A+B clips):");
            drag_value_scroll_u32(ui, &mut app.effects.editing_rattle_repeat_count, 2..=128, 2.0);
            ui.end_row();
        });
        // Always an even number of whole [A, B] pairs, in steps of 2.
        app.effects.editing_rattle_repeat_count = (app.effects.editing_rattle_repeat_count / 2).max(1) * 2;
        }

        if show_section("Pan Toggle", &["Fade-in side", "High dB (fade-in end / fade-out start)", "Low dB (fade-in start / fade-out end)"]) {
        section_header(ui, "Pan Toggle");
        ui.label("Splits a stereo clip's channels, fades one up and the other down.");
        ui.horizontal(|ui| {
            ui.label("Fade-in side:");
            ui.radio_value(&mut app.effects.editing_pan_toggle_direction, PanToggleDirection::Left, "Left");
            ui.radio_value(&mut app.effects.editing_pan_toggle_direction, PanToggleDirection::Right, "Right");
        });
        egui::Grid::new("pan_toggle_grid").num_columns(2).show(ui, |ui| {
            ui.label("High dB (fade-in end / fade-out start):");
            drag_value_scroll(ui, &mut app.effects.editing_pan_toggle_high_db, -60.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Low dB (fade-in start / fade-out end):");
            drag_value_scroll(ui, &mut app.effects.editing_pan_toggle_low_db, -60.0..=24.0, 0.1);
            ui.end_row();
        });
        }

        if show_section(
            "Trip Toggler",
            &[
                "Detection mode", "Starts", "Shift mode", "Detail (detection fine-tune)", "Fade Curve Adjust (-100..100)",
                "Gradual High dB", "Gradual Low dB", "Instant High Gain Step (dB)", "Instant Low Gain Step (dB)",
                "Instant High Fade (dB)", "Instant Low Fade (dB)",
            ],
        ) {
        section_header(ui, "Trip Toggler");
        ui.label("Finds clear low points and alternates a fade down/up across the segments.");
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
            drag_value_scroll(ui, &mut app.effects.editing_tt_detail, 0.01..=10.0, 0.05);
            ui.end_row();
            ui.label("Fade Curve Adjust (-100..100):");
            drag_value_scroll(ui, &mut app.effects.editing_tt_fade_curve_adjust, -100.0..=100.0, 1.0);
            ui.end_row();
            ui.label("Gradual High dB:");
            drag_value_scroll(ui, &mut app.effects.editing_tt_high_db, -60.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Gradual Low dB:");
            drag_value_scroll(ui, &mut app.effects.editing_tt_low_db, -60.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Instant High Gain Step (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_tt_instant_high_gain_db, -60.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Instant Low Gain Step (dB):");
            drag_value_scroll(ui, &mut app.effects.editing_tt_instant_low_gain_db, -60.0..=24.0, 0.1);
            ui.end_row();
            ui.label("Instant High Fade (dB):");
            ui.horizontal(|ui| {
                drag_value_scroll(ui, &mut app.effects.editing_tt_instant_high_fade_start_db, -60.0..=24.0, 0.1);
                drag_value_scroll(ui, &mut app.effects.editing_tt_instant_high_fade_end_db, -60.0..=24.0, 0.1);
            });
            ui.end_row();
            ui.label("Instant Low Fade (dB):");
            ui.horizontal(|ui| {
                drag_value_scroll(ui, &mut app.effects.editing_tt_instant_low_fade_start_db, -60.0..=24.0, 0.1);
                drag_value_scroll(ui, &mut app.effects.editing_tt_instant_low_fade_end_db, -60.0..=24.0, 0.1);
            });
            ui.end_row();
        });
        }

        if !any_section_shown {
            ui.add_space(12.0);
            ui.weak(format!("No effect settings match \"{}\".", app.effects.settings_search.trim()));
        }
        });

        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                ok = true;
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if ui
                .button("Reset to Defaults")
                .on_hover_text("Resets every field above back to its built-in default — click OK to apply, or Cancel to discard.")
                .clicked()
            {
                app.effects.reset_editing_to_defaults();
            }
        });
        ui.add_space(1.0);
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

/// Applies `f` to every clip in `targets`, then toasts a confirmation
/// naming `label` (the effect) and how many clips it landed on — or, if
/// `targets` is empty (nothing selected), toasts that instead of silently
/// doing nothing, without ever taking the project lock.
fn apply_to_targets(label: &str, app: &mut RakunatorApp, targets: &[ClipId], f: impl Fn(&mut crate::project::Project, ClipId)) {
    if targets.is_empty() {
        toast::show(app, format!("{label}: no clip selected"));
        return;
    }
    {
        let mut project = app.project.lock().unwrap();
        for &id in targets {
            f(&mut project, id);
        }
    }
    let n = targets.len();
    let plural = if n == 1 { "clip" } else { "clips" };
    toast::show(app, format!("{label} applied to {n} {plural}"));
}
