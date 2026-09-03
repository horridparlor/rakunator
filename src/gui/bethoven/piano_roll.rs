//! The piano-roll grid: pitch rows (filtered to the active section's
//! scale), tick-based horizontal placement, and all the mouse/keyboard note
//! editing (add/select/move/resize/marquee/gain/pan).

use super::RakunatorApp;
use crate::bethoven::melody::{self, GRID_TICKS, PPQ};
use crate::bethoven::scales;
use crate::bethoven::Instrument;
use egui::{Color32, Pos2, Rect, Sense, Stroke};
use std::collections::HashSet;

const ROW_HEIGHT: f32 = 16.0;
const KEY_COL_WIDTH: f32 = 46.0;
const MIN_MIDI: u8 = 36; // C2
const MAX_MIDI: u8 = 96; // C7
const RESIZE_ZONE_PX: f32 = 6.0;
const GAIN_STEP: f32 = 0.05;
const PAN_STEP: i32 = 5;
const BAR_TICKS: u32 = melody::BEATS_PER_BAR * PPQ;

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
    }
}

pub(super) fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    let mut project = app.project.lock().unwrap();

    let Some((root, scale_index)) = app.bethoven.active_section(&project).map(|s| (s.root, s.scale_index)) else {
        ui.label("No section yet \u{2014} use \"+ Section\" above to create one.");
        return;
    };

    let rows: Vec<u8> = (MIN_MIDI..=MAX_MIDI).rev().filter(|&m| scales::is_in_scale(root, scale_index, m)).collect();

    let (rect, grid_response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ui.available_height()), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

    let scroll_x = app.bethoven.scroll_x;
    let scroll_y = app.bethoven.scroll_y;
    let px_per_tick = app.bethoven.px_per_tick;
    let grid_left = rect.left() + KEY_COL_WIDTH;
    let x_for_tick = |tick: i64| -> f32 { grid_left + (tick as f32 - scroll_x) * px_per_tick };
    let tick_for_x = |x: f32| -> f32 { (x - grid_left) / px_per_tick + scroll_x };
    let y_for_row = |row: usize| -> f32 { rect.top() + row as f32 * ROW_HEIGHT - scroll_y };

    // Row backgrounds + piano-key gutter.
    for (row, &midi) in rows.iter().enumerate() {
        let y = y_for_row(row);
        if y + ROW_HEIGHT < rect.top() || y > rect.bottom() {
            continue;
        }
        let row_rect = Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), ROW_HEIGHT));
        let shade = if midi % 12 == 0 { 0.06 } else { 0.0 }; // faint highlight on every C
        if shade > 0.0 {
            painter.rect_filled(row_rect, 0.0, Color32::from_white_alpha((shade * 255.0) as u8));
        }
        painter.line_segment(
            [egui::pos2(rect.left(), y + ROW_HEIGHT), egui::pos2(rect.right(), y + ROW_HEIGHT)],
            Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.5)),
        );
        let key_rect =
            Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(KEY_COL_WIDTH, ROW_HEIGHT));
        painter.rect_filled(key_rect, 0.0, ui.visuals().panel_fill);
        painter.text(
            egui::pos2(key_rect.left() + 4.0, key_rect.center().y),
            egui::Align2::LEFT_CENTER,
            scales::note_name(midi),
            egui::FontId::monospace(10.0),
            ui.visuals().text_color(),
        );
    }

    // Bar grid lines.
    let first_bar = (scroll_x as u32 / BAR_TICKS) * BAR_TICKS;
    let mut bar_tick = first_bar;
    loop {
        let x = x_for_tick(bar_tick as i64);
        if x > rect.right() {
            break;
        }
        if x >= grid_left {
            let bold = (bar_tick / BAR_TICKS).is_multiple_of(4);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(if bold { 1.5 } else { 1.0 }, Color32::from_white_alpha(if bold { 30 } else { 14 })),
            );
        }
        bar_tick += BAR_TICKS;
    }

    let pointer_pos = ui.ctx().pointer_hover_pos();
    let mouse_over_grid = pointer_pos.is_some_and(|p| rect.contains(p));
    let notches = if mouse_over_grid { crate::gui::wheel_notches(ui) } else { 0.0 };
    let shift = ui.input(|i| i.modifiers.shift);
    let mut notches_consumed = false;

    // Notes (drawn/interacted after the background so they take priority
    // for clicks/drags at the same screen position).
    let notes = app.bethoven.active_section(&project).map(|s| s.notes.clone()).unwrap_or_default();
    let mut note_rects: Vec<(u32, Rect)> = Vec::new();

    for note in &notes {
        let Some(row) = rows.iter().position(|&m| m == note.pitch) else { continue };
        let y = y_for_row(row);
        let x0 = x_for_tick(note.start_tick as i64);
        let x1 = x_for_tick(note.end_tick() as i64);
        if x1 < grid_left || x0 > rect.right() || y + ROW_HEIGHT < rect.top() || y > rect.bottom() {
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

        if resp.hovered() && selected && notches != 0.0 && !notches_consumed {
            notches_consumed = true;
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

        if resp.drag_started() {
            if !app.bethoven.selection.contains(&note.id) {
                app.bethoven.selection.clear();
                app.bethoven.selection.insert(note.id);
            }
            let in_right_zone =
                resp.interact_pointer_pos().is_some_and(|p| p.x >= note_rect.right() - RESIZE_ZONE_PX);
            let in_left_zone =
                resp.interact_pointer_pos().is_some_and(|p| p.x <= note_rect.left() + RESIZE_ZONE_PX);
            app.bethoven.drag = Some(if in_right_zone {
                Drag::ResizeRight { id: note.id, accum_ticks: 0.0 }
            } else if in_left_zone {
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
                        let steps = (*accum_ticks / step).trunc();
                        let pitch_steps = accum_pitch.trunc();
                        if steps != 0.0 || pitch_steps != 0.0 {
                            *accum_ticks -= steps * step;
                            *accum_pitch -= pitch_steps;
                            move_apply = Some(((steps * step) as i64, pitch_steps as i32, ids.clone()));
                        }
                    }
                    Drag::ResizeRight { id, accum_ticks } => {
                        *accum_ticks += delta.x / px_per_tick;
                        let step = GRID_TICKS as f32;
                        let steps = (*accum_ticks / step).trunc();
                        if steps != 0.0 {
                            *accum_ticks -= steps * step;
                            let dt = steps as i64 * GRID_TICKS as i64;
                            let new_len = (note.length_ticks as i64 + dt).max(GRID_TICKS as i64) as u32;
                            resize_apply = Some((*id, note.start_tick, new_len));
                        }
                    }
                    Drag::ResizeLeft { id, accum_ticks } => {
                        *accum_ticks += delta.x / px_per_tick;
                        let step = GRID_TICKS as f32;
                        let steps = (*accum_ticks / step).trunc();
                        if steps != 0.0 {
                            *accum_ticks -= steps * step;
                            let dt = steps as i64 * GRID_TICKS as i64;
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
                    section.move_notes(&ids, dt, dp);
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

    // Background: click-to-add, Shift+drag marquee, plain/Shift wheel to
    // pan the view when no selected note under the cursor consumed it.
    if !shift
        && grid_response.clicked()
        && let Some(pos) = grid_response.interact_pointer_pos()
        && pos.x >= grid_left
    {
        let tick = melody::snap_ticks(tick_for_x(pos.x) as i64);
        let row_index = ((pos.y - rect.top() + scroll_y) / ROW_HEIGHT).floor();
        if row_index >= 0.0 && (row_index as usize) < rows.len() {
            let pitch = rows[row_index as usize];
            let (default_len, default_inst) = app
                .bethoven
                .active_melody(&project)
                .map(|m| (m.default_note_length_ticks, m.default_instrument))
                .unwrap_or((PPQ, Instrument::Piano));
            if let Some(section) = app.bethoven.active_section_mut(&mut project) {
                let id = section.add_note(pitch, tick, default_len, default_inst);
                app.bethoven.selection = std::iter::once(id).collect();
            }
            app.bethoven.mark_dirty();
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

    if !notches_consumed && mouse_over_grid && notches != 0.0 {
        if shift {
            app.bethoven.scroll_x = (app.bethoven.scroll_x - notches * GRID_TICKS as f32 * 4.0).max(0.0);
        } else {
            app.bethoven.scroll_y = (app.bethoven.scroll_y - notches * ROW_HEIGHT * 3.0).max(0.0);
        }
    }

    // Preview playhead.
    if app.bethoven.is_playing() {
        let bpm = app.bethoven.active_melody(&project).map(|m| m.bpm).unwrap_or(120.0);
        let position_samples = app.bethoven.preview_engine.position();
        let tick = melody::samples_to_ticks(position_samples, bpm, app.sample_rate_hz);
        let x = x_for_tick(tick as i64);
        if x >= grid_left && x <= rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(2.0, Color32::from_rgb(230, 200, 40)),
            );
        }
    }
}
