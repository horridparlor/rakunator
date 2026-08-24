use crate::export;
use std::sync::Arc;
use std::thread;

use super::RakunatorApp;

pub struct ExportDialogState {
    pub open: bool,
    file_name: String,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        ExportDialogState {
            open: false,
            file_name: "mixdown".to_string(),
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
        let project = Arc::clone(&app.project);
        let file_name = app.export_dialog.file_name.clone();
        thread::spawn(move || {
            let snapshot = project.lock().unwrap().clone();
            export::export_project(&snapshot, &file_name);
        });
        app.export_dialog.open = false;
    }
}
