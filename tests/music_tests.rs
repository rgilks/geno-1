// Host-side integration tests for core music engine.
// The main crate is wasm-only, so we include the pure-Rust module directly.

#![allow(dead_code)]
mod music {
    include!("../src/core/music.rs");
}

use music::*;
use std::time::Duration;

fn make_engine() -> MusicEngine {
    let configs = vec![
        VoiceConfig {
            waveform: Waveform::Sine,
            base_position: glam::Vec3::new(-1.0, 0.0, 0.0),
            trigger_probability: 0.4,
            octave_offset: -1,
            base_duration: 0.4,
        },
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: glam::Vec3::new(1.0, 0.0, 0.0),
            trigger_probability: 0.6,
            octave_offset: 0,
            base_duration: 0.25,
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: glam::Vec3::new(0.0, 0.0, -1.0),
            trigger_probability: 0.3,
            octave_offset: 1,
            base_duration: 0.6,
        },
    ];
    let params = EngineParams::default();
    MusicEngine::new(configs, params, 42)
}

#[test]
fn midi_to_hz_matches_a4_and_octave() {
    let a4 = midi_to_hz(69.0);
    assert!((a4 - 440.0).abs() < 1e-4);
    let a5 = midi_to_hz(81.0);
    assert!((a5 - 880.0).abs() < 1e-3);
    assert!((a5 / a4 - 2.0).abs() < 1e-4);
}

#[test]
fn midi_to_hz_is_monotonic_over_range() {
    let mut prev = midi_to_hz(20.0);
    for m in 21..=100 {
        let f = midi_to_hz(m as f32);
        assert!(f > prev, "frequency not increasing at midi {m}");
        prev = f;
    }
}

#[test]
fn engine_tick_emits_some_events_over_time() {
    let mut engine = make_engine();
    let mut events = Vec::new();
    let seconds_per_beat = 60.0 / engine.params.bpm as f64;
    for _ in 0..200 {
        engine.tick(Duration::from_secs_f64(seconds_per_beat / 2.0), &mut events);
    }
    assert!(!events.is_empty(), "expected some scheduled events");
    for ev in &events {
        assert!(ev.voice_index < engine.voices.len());
        assert!(ev.frequency_hz > 0.0);
        assert!(ev.velocity >= 0.0 && ev.velocity <= 1.0);
        assert!(ev.duration_sec > 0.0);
    }
}

#[test]
fn engine_toggle_mute_and_solo() {
    let mut engine = make_engine();
    assert!(!engine.voices[1].muted);
    engine.toggle_mute(1);
    assert!(engine.voices[1].muted);
    engine.toggle_mute(1);
    assert!(!engine.voices[1].muted);

    engine.toggle_solo(2);
    for (i, v) in engine.voices.iter().enumerate() {
        if i == 2 {
            assert!(!v.muted);
        } else {
            assert!(v.muted);
        }
    }
    engine.toggle_solo(2);
    for v in engine.voices.iter() {
        assert!(!v.muted);
    }
}

// Property-based tests for midi_to_hz function
#[test]
fn midi_to_hz_octave_doubling_property() {
    // Property: Adding 12 semitones (one octave) should double the frequency
    for midi in 20..100 {
        let freq1 = midi_to_hz(midi as f32);
        let freq2 = midi_to_hz((midi + 12) as f32);
        let ratio = freq2 / freq1;
        assert!(
            (ratio - 2.0).abs() < 1e-6,
            "Octave doubling failed for MIDI {midi}: {freq1} -> {freq2} (ratio: {ratio})"
        );
    }
}

#[test]
fn midi_to_hz_semitone_ratio_property() {
    // Property: Each semitone should multiply frequency by 2^(1/12) ≈ 1.059463
    let semitone_ratio = 2.0_f32.powf(1.0 / 12.0);

    for midi in 30..90 {
        let freq1 = midi_to_hz(midi as f32);
        let freq2 = midi_to_hz((midi + 1) as f32);
        let actual_ratio = freq2 / freq1;
        assert!(
            (actual_ratio - semitone_ratio).abs() < 1e-6,
            "Semitone ratio failed for MIDI {midi} -> {}: expected {semitone_ratio}, got {actual_ratio}",
            midi + 1
        );
    }
}

#[test]
fn midi_to_hz_fractional_values() {
    // Test that fractional MIDI values work correctly (for microtonal support)
    let midi_60 = midi_to_hz(60.0); // C4
    let midi_60_5 = midi_to_hz(60.5); // C4 + 50 cents
    let midi_61 = midi_to_hz(61.0); // C#4

    // 50 cents should be halfway between C4 and C#4 in log frequency space
    let log_60 = midi_60.ln();
    let log_60_5 = midi_60_5.ln();
    let log_61 = midi_61.ln();

    let expected_log_60_5 = (log_60 + log_61) / 2.0;
    assert!(
        (log_60_5 - expected_log_60_5).abs() < 1e-6,
        "Fractional MIDI value 60.5 should be logarithmic midpoint between 60 and 61"
    );
}

#[test]
fn midi_to_hz_extreme_values() {
    // Test extreme but valid MIDI values
    let very_low = midi_to_hz(0.0); // C-1, ~8.18 Hz
    let very_high = midi_to_hz(127.0); // G9, ~12543 Hz

    assert!(
        very_low > 0.0 && very_low < 20.0,
        "MIDI 0 should be audible bass frequency"
    );
    assert!(
        very_high > 10000.0 && very_high < 15000.0,
        "MIDI 127 should be very high frequency"
    );

    // Test that extreme values don't cause overflow/underflow
    assert!(
        very_low.is_finite(),
        "Very low MIDI should produce finite frequency"
    );
    assert!(
        very_high.is_finite(),
        "Very high MIDI should produce finite frequency"
    );
}

#[test]
fn midi_to_hz_negative_values() {
    // Test that negative MIDI values work (sub-audio frequencies)
    let neg_midi = midi_to_hz(-12.0); // One octave below MIDI 0
    let zero_midi = midi_to_hz(0.0);

    let ratio = zero_midi / neg_midi;
    assert!(
        (ratio - 2.0).abs() < 1e-6,
        "MIDI -12 should be exactly one octave below MIDI 0"
    );
}
