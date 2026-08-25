use crate::export;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::thread;

use super::{toast, RakunatorApp};

pub struct ExportDialogState {
    pub open: bool,
    file_name: String,
    /// Set instead of exporting immediately when the target `.wav`/`.mp3`
    /// already exists — the confirm popup reads this, and only starts the
    /// export once the user confirms.
    confirm_overwrite: Option<String>,
    /// Set by the background export thread when it finishes (`Ok` names the
    /// files written, `Err` a message) — polled and turned into a toast
    /// (then cleared) every frame regardless of whether the dialog itself
    /// is still open, since it always closes immediately once the export
    /// starts, well before the render/encode actually finishes.
    result: Arc<Mutex<Option<Result<String, String>>>>,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        ExportDialogState {
            open: false,
            file_name: "Untitled Project".to_string(),
            confirm_overwrite: None,
            result: Arc::new(Mutex::new(None)),
        }
    }
}

impl ExportDialogState {
    /// Overrides the export filename, e.g. to default it to the current
    /// project's saved/loaded name when the dialog is opened.
    pub fn set_file_name(&mut self, name: String) {
        self.file_name = name;
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
            if ui.button("Export").clicked() {
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
        let file_name = app.export_dialog.file_name.clone();
        let (wav_path, mp3_path) = export::export_paths(&file_name);
        if wav_path.exists() || mp3_path.exists() {
            app.export_dialog.confirm_overwrite = Some(file_name);
        } else {
            start_export(ctx, app, file_name);
        }
    }

    draw_overwrite_confirm(ctx, app);
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
                if ui.button("Overwrite").clicked() {
                    overwrite = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });

    if overwrite {
        start_export(ctx, app, file_name);
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
