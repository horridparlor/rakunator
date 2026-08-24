mod audio_engine;
mod export;
mod gui;
mod project;
mod waveform;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Rakunator",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(gui::RakunatorApp::new()))),
    )
}
