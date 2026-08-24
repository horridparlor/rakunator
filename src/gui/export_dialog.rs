use crate::export;
use std::sync::Arc;
use std::thread;

use super::RakunatorApp;

#[derive(Default)]
pub struct ExportDialogState {
    pub open: bool,
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
            ui.label(
                "Renders the full multi-track mixdown to mixdown.wav and \
                 mixdown.mp3 in your Downloads folder.",
            );
            if ui.button("Export").clicked() {
                do_export = true;
            }
        });

    app.export_dialog.open = open;

    if do_export {
        let project = Arc::clone(&app.project);
        thread::spawn(move || {
            let snapshot = project.lock().unwrap().clone();
            export::export_project(&snapshot);
        });
        app.export_dialog.open = false;
    }
}
