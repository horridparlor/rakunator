use crate::project::{self, Project};
use std::path::PathBuf;

use super::{toast, RakunatorApp};

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
/// thread (as used for export) isn't worth the added complexity here. A
/// successful Save/Save As/Load closes the dialog immediately and shows a
/// toast, rather than lingering on an in-dialog status line.
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
            let path_response = ui.add(egui::TextEdit::singleline(&mut state.path_text).desired_width(320.0));
            // Enter in the path field loads it directly, the same as
            // clicking "Load" — `lost_focus` alone would also fire on
            // Tab/click-away, so it's gated on Enter actually being the key
            // that caused it.
            if path_response.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                load = true;
            }
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
        save_as(app);
    }
    // "Select..." loads immediately once a file is picked — there's no
    // separate "Load" button to press afterward.
    if browse_load {
        let starting_dir = PathBuf::from(&app.project_file_dialog.path_text);
        let dialog = rfd::FileDialog::new().add_filter("Rakunator Project", &["raku"]);
        let dialog = match starting_dir.parent() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        if let Some(path) = dialog.pick_file() {
            app.project_file_dialog.path_text = path.display().to_string();
            do_load(app, &path);
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
        do_load(app, &path);
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

/// Saves the project straight to whatever path is currently set in this
/// dialog (its default, if it's never been opened, is the same
/// "Downloads/project.raku" default used elsewhere in this module) — for
/// the Ctrl+S shortcut. Skips the overwrite-confirmation popup that the
/// "Save" button goes through, since Ctrl+S always targets the project's
/// own current file rather than an arbitrary new path.
pub fn save_current(app: &mut RakunatorApp) {
    let path = PathBuf::from(&app.project_file_dialog.path_text);
    do_save(app, &path);
}

/// Opens a native "Save As" file picker and saves there once a location is
/// chosen — for the "Save as..." button and the Ctrl+Shift+S shortcut.
/// Skips our own overwrite-confirmation popup (unlike the typed-path
/// "Save" button): the native dialog already asks to confirm overwriting
/// an existing file itself.
pub fn save_as(app: &mut RakunatorApp) {
    let starting_dir = PathBuf::from(&app.project_file_dialog.path_text);
    let dialog = rfd::FileDialog::new().add_filter("Rakunator Project", &["raku"]);
    let dialog = match starting_dir.parent() {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    };
    if let Some(path) = dialog.save_file() {
        app.project_file_dialog.path_text = path.display().to_string();
        do_save(app, &path);
    }
}

/// Loads `path` as the project, replacing whatever's currently open, and
/// closes the dialog immediately on success (unlike a save, there's no
/// status line worth lingering on — the loaded project is now just what's
/// on screen).
fn do_load(app: &mut RakunatorApp, path: &std::path::Path) {
    let now = timestamp();
    match project::persistence::load_project(path) {
        Ok(loaded) => {
            replace_project(app, loaded);
            app.project_name = file_stem(path);
            app.project_file_dialog.status = None;
            app.project_file_dialog.open = false;
            toast::show(app, format!("Loaded {}", display_file_name(path)));
        }
        Err(e) => {
            app.project_file_dialog.status = Some(format!("Load failed at {now}: {e}"));
        }
    }
}

/// Serializes the current project to `path` — the actual save, run either
/// directly (the target didn't already exist) or after
/// `draw_overwrite_confirm`. On success, closes the dialog immediately and
/// shows a toast (rather than lingering on an in-dialog status line the
/// way a failure does, below, since there's nothing left to look at once
/// it's closed).
fn do_save(app: &mut RakunatorApp, path: &std::path::Path) {
    let snapshot = app.project.lock().unwrap().clone();
    let result = project::persistence::save_project(&snapshot, path);
    match result {
        Ok(()) => {
            app.project_name = file_stem(path);
            app.project_file_dialog.status = None;
            app.project_file_dialog.open = false;
            toast::show(app, format!("Saved {}", display_file_name(path)));
        }
        Err(e) => {
            let now = timestamp();
            app.project_file_dialog.status = Some(format!("Save failed at {now}: {e}"));
        }
    }
}

/// Current wall-clock time (HH:MM:SS), so repeated Save/Load presses show
/// a visibly different status message even when the path is unchanged.
fn timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

/// `path`'s file name (with extension) for a short human-facing message —
/// e.g. the toast shown after Ctrl+S or loading a project — falling back to
/// "project" for the rare path with no file name component at all.
fn display_file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string()
}

fn file_stem(path: &std::path::Path) -> Option<String> {
    path.file_stem().and_then(|s| s.to_str()).map(str::to_string)
}

fn replace_project(app: &mut RakunatorApp, loaded: Project) {
    app.engine.stop();
    *app.project.lock().unwrap() = loaded;
}
