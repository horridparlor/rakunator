use crate::project::{self, Project};
use std::path::PathBuf;

use super::RakunatorApp;

pub struct ProjectFileDialogState {
    pub open: bool,
    path_text: String,
    status: Option<String>,
    /// Set instead of saving immediately when "Save" targets a path that
    /// already exists — the confirm popup (`draw_overwrite_confirm`) reads
    /// this, and actually saves only once the user confirms.
    confirm_overwrite_path: Option<PathBuf>,
}

impl Default for ProjectFileDialogState {
    fn default() -> Self {
        let default_path = dirs::download_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("project.raku");
        ProjectFileDialogState {
            open: false,
            path_text: default_path.display().to_string(),
            status: None,
            confirm_overwrite_path: None,
        }
    }
}

/// Draws the "Project File" modal: a path field plus Save/Load buttons for
/// `.raku` project files. Runs synchronously on the GUI thread — project
/// sizes at this app's scale serialize fast enough that a background
/// thread (as used for export) isn't worth the added complexity here.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.project_file_dialog.open {
        return;
    }

    let mut open = true;
    let mut save = false;
    let mut load = false;
    let state = &mut app.project_file_dialog;

    let mut browse_save = false;
    let mut browse_load = false;

    egui::Window::new("Project File").open(&mut open).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y += 4.0;
        ui.horizontal(|ui| {
            ui.label("Path:");
            ui.add(egui::TextEdit::singleline(&mut state.path_text).desired_width(320.0));
        });
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                save = true;
            }
            if ui.button("Save as...").clicked() {
                browse_save = true;
            }
            if ui.button("Load").clicked() {
                load = true;
            }
            if ui.button("Select...").clicked() {
                browse_load = true;
            }
        });
        if let Some(status) = &state.status {
            ui.label(status);
        }
    });

    if browse_save {
        let starting_dir = PathBuf::from(&app.project_file_dialog.path_text);
        let dialog = rfd::FileDialog::new().add_filter("Rakunator Project", &["raku"]);
        let dialog = match starting_dir.parent() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        if let Some(path) = dialog.save_file() {
            app.project_file_dialog.path_text = path.display().to_string();
        }
    }
    if browse_load {
        let starting_dir = PathBuf::from(&app.project_file_dialog.path_text);
        let dialog = rfd::FileDialog::new().add_filter("Rakunator Project", &["raku"]);
        let dialog = match starting_dir.parent() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        if let Some(path) = dialog.pick_file() {
            app.project_file_dialog.path_text = path.display().to_string();
        }
    }

    app.project_file_dialog.open = open;

    if save {
        let path = PathBuf::from(&app.project_file_dialog.path_text);
        if path.exists() {
            app.project_file_dialog.confirm_overwrite_path = Some(path);
        } else {
            do_save(app, &path);
        }
    }

    draw_overwrite_confirm(ctx, app);

    if load {
        let path = PathBuf::from(&app.project_file_dialog.path_text);
        let now = timestamp();
        match project::persistence::load_project(&path) {
            Ok(loaded) => {
                replace_project(app, loaded);
                app.project_name = file_stem(&path);
                app.project_file_dialog.status = Some(format!("Loaded {} at {now}", path.display()));
            }
            Err(e) => {
                app.project_file_dialog.status = Some(format!("Load failed at {now}: {e}"));
            }
        }
    }
}

/// A small modal on top of the "Project File" window, shown instead of
/// saving immediately whenever "Save" targets a path that already exists
/// on disk — confirms before silently overwriting it.
fn draw_overwrite_confirm(ctx: &egui::Context, app: &mut RakunatorApp) {
    let Some(path) = app.project_file_dialog.confirm_overwrite_path.clone() else {
        return;
    };

    let mut overwrite = false;
    let mut cancel = false;
    egui::Window::new("Overwrite file?")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(format!("{} already exists.", path.display()));
            ui.label("Overwrite it?");
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
        do_save(app, &path);
        app.project_file_dialog.confirm_overwrite_path = None;
    } else if cancel {
        app.project_file_dialog.confirm_overwrite_path = None;
    }
}

/// Serializes the current project to `path`, updating the status line and
/// (on success) `project_name` — the actual save, run either directly (the
/// target didn't already exist) or after `draw_overwrite_confirm`.
fn do_save(app: &mut RakunatorApp, path: &std::path::Path) {
    let snapshot = app.project.lock().unwrap().clone();
    let result = project::persistence::save_project(&snapshot, path);
    let now = timestamp();
    app.project_file_dialog.status = Some(match result {
        Ok(()) => {
            app.project_name = file_stem(path);
            format!("Saved to {} at {now}", path.display())
        }
        Err(e) => format!("Save failed at {now}: {e}"),
    });
}

/// Current wall-clock time (HH:MM:SS), so repeated Save/Load presses show
/// a visibly different status message even when the path is unchanged.
fn timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn file_stem(path: &std::path::Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

fn replace_project(app: &mut RakunatorApp, loaded: Project) {
    app.engine.stop();
    *app.project.lock().unwrap() = loaded;
}
