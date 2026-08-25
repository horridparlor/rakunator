use super::RakunatorApp;
use crate::export::config::APP_VERSION;

/// Draws the "Help" window: a searchable reference of every shortcut,
/// mouse interaction, and effect the app supports, grouped by topic.
pub fn draw(ctx: &egui::Context, app: &mut RakunatorApp) {
    if !app.help_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Help")
        .open(&mut open)
        .default_width(520.0)
        .max_height(640.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                // Right-to-left so the Clear button claims its width first
                // (from the right edge inward); the text edit then fills
                // exactly what's left via `available_width()`, rather than
                // the old left-to-right order where the text edit's
                // `desired_width(INFINITY)` grabbed the *entire* row first
                // and the button got tacked on after it — overflowing the
                // row (and, since the window sizes to fit its content, the
                // whole window) by one button's width, which is why the
                // Clear button never lined up under the window's own close
                // button above it.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !app.help_search.is_empty() && ui.button("\u{2715}").on_hover_text("Clear").clicked() {
                        app.help_search.clear();
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut app.help_search)
                            .hint_text("shortcut, effect, or keyword...")
                            .desired_width(ui.available_width()),
                    );
                });
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let query = app.help_search.trim().to_lowercase();
                let mut any_match = false;

                for entry in help_sections() {
                    // A query matching the section title (e.g. "Recording")
                    // shows every row under it, not just whichever rows
                    // happen to also mention the word themselves.
                    let title_matches = !query.is_empty() && entry.title.to_lowercase().contains(&query);
                    let matches: Vec<&(&str, &str)> = entry
                        .rows
                        .iter()
                        .filter(|(action, description)| {
                            query.is_empty()
                                || title_matches
                                || action.to_lowercase().contains(&query)
                                || description.to_lowercase().contains(&query)
                        })
                        .collect();
                    if matches.is_empty() {
                        continue;
                    }
                    any_match = true;
                    section(ui, entry.title);
                    for (action, description) in matches {
                        row(ui, action, description);
                    }
                }

                if !any_match {
                    ui.add_space(12.0);
                    ui.weak(format!("No shortcuts or effects match \"{}\".", app.help_search.trim()));
                }

                ui.add_space(12.0);
                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.weak(format!("Rakunator v{APP_VERSION} \u{2014} \u{00A9}2026 Rakuel"));
                });
            });
        });

    app.help_open = open;
}

struct HelpSection {
    title: &'static str,
    rows: &'static [(&'static str, &'static str)],
}

/// All Help content, as plain data — kept separate from drawing so the
/// search box above can filter rows (and skip whole sections that don't
/// match) without needing a parallel "does this match?" pass through a
/// pile of imperative `ui.label(...)` calls.
fn help_sections() -> &'static [HelpSection] {
    &[
        HelpSection {
            title: "Playback",
            rows: &[
                ("Space", "Play/Pause — pausing jumps back to where playback started"),
                ("F11", "Toggle fullscreen"),
                ("Ctrl+M", "Toggle maximize/restore the window"),
                ("Ctrl+Escape", "Quit the application"),
                (
                    "Click the ruler / a track lane",
                    "Seek the playhead there — snaps to a nearby clip edge if one's close (yellow flash), Audacity-style",
                ),
                ("Zoom In / Zoom Out (toolbar)", "Zoom the timeline"),
                (
                    "Ctrl+Scroll / Alt+Scroll on the timeline",
                    "Zoom in/out (if one is grabbed by your window manager or remote-desktop client, try the other, or use the buttons)",
                ),
                ("Shift+Scroll on the ruler", "Pan horizontally"),
                (
                    "Horizontal scroll (trackpad swipe / tilt-wheel) on the ruler or a track",
                    "Pan horizontally, no modifier needed",
                ),
                (
                    "Shift+Scroll on a track",
                    "Zoom that track's waveform vertically, to see quiet detail (visual only)",
                ),
                ("Scroll over the Pan / Vol slider", "Nudge it by one step (5% / 0.05)"),
                ("Bottom scrollbar", "Click/drag to scroll through the song"),
            ],
        },
        HelpSection {
            title: "Recording",
            rows: &[
                (
                    "\u{25cf} (toolbar, next to Play) / \"R\"",
                    "Capture the default microphone from wherever the playhead is, landing on the selected track, the track the current clip selection is on, the track last clicked, or (with none of those) an empty last track — otherwise a new track; click it again, or press R or Space, to stop (hover it to see elapsed time)",
                ),
                (
                    "While recording",
                    "Playback runs so you can hear/see existing tracks as you record; the capture grows live on its own track's lane, turning amber then red if it hits full scale, and the timeline follows it as it grows; the rest of the UI is locked until you stop",
                ),
                ("Stopping a recording", "Playback stops and the playhead returns to where the recording started"),
            ],
        },
        HelpSection {
            title: "Selecting clips & tracks",
            rows: &[
                ("Click the top half of a clip", "Select just that clip"),
                ("Click the bottom half of a clip", "Move the playhead there, like clicking the lane behind it"),
                ("Shift+Click a clip", "Add/remove that clip from the selection"),
                ("Shift+Drag empty timeline space", "Marquee-select every clip the box touches"),
                ("Click a track's empty header area", "Select the whole track"),
                ("Shift+Click a track header", "Add/remove that track from the selection"),
                (
                    "Shift+Left / Shift+Right",
                    "Jump the selected clip(s) to the very start / right after the last clip, or move the playhead there if nothing's selected",
                ),
            ],
        },
        HelpSection {
            title: "Editing clips",
            rows: &[
                ("Drag a clip", "Move it in time (drop on another track to move it there)"),
                ("Ctrl+Drag a clip", "Duplicate it instead of moving it"),
                ("Drag a clip's left/right edge", "Trim it smaller — drag back out to recover trimmed audio"),
                (
                    "Left / Right arrows",
                    "Nudge the selected clip(s) in time by a small step, or move the playhead if nothing's selected",
                ),
                ("\"S\"", "Split the selected clip(s) at the playhead"),
                ("\"I\" while hovering a clip", "Split that clip at the playhead (even if it isn't selected)"),
                ("Ctrl+J", "Join the selected clips (on the same track) into one"),
                ("Ctrl+X / Ctrl+C / Ctrl+V", "Cut / Copy / Paste the selected clip(s)"),
                ("Delete / Backspace", "Delete the selected track(s), or the selected clip(s) if no track is selected"),
                ("Ctrl+D (with clip(s) selected)", "Duplicate the selected clip(s) in place"),
                ("Right-click a clip", "Cut/Copy/Duplicate/Split/\"Duplicate to track\" menu"),
                ("Ctrl+Z / Ctrl+Shift+Z", "Undo / Redo"),
            ],
        },
        HelpSection {
            title: "Fade & mute",
            rows: &[
                ("Ctrl+F / Ctrl+Shift+F", "Fade in/out the selected track(s) or clip(s)"),
                ("Ctrl+L", "Mute the selected track(s) or clip(s)"),
                (
                    "Adjustable Fade In/Out",
                    "Fade between two configurable dB points (Edit steps...) instead of the fixed silence\u{2194}full fades above",
                ),
                (
                    "Fade Toggle",
                    "For each selected track, alternates Adjustable Fade In/Out across its clips in timeline order",
                ),
            ],
        },
        HelpSection {
            title: "Effects — pitch, tempo & volume",
            rows: &[
                ("Pitch Up/Down", "\"Tape speed\" resample — changes pitch and duration together"),
                ("Volume Up/Down", "Adjust gain by a configurable step (in dB)"),
                (
                    "Tempo Up/Down",
                    "WSOLA time-stretch by a configurable percent (Audacity's \"Change Tempo\" convention) — changes duration only, pitch is untouched",
                ),
                (
                    "Sliding Stretch",
                    "Ramps tempo % and pitch (semitones) independently from an initial value (clip start) to a final value (clip end); each knob is fully configurable in Edit steps...",
                ),
                ("Ctrl+R", "Repeat the last-used pitch/volume/tempo effect (not fade — it has its own shortcut)"),
            ],
        },
        HelpSection {
            title: "Effects — EQ & tone",
            rows: &[
                (
                    "Give to Speech",
                    "Cuts -4 dB across roughly 2 kHz-5 kHz to tame harshness in that presence range",
                ),
                (
                    "Telephone",
                    "Bandpass EQ approximating classic telephone bandwidth (~300 Hz-3400 Hz), matching Audacity's \"Telephone\" Equalization preset",
                ),
            ],
        },
        HelpSection {
            title: "Effects — dynamics, space & character",
            rows: &[
                (
                    "Reverb",
                    "Freeverb-style reverb with the same controls as Audacity's Reverb (Room Size, Reverberance, HF Damping, Tone Low/High, Wet/Dry Gain, Stereo Width, Pre-Delay, Wet Only), all configurable in Edit steps...",
                ),
                (
                    "Echo",
                    "Audacity-style recursive echo: output[n] = input[n] + decay \u{d7} output[n - delay]; Delay (seconds) and Decay (0.0-2.0, a decay \u{2265}1.0 builds up rather than fading) are configurable in Edit steps...",
                ),
                (
                    "Distortion (Hard Clip)",
                    "Boosts by a configurable Drive (dB) then hard-clips anything beyond a configurable threshold",
                ),
                (
                    "Autotune",
                    "Runs a full mastering chain in one click: blind noise reduction, an EQ filter curve, normalize, compressor, limiter, then reverb — approximates a specific Audacity effect-chain export using standard equivalents for each stage (not a byte-exact port)",
                ),
                (
                    "Rattle",
                    "Builds pitch/tempo-shifted \"up\" and \"down\" copies of the clip, repeats the pair back-to-back (Repeat Count) into one clip placed right after the original, then applies its own Adjustable Fade In and its own Sliding Stretch — all values configurable in Edit steps..., separate from those effects' regular settings",
                ),
                (
                    "Trip Toggler",
                    "Finds clear quiet \"low points\" in the clip (a smoothed loudness envelope, peaks filtered by prominence and minimum spacing) and alternates a fade down and a fade up across the resulting segments — ported from a Python waveform-editing script, applied to the selection instead of whole files; every detection and fade value (High/Low dB points, Basic/Super detection mode, detail, Gradual/Instant shift mode, and the Instant-mode step/fade values) is configurable in Edit steps..., and it starts High or Low per the same setting, alternating across multiple targeted clips the way the original script alternated across files",
                ),
            ],
        },
        HelpSection {
            title: "Effects — utility",
            rows: &[
                ("Invert", "Multiplies every sample by -1 (phase invert)"),
                ("Reverse", "Reverses the clip's audio in time"),
                ("Swap Channels", "Swaps left/right on a stereo clip; no effect on mono clips"),
                (
                    "Pan Toggle",
                    "Splits a stereo clip's left/right channels apart, ramps one side from a configurable Low dB up to a High dB (fading in) and the other from High down to Low (fading out) across the clip, then recombines them; which side fades in is a configurable direction (Left/Right), and the dB points are its own values in Edit steps... (default +6/-4). No effect on mono clips.",
                ),
            ],
        },
        HelpSection {
            title: "Effects — general",
            rows: &[
                ("\"Edit steps...\"", "Every effect's adjustable values live in one dialog, grouped by effect"),
                ("Effects apply to", "Every clip on the selected track(s) if any are selected, otherwise the clip selection"),
            ],
        },
        HelpSection {
            title: "Tracks",
            rows: &[
                ("Add Track (toolbar) / Ctrl+N", "Add a new empty track"),
                ("Track \"...\" menu", "Move up/down/top/bottom/by N, duplicate, reset pan & volume, delete"),
                ("Ctrl+D (with track(s) selected)", "Duplicate the selected track(s) — each copy lands directly below its original"),
                ("Pan slider", "5% steps — left/right balance on stereo tracks, equal-power pan on mono tracks"),
                ("Mute (M) / Solo (S)", "Standard mixing controls — soloing any track mutes all non-soloed ones"),
                (
                    "Stereo tracks",
                    "Show the left channel's waveform on top and the right channel's below it, in the same clip",
                ),
                ("Track \"...\" menu \u{2192} Split to mono", "Splits a stereo track into two new mono tracks (L / R)"),
                (
                    "Track \"...\" menu \u{2192} Merge with track below",
                    "Combines this mono track with the mono track below it into one stereo track",
                ),
                (
                    "Amber hazard stripe on a lane",
                    "Two or more clips overlap in time there (e.g. after recording landed on top of existing material)",
                ),
            ],
        },
        HelpSection {
            title: "Files",
            rows: &[
                ("Create Wave... (toolbar)", "Generate a sine/square/triangle/sawtooth clip, or import an audio file"),
                ("Drag a .wav file onto the window", "Import it as a new track (stereo files import as a stereo track)"),
                ("Project File... (toolbar)", "Save/Load a .raku project file, with a native file picker"),
                ("Ctrl+S", "Save straight to the project's current file (or the default path, if it's never been saved)"),
                ("Export Project... (toolbar)", "Render the full mixdown to .wav/.mp3 under a name you choose"),
            ],
        },
    ]
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.heading(title);
    ui.separator();
}

fn row(ui: &mut egui::Ui, action: &str, description: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(action).strong().monospace());
        // Explicit `.wrap()` rather than relying on the ambient default, so
        // a long description reliably wraps onto more lines instead of
        // ever reporting an unbroken width the window might size itself to.
        ui.add(egui::Label::new(format!(" — {description}")).wrap());
    });
}
