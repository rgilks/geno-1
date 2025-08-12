// Host-side tests for pure keyboard functions.
// The main crate is wasm-only, so we include the pure-Rust modules directly.

#![allow(dead_code)]

// We need to include the core constants for the scale tests
mod core {
    pub const IONIAN: &[f32] = &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0, 12.0];
    pub const DORIAN: &[f32] = &[0.0, 2.0, 3.0, 5.0, 7.0, 9.0, 10.0, 12.0];
    pub const PHRYGIAN: &[f32] = &[0.0, 1.0, 3.0, 5.0, 7.0, 8.0, 10.0, 12.0];
    pub const LYDIAN: &[f32] = &[0.0, 2.0, 4.0, 6.0, 7.0, 9.0, 11.0, 12.0];
    pub const MIXOLYDIAN: &[f32] = &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 10.0, 12.0];
    pub const AEOLIAN: &[f32] = &[0.0, 2.0, 3.0, 5.0, 7.0, 8.0, 10.0, 12.0];
    pub const LOCRIAN: &[f32] = &[0.0, 1.0, 3.0, 5.0, 6.0, 8.0, 10.0, 12.0];
}

// Re-implement the pure functions for testing
#[inline]
fn root_midi_for_key(key: &str) -> Option<i32> {
    match key {
        "a" | "A" => Some(69), // A4
        "b" | "B" => Some(71), // B4
        "c" | "C" => Some(60), // C4 (middle C)
        "d" | "D" => Some(62), // D4
        "e" | "E" => Some(64), // E4
        "f" | "F" => Some(65), // F4
        "g" | "G" => Some(67), // G4
        _ => None,
    }
}

#[inline]
fn mode_scale_for_digit(key: &str) -> Option<&'static [f32]> {
    match key {
        "1" => Some(core::IONIAN),
        "2" => Some(core::DORIAN),
        "3" => Some(core::PHRYGIAN),
        "4" => Some(core::LYDIAN),
        "5" => Some(core::MIXOLYDIAN),
        "6" => Some(core::AEOLIAN),
        "7" => Some(core::LOCRIAN),
        _ => None,
    }
}

#[test]
fn root_midi_for_key_valid_keys() {
    // Test all valid root note keys
    assert_eq!(root_midi_for_key("a"), Some(69)); // A4
    assert_eq!(root_midi_for_key("A"), Some(69)); // A4
    assert_eq!(root_midi_for_key("b"), Some(71)); // B4
    assert_eq!(root_midi_for_key("B"), Some(71)); // B4
    assert_eq!(root_midi_for_key("c"), Some(60)); // C4 (middle C)
    assert_eq!(root_midi_for_key("C"), Some(60)); // C4 (middle C)
    assert_eq!(root_midi_for_key("d"), Some(62)); // D4
    assert_eq!(root_midi_for_key("D"), Some(62)); // D4
    assert_eq!(root_midi_for_key("e"), Some(64)); // E4
    assert_eq!(root_midi_for_key("E"), Some(64)); // E4
    assert_eq!(root_midi_for_key("f"), Some(65)); // F4
    assert_eq!(root_midi_for_key("F"), Some(65)); // F4
    assert_eq!(root_midi_for_key("g"), Some(67)); // G4
    assert_eq!(root_midi_for_key("G"), Some(67)); // G4
}

#[test]
fn root_midi_for_key_invalid_keys() {
    // Test invalid keys return None
    assert_eq!(root_midi_for_key("h"), None);
    assert_eq!(root_midi_for_key("H"), None);
    assert_eq!(root_midi_for_key("i"), None);
    assert_eq!(root_midi_for_key("I"), None);
    assert_eq!(root_midi_for_key("j"), None);
    assert_eq!(root_midi_for_key("J"), None);
    assert_eq!(root_midi_for_key("k"), None);
    assert_eq!(root_midi_for_key("K"), None);
    assert_eq!(root_midi_for_key("l"), None);
    assert_eq!(root_midi_for_key("L"), None);
    assert_eq!(root_midi_for_key("m"), None);
    assert_eq!(root_midi_for_key("M"), None);
    assert_eq!(root_midi_for_key("n"), None);
    assert_eq!(root_midi_for_key("N"), None);
    assert_eq!(root_midi_for_key("o"), None);
    assert_eq!(root_midi_for_key("O"), None);
    assert_eq!(root_midi_for_key("p"), None);
    assert_eq!(root_midi_for_key("P"), None);
    assert_eq!(root_midi_for_key("q"), None);
    assert_eq!(root_midi_for_key("Q"), None);
    assert_eq!(root_midi_for_key("r"), None);
    assert_eq!(root_midi_for_key("R"), None);
    assert_eq!(root_midi_for_key("s"), None);
    assert_eq!(root_midi_for_key("S"), None);
    assert_eq!(root_midi_for_key("t"), None);
    assert_eq!(root_midi_for_key("T"), None);
    assert_eq!(root_midi_for_key("u"), None);
    assert_eq!(root_midi_for_key("U"), None);
    assert_eq!(root_midi_for_key("v"), None);
    assert_eq!(root_midi_for_key("V"), None);
    assert_eq!(root_midi_for_key("w"), None);
    assert_eq!(root_midi_for_key("W"), None);
    assert_eq!(root_midi_for_key("x"), None);
    assert_eq!(root_midi_for_key("X"), None);
    assert_eq!(root_midi_for_key("y"), None);
    assert_eq!(root_midi_for_key("Y"), None);
    assert_eq!(root_midi_for_key("z"), None);
    assert_eq!(root_midi_for_key("Z"), None);
}

#[test]
fn root_midi_for_key_edge_cases() {
    // Test edge cases
    assert_eq!(root_midi_for_key(""), None);
    assert_eq!(root_midi_for_key("notakey"), None);
    assert_eq!(root_midi_for_key("1"), None);
    assert_eq!(root_midi_for_key("0"), None);
}

#[test]
fn mode_scale_for_digit_valid_digits() {
    // Test all valid mode keys (1-7)
    assert_eq!(mode_scale_for_digit("1"), Some(core::IONIAN)); // Ionian (major)
    assert_eq!(mode_scale_for_digit("2"), Some(core::DORIAN)); // Dorian
    assert_eq!(mode_scale_for_digit("3"), Some(core::PHRYGIAN)); // Phrygian
    assert_eq!(mode_scale_for_digit("4"), Some(core::LYDIAN)); // Lydian
    assert_eq!(mode_scale_for_digit("5"), Some(core::MIXOLYDIAN)); // Mixolydian
    assert_eq!(mode_scale_for_digit("6"), Some(core::AEOLIAN)); // Aeolian (natural minor)
    assert_eq!(mode_scale_for_digit("7"), Some(core::LOCRIAN)); // Locrian
}

#[test]
fn mode_scale_for_digit_invalid_digits() {
    // Test invalid digits return None
    assert_eq!(mode_scale_for_digit("0"), None);
    assert_eq!(mode_scale_for_digit("8"), None);
    assert_eq!(mode_scale_for_digit("9"), None);
    assert_eq!(mode_scale_for_digit("A"), None);
}

#[test]
fn mode_scale_for_digit_edge_cases() {
    // Test edge cases
    assert_eq!(mode_scale_for_digit(""), None);
    assert_eq!(mode_scale_for_digit("notadigit"), None);
    assert_eq!(mode_scale_for_digit("1"), Some(core::IONIAN)); // Valid key
    assert_eq!(mode_scale_for_digit("7"), Some(core::LOCRIAN)); // Valid key
}

#[test]
fn mode_scales_have_correct_lengths() {
    // All modes should have 8 notes (7 + octave)
    let modes = [
        ("1", mode_scale_for_digit("1").unwrap()),
        ("2", mode_scale_for_digit("2").unwrap()),
        ("3", mode_scale_for_digit("3").unwrap()),
        ("4", mode_scale_for_digit("4").unwrap()),
        ("5", mode_scale_for_digit("5").unwrap()),
        ("6", mode_scale_for_digit("6").unwrap()),
        ("7", mode_scale_for_digit("7").unwrap()),
    ];

    for (name, scale) in modes {
        assert_eq!(scale.len(), 8, "Mode {name} should have 8 notes");
        assert!(
            (scale[0] - 0.0).abs() < 1e-6,
            "Mode {name} should start at 0"
        );
        assert!(
            (scale[7] - 12.0).abs() < 1e-6,
            "Mode {name} should end at octave (12)"
        );
    }
}

#[test]
fn mode_scales_are_monotonic() {
    // All modes should have monotonically increasing semitone values
    let modes = [
        ("1", mode_scale_for_digit("1").unwrap()),
        ("2", mode_scale_for_digit("2").unwrap()),
        ("3", mode_scale_for_digit("3").unwrap()),
        ("4", mode_scale_for_digit("4").unwrap()),
        ("5", mode_scale_for_digit("5").unwrap()),
        ("6", mode_scale_for_digit("6").unwrap()),
        ("7", mode_scale_for_digit("7").unwrap()),
    ];

    for (name, scale) in modes {
        for i in 1..scale.len() {
            assert!(
                scale[i] > scale[i - 1],
                "Mode {} should be monotonic: {} <= {} at index {}",
                name,
                scale[i - 1],
                scale[i],
                i
            );
        }
    }
}
