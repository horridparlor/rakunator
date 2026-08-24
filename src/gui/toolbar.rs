use super::RakunatorApp;

pub fn draw(ui: &mut egui::Ui, app: &mut RakunatorApp) {
    ui.horizontal(|ui| {
        if ui.button("Add Track").clicked() {
            app.project.lock().unwrap().add_track();
        }
        ui.separator();
        if ui.button("Create Wave...").clicked() {
            app.wave_dialog.open = true;
        }
        if ui.button("Export Project...").clicked() {
            app.export_dialog.open = true;
        }
        ui.separator();
        if ui.button("Play").clicked() {
            app.engine.play();
        }
        if ui.button("Pause").clicked() {
            app.engine.pause();
        }
        if ui.button("Stop").clicked() {
            app.engine.stop();
        }
    });
}
