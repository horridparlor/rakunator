fn main() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    options.viewport = options.viewport.with_icon(rakunator::gui::icon::app_icon());

    // winit's Wayland backend doesn't emit DroppedFile/HoveredFile events
    // (only X11, macOS and Windows do), so dragging a .wav from the file
    // manager silently does nothing on a native Wayland session. Forcing
    // the X11 backend runs the window through XWayland instead, which
    // every mainstream Linux desktop still provides, and keeps
    // drag-and-drop import working regardless of session type.
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_x11();
        }));
    }

    eframe::run_native(
        "Rakunator",
        options,
        Box::new(|cc| Ok(Box::new(rakunator::gui::RakunatorApp::new(cc)))),
    )
}
