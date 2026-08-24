use crate::audio_engine::AudioEngine;
use crate::project::{ClipId, Project, TrackId};
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, Vec2};

use super::ROW_HEIGHT;

/// ~480 pixels per second at a 48kHz project rate; only used as the
/// initial zoom level, not a hard assumption elsewhere.
const DEFAULT_PX_PER_SAMPLE: f32 = 480.0 / 48_000.0;

pub struct TimelineState {
    pub px_per_sample: f32,
    pub scroll_x_samples: f32,
    /// Screen-space y-coordinate of the top of the first track row this
    /// frame; used to map the pointer's y position to a track row.
    pub tracks_top_y: f32,
    /// Where cut/copy/paste should target next: last clicked track+sample.
    pub last_click: Option<(TrackId, u64)>,
    drag: Option<DragState>,
}

impl Default for TimelineState {
    fn default() -> Self {
        TimelineState {
            px_per_sample: DEFAULT_PX_PER_SAMPLE,
            scroll_x_samples: 0.0,
            tracks_top_y: 0.0,
            last_click: None,
            drag: None,
        }
    }
}

struct DragState {
    clip_id: ClipId,
    origin_track: TrackId,
    len_samples: u64,
    grab_offset_samples: i64,
    duplicate: bool,
}

struct ClipSnapshot {
    id: ClipId,
    name: String,
    start_sample: u64,
    len_samples: u64,
}

enum ClipMenuAction {
    Cut,
    Copy,
    DuplicateHere,
    DuplicateTo(TrackId),
}

/// Draws one track's timeline lane: its clips, the playhead, and handles
/// clip selection, drag-to-move/duplicate, and the cut/copy/duplicate
/// context menu. `all_track_ids` is every track in display order, used to
/// map the pointer's row during a cross-track drag.
pub fn draw_lane(
    ui: &mut egui::Ui,
    project: &mut Project,
    track_id: TrackId,
    all_track_ids: &[TrackId],
    state: &mut TimelineState,
    playhead_sample: u64,
    engine: &AudioEngine,
) {
    let size = Vec2::new(ui.available_width().max(200.0), ROW_HEIGHT);
    let (rect, lane_response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    ui.painter()
        .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

    let px_per_sample = state.px_per_sample;
    let scroll = state.scroll_x_samples;
    let x_for = |sample: u64| rect.left() + (sample as f32 - scroll) * px_per_sample;
    let sample_for_x =
        |x: f32| (((x - rect.left()) / px_per_sample) + scroll).max(0.0) as u64;

    if lane_response.clicked()
        && let Some(pos) = lane_response.interact_pointer_pos() {
            let sample = sample_for_x(pos.x);
            state.last_click = Some((track_id, sample));
            project.selection = None;
            engine.seek(sample);
        }

    // Snapshot this track's clips so the interactive loop below never holds
    // a live borrow of `project`, leaving it free to mutate on click/menu
    // actions within the same iteration.
    let clips: Vec<ClipSnapshot> = project
        .track(track_id)
        .map(|t| {
            t.clips
                .iter()
                .map(|c| ClipSnapshot {
                    id: c.id,
                    name: c.name.clone(),
                    start_sample: c.start_sample,
                    len_samples: c.len_samples(),
                })
                .collect()
        })
        .unwrap_or_default();
    let track_names: Vec<(TrackId, String)> =
        project.tracks.iter().map(|t| (t.id, t.name.clone())).collect();

    let pointer_target_track = if state.drag.is_some() {
        ui.ctx().pointer_interact_pos().and_then(|p| {
            let row = (p.y - state.tracks_top_y) / ROW_HEIGHT;
            if row < 0.0 {
                None
            } else {
                all_track_ids.get(row as usize).copied()
            }
        })
    } else {
        None
    };

    for clip in &clips {
        let being_dragged = state
            .drag
            .as_ref()
            .map(|d| d.clip_id == clip.id)
            .unwrap_or(false);

        let x = x_for(clip.start_sample);
        let w = (clip.len_samples as f32 * px_per_sample).max(2.0);
        let clip_rect = Rect::from_min_size(
            egui::pos2(x, rect.top() + 4.0),
            Vec2::new(w, ROW_HEIGHT - 8.0),
        );

        let id = egui::Id::new(("clip", clip.id.0));
        let response = ui.interact(clip_rect, id, Sense::click_and_drag());

        if response.drag_started() {
            let grab = ui
                .ctx()
                .pointer_interact_pos()
                .map(|p| ((p.x - x) / px_per_sample) as i64)
                .unwrap_or(0);
            let duplicate = ui.ctx().input(|i| i.modifiers.command || i.modifiers.alt);
            state.drag = Some(DragState {
                clip_id: clip.id,
                origin_track: track_id,
                len_samples: clip.len_samples,
                grab_offset_samples: grab,
                duplicate,
            });
        }

        if response.clicked() {
            project.selection = Some(clip.id);
            state.last_click = Some((track_id, clip.start_sample));
        }

        if response.drag_stopped() {
            let should_commit = state
                .drag
                .as_ref()
                .map(|d| d.clip_id == clip.id)
                .unwrap_or(false);
            if should_commit
                && let Some(drag) = state.drag.take() {
                    let target = pointer_target_track.unwrap_or(drag.origin_track);
                    let pointer_x = ui
                        .ctx()
                        .pointer_interact_pos()
                        .map(|p| p.x)
                        .unwrap_or(x);
                    let new_start =
                        (sample_for_x(pointer_x) as i64 - drag.grab_offset_samples).max(0) as u64;
                    if drag.duplicate {
                        project.duplicate_clip(drag.clip_id, target, new_start);
                    } else {
                        project.move_clip(drag.clip_id, target, new_start);
                    }
                }
        }

        let selected = project.selection == Some(clip.id);
        draw_clip_rect(ui, clip_rect, &clip.name, being_dragged, selected);

        let mut menu_action: Option<ClipMenuAction> = None;
        response.context_menu(|ui| {
            if ui.button("Cut").clicked() {
                menu_action = Some(ClipMenuAction::Cut);
                ui.close();
            }
            if ui.button("Copy").clicked() {
                menu_action = Some(ClipMenuAction::Copy);
                ui.close();
            }
            if ui.button("Duplicate here").clicked() {
                menu_action = Some(ClipMenuAction::DuplicateHere);
                ui.close();
            }
            ui.menu_button("Duplicate to track", |ui| {
                for (id, name) in &track_names {
                    if ui.button(name).clicked() {
                        menu_action = Some(ClipMenuAction::DuplicateTo(*id));
                        ui.close();
                    }
                }
            });
        });

        match menu_action {
            Some(ClipMenuAction::Cut) => project.cut_clip(clip.id),
            Some(ClipMenuAction::Copy) => project.copy_clip(clip.id),
            Some(ClipMenuAction::DuplicateHere) => {
                let new_start = clip.start_sample + clip.len_samples;
                project.duplicate_clip(clip.id, track_id, new_start);
            }
            Some(ClipMenuAction::DuplicateTo(target)) => {
                project.duplicate_clip(clip.id, target, clip.start_sample);
            }
            None => {}
        }
    }

    // Ghost preview of the dragged clip while it's hovering over this lane.
    if let Some(drag) = &state.drag
        && pointer_target_track == Some(track_id)
            && let Some(pointer_x) = ui.ctx().pointer_interact_pos().map(|p| p.x) {
                let new_start =
                    (sample_for_x(pointer_x) as i64 - drag.grab_offset_samples).max(0) as u64;
                let gx = x_for(new_start);
                let gw = (drag.len_samples as f32 * px_per_sample).max(2.0);
                let ghost_rect = Rect::from_min_size(
                    egui::pos2(gx, rect.top() + 4.0),
                    Vec2::new(gw, ROW_HEIGHT - 8.0),
                );
                ui.painter().rect_filled(
                    ghost_rect,
                    CornerRadius::from(4.0),
                    Color32::from_rgba_unmultiplied(120, 170, 255, 90),
                );
                ui.painter().rect_stroke(
                    ghost_rect,
                    CornerRadius::from(4.0),
                    Stroke::new(1.5, Color32::from_rgb(120, 170, 255)),
                    StrokeKind::Middle,
                );
            }

    let playhead_x = x_for(playhead_sample);
    if playhead_x >= rect.left() && playhead_x <= rect.right() {
        ui.painter().line_segment(
            [
                egui::pos2(playhead_x, rect.top()),
                egui::pos2(playhead_x, rect.bottom()),
            ],
            Stroke::new(1.5, Color32::from_rgb(220, 60, 60)),
        );
    }
}

fn draw_clip_rect(ui: &egui::Ui, rect: Rect, name: &str, dragging: bool, selected: bool) {
    let visuals = ui.visuals();
    let fill = if dragging {
        Color32::from_rgba_unmultiplied(150, 150, 150, 60)
    } else if selected {
        visuals.selection.bg_fill
    } else {
        visuals.widgets.inactive.bg_fill
    };
    let stroke_color = if selected {
        visuals.selection.stroke.color
    } else {
        visuals.widgets.inactive.bg_stroke.color
    };
    let text_color = visuals.text_color();

    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::from(4.0), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::from(4.0),
        Stroke::new(1.0, stroke_color),
        StrokeKind::Middle,
    );
    painter.text(
        rect.left_top() + Vec2::new(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::default(),
        text_color,
    );
}
