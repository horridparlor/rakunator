//! Bethoven's GUI: a self-contained piano-roll composer window, opened from
//! the toolbar. Owns a private single-track scratch `Project` + its own
//! `AudioEngine` purely for preview playback of the section currently being
//! edited — fully decoupled from the main timeline's transport, undo
//! history and tracks (see the plan's "Key architectural decisions").

mod piano_roll;
mod toolbar;

use crate::audio_engine::AudioEngine;
use crate::bethoven::melody::{self, Melody, Note, Section};
use crate::project::{Project, TrackId};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::RakunatorApp;

/// How many melody snapshots Bethoven's own undo history keeps — same cap
/// as `project::MAX_UNDO_HISTORY`, for the same reason (bounded memory over
/// a long editing session).
const MAX_BETHOVEN_UNDO: usize = 50;

/// A new section's not-yet-committed settings, edited in a small popup
/// before "Add" actually creates it.
struct NewSectionDraft {
    name: String,
    bars: u32,
    root: u8,
    scale_index: usize,
}

impl Default for NewSectionDraft {
    fn default() -> Self {
        NewSectionDraft { name: "Section".to_string(), bars: melody::DEFAULT_SECTION_BARS, root: 0, scale_index: 0 }
    }
}

pub struct BethovenState {
    pub open: bool,
    /// Whether Bethoven is the thing the user is currently interacting
    /// with, tracked by which side of its window rect the last click
    /// landed on (see `draw`) — while it's open but *not* focused (the
    /// user clicked back over the main timeline behind it), keyboard
    /// shortcuts route to the main app instead of here, so e.g. Ctrl+X on
    /// a main-timeline clip isn't swallowed by Bethoven's own Ctrl+X.
    pub has_focus: bool,
    /// Whether the Bethoven window currently fills the whole screen rect
    /// (toggled from its own toolbar) instead of floating/resizable.
    fullscreen: bool,
    active_melody_id: Option<u32>,
    active_section_id: Option<u32>,
    /// Selected note ids, scoped to whichever section is currently active
    /// (switching sections clears it — a note id is only unique within its
    /// own section).
    selection: HashSet<u32>,
    clipboard: Vec<Note>,
    scroll_x: f32,
    scroll_y: f32,
    px_per_tick: f32,
    /// Set whenever an edit could change the active section's audio, so the
    /// preview buffer gets rebuilt before the next time it matters
    /// (immediately if already playing, otherwise lazily on the next Play).
    dirty: bool,
    preview_project: Arc<Mutex<Project>>,
    preview_engine: AudioEngine,
    preview_track: TrackId,
    play_start_sample: Option<u64>,
    play_start_scroll_x: Option<f32>,
    drag: Option<piano_roll::Drag>,
    new_section_draft: Option<NewSectionDraft>,
    renaming_melody: Option<String>,
    renaming_section: Option<String>,
    /// Where a click on the ruler, or a keyboard nudge, snapped the
    /// playhead to a nearby note edge, and when — drawn as a brief yellow
    /// alignment line, the same idea as the main timeline's
    /// `click_snap_flash`. A live drag's own snap (see `piano_roll::draw`)
    /// is drawn straight from that frame's drag state instead of this.
    snap_flash: Option<(u32, Instant)>,
    /// Per-melody undo/redo history — snapshots of the whole `Melody` (so
    /// a snapshot always knows which melody it belongs to via its `id`,
    /// even if the active melody is switched between an edit and an undo).
    /// Deliberately lives here, not on `Melody`/`Section` themselves, so
    /// those stay plain, headless-testable data — same split as
    /// `Project`'s own undo stack.
    undo_stack: Vec<Melody>,
    redo_stack: Vec<Melody>,
}

impl BethovenState {
    pub fn new(sample_rate_hz: u32) -> Self {
        let preview_project = Arc::new(Mutex::new(Project::new(sample_rate_hz)));
        let preview_track = preview_project.lock().unwrap().tracks[0].id;
        let preview_engine = AudioEngine::start(Arc::clone(&preview_project));
        BethovenState {
            open: false,
            has_focus: true,
            fullscreen: false,
            active_melody_id: None,
            active_section_id: None,
            selection: HashSet::new(),
            clipboard: Vec::new(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            px_per_tick: 0.2,
            dirty: true,
            preview_project,
            preview_engine,
            preview_track,
            play_start_sample: None,
            play_start_scroll_x: None,
            drag: None,
            new_section_draft: None,
            renaming_melody: None,
            renaming_section: None,
            snap_flash: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Makes sure there's always a melody and section to edit: creates a
    /// first melody the very first time Bethoven is opened on a project
    /// that has none yet, and falls back to the first melody/section
    /// whenever the previously-active one no longer exists (e.g. it was
    /// just deleted).
    fn ensure_active(&mut self, project: &mut Project) {
        if project.melodies.is_empty() {
            let id = project.next_melody_id();
            project.melodies.push(Melody::new(id, "Melody 1".to_string()));
        }
        if !self.active_melody_id.is_some_and(|id| project.melodies.iter().any(|m| m.id == id)) {
            self.active_melody_id = project.melodies.first().map(|m| m.id);
            self.active_section_id = None;
            self.selection.clear();
        }
        let section_valid = self
            .active_melody(project)
            .is_some_and(|m| self.active_section_id.is_some_and(|id| m.section(id).is_some()));
        if !section_valid {
            self.active_section_id = self.active_melody(project).and_then(|m| m.sections.first()).map(|s| s.id);
            self.selection.clear();
        }
    }

    fn active_melody<'a>(&self, project: &'a Project) -> Option<&'a Melody> {
        self.active_melody_id.and_then(|id| project.melodies.iter().find(|m| m.id == id))
    }

    fn active_melody_mut<'a>(&self, project: &'a mut Project) -> Option<&'a mut Melody> {
        let id = self.active_melody_id?;
        project.melodies.iter_mut().find(|m| m.id == id)
    }

    fn active_section<'a>(&self, project: &'a Project) -> Option<&'a Section> {
        let section_id = self.active_section_id?;
        self.active_melody(project)?.section(section_id)
    }

    fn active_section_mut<'a>(&self, project: &'a mut Project) -> Option<&'a mut Section> {
        let section_id = self.active_section_id?;
        self.active_melody_mut(project)?.section_mut(section_id)
    }

    /// Marks the preview buffer stale — called after any edit that could
    /// change the active section's audio.
    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn is_playing(&self) -> bool {
        self.preview_engine.is_playing()
    }

    /// Snapshots the active melody for undo, before a mutating edit. Called
    /// once per discrete action (a click, a whole drag gesture, one wheel
    /// notch) — never per-frame during a drag, which would flood the stack
    /// with near-identical states.
    fn record_undo(&mut self, project: &Project) {
        if let Some(melody) = self.active_melody(project) {
            self.undo_stack.push(melody.clone());
            if self.undo_stack.len() > MAX_BETHOVEN_UNDO {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }

    /// Ctrl+Z: reverts the active melody to its state before the last
    /// recorded edit, switching to it if a different melody was active.
    fn undo(&mut self, project: &mut Project) {
        let Some(prev) = self.undo_stack.pop() else { return };
        if let Some(current) = self.active_melody(project).cloned() {
            self.redo_stack.push(current);
        }
        let id = prev.id;
        if let Some(slot) = project.melodies.iter_mut().find(|m| m.id == id) {
            *slot = prev;
        }
        self.active_melody_id = Some(id);
        self.selection.clear();
        self.mark_dirty();
    }

    /// Ctrl+Shift+Z: re-applies the most recently undone edit.
    fn redo(&mut self, project: &mut Project) {
        let Some(next) = self.redo_stack.pop() else { return };
        if let Some(current) = self.active_melody(project).cloned() {
            self.undo_stack.push(current);
        }
        let id = next.id;
        if let Some(slot) = project.melodies.iter_mut().find(|m| m.id == id) {
            *slot = next;
        }
        self.active_melody_id = Some(id);
        self.selection.clear();
        self.mark_dirty();
    }

    /// Space / the toolbar Play-Pause button: starts preview playback of
    /// the active section from wherever the playhead currently sits, or
    /// pauses and snaps the transport and the piano roll's scroll position
    /// back to where Play was last pressed — mirrors
    /// `RakunatorApp::start_playback`/`pause_playback`. Clicking the ruler,
    /// or nudging with the arrow keys, moves that resting spot (see
    /// `piano_roll::{nudge_playhead, jump_playhead}` and the ruler's
    /// click-to-seek) the same way seeking the main timeline does.
    fn toggle_playback(&mut self) {
        if self.preview_engine.is_playing() {
            self.preview_engine.pause();
            if let Some(pos) = self.play_start_sample.take() {
                self.preview_engine.seek(pos);
            }
            if let Some(x) = self.play_start_scroll_x.take() {
                self.scroll_x = x;
            }
        } else {
            self.play_start_sample = Some(self.preview_engine.position());
            self.play_start_scroll_x = Some(self.scroll_x);
            self.mark_dirty();
            self.preview_engine.play();
        }
    }

    /// Rebuilds the scratch project's single preview clip from the active
    /// section, if it's been marked dirty since the last rebuild.
    fn refresh_preview(&mut self, sample_rate_hz: u32, main_project: &Arc<Mutex<Project>>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let samples = {
            let project = main_project.lock().unwrap();
            let bpm = self.active_melody(&project).map(|m| m.bpm).unwrap_or(120.0);
            self.active_section(&project).map(|s| melody::render_section(s, bpm, sample_rate_hz))
        };
        let Some(samples) = samples else { return };
        let mut preview = self.preview_project.lock().unwrap();
        if let Some(track) = preview.track_mut(self.preview_track) {
            track.clips.clear();
        }
        preview.add_clip_channels(self.preview_track, "preview".to_string(), 0, samples, 2);
    }
}

/// Entry point called once per frame from `app.rs`, alongside the other
/// dialogs.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.bethoven.open {
        return;
    }
    {
        let mut project = app.project.lock().unwrap();
        app.bethoven.ensure_active(&mut project);
    }

    let mut open = true;
    let fullscreen = app.bethoven.fullscreen;
    let mut window = egui::Window::new("Bethoven")
        .open(&mut open)
        .resizable(!fullscreen)
        .frame(super::window_frame(ctx, 1, 1, 1, 1));
    window = if fullscreen {
        let screen_rect = ctx.input(|i| i.viewport_rect());
        window.fixed_rect(screen_rect).movable(false)
    } else {
        window.default_size([960.0, 620.0])
    };
    let inner = window.show(ctx, |ui| {
        toolbar::draw(ui, app);
        ui.separator();
        piano_roll::draw(ui, app);
    });
    app.bethoven.open = open;

    // A click anywhere updates whether Bethoven currently "has focus": on
    // its own window, focus turns on; anywhere else (the main timeline,
    // one of its own dialogs), focus turns off, so `app.rs` knows to route
    // that keystroke's shortcuts to the main app instead from then on.
    if let Some(inner) = &inner
        && ctx.input(|i| i.pointer.any_click())
        && let Some(click_pos) = ctx.input(|i| i.pointer.interact_pos())
    {
        app.bethoven.has_focus = inner.response.rect.contains(click_pos);
    }

    app.bethoven.refresh_preview(app.sample_rate_hz, &app.project);
    if app.bethoven.is_playing() {
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

/// Bethoven-scoped keyboard shortcuts, checked instead of the main
/// timeline's `handle_shortcuts` while this window is open (see
/// `app.rs`'s `handle_shortcuts`) — so e.g. Ctrl+D can mean "delete notes"
/// here without colliding with the main app's Ctrl+D "duplicate".
pub fn handle_shortcuts(ui: &egui::Ui, app: &mut RakunatorApp) {
    if !app.bethoven.open || ui.ctx().egui_wants_keyboard_input() {
        return;
    }

    #[allow(clippy::type_complexity)]
    let (space, delete, cut, copy, paste, alt_r, undo, redo, nudge_left, nudge_right, jump_start, jump_end) =
        ui.ctx().input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.modifiers.command && i.key_pressed(egui::Key::D)
                    || i.key_pressed(egui::Key::Delete)
                    || i.key_pressed(egui::Key::Backspace),
                i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
                i.modifiers.alt && i.key_pressed(egui::Key::R),
                i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z),
                !i.modifiers.shift && i.key_pressed(egui::Key::ArrowLeft),
                !i.modifiers.shift && i.key_pressed(egui::Key::ArrowRight),
                i.modifiers.shift && i.key_pressed(egui::Key::ArrowLeft),
                i.modifiers.shift && i.key_pressed(egui::Key::ArrowRight),
            )
        });

    if space {
        app.bethoven.toggle_playback();
    }

    if nudge_left || nudge_right {
        let project_arc = Arc::clone(&app.project);
        let project = project_arc.lock().unwrap();
        piano_roll::nudge_playhead(app, &project, nudge_right);
    } else if jump_start || jump_end {
        let project_arc = Arc::clone(&app.project);
        let project = project_arc.lock().unwrap();
        piano_roll::jump_playhead(app, &project, jump_end);
    }

    if undo || redo {
        let mut project = app.project.lock().unwrap();
        if undo {
            app.bethoven.undo(&mut project);
        } else {
            app.bethoven.redo(&mut project);
        }
        return;
    }

    let selected: Vec<u32> = app.bethoven.selection.iter().copied().collect();
    if selected.is_empty() && !paste {
        return;
    }

    let mut project = app.project.lock().unwrap();

    if (delete || cut) && !selected.is_empty() {
        if cut && let Some(section) = app.bethoven.active_section(&project) {
            app.bethoven.clipboard = selected.iter().filter_map(|id| section.note(*id).cloned()).collect();
        }
        app.bethoven.record_undo(&project);
        if let Some(section) = app.bethoven.active_section_mut(&mut project) {
            section.delete_notes(&selected);
            app.bethoven.selection.clear();
            app.bethoven.mark_dirty();
        }
    } else if copy && !selected.is_empty() {
        if let Some(section) = app.bethoven.active_section(&project) {
            app.bethoven.clipboard = selected.iter().filter_map(|id| section.note(*id).cloned()).collect();
        }
    } else if paste && !app.bethoven.clipboard.is_empty() {
        let earliest = app.bethoven.clipboard.iter().map(|n| n.start_tick).min().unwrap_or(0);
        let notes = app.bethoven.clipboard.clone();
        app.bethoven.record_undo(&project);
        if let Some(section) = app.bethoven.active_section_mut(&mut project) {
            let mut new_selection = HashSet::new();
            for note in notes {
                let offset = note.start_tick.saturating_sub(earliest);
                let id = section.add_note(note.pitch, offset, note.length_ticks, note.instrument);
                if let Some(n) = section.notes.iter_mut().find(|n| n.id == id) {
                    n.gain = note.gain;
                    n.pan = note.pan;
                }
                new_selection.insert(id);
            }
            app.bethoven.selection = new_selection;
            app.bethoven.mark_dirty();
        }
    } else if alt_r && !selected.is_empty() {
        app.bethoven.record_undo(&project);
        if let Some(section) = app.bethoven.active_section_mut(&mut project) {
            section.reset_gain_pan(&selected);
            app.bethoven.mark_dirty();
        }
    }
}
