//! The piano-roll grid: a ruler (click-to-seek, Ctrl+scroll zoom) above
//! pitch rows (filtered to the active section's scale), tick-based
//! horizontal placement, and all the mouse/keyboard note editing
//! (add/select/move/resize/marquee/gain/pan), including snapping notes to
//! each other's edges the same way the main timeline snaps clips.

use super::RakunatorApp;
use crate::bethoven::melody::{self, GRID_TICKS, PPQ};
use crate::bethoven::scales;
use crate::bethoven::Instrument;
use crate::project::Project;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use std::collections::HashSet;
use std::time::{Duration, Instant};

const ROW_HEIGHT: f32 = 20.0;
const KEY_COL_WIDTH: f32 = 52.0;
const MIN_MIDI: u8 = 36; // C2
const MAX_MIDI: u8 = 96; // C7
/// Matches the main timeline's own clip-edge grab zone (`timeline::EDGE_GRAB_PX`)
/// — a narrower zone here made it easy to miss the edge and start a Move
/// instead of a resize, especially on longer notes.
const RESIZE_ZONE_PX: f32 = 12.0;
const GAIN_STEP: f32 = 0.05;
const PAN_STEP: i32 = 5;
const BAR_TICKS: u32 = melody::BEATS_PER_BAR * PPQ;
const PIANO_RULER_HEIGHT: f32 = 20.0;
const MIN_PX_PER_TICK: f32 = 0.02;
const MAX_PX_PER_TICK: f32 = 4.0;
/// How close (in pixels) a moved/resized note edge, or a ruler click, has
/// to land to another note's edge to snap to it. Smaller than the main
/// timeline's own `SNAP_PX` (8.0) — notes sit much closer together than
/// clips typically do, so the same radius made snapping feel oppressive,
/// fighting small deliberate moves near (but not exactly at) another note.
const SNAP_PX: f32 = 4.0;
/// How long the yellow alignment line stays visible after snapping —
/// mirrors the main timeline's `CLICK_SNAP_FLASH`.
const SNAP_FLASH: Duration = Duration::from_millis(350);
/// How far Left/Right nudge the playhead per key press, in screen pixels at
/// the current zoom — mirrors the main timeline's `NUDGE_PIXELS`.
const NUDGE_PIXELS: f32 = 4.0;
/// Bottom horizontal scrollbar — same sizing/behavior as the main
/// timeline's (`timeline::SCROLLBAR_HEIGHT` etc.), just working in ticks
/// instead of samples.
const SCROLLBAR_HEIGHT: f32 = 16.0;
const SCROLLBAR_MIN_THUMB_PX: f32 = 24.0;
const SCROLLBAR_TAIL_FRACTION: f32 = 0.75;

pub(super) enum Drag {
    Move { ids: Vec<u32>, accum_ticks: f32, accum_pitch: f32 },
    ResizeRight { id: u32, accum_ticks: f32 },
    ResizeLeft { id: u32, accum_ticks: f32 },
    Marquee { anchor: Pos2 },
}

pub(super) fn instrument_color(instrument: Instrument) -> Color32 {
    match instrument {
        Instrument::Piano => Color32::from_rgb(120, 170, 255),
        Instrument::Strings => Color32::from_rgb(180, 120, 235),
        Instrument::Bass => Color32::from_rgb(255, 140, 90),
        Instrument::Guitar => Color32::from_rgb(230, 190, 60),
        Instrument::Synth => Color32::from_rgb(80, 220, 180),
        Instrument::Drum => Color32::from_rgb(230, 80, 80),
        Instrument::Snare => Color32::from_rgb(230, 130, 160),
        Instrument::HiHat => Color32::from_rgb(210, 210, 210),
        Instrument::Lead => Color32::from_rgb(255, 210, 90),
        Instrument::Cowbell => Color32::from_rgb(160, 130, 90),
        Instrument::Bass808 => Color32::from_rgb(140, 90, 220),
        Instrument::Sub => Color32::from_rgb(70, 110, 200),
        Instrument::Violin => Color32::from_rgb(178, 90, 50),
    }
}

/// Picks a labeled-tick spacing (in seconds) so roughly `TARGET_TICKS`
/// labels fit across the visible ruler width — same step table as the main
/// timeline's own ruler (`timeline::pick_tick_step`, private to that
/// module, so duplicated here rather than exposed just for this).
fn pick_tick_step(seconds_visible: f32) -> f32 {
    const STEPS: [f32; 10] = [0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0];
    const TARGET_TICKS: f32 = 10.0;
    for &step in &STEPS {
        if seconds_visible / step <= TARGET_TICKS {
            return step;
        }
    }
    600.0
}

/// Snaps `candidate` to the nearest value in `targets` within `SNAP_PX`
/// pixels at the current zoom, or returns it unchanged if none are close —
/// same approach as the main timeline's `snap_sample`.
fn snap_tick(candidate: i64, targets: &[i64], px_per_tick: f32) -> i64 {
    let threshold = ((SNAP_PX / px_per_tick).max(1.0)) as i64;
    targets
        .iter()
        .copied()
        .filter(|&t| (t - candidate).abs() <= threshold)
        .min_by_key(|&t| (t - candidate).abs())
        .unwrap_or(candidate)
}

fn nudge_ticks(px_per_tick: f32) -> i64 {
    ((NUDGE_PIXELS / px_per_tick).round() as i64).max(1)
}

/// Left/Right (no Shift): nudges the preview playhead by one `nudge_ticks`
/// step, stopping exactly at the nearest note edge in the active section
/// instead of stepping past it — mirrors `app.rs`'s playhead/clip nudge.
pub(super) fn nudge_playhead(app: &mut RakunatorApp, project: &Project, forward: bool) {
    let bpm = app.bethoven.active_melody(project).map(|m| m.bpm).unwrap_or(120.0);
    let Some(section) = app.bethoven.active_section(project) else { return };
    let current_tick = melody::samples_to_ticks(app.bethoven.preview_engine.position(), bpm, app.sample_rate_hz) as i64;
    let step = nudge_ticks(app.bethoven.px_per_tick);
    let naive = (current_tick + if forward { step } else { -step }).max(0);
    let edges: Vec<i64> = section.notes.iter().flat_map(|n| [n.start_tick as i64, n.end_tick() as i64]).collect();
    let blocking_edge = if forward {
        edges.iter().copied().filter(|&e| e > current_tick && e <= naive).min()
    } else {
        edges.iter().copied().filter(|&e| e < current_tick && e >= naive).max()
    };
    let new_tick = blocking_edge.unwrap_or(naive).max(0);
    app.bethoven.preview_engine.seek(melody::ticks_to_samples(new_tick as u32, bpm, app.sample_rate_hz));
    if let Some(edge) = blocking_edge {
        app.bethoven.snap_flash = Some((edge as u32, Instant::now()));
    }
}

/// Shift+Left/Right: jumps the preview playhead straight to the very start
/// or the end of the active section.
pub(super) fn jump_playhead(app: &mut RakunatorApp, project: &Project, to_end: bool) {
    let bpm = app.bethoven.active_melody(project).map(|m| m.bpm).unwrap_or(120.0);
    let tick = if to_end { app.bethoven.active_section(project).map(|s| s.length_ticks).unwrap_or(0) } else { 0 };
    app.bethoven.preview_engine.seek(melody::ticks_to_samples(tick, bpm, app.sample_rate_hz));
}

pub(super) fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    let mut project = app.project.lock().unwrap();

    let Some((root, scale_index, section_length_ticks)) =
        app.bethoven.active_section(&project).map(|s| (s.root, s.scale_index, s.length_ticks))
    else {
        ui.label("No section yet \u{2014} use \"+ Section\" above to create one.");
        return;
    };
    let bpm = app.bethoven.active_melody(&project).map(|m| m.bpm).unwrap_or(120.0);
    let rows: Vec<u8> = (MIN_MIDI..=MAX_MIDI).rev().filter(|&m| scales::is_in_scale(root, scale_index, m)).collect();
    let notes = app.bethoven.active_section(&project).map(|s| s.notes.clone()).unwrap_or_default();
    let edge_targets: Vec<i64> = notes.iter().flat_map(|n| [n.start_tick as i64, n.end_tick() as i64]).collect();
    let content_end_ticks =
        section_length_ticks.max(notes.iter().map(|n| n.end_tick()).max().unwrap_or(0)) as f32;

    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ui.available_height()), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let ruler_rect = Rect::from_min_size(rect.min, egui::vec2(rect.width(), PIANO_RULER_HEIGHT));
    let scrollbar_rect = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() - SCROLLBAR_HEIGHT),
        egui::vec2(rect.width(), SCROLLBAR_HEIGHT),
    );
    let grid_rect =
        Rect::from_min_max(egui::pos2(rect.left(), rect.top() + PIANO_RULER_HEIGHT), egui::pos2(rect.right(), scrollbar_rect.top()));
    let ruler_response = ui.interact(ruler_rect, ui.id().with("bethoven_ruler"), Sense::click());
    let grid_response = ui.interact(grid_rect, ui.id().with("bethoven_grid"), Sense::click_and_drag());

    let grid_left = grid_rect.left() + KEY_COL_WIDTH;
    let pointer_pos = ui.ctx().pointer_hover_pos();
    // Geometric containment alone isn't enough: an open popup (e.g. the
    // scale/root combo boxes' dropdown lists in the toolbar above) can
    // render on top of these same screen coordinates on a different layer,
    // and without this check its wheel scrolling would also fall through
    // and scroll the note rows underneath it at the same time.
    let pointer_layer_matches = pointer_pos.is_some_and(|p| ui.ctx().layer_id_at(p) == Some(ui.layer_id()));
    let mouse_over_grid = pointer_layer_matches && pointer_pos.is_some_and(|p| grid_rect.contains(p));
    let mouse_over_ruler = pointer_layer_matches && pointer_pos.is_some_and(|p| ruler_rect.contains(p));
    let ctrl = ui.input(|i| i.modifiers.command);
    let shift = ui.input(|i| i.modifiers.shift);
    let notches = if mouse_over_grid || mouse_over_ruler { crate::gui::wheel_notches(ui) } else { 0.0 };
    let mut notches_consumed = false;

    // Ctrl+scroll zooms the piano roll, keeping the tick under the pointer
    // fixed — same technique as the main timeline's Ctrl+scroll zoom.
    if (mouse_over_grid || mouse_over_ruler) && ctrl && notches != 0.0 {
        notches_consumed = true;
        if let Some(p) = pointer_pos {
            let old_px = app.bethoven.px_per_tick;
            let tick_at_pointer = (p.x - grid_left) / old_px + app.bethoven.scroll_x;
            let zoom = if notches > 0.0 { 1.15 } else { 1.0 / 1.15 };
            let new_px = (old_px * zoom).clamp(MIN_PX_PER_TICK, MAX_PX_PER_TICK);
            app.bethoven.px_per_tick = new_px;
            app.bethoven.scroll_x = (tick_at_pointer - (p.x - grid_left) / new_px).max(0.0);
        }
    }

    let scroll_x = app.bethoven.scroll_x;
    let scroll_y = app.bethoven.scroll_y;
    let px_per_tick = app.bethoven.px_per_tick;
    let x_for_tick = |tick: i64| -> f32 { grid_left + (tick as f32 - scroll_x) * px_per_tick };
    let tick_for_x = |x: f32| -> f32 { (x - grid_left) / px_per_tick + scroll_x };
    let y_for_row = |row: usize| -> f32 { grid_rect.top() + row as f32 * ROW_HEIGHT - scroll_y };

    let full_painter = ui.painter_at(rect);
    let ruler_painter = ui.painter_at(ruler_rect);
    let painter = ui.painter_at(grid_rect);
    painter.rect_filled(grid_rect, 0.0, ui.visuals().extreme_bg_color);
    ruler_painter.rect_filled(ruler_rect, 0.0, ui.visuals().faint_bg_color);

    // Ruler click-to-seek: like the main timeline's ruler, snaps to a
    // nearby note edge (Audacity-style) rather than the placement grid.
    if ruler_response.clicked()
        && let Some(pos) = ruler_response.interact_pointer_pos()
    {
        let raw_tick = tick_for_x(pos.x).max(0.0) as i64;
        let snapped = snap_tick(raw_tick, &edge_targets, px_per_tick).max(0);
        app.bethoven.preview_engine.seek(melody::ticks_to_samples(snapped as u32, bpm, app.sample_rate_hz));
        if snapped != raw_tick {
            app.bethoven.snap_flash = Some((snapped as u32, Instant::now()));
        }
    }

    // Bar grid lines, in the grid area only.
    let first_bar = (scroll_x as u32 / BAR_TICKS) * BAR_TICKS;
    let mut bar_tick = first_bar;
    loop {
        let x = x_for_tick(bar_tick as i64);
        if x > grid_rect.right() {
            break;
        }
        if x >= grid_left {
            let bold = (bar_tick / BAR_TICKS).is_multiple_of(4);
            painter.line_segment(
                [egui::pos2(x, grid_rect.top()), egui::pos2(x, grid_rect.bottom())],
                Stroke::new(if bold { 1.5 } else { 1.0 }, Color32::from_white_alpha(if bold { 30 } else { 14 })),
            );
        }
        bar_tick += BAR_TICKS;
    }

    // Ruler ticks/labels: time-based (seconds/minutes) and density-adaptive
    // to the current zoom, exactly like the main timeline's own ruler — so
    // there's always a readable spread of labels across the visible range
    // (a fixed one-per-bar spacing could get too dense or too sparse), and
    // each label sits centered under its own tick line rather than beside it.
    let ticks_per_second = PPQ as f32 * bpm / 60.0;
    if ticks_per_second > 0.0 {
        let seconds_visible = (grid_rect.width() / px_per_tick) / ticks_per_second;
        let step_seconds = pick_tick_step(seconds_visible);
        let step_ticks = (step_seconds * ticks_per_second).round().max(1.0) as u32;
        let first_tick = (scroll_x.max(0.0) as u32 / step_ticks) * step_ticks;
        let mut tick = first_tick;
        loop {
            let x = x_for_tick(tick as i64);
            if x > grid_rect.right() {
                break;
            }
            if x >= grid_left {
                ruler_painter.line_segment(
                    [egui::pos2(x, ruler_rect.bottom() - 6.0), egui::pos2(x, ruler_rect.bottom())],
                    Stroke::new(1.0, Color32::GRAY),
                );
                let secs = tick as f32 / ticks_per_second;
                ruler_painter.text(
                    egui::pos2(x, ruler_rect.top() + 1.0),
                    Align2::CENTER_TOP,
                    crate::gui::timeline::format_time(secs),
                    FontId::proportional(10.0),
                    ui.visuals().text_color(),
                );
            }
            tick += step_ticks;
        }
    }

    // Row backgrounds + piano-key gutter.
    for (row, &midi) in rows.iter().enumerate() {
        let y = y_for_row(row);
        if y + ROW_HEIGHT < grid_rect.top() || y > grid_rect.bottom() {
            continue;
        }
        let row_rect = Rect::from_min_size(egui::pos2(grid_rect.left(), y), egui::vec2(grid_rect.width(), ROW_HEIGHT));
        // Color-coded like a real keyboard: white-key rows get a faint lift
        // off the base background, black-key rows stay bare, and every C
        // (an octave boundary) gets a stronger highlight on top of that.
        let is_black_key = matches!(midi % 12, 1 | 3 | 6 | 8 | 10);
        if !is_black_key {
            painter.rect_filled(row_rect, 0.0, Color32::from_white_alpha(12));
        }
        if midi % 12 == 0 {
            painter.rect_filled(row_rect, 0.0, Color32::from_white_alpha(26));
        }
        painter.line_segment(
            [egui::pos2(grid_rect.left(), y + ROW_HEIGHT), egui::pos2(grid_rect.right(), y + ROW_HEIGHT)],
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.5)),
        );
        let key_rect =
            Rect::from_min_size(egui::pos2(grid_rect.left(), y), egui::vec2(KEY_COL_WIDTH, ROW_HEIGHT));
        let (key_color, key_text_color) = if is_black_key {
            (Color32::from_rgb(32, 32, 38), Color32::from_rgb(215, 215, 220))
        } else {
            (Color32::from_rgb(205, 205, 210), Color32::from_rgb(25, 25, 28))
        };
        painter.rect_filled(key_rect, 0.0, key_color);
        painter.text(
            egui::pos2(key_rect.left() + 7.0, key_rect.center().y),
            Align2::LEFT_CENTER,
            scales::note_name(midi),
            FontId::monospace(12.0),
            key_text_color,
        );
    }

    // Notes (drawn/interacted after the background so they take priority
    // for clicks/drags at the same screen position).
    let mut note_rects: Vec<(u32, Rect)> = Vec::new();
    let mut snap_hit: Option<i64> = None;

    for note in &notes {
        let Some(row) = rows.iter().position(|&m| m == note.pitch) else { continue };
        let y = y_for_row(row);
        let x0 = x_for_tick(note.start_tick as i64);
        let x1 = x_for_tick(note.end_tick() as i64);
        if x1 < grid_left || x0 > grid_rect.right() || y + ROW_HEIGHT < grid_rect.top() || y > grid_rect.bottom() {
            continue;
        }
        let note_rect = Rect::from_min_max(egui::pos2(x0.max(grid_left), y), egui::pos2(x1, y + ROW_HEIGHT));
        let id_source = ui.id().with(("bethoven_note", note.id));
        let resp = ui.interact(note_rect, id_source, Sense::click_and_drag());
        note_rects.push((note.id, note_rect));

        let selected = app.bethoven.selection.contains(&note.id);
        let base = instrument_color(note.instrument);
        let alpha = (40.0 + note.gain.clamp(0.0, 1.0) * 215.0).round() as u8;
        let color = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), alpha);
        painter.rect_filled(note_rect, 3.0, color);
        painter.rect_stroke(
            note_rect,
            3.0,
            Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { Color32::WHITE } else { base }),
            egui::StrokeKind::Middle,
        );
        // Pan visualization: a small marker sliding across the note's
        // width from its left edge (full left) to its right edge (full
        // right) — hidden at pan 0 (dead center, the common case) so it
        // doesn't clutter every unpanned note, and skipped on notes too
        // narrow to show it meaningfully.
        if note.pan != 0 && note_rect.width() > 10.0 {
            let pan_frac = (note.pan as f32 + 100.0) / 200.0;
            let marker = egui::pos2(note_rect.left() + note_rect.width() * pan_frac, note_rect.top() + 3.0);
            painter.circle_filled(marker, 2.5, Color32::WHITE);
            painter.circle_stroke(marker, 2.5, Stroke::new(0.6, Color32::BLACK));
        }

        if !ctrl && resp.hovered() && selected && notches != 0.0 && !notches_consumed {
            notches_consumed = true;
            app.bethoven.record_undo(&project);
            let ids: Vec<u32> = app.bethoven.selection.iter().copied().collect();
            if let Some(section) = app.bethoven.active_section_mut(&mut project) {
                if shift {
                    // Wheel-up (positive notches) pans left, wheel-down pans right.
                    section.adjust_pan(&ids, -(notches.signum() as i32) * PAN_STEP);
                } else {
                    section.adjust_gain(&ids, notches.signum() * GAIN_STEP);
                }
            }
            app.bethoven.mark_dirty();
        }

        if resp.clicked() && !resp.dragged() {
            if ui.input(|i| i.modifiers.shift) {
                if !app.bethoven.selection.remove(&note.id) {
                    app.bethoven.selection.insert(note.id);
                }
            } else {
                app.bethoven.selection.clear();
                app.bethoven.selection.insert(note.id);
            }
        }

        if resp.secondary_clicked() {
            let ids: Vec<u32> = if app.bethoven.selection.contains(&note.id) {
                app.bethoven.selection.iter().copied().collect()
            } else {
                vec![note.id]
            };
            app.bethoven.record_undo(&project);
            if let Some(section) = app.bethoven.active_section_mut(&mut project) {
                section.delete_notes(&ids);
            }
            app.bethoven.selection.retain(|id| !ids.contains(id));
            app.bethoven.mark_dirty();
        }

        // A note's true right edge can scroll off the visible grid once
        // it's long enough — clamping the hit-test zone to whichever is
        // reachable (the real edge, or the grid's own right boundary) means
        // it's always grabbable instead of silently falling back to "move"
        // once you can no longer click within `RESIZE_ZONE_PX` of an edge
        // that isn't actually on screen anymore.
        let right_zone_edge = note_rect.right().min(grid_rect.right());
        // On a short note the two edge zones can otherwise cover the whole
        // note, leaving no "body" left to grab for a plain move at all —
        // shrink them so at least the middle third always stays movable.
        let zone_px = RESIZE_ZONE_PX.min(note_rect.width() / 3.0);
        let in_right_zone = |p: egui::Pos2| p.x >= right_zone_edge - zone_px;
        let in_left_zone = |p: egui::Pos2| p.x <= note_rect.left() + zone_px;

        // Cursor feedback for what a drag starting here would do — a
        // resize cursor over either edge, a grab/grabbing hand over the
        // body, so the resize zones read as interactive before you commit
        // to a drag.
        if resp.dragged() {
            let icon = match app.bethoven.drag {
                Some(Drag::ResizeLeft { .. }) | Some(Drag::ResizeRight { .. }) => egui::CursorIcon::ResizeHorizontal,
                _ => egui::CursorIcon::Grabbing,
            };
            ui.ctx().set_cursor_icon(icon);
        } else if resp.hovered() {
            let icon = if pointer_pos.is_some_and(|p| in_left_zone(p) || in_right_zone(p)) {
                egui::CursorIcon::ResizeHorizontal
            } else {
                egui::CursorIcon::Grab
            };
            ui.ctx().set_cursor_icon(icon);
        }

        if resp.drag_started() {
            if !app.bethoven.selection.contains(&note.id) {
                app.bethoven.selection.clear();
                app.bethoven.selection.insert(note.id);
            }
            app.bethoven.record_undo(&project);
            // The *press* position, not wherever the pointer has drifted to
            // by the time egui decides this is actually a drag (it only
            // commits past a small movement threshold) — on a short note
            // those few pixels of drift could already be enough to land in
            // a different zone than the one you actually pressed down in,
            // flipping move/resize right at the start of the gesture.
            let start_pos = ui.ctx().input(|i| i.pointer.press_origin());
            app.bethoven.drag = Some(if start_pos.is_some_and(in_right_zone) {
                Drag::ResizeRight { id: note.id, accum_ticks: 0.0 }
            } else if start_pos.is_some_and(in_left_zone) {
                Drag::ResizeLeft { id: note.id, accum_ticks: 0.0 }
            } else {
                Drag::Move { ids: app.bethoven.selection.iter().copied().collect(), accum_ticks: 0.0, accum_pitch: 0.0 }
            });
        }

        if resp.dragged() {
            let delta = resp.drag_delta();
            let mut move_apply: Option<(i64, i32, Vec<u32>)> = None;
            let mut resize_apply: Option<(u32, u32, u32)> = None;
            if let Some(drag) = app.bethoven.drag.as_mut() {
                match drag {
                    Drag::Move { accum_ticks, accum_pitch, ids } => {
                        *accum_ticks += delta.x / px_per_tick;
                        *accum_pitch += -delta.y / ROW_HEIGHT;
                        let step = GRID_TICKS as f32;
                        let steps = (*accum_ticks / step).round();
                        let pitch_steps = accum_pitch.round();
                        if steps != 0.0 || pitch_steps != 0.0 {
                            *accum_ticks -= steps * step;
                            *accum_pitch -= pitch_steps;
                            let mut dt = (steps * step) as i64;
                            // Snap the moving note's leading edge first,
                            // then its trailing edge, to any other note's
                            // edge — same two-step approach the main
                            // timeline uses for moving a clip.
                            let candidate_start = note.start_tick as i64 + dt;
                            let snapped_start = snap_tick(candidate_start, &edge_targets, px_per_tick);
                            if snapped_start != candidate_start {
                                dt += snapped_start - candidate_start;
                                snap_hit = Some(snapped_start);
                            } else {
                                let candidate_end = candidate_start + note.length_ticks as i64;
                                let snapped_end = snap_tick(candidate_end, &edge_targets, px_per_tick);
                                if snapped_end != candidate_end {
                                    dt += snapped_end - candidate_end;
                                    snap_hit = Some(snapped_end);
                                }
                            }
                            move_apply = Some((dt, pitch_steps as i32, ids.clone()));
                        }
                    }
                    Drag::ResizeRight { id, accum_ticks } => {
                        *accum_ticks += delta.x / px_per_tick;
                        let step = GRID_TICKS as f32;
                        let steps = (*accum_ticks / step).round();
                        if steps != 0.0 {
                            *accum_ticks -= steps * step;
                            let mut dt = steps as i64 * GRID_TICKS as i64;
                            let candidate_end = note.start_tick as i64 + note.length_ticks as i64 + dt;
                            let snapped_end = snap_tick(candidate_end, &edge_targets, px_per_tick);
                            if snapped_end != candidate_end {
                                dt += snapped_end - candidate_end;
                                snap_hit = Some(snapped_end);
                            }
                            let new_len = (note.length_ticks as i64 + dt).max(GRID_TICKS as i64) as u32;
                            resize_apply = Some((*id, note.start_tick, new_len));
                        }
                    }
                    Drag::ResizeLeft { id, accum_ticks } => {
                        *accum_ticks += delta.x / px_per_tick;
                        let step = GRID_TICKS as f32;
                        let steps = (*accum_ticks / step).round();
                        if steps != 0.0 {
                            *accum_ticks -= steps * step;
                            let mut dt = steps as i64 * GRID_TICKS as i64;
                            let candidate_start = note.start_tick as i64 + dt;
                            let snapped_start = snap_tick(candidate_start, &edge_targets, px_per_tick);
                            if snapped_start != candidate_start {
                                dt += snapped_start - candidate_start;
                                snap_hit = Some(snapped_start);
                            }
                            let new_start = (note.start_tick as i64 + dt).max(0) as u32;
                            let new_len = (note.length_ticks as i64 - dt).max(GRID_TICKS as i64) as u32;
                            resize_apply = Some((*id, new_start, new_len));
                        }
                    }
                    Drag::Marquee { .. } => {}
                }
            }
            if let Some((dt, dp, ids)) = move_apply {
                if let Some(section) = app.bethoven.active_section_mut(&mut project) {
                    section.move_notes_by_row(&ids, dt, dp, &rows);
                }
                app.bethoven.mark_dirty();
            }
            if let Some((id, new_start, new_len)) = resize_apply {
                if let Some(section) = app.bethoven.active_section_mut(&mut project) {
                    section.resize_note(id, new_start, new_len);
                }
                app.bethoven.mark_dirty();
            }
        }

        if resp.drag_stopped() {
            app.bethoven.drag = None;
        }
    }

    // Whichever single note is currently selected is the "last touched"
    // one — its length/instrument become the defaults for the next note
    // you place, live, with no separate shortcut needed. Ambiguous with 0
    // or 2+ notes selected, so defaults just stay whatever they were last
    // set to in that case.
    if let &[only_id] = app.bethoven.selection.iter().copied().collect::<Vec<u32>>().as_slice()
        && let Some(note) = notes.iter().find(|n| n.id == only_id)
        && let Some(melody) = app.bethoven.active_melody_mut(&mut project)
        && (melody.default_note_length_ticks != note.length_ticks || melody.default_instrument != note.instrument)
    {
        melody.default_note_length_ticks = note.length_ticks;
        melody.default_instrument = note.instrument;
    }

    if let Some(tick) = snap_hit {
        app.bethoven.snap_flash = Some((tick.max(0) as u32, Instant::now()));
    }

    // Background click: with a selection active, the first click on empty
    // space only clears it (so a stray click while notes are selected can't
    // accidentally drop a new note on top of them) — a second click, now
    // with nothing selected, actually places a note.
    if !shift && grid_response.clicked() {
        if !app.bethoven.selection.is_empty() {
            app.bethoven.selection.clear();
        } else if let Some(pos) = grid_response.interact_pointer_pos()
            && pos.x >= grid_left
        {
            let tick = melody::snap_ticks(tick_for_x(pos.x) as i64);
            let row_index = ((pos.y - grid_rect.top() + scroll_y) / ROW_HEIGHT).floor();
            if row_index >= 0.0 && (row_index as usize) < rows.len() {
                let pitch = rows[row_index as usize];
                let (default_len, default_inst) = app
                    .bethoven
                    .active_melody(&project)
                    .map(|m| (m.default_note_length_ticks, m.default_instrument))
                    .unwrap_or((PPQ, Instrument::Piano));
                app.bethoven.record_undo(&project);
                if let Some(section) = app.bethoven.active_section_mut(&mut project) {
                    let id = section.add_note(pitch, tick, default_len, default_inst);
                    app.bethoven.selection = std::iter::once(id).collect();
                }
                app.bethoven.mark_dirty();
            }
        }
    }

    if shift
        && grid_response.drag_started()
        && let Some(pos) = grid_response.interact_pointer_pos()
    {
        app.bethoven.drag = Some(Drag::Marquee { anchor: pos });
    }
    let marquee_anchor = match &app.bethoven.drag {
        Some(Drag::Marquee { anchor }) => Some(*anchor),
        _ => None,
    };
    if let Some(anchor) = marquee_anchor {
        if grid_response.dragged()
            && let Some(cur) = pointer_pos
        {
            let marquee_rect = Rect::from_two_pos(anchor, cur);
            painter.rect_filled(marquee_rect, 0.0, Color32::from_rgba_unmultiplied(120, 170, 255, 40));
            painter.rect_stroke(
                marquee_rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(120, 170, 255)),
                egui::StrokeKind::Middle,
            );
        }
        if grid_response.drag_stopped() {
            if let Some(cur) = pointer_pos {
                let marquee_rect = Rect::from_two_pos(anchor, cur);
                let newly: HashSet<u32> =
                    note_rects.iter().filter(|(_, r)| marquee_rect.intersects(*r)).map(|(id, _)| *id).collect();
                app.bethoven.selection = newly;
            }
            app.bethoven.drag = None;
        }
    }

    if !ctrl && !notches_consumed && mouse_over_grid && notches != 0.0 {
        if shift {
            app.bethoven.scroll_x = (app.bethoven.scroll_x - notches * GRID_TICKS as f32 * 4.0).max(0.0);
        } else {
            app.bethoven.scroll_y = (app.bethoven.scroll_y - notches * ROW_HEIGHT * 3.0).max(0.0);
        }
    }

    // Nothing below this point needs the project lock, and `draw_scrollbar`
    // needs `app` back by unique reference.
    drop(project);
    draw_scrollbar(ui, app, scrollbar_rect, content_end_ticks);

    // Playhead (spans the ruler and the grid).
    if app.bethoven.is_playing() {
        let position_samples = app.bethoven.preview_engine.position();
        let tick = melody::samples_to_ticks(position_samples, bpm, app.sample_rate_hz);
        let x = x_for_tick(tick as i64);
        if x >= grid_left && x <= rect.right() {
            full_painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(2.0, Color32::from_rgb(230, 200, 40)),
            );
        }
    }

    // Snap alignment line — live while a drag is actively snapped, and for
    // a brief moment afterward (also covers ruler-click and keyboard-nudge
    // snaps, which have no "while dragging" span of their own).
    if let Some((tick, at)) = app.bethoven.snap_flash
        && at.elapsed() < SNAP_FLASH
    {
        let x = x_for_tick(tick as i64);
        if x >= grid_left && x <= rect.right() {
            full_painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(2.0, Color32::from_rgb(230, 200, 40)),
            );
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

/// Bottom horizontal scrollbar for the piano roll — click/drag to scroll
/// through the section, same behavior as the main timeline's
/// `timeline::draw_horizontal_scrollbar`, just in ticks instead of samples.
fn draw_scrollbar(ui: &mut egui::Ui, app: &mut RakunatorApp, rect: Rect, content_end_ticks: f32) {
    let response = ui.interact(rect, ui.id().with("bethoven_scrollbar"), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().faint_bg_color);

    let bar_w = rect.width();
    let px_per_tick = app.bethoven.px_per_tick;
    let visible_ticks = (bar_w / px_per_tick).max(1.0);
    let total_ticks = (content_end_ticks + visible_ticks * SCROLLBAR_TAIL_FRACTION).max(visible_ticks);
    let max_scroll = (total_ticks - visible_ticks).max(0.0);

    let thumb_w = (bar_w * visible_ticks / total_ticks).clamp(SCROLLBAR_MIN_THUMB_PX, bar_w);
    let track_w = (bar_w - thumb_w).max(0.0);

    if response.dragged() && track_w > 0.0 && max_scroll > 0.0 {
        let delta_scroll = response.drag_delta().x / track_w * max_scroll;
        app.bethoven.scroll_x = (app.bethoven.scroll_x + delta_scroll).clamp(0.0, max_scroll);
    }

    let thumb_x = rect.left()
        + if max_scroll > 0.0 { (app.bethoven.scroll_x.clamp(0.0, max_scroll) / max_scroll) * track_w } else { 0.0 };
    let thumb_rect = Rect::from_min_size(egui::pos2(thumb_x, rect.top() + 2.0), egui::vec2(thumb_w, rect.height() - 4.0));
    let thumb_color =
        if response.dragged() { ui.visuals().widgets.active.bg_fill } else { ui.visuals().widgets.inactive.bg_fill };
    painter.rect_filled(thumb_rect, 3.0, thumb_color);
}
