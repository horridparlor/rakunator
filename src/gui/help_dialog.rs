use super::RakunatorApp;
use crate::export::config::APP_VERSION;

/// Draws the "Help" window: a readable reference of every shortcut and
/// mouse interaction the app supports, grouped by topic.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.help_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Help")
        .open(&mut open)
        .default_width(480.0)
        .max_height(600.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                section(ui, "Playback");
                row(ui, "Space", "Play/Pause — pausing jumps back to where playback started");
                row(ui, "Click the ruler / a track lane", "Seek the playhead there");
                row(ui, "Zoom In / Zoom Out (toolbar)", "Zoom the timeline");
                row(ui, "Ctrl+Scroll on the timeline", "Zoom in/out (may be intercepted by some window managers — use the buttons if so)");
                row(ui, "Shift+Scroll on the ruler", "Pan horizontally");
                row(ui, "Horizontal scroll (trackpad swipe / tilt-wheel) on the ruler or a track", "Pan horizontally, no modifier needed");
                row(ui, "Shift+Scroll on a track", "Zoom that track's waveform vertically, to see quiet detail (visual only)");
                row(ui, "Scroll over the Pan / Vol slider", "Nudge it by one step (5% / 0.05)");
                row(ui, "Bottom scrollbar", "Click/drag to scroll through the song");

                section(ui, "Recording");
                row(
                    ui,
                    "\u{25cf} (toolbar, next to Play) / \"R\"",
                    "Capture the default microphone from wherever the playhead is, onto the selected track if exactly one is selected, otherwise a new track; click it again, or press R or Space, to stop (hover it to see elapsed time)",
                );
                row(ui, "While recording", "Playback runs so you can hear/see existing tracks as you record; a live waveform strip below the toolbar shows the input level, turning red with a \"CLIPPING\" warning if it hits full scale; the rest of the UI is locked until you stop");
                row(ui, "Stopping a recording", "Playback stops and the playhead returns to where the recording started");

                section(ui, "Selecting clips & tracks");
                row(ui, "Click the top half of a clip", "Select just that clip");
                row(ui, "Click the bottom half of a clip", "Move the playhead there, like clicking the lane behind it");
                row(ui, "Shift+Click a clip", "Add/remove that clip from the selection");
                row(ui, "Shift+Drag empty timeline space", "Marquee-select every clip the box touches");
                row(ui, "Click a track's empty header area", "Select the whole track");
                row(ui, "Shift+Click a track header", "Add/remove that track from the selection");
                row(ui, "Shift+Left / Shift+Right", "Jump the selected clip(s) to the very start / right after the last clip, or move the playhead there if nothing's selected");

                section(ui, "Editing clips");
                row(ui, "Drag a clip", "Move it in time (drop on another track to move it there)");
                row(ui, "Ctrl+Drag a clip", "Duplicate it instead of moving it");
                row(ui, "Drag a clip's left/right edge", "Trim it smaller — drag back out to recover trimmed audio");
                row(ui, "Left / Right arrows", "Nudge the selected clip(s) in time by a small step, or move the playhead if nothing's selected");
                row(ui, "\"S\"", "Split the selected clip(s) at the playhead");
                row(ui, "\"I\" while hovering a clip", "Split that clip at the playhead (even if it isn't selected)");
                row(ui, "Ctrl+J", "Join the selected clips (on the same track) into one");
                row(ui, "Ctrl+X / Ctrl+C / Ctrl+V", "Cut / Copy / Paste the selected clip(s)");
                row(ui, "Delete / Backspace", "Delete the selected track(s), or the selected clip(s) if no track is selected");
                row(ui, "Ctrl+D", "Duplicate the selected clip(s) in place");
                row(ui, "Right-click a clip", "Cut/Copy/Duplicate/Split/\"Duplicate to track\" menu");
                row(ui, "Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo");

                section(ui, "Fade & mute");
                row(ui, "Ctrl+F / Ctrl+Shift+F", "Fade in/out the selected track(s) or clip(s)");
                row(ui, "Ctrl+L", "Mute the selected track(s) or clip(s)");

                section(ui, "Effects (toolbar)");
                row(ui, "Pitch Up/Down", "Resample the clip (tape-speed pitch shift, changes duration too)");
                row(ui, "Volume Up/Down", "Adjust gain by a configurable step (in dB)");
                row(ui, "\"Edit steps...\"", "Set Up/Down step sizes independently for pitch and volume");
                row(ui, "Ctrl+R", "Repeat the last-used pitch/volume effect (not fade — it has its own shortcut)");
                row(ui, "Effects apply to", "Every clip on the selected track(s) if any are selected, otherwise the clip selection");

                section(ui, "Tracks");
                row(ui, "Add Track (toolbar) / Ctrl+N", "Add a new empty track");
                row(ui, "Track \"...\" menu", "Move up/down/top/bottom/by N, duplicate, reset pan & volume, delete");
                row(ui, "Pan slider", "5% steps — left/right balance on stereo tracks, equal-power pan on mono tracks");
                row(ui, "Mute (M) / Solo (S)", "Standard mixing controls — soloing any track mutes all non-soloed ones");
                row(ui, "Stereo tracks", "Show the left channel's waveform on top and the right channel's below it, in the same clip");
                row(ui, "Track \"...\" menu \u{2192} Split to mono", "Splits a stereo track into two new mono tracks (L / R)");
                row(ui, "Track \"...\" menu \u{2192} Merge with track below", "Combines this mono track with the mono track below it into one stereo track");

                section(ui, "Files");
                row(ui, "Create Wave... (toolbar)", "Generate a sine/square/triangle/sawtooth clip, or import an audio file");
                row(ui, "Drag a .wav file onto the window", "Import it as a new track (stereo files import as a stereo track)");
                row(ui, "Project File... (toolbar)", "Save/Load a .raku project file, with a native file picker");
                row(ui, "Export Project... (toolbar)", "Render the full mixdown to .wav/.mp3 under a name you choose");

                ui.add_space(12.0);
                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.weak(format!("Rakunator v{APP_VERSION} \u{2014} \u{00A9}2026 Rakuel"));
                });
            });
        });

    app.help_open = open;
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.heading(title);
    ui.separator();
}

fn row(ui: &mut egui::Ui, action: &str, description: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(action).strong().monospace());
        ui.label(format!(" — {description}"));
    });
}
