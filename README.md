# Rakunator

A small multi-track, Audacity-style digital audio workstation (DAW), built
as a Rust desktop app with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui).

Create/import audio onto any number of named tracks (mono or stereo), then
arrange, trim, split, join, fade, pitch-shift, and mix them on a
zoomable timeline, and export the result to `.wav` or `.mp3`. Projects can
be saved to and loaded from `.raku` files.

Open the in-app **Help** menu (toolbar) for a full, up-to-date list of
every shortcut and mouse interaction — it's the authoritative reference
for day-to-day use. This README covers building, running, and testing.

## Features at a glance

- Any number of tracks — add, rename, reorder (up/down/top/bottom/by N),
  duplicate, delete, mute/solo, pan (5% steps), volume, live level meters.
- Stereo tracks: dual left/right waveform display, "Split to mono" and
  "Merge with track below" in the track menu.
- Timeline: zoom (buttons or Ctrl+scroll), horizontal scroll (Shift+scroll
  or the bottom scrollbar), click-to-seek, drag/trim/split/join clips,
  cross-track clip-edge snapping.
- Selection: click, Shift+click, click-drag marquee, ranged (partial-clip)
  selection for targeted fades/mutes, multi-track selection.
- Editing: cut/copy/paste/duplicate, undo/redo, drag-and-drop `.wav`
  import, native file pickers for import/export/project save-load.
- Effects: pitch shift, volume, fade in/out, adjustable (dB-configurable)
  fade in/out, fade-toggle (alternating fades across a track's clips),
  all with independently configurable step sizes and a "repeat last
  effect" shortcut.
- Export to `.wav`/`.mp3` under a name you choose; save/load full
  projects as `.raku` (JSON) files.

## Tools & technologies used

| Crate | Purpose |
|---|---|
| [`egui`](https://crates.io/crates/egui) / [`eframe`](https://crates.io/crates/eframe) | Immediate-mode GUI toolkit and application shell |
| [`cpal`](https://crates.io/crates/cpal) | Cross-platform audio output (playback) |
| [`ringbuf`](https://crates.io/crates/ringbuf) | Lock-free ring buffer between the mixer and the audio callback |
| [`hound`](https://crates.io/crates/hound) | WAV read/write |
| [`mp3lame-encoder`](https://crates.io/crates/mp3lame-encoder) | MP3 export |
| [`rfd`](https://crates.io/crates/rfd) | Native "open/save file" dialogs |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | `.raku` project file (de)serialization |
| [`chrono`](https://crates.io/crates/chrono) | Timestamps shown in save/load status messages |
| [`dirs`](https://crates.io/crates/dirs) | Locating a sensible default folder for exports |

Written in Rust (2024 edition).

## Running the project

Requires a recent stable [Rust toolchain](https://rustup.rs/).

```sh
cargo run
```

For a faster (optimized) build:

```sh
cargo run --release
```

## Running the tests

```sh
cargo test
```

This runs the integration test suite under `tests/` — covering the core
editing model (tracks/clips/effects/undo-redo), the mixing/panning math,
and `.raku` save/load round-tripping. These tests are hermetic (no audio
device or window needed), so they run fine in CI or headless environments.

It's also worth periodically checking for lints:

```sh
cargo clippy --all-targets
```

## Building

### Linux

Native build (run on the machine you'll use it on):

```sh
cargo build --release
```

The binary is written to `target/release/rakunator`. Linux builds need the
usual GUI/audio dev headers available on most desktop distros already —
if `cargo build` fails on missing system libraries, install your distro's
ALSA (`libasound2-dev` on Debian/Ubuntu) and, if you're on Wayland/X11,
the corresponding windowing dev packages.

### Windows

The simplest, most reliable option is a **native build on Windows** itself
(recommended): install [Rust via rustup](https://rustup.rs/) with the MSVC
toolchain, then from a checkout of this repo:

```sh
cargo build --release
```

The binary is written to `target\release\rakunator.exe`.

**Cross-compiling from Linux to Windows** is possible but more fragile,
since this is a GUI + audio app pulling in OS-specific backends
(`cpal`/`rfd`) rather than a plain CLI tool — expect to troubleshoot. One
path, targeting the GNU ABI:

```sh
rustup target add x86_64-pc-windows-gnu
# Debian/Ubuntu: sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

This has not been verified in this project's CI/dev environment — if it
doesn't work out of the box, building natively on Windows (or in a
Windows VM/CI runner) is the dependable fallback.

## Project layout

- `src/audio_engine/` — realtime playback: transport, producer/consumer
  ring-buffer handoff, per-track level metering, and the pure mixing/
  panning math (`mix.rs`) shared by both live playback and export.
- `src/project/` — the core editing model: `Project`/`Track`/`Clip`,
  undo/redo, `.raku` save/load (`persistence.rs`), WAV import
  (`import.rs`), and generated test waveforms (`generate.rs`).
- `src/export/` — renders a project to `.wav`/`.mp3`.
- `src/gui/` — the egui-based UI: toolbar, timeline/track views, and the
  various dialogs (Create Wave, Export, Project File, Effects, Help).
- `tests/` — integration tests over `src/project` and
  `src/audio_engine::mix`.
