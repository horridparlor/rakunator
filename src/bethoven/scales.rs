//! Scale reference data (from the project's music-scales reference: 43
//! scale types, each a formula of semitone offsets from the root). Roots
//! and pitch classes are normalized to sharps (C, C#, D, D#, E, F, F#, G,
//! G#, A, A#, B), matching that reference's own normalization — a scale's
//! actual notes for any root are computed by rotation, never parsed from
//! the source document at runtime.

/// Root note names, index 0..=11 = C..B (sharps only).
pub const ROOT_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Every scale type: `(name, semitone offsets from the root)`. Grouped in
/// the same order/categories as the reference document, though the
/// category headers themselves aren't needed at runtime.
pub const SCALES: &[(&str, &[i8])] = &[
    // Diatonic scales & modes
    ("Major (Ionian)", &[0, 2, 4, 5, 7, 9, 11]),
    ("Dorian", &[0, 2, 3, 5, 7, 9, 10]),
    ("Phrygian", &[0, 1, 3, 5, 7, 8, 10]),
    ("Lydian", &[0, 2, 4, 6, 7, 9, 11]),
    ("Mixolydian", &[0, 2, 4, 5, 7, 9, 10]),
    ("Natural Minor (Aeolian)", &[0, 2, 3, 5, 7, 8, 10]),
    ("Locrian", &[0, 1, 3, 5, 6, 8, 10]),
    // Minor-family scales
    ("Harmonic Minor", &[0, 2, 3, 5, 7, 8, 11]),
    ("Melodic Minor (ascending / jazz)", &[0, 2, 3, 5, 7, 9, 11]),
    ("Dorian b2", &[0, 1, 3, 5, 7, 9, 10]),
    ("Lydian Augmented", &[0, 2, 4, 6, 8, 9, 11]),
    ("Lydian Dominant", &[0, 2, 4, 6, 7, 9, 10]),
    ("Mixolydian b6", &[0, 2, 4, 5, 7, 8, 10]),
    ("Locrian #2", &[0, 2, 3, 5, 6, 8, 10]),
    ("Altered / Super Locrian", &[0, 1, 3, 4, 6, 8, 10]),
    // Pentatonic & blues scales
    ("Major Pentatonic", &[0, 2, 4, 7, 9]),
    ("Minor Pentatonic", &[0, 3, 5, 7, 10]),
    ("Major Blues", &[0, 2, 3, 4, 7, 9]),
    ("Minor Blues", &[0, 3, 5, 6, 7, 10]),
    ("Egyptian / Suspended Pentatonic", &[0, 2, 5, 7, 10]),
    ("Hirajoshi", &[0, 2, 3, 7, 8]),
    ("In Sen", &[0, 1, 5, 7, 10]),
    ("Iwato", &[0, 1, 5, 6, 10]),
    // Symmetric & synthetic scales
    ("Chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
    ("Whole Tone", &[0, 2, 4, 6, 8, 10]),
    ("Diminished - Whole/Half", &[0, 2, 3, 5, 6, 8, 9, 11]),
    ("Diminished - Half/Whole", &[0, 1, 3, 4, 6, 7, 9, 10]),
    ("Augmented", &[0, 3, 4, 7, 8, 11]),
    ("Tritone", &[0, 1, 4, 6, 7, 10]),
    // Bebop & jazz scales
    ("Bebop Dominant", &[0, 2, 4, 5, 7, 9, 10, 11]),
    ("Bebop Major", &[0, 2, 4, 5, 7, 8, 9, 11]),
    ("Bebop Dorian", &[0, 2, 3, 4, 5, 7, 9, 10]),
    ("Bebop Melodic Minor", &[0, 2, 3, 5, 7, 8, 9, 11]),
    // Common world / folk / exotic scales
    ("Phrygian Dominant", &[0, 1, 4, 5, 7, 8, 10]),
    ("Double Harmonic Major / Byzantine", &[0, 1, 4, 5, 7, 8, 11]),
    ("Hungarian Minor", &[0, 2, 3, 6, 7, 8, 11]),
    ("Hungarian Major", &[0, 3, 4, 6, 7, 9, 10]),
    ("Neapolitan Minor", &[0, 1, 3, 5, 7, 8, 11]),
    ("Neapolitan Major", &[0, 1, 3, 5, 7, 9, 11]),
    ("Persian", &[0, 1, 4, 5, 6, 8, 11]),
    ("Enigmatic", &[0, 1, 4, 6, 8, 10, 11]),
    ("Prometheus", &[0, 2, 4, 6, 9, 10]),
    ("Acoustic / Overtone", &[0, 2, 4, 6, 7, 9, 10]),
];

/// The 12 pitch classes (C..B) that belong to `scale_index`'s formula
/// rooted at `root` (0..=11). Out-of-range `scale_index` yields no pitch
/// classes at all (nothing shown/placeable), rather than panicking.
pub fn pitch_classes(root: u8, scale_index: usize) -> [bool; 12] {
    let mut pcs = [false; 12];
    if let Some((_, intervals)) = SCALES.get(scale_index) {
        for &iv in *intervals {
            let pc = (root as i32 + iv as i32).rem_euclid(12) as usize;
            pcs[pc] = true;
        }
    }
    pcs
}

/// Whether `midi_note` (any octave) is a member of `scale_index` rooted at
/// `root`.
pub fn is_in_scale(root: u8, scale_index: usize, midi_note: u8) -> bool {
    pitch_classes(root, scale_index)[(midi_note as usize) % 12]
}

/// Display name for `midi_note`, e.g. 60 -> "C4" (using the sharps-only
/// naming from `ROOT_NAMES`, middle C = C4 per the common MIDI convention
/// where octave = midi_note / 12 - 1).
pub fn note_name(midi_note: u8) -> String {
    let pc = (midi_note % 12) as usize;
    let octave = (midi_note as i32) / 12 - 1;
    format!("{}{}", ROOT_NAMES[pc], octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_scale_count_matches_reference() {
        assert_eq!(SCALES.len(), 43);
    }

    #[test]
    fn c_major_pitch_classes() {
        let pcs = pitch_classes(0, 0); // C, Major (Ionian)
        // C D E F G A B -> pitch classes 0,2,4,5,7,9,11
        let expected = [0, 2, 4, 5, 7, 9, 11];
        for i in 0..12u8 {
            assert_eq!(pcs[i as usize], expected.contains(&i), "pitch class {i}");
        }
    }

    #[test]
    fn a_natural_minor_matches_c_major_relative() {
        // A Natural Minor (Aeolian) shares the same pitch classes as C Major
        // (relative minor).
        let a_index = SCALES.iter().position(|(n, _)| *n == "Natural Minor (Aeolian)").unwrap();
        let a_minor = pitch_classes(9, a_index); // root = A
        let c_major = pitch_classes(0, 0);
        assert_eq!(a_minor, c_major);
    }

    #[test]
    fn e_phrygian_pitch_classes() {
        let idx = SCALES.iter().position(|(n, _)| *n == "Phrygian").unwrap();
        let pcs = pitch_classes(4, idx); // root E
        // E F G A B C D -> pitch classes 4,5,7,9,11,0,2
        let expected = [4, 5, 7, 9, 11, 0, 2];
        for i in 0..12u8 {
            assert_eq!(pcs[i as usize], expected.contains(&i), "pitch class {i}");
        }
    }

    #[test]
    fn note_name_middle_c() {
        assert_eq!(note_name(60), "C4");
    }
}
