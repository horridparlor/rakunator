fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Rakunator",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(rakunator::gui::RakunatorApp::new(cc)))),
    )
}
