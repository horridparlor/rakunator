use crate::export;
use std::sync::Arc;
use std::thread;

use super::RakunatorApp;

pub struct ExportDialogState {
    pub open: bool,
    file_name: String,
    /// Set instead of exporting immediately when the target `.wav`/`.mp3`
    /// already exists — the confirm popup reads this, and only starts the
    /// export once the user confirms.
    confirm_overwrite: Option<String>,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        ExportDialogState {
            open: false,
            file_name: "Untitled Project".to_string(),
            confirm_overwrite: None,
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
    if !app.export_dialog.open {
        return;
    }

    let mut open = true;
    let mut do_export = false;

    egui::Window::new("Export Project")
        .open(&mut open)
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
        });

    app.export_dialog.open = open;

    if do_export {
        let file_name = app.export_dialog.file_name.clone();
        let (wav_path, mp3_path) = export::export_paths(&file_name);
        if wav_path.exists() || mp3_path.exists() {
            app.export_dialog.confirm_overwrite = Some(file_name);
        } else {
            start_export(app, file_name);
        }
    }

    draw_overwrite_confirm(ctx, app);
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
        start_export(app, file_name);
        app.export_dialog.confirm_overwrite = None;
    } else if cancel {
        app.export_dialog.confirm_overwrite = None;
    }
}

/// Renders the mixdown on a background thread and closes the dialog — the
/// actual export, run either directly (the target didn't already exist)
/// or after `draw_overwrite_confirm`.
fn start_export(app: &mut RakunatorApp, file_name: String) {
    let project = Arc::clone(&app.project);
    thread::spawn(move || {
        let snapshot = project.lock().unwrap().clone();
        export::export_project(&snapshot, &file_name);
    });
    app.export_dialog.open = false;
}
