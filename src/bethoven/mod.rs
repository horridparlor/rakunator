//! Bethoven: the scale-constrained piano-roll composer. This module is the
//! headless data model (scales, instruments, notes/sections/melodies) and
//! their synthesis/mixdown — the GUI half lives in `crate::gui::bethoven`.

pub mod instrument;
pub mod melody;
pub mod scales;

pub use instrument::Instrument;
pub use melody::{Melody, Note, Section};
