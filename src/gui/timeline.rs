use crate::audio_engine::AudioEngine;
use crate::project::{ClipId, Project, TrackId};
use egui::{Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, Vec2};

use super::{ROW_HEIGHT, RULER_HEIGHT, TRACK_ROW_STEP};

/// ~480 pixels per second at a 48kHz project rate; only used as the
/// initial zoom level, not a hard assumption elsewhere.
const DEFAULT_PX_PER_SAMPLE: f32 = 480.0 / 48_000.0;
const MIN_PX_PER_SAMPLE: f32 = 0.0005;
const MAX_PX_PER_SAMPLE: f32 = 2.0;
/// How close (in pixels) a drag has to start to a clip's edge to trim it
/// instead of moving it.
const EDGE_GRAB_PX: f32 = 12.0;
/// How close (in pixels) a moved/trimmed edge has to land to another
/// clip's edge (anywhere in the project) to snap to it.
const SNAP_PX: f32 = 8.0;

pub struct TimelineState {
    pub px_per_sample: f32,
    pub scroll_x_samples: f32,
    /// Screen-space y-coordinate of the top of the first track row this
    /// frame; used to map the pointer's y position to a track row.
    pub tracks_top_y: f32,
    /// Where cut/copy/paste should target next: last clicked track+sample.
    pub last_click: Option<(TrackId, u64)>,
    /// An Alt-dragged sub-range selection (from Ctrl+L's "mute this
    /// portion" or a ranged fade), one entry per clip it touches — it can
    /// span the end of one clip and the start of the next few. Persisted
    /// until the user drags a new one.
    pub range_selections: Vec<(ClipId, u64, u64)>,
    /// Screen-space x of every lane's left edge, measured last frame (the
    /// ruler, drawn before the lanes, reuses this so its ticks and playhead
    /// line stay pixel-aligned with the lanes below it).
    lane_left_x: Option<f32>,
    /// Where a Shift+drag marquee selection started, in screen space.
    marquee_anchor: Option<egui::Pos2>,
    /// Sample position of the clip edge a move/trim is currently snapped
    /// to, if any — drawn as a yellow alignment line. Reset every frame
    /// (by the caller) and set at most once, by whichever lane is drawing
    /// the clip currently being dragged.
    snap_indicator: Option<u64>,
    drag: Option<DragState>,
}

impl Default for TimelineState {
    fn default() -> Self {
        TimelineState {
            px_per_sample: DEFAULT_PX_PER_SAMPLE,
            scroll_x_samples: 0.0,
            tracks_top_y: 0.0,
            last_click: None,
            range_selections: Vec::new(),
            lane_left_x: None,
            marquee_anchor: None,
            snap_indicator: None,
            drag: None,
        }
    }
}

impl TimelineState {
    /// Zooms by `factor` (>1 zooms in, <1 zooms out), clamped to sane
    /// bounds. Used by the toolbar's Zoom In/Out buttons — a reliable
    /// fallback since Ctrl+scroll-to-zoom can be intercepted by the
    /// desktop environment/compositor before it ever reaches the app.
    pub fn zoom(&mut self, factor: f32) {
        self.px_per_sample = (self.px_per_sample * factor).clamp(MIN_PX_PER_SAMPLE, MAX_PX_PER_SAMPLE);
    }

    /// Where an in-progress Shift+drag marquee selection started, if any —
    /// for drawing the live selection-rectangle overlay.
    pub fn marquee_anchor(&self) -> Option<egui::Pos2> {
        self.marquee_anchor
    }

    /// The sample position of the clip edge a move/trim is currently
    /// snapped to, if any — for drawing the yellow alignment line.
    pub fn snap_indicator(&self) -> Option<u64> {
        self.snap_indicator
    }

    /// Clears the snap indicator; call once per frame before drawing any
    /// lanes, since at most one lane (whichever holds the dragged clip)
    /// will set it again that same frame.
    pub fn clear_snap_indicator(&mut self) {
        self.snap_indicator = None;
    }

    /// Converts a sample position to screen-space x, using last frame's
    /// measured lane left edge. `None` before the first lane has drawn.
    pub fn x_for_sample(&self, sample: u64) -> Option<f32> {
        self.lane_left_x
            .map(|left| left + (sample as f32 - self.scroll_x_samples) * self.px_per_sample)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Move { duplicate: bool },
    TrimStart,
    TrimEnd,
    RangeSelect { anchor_sample: u64 },
}

struct DragState {
    /// `None` for a range-select drag started on empty lane space (not
    /// tied to any particular clip).
    clip_id: Option<ClipId>,
    origin_track: TrackId,
    len_samples: u64,
    grab_offset_samples: i64,
    mode: DragMode,
}

struct ClipSnapshot {
    id: ClipId,
    name: String,
    start_sample: u64,
    len_samples: u64,
    channels: u8,
    samples: Vec<f32>,
}

/// Clips a `[lo, hi)` range-select drag to every clip in `clips` it
/// overlaps, e.g. the tail of one clip and the head of the next few.
fn overlapping_range_selections(clips: &[ClipSnapshot], lo: u64, hi: u64) -> Vec<(ClipId, u64, u64)> {
    let mut out = Vec::new();
    for c in clips {
        let clip_lo = lo.max(c.start_sample);
        let clip_hi = hi.min(c.start_sample + c.len_samples);
        if clip_hi > clip_lo {
            out.push((c.id, clip_lo, clip_hi));
        }
    }
    out
}

enum ClipMenuAction {
    Cut,
    Copy,
    DuplicateHere,
    SplitAt(u64),
    DuplicateTo(TrackId),
}

/// Draws the time ruler above the track list: second/minute tick marks,
/// a playhead marker, and click-to-seek. Shares `state`'s zoom/scroll with
/// every track lane so their horizontal coordinates always line up.
pub fn draw_ruler(
    ui: &mut egui::Ui,
    state: &mut TimelineState,
    playhead_sample: u64,
    sample_rate_hz: u32,
    engine: &AudioEngine,
) {
    let size = Vec2::new(ui.available_width().max(200.0), RULER_HEIGHT);
    let (mut rect, response) = ui.allocate_exact_size(size, Sense::click());
    // The ruler is drawn before the lanes (outside their `ScrollArea`), so
    // its own layout can end up a pixel or two off from theirs. Snap to
    // last frame's measured lane-left so ticks/playhead line up exactly.
    if let Some(lane_left_x) = state.lane_left_x {
        rect = rect.translate(egui::vec2(lane_left_x - rect.left(), 0.0));
    }

    ui.painter().rect_filled(rect, 0.0, ui.visuals().faint_bg_color);

    handle_zoom_and_pan(ui, response.hovered(), rect, state);

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos() {
            let sample = sample_for_x(pos.x, rect, state);
            engine.seek(sample);
        }

    let seconds_visible = (rect.width() / state.px_per_sample) / sample_rate_hz as f32;
    let step_secs = pick_tick_step(seconds_visible);
    let step_samples = (step_secs * sample_rate_hz as f32).round() as u64;
    if step_samples > 0 {
        let first_tick = (state.scroll_x_samples.max(0.0) as u64 / step_samples) * step_samples;
        let mut sample = first_tick;
        loop {
            let x = x_for(sample, rect, state);
            if x > rect.right() {
                break;
            }
            if x >= rect.left() {
                ui.painter().line_segment(
                    [egui::pos2(x, rect.bottom() - 6.0), egui::pos2(x, rect.bottom())],
                    Stroke::new(1.0, Color32::GRAY),
                );
                let secs = sample as f32 / sample_rate_hz as f32;
                ui.painter().text(
                    egui::pos2(x + 2.0, rect.top()),
                    egui::Align2::LEFT_TOP,
                    format_time(secs),
                    egui::FontId::proportional(10.0),
                    ui.visuals().text_color(),
                );
            }
            sample += step_samples;
        }
    }

    let playhead_x = x_for(playhead_sample, rect, state);
    if playhead_x >= rect.left() && playhead_x <= rect.right() {
        ui.painter().line_segment(
            [egui::pos2(playhead_x, rect.top()), egui::pos2(playhead_x, rect.bottom())],
            Stroke::new(2.0, Color32::from_rgb(220, 60, 60)),
        );
    }
}

/// Height of the horizontal-scroll bar drawn under the track list.
pub const SCROLLBAR_HEIGHT: f32 = 16.0;
const SCROLLBAR_MIN_THUMB_PX: f32 = 24.0;
/// Extra scrollable room past the last clip, so there's somewhere to drop
/// new content beyond the current end of the project.
const SCROLLBAR_TAIL_SECONDS: f32 = 5.0;

/// Draws a always-visible horizontal scrollbar under the track list —
/// drag it (from anywhere on the bar, not just the thumb) to scroll
/// through the song. Shares `state`'s zoom/scroll with the ruler and every
/// lane, so it stays in sync with them.
pub fn draw_horizontal_scrollbar(
    ui: &mut egui::Ui,
    state: &mut TimelineState,
    content_end_sample: u64,
    sample_rate_hz: u32,
) {
    let size = Vec2::new(ui.available_width().max(200.0), SCROLLBAR_HEIGHT);
    let (mut rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if let Some(lane_left_x) = state.lane_left_x {
        rect = rect.translate(egui::vec2(lane_left_x - rect.left(), 0.0));
    }

    ui.painter()
        .rect_filled(rect, 3.0, ui.visuals().faint_bg_color);

    let bar_w = rect.width();
    let visible_samples = (bar_w / state.px_per_sample).max(1.0);
    let total_samples = (content_end_sample as f32 + sample_rate_hz as f32 * SCROLLBAR_TAIL_SECONDS)
        .max(visible_samples);
    let max_scroll = (total_samples - visible_samples).max(0.0);

    let thumb_w = (bar_w * visible_samples / total_samples).clamp(SCROLLBAR_MIN_THUMB_PX, bar_w);
    let track_w = (bar_w - thumb_w).max(0.0);

    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
        && track_w > 0.0
    {
        let rel = ((pos.x - thumb_w / 2.0 - rect.left()) / track_w).clamp(0.0, 1.0);
        state.scroll_x_samples = rel * max_scroll;
    }

    let thumb_x = rect.left()
        + if max_scroll > 0.0 {
            (state.scroll_x_samples.clamp(0.0, max_scroll) / max_scroll) * track_w
        } else {
            0.0
        };
    let thumb_rect = Rect::from_min_size(
        egui::pos2(thumb_x, rect.top() + 2.0),
        Vec2::new(thumb_w, rect.height() - 4.0),
    );
    let thumb_color = if response.dragged() {
        ui.visuals().widgets.active.bg_fill
    } else {
        ui.visuals().widgets.inactive.bg_fill
    };
    ui.painter().rect_filled(thumb_rect, 3.0, thumb_color);
}

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

fn format_time(total_secs: f32) -> String {
    let total_secs = total_secs.max(0.0);
    let minutes = (total_secs / 60.0) as u32;
    let secs = total_secs - (minutes as f32) * 60.0;
    if minutes > 0 {
        format!("{minutes}:{secs:04.1}")
    } else {
        format!("{secs:.1}s")
    }
}

fn x_for(sample: u64, rect: Rect, state: &TimelineState) -> f32 {
    rect.left() + (sample as f32 - state.scroll_x_samples) * state.px_per_sample
}

fn sample_for_x(x: f32, rect: Rect, state: &TimelineState) -> u64 {
    (((x - rect.left()) / state.px_per_sample) + state.scroll_x_samples).max(0.0) as u64
}

/// Snaps `candidate` to the nearest value in `targets` within `SNAP_PX`
/// pixels at the current zoom, or returns it unchanged if none are close.
fn snap_sample(candidate: i64, targets: &[u64], px_per_sample: f32) -> i64 {
    let threshold = ((SNAP_PX / px_per_sample).max(1.0)) as i64;
    targets
        .iter()
        .map(|&t| t as i64)
        .filter(|&t| (t - candidate).abs() <= threshold)
        .min_by_key(|&t| (t - candidate).abs())
        .unwrap_or(candidate)
}

/// Snaps a moving clip's new start position, trying its leading edge
/// first and its trailing edge second (whichever lands on a snap target).
fn snap_move_start(new_start: i64, len_samples: i64, targets: &[u64], px_per_sample: f32) -> i64 {
    snap_move_start_with_indicator(new_start, len_samples, targets, px_per_sample).0
}

/// Same as `snap_move_start`, but also reports the sample position that
/// was snapped to (for drawing the yellow alignment line), or `None` if
/// nothing was close enough to snap.
fn snap_move_start_with_indicator(
    new_start: i64,
    len_samples: i64,
    targets: &[u64],
    px_per_sample: f32,
) -> (i64, Option<u64>) {
    let snapped_start = snap_sample(new_start, targets, px_per_sample);
    if snapped_start != new_start {
        return (snapped_start, Some(snapped_start.max(0) as u64));
    }
    let end = new_start + len_samples;
    let snapped_end = snap_sample(end, targets, px_per_sample);
    if snapped_end != end {
        return (snapped_end - len_samples, Some(snapped_end.max(0) as u64));
    }
    (new_start, None)
}

/// Ctrl+scroll zooms the timeline (keeping the sample under the pointer
/// fixed); Shift+scroll pans it horizontally. Consumes the scroll delta so
/// the enclosing vertical `ScrollArea` doesn't also react to the same wheel
/// event.
fn handle_zoom_and_pan(ui: &egui::Ui, hovered: bool, rect: Rect, state: &mut TimelineState) {
    if !hovered {
        return;
    }
    let (scroll_y, ctrl, shift) = ui
        .ctx()
        .input(|i| (i.smooth_scroll_delta.y, i.modifiers.command, i.modifiers.shift));
    if scroll_y == 0.0 || !(ctrl || shift) {
        return;
    }

    if ctrl {
        if let Some(pointer_x) = ui.ctx().pointer_interact_pos().map(|p| p.x) {
            let sample_at_pointer =
                ((pointer_x - rect.left()) / state.px_per_sample) + state.scroll_x_samples;
            let zoom = if scroll_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
            state.px_per_sample = (state.px_per_sample * zoom).clamp(MIN_PX_PER_SAMPLE, MAX_PX_PER_SAMPLE);
            state.scroll_x_samples = sample_at_pointer - (pointer_x - rect.left()) / state.px_per_sample;
        }
    } else {
        state.scroll_x_samples = (state.scroll_x_samples - scroll_y / state.px_per_sample).max(0.0);
    }

    ui.ctx().input_mut(|i| i.smooth_scroll_delta.y = 0.0);
}

/// Draws one track's timeline lane: its clips (with a waveform outline),
/// the playhead, and handles clip selection (Shift+click adds to a
/// multi-selection), drag-to-move/duplicate/trim, Alt-drag range-select,
/// "I"-to-split-under-cursor, and the cut/copy/duplicate context menu.
/// `all_track_ids` is every track in display order (for cross-track drag
/// targeting) and `snap_targets` is every clip edge in the whole project
/// (for cross-track snapping).
#[allow(clippy::too_many_arguments)]
pub fn draw_lane(
    ui: &mut egui::Ui,
    project: &mut Project,
    track_id: TrackId,
    all_track_ids: &[TrackId],
    snap_targets: &[u64],
    state: &mut TimelineState,
    playhead_sample: u64,
    engine: &AudioEngine,
) {
    let size = Vec2::new(ui.available_width().max(200.0), ROW_HEIGHT);
    let (rect, lane_response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    state.lane_left_x = Some(rect.left());

    ui.painter()
        .rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

    handle_zoom_and_pan(ui, lane_response.hovered(), rect, state);

    let px_per_sample = state.px_per_sample;
    let scroll = state.scroll_x_samples;
    let x_for = |sample: u64| rect.left() + (sample as f32 - scroll) * px_per_sample;
    let sample_for_x = |x: f32| (((x - rect.left()) / px_per_sample) + scroll).max(0.0) as u64;
    let keys_active = !ui.ctx().egui_wants_keyboard_input();

    if lane_response.clicked()
        && let Some(pos) = lane_response.interact_pointer_pos()
    {
        let sample = sample_for_x(pos.x);
        state.last_click = Some((track_id, sample));
        project.selection.clear();
        project.selected_tracks.clear();
        engine.seek(sample);
    }

    // Shift+drag on empty lane space starts a marquee selection spanning
    // however many tracks/time the drag covers (committed in
    // `draw_lane`'s caller-independent handling below, once released).
    // A plain (non-Shift) drag from empty space instead paints a
    // sub-range selection — e.g. for a ranged fade or mute — that can
    // spill into whichever clips it passes over, clipped per clip.
    if lane_response.drag_started() {
        let shift = ui.ctx().input(|i| i.modifiers.shift);
        if shift {
            state.marquee_anchor = ui.ctx().pointer_interact_pos();
        } else if let Some(pos) = ui.ctx().pointer_interact_pos() {
            state.drag = Some(DragState {
                clip_id: None,
                origin_track: track_id,
                len_samples: 0,
                grab_offset_samples: 0,
                mode: DragMode::RangeSelect {
                    anchor_sample: sample_for_x(pos.x),
                },
            });
        }
    }
    if lane_response.drag_stopped()
        && let Some(anchor) = state.marquee_anchor.take()
        && let Some(current) = ui.ctx().pointer_interact_pos()
    {
        let s1 = sample_for_x(anchor.x);
        let s2 = sample_for_x(current.x);
        let (lo_sample, hi_sample) = (s1.min(s2), s1.max(s2));
        let row1 = ((anchor.y - state.tracks_top_y) / TRACK_ROW_STEP).floor().max(0.0) as usize;
        let row2 = ((current.y - state.tracks_top_y) / TRACK_ROW_STEP).floor().max(0.0) as usize;
        let (lo_row, hi_row) = (row1.min(row2), row1.max(row2));

        let mut marquee_selection = std::collections::HashSet::new();
        for (row, id) in all_track_ids.iter().enumerate() {
            if row < lo_row || row > hi_row {
                continue;
            }
            if let Some(track) = project.track(*id) {
                for clip in &track.clips {
                    if clip.start_sample < hi_sample && clip.end_sample() > lo_sample {
                        marquee_selection.insert(clip.id);
                    }
                }
            }
        }
        project.selection = marquee_selection;
        project.selected_tracks.clear();
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
                    channels: c.channels(),
                    samples: c.visible_samples().to_vec(),
                })
                .collect()
        })
        .unwrap_or_default();
    let track_names: Vec<(TrackId, String)> =
        project.tracks.iter().map(|t| (t.id, t.name.clone())).collect();

    // Commits a range-select drag that started on empty lane space (not
    // tied to any specific clip) — the per-clip-started (Alt-drag) case is
    // committed inside the per-clip loop below instead.
    let started_here_untied = state
        .drag
        .as_ref()
        .map(|d| d.clip_id.is_none() && d.origin_track == track_id)
        .unwrap_or(false);
    if lane_response.drag_stopped() && started_here_untied
        && let Some(DragState {
            mode: DragMode::RangeSelect { anchor_sample },
            ..
        }) = state.drag.take()
            && let Some(pos) = ui.ctx().pointer_interact_pos() {
                let current = sample_for_x(pos.x);
                let lo = anchor_sample.min(current);
                let hi = anchor_sample.max(current);
                let selections = overlapping_range_selections(&clips, lo, hi);
                if !selections.is_empty() {
                    state.range_selections = selections;
                }
            }

    // Clamped (rather than `None` outside the track list's bounds) so
    // dragging a clip on the first/last track and drifting slightly
    // above/below it still resolves to that same track — otherwise the
    // ghost preview would vanish the moment the pointer left that track's
    // exact row span, which is easy to do by accident on an edge track.
    let pointer_target_track = if state.drag.is_some() && !all_track_ids.is_empty() {
        ui.ctx().pointer_interact_pos().map(|p| {
            let row = ((p.y - state.tracks_top_y) / TRACK_ROW_STEP).floor();
            let idx = (row.max(0.0) as usize).min(all_track_ids.len() - 1);
            all_track_ids[idx]
        })
    } else {
        None
    };

    for clip in &clips {
        let being_dragged = state
            .drag
            .as_ref()
            .map(|d| d.clip_id == Some(clip.id))
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
            let pointer_x = ui.ctx().pointer_interact_pos().map(|p| p.x).unwrap_or(x);
            let alt = ui.ctx().input(|i| i.modifiers.alt);
            let duplicate = ui.ctx().input(|i| i.modifiers.command);
            let mode = if alt {
                DragMode::RangeSelect {
                    anchor_sample: sample_for_x(pointer_x),
                }
            } else if (pointer_x - x).abs() <= EDGE_GRAB_PX {
                DragMode::TrimStart
            } else if (pointer_x - (x + w)).abs() <= EDGE_GRAB_PX {
                DragMode::TrimEnd
            } else {
                DragMode::Move { duplicate }
            };
            let grab = ((pointer_x - x) / px_per_sample) as i64;
            state.drag = Some(DragState {
                clip_id: Some(clip.id),
                origin_track: track_id,
                len_samples: clip.len_samples,
                grab_offset_samples: grab,
                mode,
            });
        }

        if response.clicked() {
            let shift = ui.ctx().input(|i| i.modifiers.shift);
            if shift {
                if !project.selection.remove(&clip.id) {
                    project.selection.insert(clip.id);
                }
            } else {
                project.selection.clear();
                project.selection.insert(clip.id);
            }
            project.selected_tracks.clear();
            state.last_click = Some((track_id, clip.start_sample));
        }

        // Where "I" (split-under-cursor) or the "Split here" menu item
        // would cut this clip, if the pointer is inside it.
        let split_sample = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| sample_for_x(p.x))
            .filter(|&s| s > clip.start_sample && s < clip.start_sample + clip.len_samples);

        if keys_active && response.hovered() {
            let i_pressed = ui.ctx().input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::I));
            if i_pressed
                && let Some(s) = split_sample {
                    project.split_clip(clip.id, s);
                }
        }

        let should_commit_drag = state
            .drag
            .as_ref()
            .map(|d| d.clip_id == Some(clip.id))
            .unwrap_or(false);
        if response.drag_stopped() && should_commit_drag
            && let Some(drag) = state.drag.take() {
                let pointer_x = ui.ctx().pointer_interact_pos().map(|p| p.x).unwrap_or(x);
                match drag.mode {
                    DragMode::Move { duplicate } => {
                        let target = pointer_target_track.unwrap_or(drag.origin_track);
                        let raw_new_start = sample_for_x(pointer_x) as i64 - drag.grab_offset_samples;
                        let new_start = snap_move_start(
                            raw_new_start.max(0),
                            drag.len_samples as i64,
                            snap_targets,
                            px_per_sample,
                        )
                        .max(0) as u64;
                        if duplicate {
                            project.duplicate_clip(clip.id, target, new_start);
                        } else {
                            project.move_clip(clip.id, target, new_start);
                        }
                    }
                    DragMode::TrimStart => {
                        let raw_boundary = sample_for_x(pointer_x) as i64;
                        let boundary = snap_sample(raw_boundary, snap_targets, px_per_sample);
                        project.trim_clip_start(clip.id, boundary - clip.start_sample as i64);
                    }
                    DragMode::TrimEnd => {
                        let raw_boundary = sample_for_x(pointer_x) as i64;
                        let boundary = snap_sample(raw_boundary, snap_targets, px_per_sample);
                        let end = (clip.start_sample + clip.len_samples) as i64;
                        // Positive delta shortens (matches trim_clip_start's
                        // convention): dragging the right edge leftward
                        // (boundary < end) should shorten, so it's end-minus-
                        // boundary, not boundary-minus-end.
                        project.trim_clip_end(clip.id, end - boundary);
                    }
                    DragMode::RangeSelect { anchor_sample } => {
                        // Spans every clip on this track the drag touches
                        // (e.g. the end of one clip and the start of the
                        // next few), clipped to each clip's own bounds.
                        let current = sample_for_x(pointer_x);
                        let lo = anchor_sample.min(current);
                        let hi = anchor_sample.max(current);
                        let selections = overlapping_range_selections(&clips, lo, hi);
                        if !selections.is_empty() {
                            state.range_selections = selections;
                        }
                    }
                }
            }

        let selected = project.selection.contains(&clip.id);
        draw_clip_rect(ui, clip_rect, &clip.name, &clip.samples, clip.channels, being_dragged, selected);

        // Live preview + snap indicator while trimming this clip's edge.
        if let Some(drag) = &state.drag
            && drag.clip_id == Some(clip.id)
            && matches!(drag.mode, DragMode::TrimStart | DragMode::TrimEnd)
            && let Some(pointer_x) = ui.ctx().pointer_interact_pos().map(|p| p.x)
        {
            let raw = sample_for_x(pointer_x) as i64;
            let snapped = snap_sample(raw, snap_targets, px_per_sample);
            if snapped != raw {
                state.snap_indicator = Some(snapped.max(0) as u64);
            }
            let gx = x_for(snapped.max(0) as u64);
            ui.painter().line_segment(
                [egui::pos2(gx, clip_rect.top()), egui::pos2(gx, clip_rect.bottom())],
                Stroke::new(2.0, Color32::from_rgb(120, 170, 255)),
            );
        }

        // Live preview while Alt-dragging a mute/fade-range that may span
        // multiple clips on this track.
        if let Some(drag) = &state.drag
            && drag.origin_track == track_id
            && let DragMode::RangeSelect { anchor_sample } = drag.mode
                && let Some(pointer_x) = ui.ctx().pointer_interact_pos().map(|p| p.x) {
                        let current = sample_for_x(pointer_x);
                        let lo = anchor_sample.min(current).max(clip.start_sample);
                        let hi = anchor_sample
                            .max(current)
                            .min(clip.start_sample + clip.len_samples);
                        if hi > lo {
                            draw_range_overlay(ui, clip_rect, x_for(lo), x_for(hi));
                        }
                    }
        // The persisted (committed) selection entries on this clip.
        for &(sel_id, lo, hi) in &state.range_selections {
            if sel_id == clip.id {
                draw_range_overlay(ui, clip_rect, x_for(lo), x_for(hi));
            }
        }

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
            if ui
                .add_enabled(split_sample.is_some(), egui::Button::new("Split here"))
                .clicked()
            {
                if let Some(s) = split_sample {
                    menu_action = Some(ClipMenuAction::SplitAt(s));
                }
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
            Some(ClipMenuAction::Cut) => project.cut_clips(&[clip.id]),
            Some(ClipMenuAction::Copy) => project.copy_clips(&[clip.id]),
            Some(ClipMenuAction::DuplicateHere) => {
                let new_start = clip.start_sample + clip.len_samples;
                project.duplicate_clip(clip.id, track_id, new_start);
            }
            Some(ClipMenuAction::SplitAt(sample)) => {
                project.split_clip(clip.id, sample);
            }
            Some(ClipMenuAction::DuplicateTo(target)) => {
                project.duplicate_clip(clip.id, target, clip.start_sample);
            }
            None => {}
        }
    }

    // Ghost preview of a moved (or cross-track duplicated) clip while it's
    // hovering over this lane.
    if let Some(drag) = &state.drag
        && matches!(drag.mode, DragMode::Move { .. })
        && pointer_target_track == Some(track_id)
        && let Some(pointer_x) = ui.ctx().pointer_interact_pos().map(|p| p.x)
    {
        let raw_new_start = sample_for_x(pointer_x) as i64 - drag.grab_offset_samples;
        let (new_start_i, indicator) = snap_move_start_with_indicator(
            raw_new_start.max(0),
            drag.len_samples as i64,
            snap_targets,
            px_per_sample,
        );
        if let Some(s) = indicator {
            state.snap_indicator = Some(s);
        }
        let new_start = new_start_i.max(0) as u64;
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

fn draw_range_overlay(ui: &egui::Ui, clip_rect: Rect, x_lo: f32, x_hi: f32) {
    let lo = x_lo.max(clip_rect.left());
    let hi = x_hi.min(clip_rect.right());
    if hi <= lo {
        return;
    }
    let overlay_rect = Rect::from_min_max(egui::pos2(lo, clip_rect.top()), egui::pos2(hi, clip_rect.bottom()));
    ui.painter().rect_filled(
        overlay_rect,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(255, 80, 80, 70),
    );
}

fn draw_clip_rect(
    ui: &egui::Ui,
    rect: Rect,
    name: &str,
    samples: &[f32],
    channels: u8,
    dragging: bool,
    selected: bool,
) {
    let visuals = ui.visuals();
    // Clips are always drawn on a dark fill with light text/waveform,
    // regardless of the app's light/dark theme, for consistent contrast.
    let fill = if dragging {
        Color32::from_rgba_unmultiplied(150, 150, 150, 60)
    } else if selected {
        visuals.selection.bg_fill
    } else {
        Color32::from_rgb(38, 40, 46)
    };
    let stroke_color = if selected {
        visuals.selection.stroke.color
    } else {
        Color32::from_rgb(70, 74, 82)
    };
    let text_color = Color32::from_rgb(235, 235, 240);

    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::from(4.0), fill);
    let wave_color = text_color.gamma_multiply(0.85);
    if channels >= 2 {
        // Audacity-style stereo display: left channel's waveform on top,
        // right channel's directly below it, sharing the one clip rect.
        let mid_y = rect.center().y;
        let top_rect = Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), mid_y));
        let bottom_rect = Rect::from_min_max(egui::pos2(rect.left(), mid_y), rect.right_bottom());
        let frames = samples.len() / 2;
        let mut left = Vec::with_capacity(frames);
        let mut right = Vec::with_capacity(frames);
        for frame in samples.as_chunks::<2>().0 {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        draw_waveform(painter, top_rect, &left, wave_color);
        draw_waveform(painter, bottom_rect, &right, wave_color);
        painter.line_segment(
            [egui::pos2(rect.left(), mid_y), egui::pos2(rect.right(), mid_y)],
            Stroke::new(1.0, stroke_color.gamma_multiply(0.7)),
        );
    } else {
        draw_waveform(painter, rect, samples, wave_color);
    }
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

/// Draws a min/max envelope of `samples` across `rect`: one vertical line
/// per pixel column. Each column scans at most a bounded number of samples
/// (striding through longer spans) so cost stays roughly proportional to
/// on-screen width regardless of how zoomed-out a long clip is.
fn draw_waveform(painter: &egui::Painter, rect: Rect, samples: &[f32], color: Color32) {
    if samples.is_empty() || rect.width() < 1.0 {
        return;
    }
    const MAX_SAMPLES_SCANNED_PER_COLUMN: usize = 512;

    let width_px = rect.width().round().max(1.0) as usize;
    let mid_y = rect.center().y;
    let half_h = rect.height() / 2.0 - 2.0;
    let samples_per_px = samples.len() as f32 / width_px as f32;

    for col in 0..width_px {
        let start = ((col as f32) * samples_per_px) as usize;
        let end = (((col + 1) as f32) * samples_per_px).ceil() as usize;
        let end = end.clamp(start + 1, samples.len());
        let start = start.min(samples.len() - 1);
        let slice = &samples[start..end];

        let step = (slice.len() / MAX_SAMPLES_SCANNED_PER_COLUMN).max(1);
        let mut min_v = 0.0f32;
        let mut max_v = 0.0f32;
        let mut i = 0;
        while i < slice.len() {
            let s = slice[i];
            min_v = min_v.min(s);
            max_v = max_v.max(s);
            i += step;
        }

        let x = rect.left() + col as f32;
        painter.line_segment(
            [
                egui::pos2(x, mid_y - max_v * half_h),
                egui::pos2(x, mid_y - min_v * half_h),
            ],
            Stroke::new(1.0, color),
        );
    }
}
