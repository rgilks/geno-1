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
        },
        VoiceConfig {
            waveform: Waveform::Saw,
            base_position: glam::Vec3::new(1.0, 0.0, 0.0),
        },
        VoiceConfig {
            waveform: Waveform::Triangle,
            base_position: glam::Vec3::new(0.0, 0.0, -1.0),
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
