use crate::export;
use crate::project::ProjectMetadata;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::thread;

use super::{project_file_dialog, toast, RakunatorApp};

pub struct ExportDialogState {
    pub open: bool,
    file_name: String,
    artist_name: String,
    /// Prefilled from the project's current file name when the dialog opens
    /// and no title has been set yet — see `open_for_project`.
    track_title: String,
    album_title: String,
    track_number: u32,
    year: u32,
    genre: String,
    comments: String,
    software: String,
    /// Set instead of exporting immediately when the target `.wav`/`.mp3`
    /// already exists — the confirm popup reads this, and only starts the
    /// export once the user confirms.
    confirm_overwrite: Option<String>,
    /// One-shot: set alongside `open = true` so the very next frame gives
    /// the "Export" button keyboard focus (egui treats Space/Enter on a
    /// focused clickable widget as a click), letting Ctrl+E, Enter export
    /// immediately. Consumed (via `std::mem::take`) the first time it's
    /// read, so tabbing focus elsewhere afterward sticks instead of being
    /// stolen back every frame.
    focus_export_button: bool,
    /// Same idea as `focus_export_button`, for the overwrite-confirm
    /// popup's "Overwrite" button.
    focus_overwrite_button: bool,
    /// Set by the background export thread when it finishes (`Ok` names the
    /// files written, `Err` a message) — polled and turned into a toast
    /// (then cleared) every frame regardless of whether the dialog itself
    /// is still open, since it always closes immediately once the export
    /// starts, well before the render/encode actually finishes.
    result: Arc<Mutex<Option<Result<String, String>>>>,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        let metadata = ProjectMetadata::default();
        ExportDialogState {
            open: false,
            file_name: "Untitled Project".to_string(),
            artist_name: metadata.artist_name,
            track_title: metadata.track_title,
            album_title: metadata.album_title,
            track_number: metadata.track_number,
            year: metadata.year,
            genre: metadata.genre,
            comments: metadata.comments,
            software: metadata.software,
            confirm_overwrite: None,
            focus_export_button: false,
            focus_overwrite_button: false,
            result: Arc::new(Mutex::new(None)),
        }
    }
}

/// Opens the "Export Project" dialog, populating it from the current
/// project (see `ExportDialogState::open_for_project`) — shared by the
/// toolbar's "Export Project..." button and the Ctrl+E shortcut so both
/// open the dialog exactly the same way.
pub fn open(app: &mut RakunatorApp) {
    let project_name = app.project_name.clone();
    let metadata = app.project.lock().unwrap().metadata.clone();
    app.export_dialog.open_for_project(project_name.as_deref(), &metadata);
    app.export_dialog.open = true;
    app.export_dialog.focus_export_button = true;
}

impl ExportDialogState {
    /// Populates the dialog from the current project — every field
    /// (including the export "Name:") defaults to whatever's already
    /// stored in the project's `metadata`, so a custom export name sticks
    /// across dialog opens the same way the tag fields do. `export_file_name`
    /// and Track Title/Album Title fall back to `project_name` (the last
    /// saved/loaded `.raku`'s base name) only the first time they're still
    /// unset. Called each time the "Export Project..." button opens the
    /// dialog, so it always reflects the metadata actually saved with the
    /// project rather than whatever was last typed into the dialog.
    pub fn open_for_project(&mut self, project_name: Option<&str>, metadata: &ProjectMetadata) {
        let name_fallback = project_name.unwrap_or(&self.file_name).to_string();
        self.file_name = if metadata.export_file_name.is_empty() { name_fallback.clone() } else { metadata.export_file_name.clone() };
        self.artist_name = metadata.artist_name.clone();
        self.track_title = if metadata.track_title.is_empty() { name_fallback.clone() } else { metadata.track_title.clone() };
        self.album_title = if metadata.album_title.is_empty() { name_fallback } else { metadata.album_title.clone() };
        self.track_number = metadata.track_number;
        self.year = metadata.year;
        self.genre = metadata.genre.clone();
        self.comments = metadata.comments.clone();
        self.software = metadata.software.clone();
    }
}

/// Draws the "Export Project" modal. Renders the mixdown on a background
/// thread (after cloning the project just long enough to release the
/// lock) so the GUI never freezes while exporting.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) {
    poll_result(app);

    if !app.export_dialog.open {
        return;
    }

    let mut open = true;
    let mut do_export = false;

    egui::Window::new("Export Project")
        .open(&mut open)
        .resizable(true)
        .frame(super::window_frame(ctx, 1, 1, 1, 1))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y += 4.0;
            ui.label("Renders the full multi-track mixdown to <name>.wav and <name>.mp3 in your Downloads folder.");
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut app.export_dialog.file_name).desired_width(200.0));
            });

            ui.add_space(6.0);
            ui.separator();
            ui.label("Export tags (saved with the project):");
            ui.horizontal(|ui| {
                ui.label("Artist Name:");
                ui.add(egui::TextEdit::singleline(&mut app.export_dialog.artist_name).desired_width(200.0));
            });
            ui.horizontal(|ui| {
                ui.label("Track Title:");
                ui.add(egui::TextEdit::singleline(&mut app.export_dialog.track_title).desired_width(200.0));
            });
            ui.horizontal(|ui| {
                ui.label("Album Title:");
                ui.add(egui::TextEdit::singleline(&mut app.export_dialog.album_title).desired_width(200.0));
            });
            ui.horizontal(|ui| {
                ui.label("Track Number:");
                ui.add(egui::DragValue::new(&mut app.export_dialog.track_number).range(1..=9999));
            });
            ui.horizontal(|ui| {
                ui.label("Year:");
                ui.add(egui::DragValue::new(&mut app.export_dialog.year).range(0..=9999));
            });
            ui.horizontal(|ui| {
                ui.label("Genre:");
                ui.add(egui::TextEdit::singleline(&mut app.export_dialog.genre).desired_width(200.0));
            });
            ui.horizontal(|ui| {
                ui.label("Comments:");
                ui.add(egui::TextEdit::multiline(&mut app.export_dialog.comments).desired_width(200.0).desired_rows(2));
            });
            ui.horizontal(|ui| {
                ui.label("Software:");
                ui.add(egui::TextEdit::singleline(&mut app.export_dialog.software).desired_width(200.0));
            });
            ui.add_space(6.0);

            let export_resp = ui.button("Export");
            if std::mem::take(&mut app.export_dialog.focus_export_button) {
                export_resp.request_focus();
            }
            if export_resp.clicked() {
                do_export = true;
            }

            // This dialog's content is naturally shorter than a manually
            // dragged-taller window: without claiming the leftover space,
            // the window's frame/border snaps back to hug the content
            // every frame instead of visibly growing, making a vertical
            // drag look like it does nothing.
            ui.allocate_space(egui::vec2(0.0, ui.available_height()));
        });

    app.export_dialog.open = open;

    if do_export {
        commit_metadata(app);
        let file_name = app.export_dialog.file_name.clone();
        let (wav_path, mp3_path) = export::export_paths(&file_name);
        if wav_path.exists() || mp3_path.exists() {
            app.export_dialog.confirm_overwrite = Some(file_name);
            app.export_dialog.focus_overwrite_button = true;
        } else {
            save_and_start_export(ctx, app, file_name);
        }
    }

    draw_overwrite_confirm(ctx, app);
}

/// Writes the dialog's tag fields into the project's `metadata` — called
/// right before exporting, so the render/save/export below all see the
/// same up-to-date values, and so they're what gets persisted the next
/// time the project is saved.
fn commit_metadata(app: &mut RakunatorApp) {
    let d = &app.export_dialog;
    let metadata = ProjectMetadata {
        export_file_name: d.file_name.clone(),
        artist_name: d.artist_name.clone(),
        track_title: d.track_title.clone(),
        album_title: d.album_title.clone(),
        track_number: d.track_number,
        year: d.year,
        genre: d.genre.clone(),
        comments: d.comments.clone(),
        software: d.software.clone(),
    };
    app.project.lock().unwrap().metadata = metadata;
}

/// Saves the project (carrying the metadata `commit_metadata` just wrote)
/// to its current file, then starts the actual mixdown/tag export — so the
/// updated tags are on disk in the `.raku` file too, and reload with it the
/// next time this project is opened and exported again.
fn save_and_start_export(ctx: &egui::Context, app: &mut RakunatorApp, file_name: String) {
    project_file_dialog::save_current(ctx, app);
    start_export(ctx, app, file_name);
}

/// Turns a just-finished background export into a toast, if one landed
/// since the last frame.
fn poll_result(app: &mut RakunatorApp) {
    let outcome = app.export_dialog.result.lock().unwrap().take();
    match outcome {
        Some(Ok(names)) => toast::show(app, format!("Exported {names}")),
        Some(Err(e)) => toast::show(app, format!("Export failed: {e}")),
        None => {}
    }
}

/// A small modal on top of the "Export Project" window, shown instead of
/// exporting immediately whenever the target `.wav`/`.mp3` already exists
/// on disk — confirms before silently overwriting either.
fn draw_overwrite_confirm(ctx: &egui::Context, app: &mut RakunatorApp) {
    let Some(file_name) = app.export_dialog.confirm_overwrite.clone() else {
        return;
    };
    let (wav_path, mp3_path) = export::export_paths(&file_name);

    let mut overwrite = false;
    let mut cancel = false;
    egui::Window::new("Overwrite file?")
        .collapsible(false)
        .resizable(false)
        .frame(super::window_frame(ctx, 1, 1, 1, 1))
        .show(ctx, |ui| {
            ui.label(format!("{} and/or {}", wav_path.display(), mp3_path.display()));
            ui.label("already exist. Overwrite them?");
            ui.horizontal(|ui| {
                let overwrite_resp = ui.button("Overwrite");
                if std::mem::take(&mut app.export_dialog.focus_overwrite_button) {
                    overwrite_resp.request_focus();
                }
                if overwrite_resp.clicked() {
                    overwrite = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });

    if overwrite {
        save_and_start_export(ctx, app, file_name);
        app.export_dialog.confirm_overwrite = None;
    } else if cancel {
        app.export_dialog.confirm_overwrite = None;
    }
}

/// Renders the mixdown on a background thread and closes the dialog — the
/// actual export, run either directly (the target didn't already exist) or
/// after `draw_overwrite_confirm`. `export_project` panics on an I/O
/// failure (a full disk, a Downloads folder that got deleted mid-session,
/// etc.) rather than returning a `Result`, so the render/encode runs under
/// `catch_unwind` here — the only way to turn that into a reportable
/// failure instead of silently killing the background thread — and the
/// outcome either way is handed back through `result` for `poll_result` to
/// turn into a toast, since the dialog itself is long closed by the time
/// this finishes.
fn start_export(ctx: &egui::Context, app: &mut RakunatorApp, file_name: String) {
    let project = Arc::clone(&app.project);
    let result = Arc::clone(&app.export_dialog.result);
    let ctx = ctx.clone();
    let (wav_path, mp3_path) = export::export_paths(&file_name);
    let names = format!(
        "{} / {}",
        wav_path.file_name().and_then(|s| s.to_str()).unwrap_or("mixdown.wav"),
        mp3_path.file_name().and_then(|s| s.to_str()).unwrap_or("mixdown.mp3"),
    );

    thread::spawn(move || {
        let snapshot = project.lock().unwrap().clone();
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            export::export_project(&snapshot, &file_name);
        }))
        .map(|()| names)
        .map_err(|payload| panic_message(&payload));
        *result.lock().unwrap() = Some(outcome);
        ctx.request_repaint();
    });
    app.export_dialog.open = false;
}

/// Best-effort extraction of a human-readable message from a caught
/// panic's payload — `.expect(...)`/`panic!("...")` payloads are almost
/// always a `String` or `&str`.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error (see console)".to_string())
}
