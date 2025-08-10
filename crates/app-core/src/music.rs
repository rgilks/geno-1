use glam::Vec3;
use rand::prelude::*;
use std::time::Duration;

/// Basic oscillator shape used by synths in the web front-end.
#[derive(Clone, Copy, Debug)]
pub enum Waveform {
    Sine,
    Square,
    Saw,
    Triangle,
}

/// Static configuration for a voice used at engine construction time.
///
/// Fields:
/// - `color_rgb`: base RGB color used by the visualizer for this voice
/// - `waveform`: oscillator type to synthesize this voice in the web frontend
/// - `base_position`: initial engine-space position (XZ plane; Y is typically 0)
#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub color_rgb: [f32; 3],
    pub waveform: Waveform,
    pub base_position: Vec3,
}

/// A scheduled musical event produced by the engine for playback.
///
/// Fields:
/// - `voice_index`: which voice this event belongs to (index into `voices`)
/// - `frequency_hz`: target pitch in Hertz (already converted from MIDI)
/// - `velocity`: normalized loudness 0..1 (mapped to gain envelope)
/// - `start_time_sec`: absolute start time (AudioContext time) in seconds
/// - `duration_sec`: nominal duration in seconds (envelope length)
#[derive(Clone, Debug, Default)]
pub struct NoteEvent {
    pub voice_index: usize,
    pub frequency_hz: f32,
    pub velocity: f32,
    pub start_time_sec: f64,
    pub duration_sec: f32,
}

/// Mutable runtime state per voice.
#[derive(Clone, Debug)]
pub struct VoiceState {
    pub position: Vec3,
    pub muted: bool,
}

/// Global engine parameters controlling tempo and scale.
///
/// - `bpm` controls the tempo of the scheduler (beats per minute)
/// - `scale` is the allowed pitch degree set, expressed as semitone offsets
/// - `root_midi` is the MIDI note number of the tonal center (e.g., 60 for C4)
#[derive(Clone, Debug)]
pub struct EngineParams {
    pub bpm: f32,
    pub scale: &'static [i32],
    pub root_midi: i32,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            bpm: 110.0,
            scale: C_MAJOR_PENTATONIC,
            root_midi: 60, // Middle C
        }
    }
}

/// Default five-note scale centered around middle C.
pub const C_MAJOR_PENTATONIC: &[i32] = &[0, 2, 4, 7, 9, 12];

/// Diatonic modes (relative semitone degrees)
pub const IONIAN: &[i32] = &[0, 2, 4, 5, 7, 9, 11, 12]; // major
pub const DORIAN: &[i32] = &[0, 2, 3, 5, 7, 9, 10, 12];
pub const PHRYGIAN: &[i32] = &[0, 1, 3, 5, 7, 8, 10, 12];
pub const LYDIAN: &[i32] = &[0, 2, 4, 6, 7, 9, 11, 12];
pub const MIXOLYDIAN: &[i32] = &[0, 2, 4, 5, 7, 9, 10, 12];
pub const AEOLIAN: &[i32] = &[0, 2, 3, 5, 7, 8, 10, 12]; // natural minor
pub const LOCRIAN: &[i32] = &[0, 1, 3, 5, 6, 8, 10, 12];

/// Random generative scheduler producing `NoteEvent`s on an eighth-note grid.
///
/// The engine maintains per-voice state and RNGs. On each tick, it advances an
/// internal accumulator based on the configured tempo (`params.bpm`) and emits
/// events aligned to an eighth-note grid. Voices have distinct trigger
/// probabilities, octave ranges, and base durations to create a simple texture.
///
/// Typical usage:
/// - Construct with `MusicEngine::new(configs, params, seed)`
/// - Call `tick(dt, now_sec, &mut out_events)` regularly to schedule audio
/// - Use `toggle_mute`, `toggle_solo`, `reseed_voice`, and `set_voice_position`
///   to interact with the engine state
pub struct MusicEngine {
    pub voices: Vec<VoiceState>,
    pub configs: Vec<VoiceConfig>,
    pub params: EngineParams,
    rngs: Vec<StdRng>,
    solo_index: Option<usize>,
    beat_accum: f64,
}

impl MusicEngine {
    /// Construct a new engine with voices derived from the provided configs.
    pub fn new(configs: Vec<VoiceConfig>, params: EngineParams, seed: u64) -> Self {
        let voices = configs
            .iter()
            .map(|c| VoiceState {
                position: c.base_position,
                muted: false,
            })
            .collect::<Vec<_>>();

        // Derive per-voice RNGs from base seed so we can reseed voices independently
        let rngs = (0..voices.len())
            .map(|i| {
                let mix = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                StdRng::seed_from_u64(mix)
            })
            .collect::<Vec<_>>();

        Self {
            voices,
            configs,
            params,
            rngs,
            solo_index: None,
            beat_accum: 0.0,
        }
    }

    /// Set beats-per-minute for the internal scheduler.
    pub fn set_bpm(&mut self, bpm: f32) {
        self.params.bpm = bpm;
    }

    /// Toggle mute flag for a voice.
    pub fn toggle_mute(&mut self, voice_index: usize) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.muted = !v.muted;
        }
    }

    /// Explicitly set mute flag for a voice.
    pub fn set_voice_muted(&mut self, voice_index: usize, muted: bool) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.muted = muted;
        }
    }

    /// Update the engine-space position of a voice.
    pub fn set_voice_position(&mut self, voice_index: usize, pos: Vec3) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.position = pos;
        }
    }

    /// Reseed the per-voice RNG. If `seed` is None, a new random seed is chosen.
    pub fn reseed_voice(&mut self, voice_index: usize, seed: Option<u64>) {
        if let Some(r) = self.rngs.get_mut(voice_index) {
            let new_seed = seed.unwrap_or_else(|| r.gen());
            *r = StdRng::seed_from_u64(new_seed);
        }
    }

    /// Solo a voice. Toggling solo on the same voice clears solo mode.
    pub fn toggle_solo(&mut self, voice_index: usize) {
        match self.solo_index {
            Some(idx) if idx == voice_index => {
                // Clear solo -> unmute all
                self.solo_index = None;
                for v in &mut self.voices {
                    v.muted = false;
                }
            }
            _ => {
                self.solo_index = Some(voice_index);
                for (i, v) in self.voices.iter_mut().enumerate() {
                    v.muted = i != voice_index;
                }
            }
        }
    }

    /// Advance the scheduler by `dt`, pushing any newly scheduled `NoteEvent`s into `out_events`.
    pub fn tick(&mut self, dt: Duration, now_sec: f64, out_events: &mut Vec<NoteEvent>) {
        let seconds_per_beat = 60.0 / self.params.bpm as f64;
        self.beat_accum += dt.as_secs_f64();
        while self.beat_accum >= seconds_per_beat / 2.0 {
            // eighth notes grid
            self.beat_accum -= seconds_per_beat / 2.0;
            self.schedule_step(now_sec, out_events);
        }
    }

    /// Schedule a single grid step for all voices.
    fn schedule_step(&mut self, now_sec: f64, out_events: &mut Vec<NoteEvent>) {
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.muted {
                continue;
            }
            let prob = MusicEngine::voice_trigger_probability(i);
            let rng = &mut self.rngs[i];
            if rng.gen::<f32>() < prob {
                let degree = *self.params.scale.choose(rng).unwrap_or(&0);
                let octave = MusicEngine::voice_octave_offset(i);
                let midi = self.params.root_midi + degree + octave * 12;
                let freq = midi_to_hz(midi as f32);
                let vel = 0.4 + rng.gen::<f32>() * 0.6;
                let dur = MusicEngine::voice_base_duration(i) + rng.gen::<f32>() * 0.2;
                out_events.push(NoteEvent {
                    voice_index: i,
                    frequency_hz: freq,
                    velocity: vel,
                    start_time_sec: now_sec + 0.02,
                    duration_sec: dur,
                });
            }
        }
    }

    #[inline]
    fn voice_trigger_probability(voice_index: usize) -> f32 {
        match voice_index {
            0 => 0.4,
            1 => 0.6,
            _ => 0.3,
        }
    }

    #[inline]
    fn voice_octave_offset(voice_index: usize) -> i32 {
        match voice_index {
            0 => -1,
            1 => 0,
            _ => 1,
        }
    }

    #[inline]
    fn voice_base_duration(voice_index: usize) -> f32 {
        match voice_index {
            0 => 0.4,
            1 => 0.25,
            _ => 0.6,
        }
    }
}

/// Convert a MIDI note number to Hertz (A4=440 Hz).
///
/// Monotonic and exhibits octave symmetry: +12 semitones doubles the frequency.
pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * (2.0_f32).powf((midi - 69.0) / 12.0)
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn engine_initializes_from_configs() {
        let configs = vec![
            VoiceConfig {
                color_rgb: [1.0, 0.0, 0.0],
                waveform: Waveform::Sine,
                base_position: Vec3::new(-1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 1.0, 0.0],
                waveform: Waveform::Saw,
                base_position: Vec3::new(1.0, 0.0, 0.0),
            },
        ];
        let params = EngineParams::default();
        let engine = MusicEngine::new(configs.clone(), params, 42);
        assert_eq!(engine.voices.len(), configs.len());
        assert_eq!(engine.configs.len(), configs.len());
        assert_eq!(engine.voices[0].position, configs[0].base_position);
        assert_eq!(engine.voices[1].position, configs[1].base_position);
    }

    #[test]
    fn set_voice_position_updates_state() {
        let configs = vec![VoiceConfig {
            color_rgb: [1.0, 0.0, 0.0],
            waveform: Waveform::Sine,
            base_position: Vec3::new(0.0, 0.0, 0.0),
        }];
        let params = EngineParams::default();
        let mut engine = MusicEngine::new(configs, params, 1);
        engine.set_voice_position(0, Vec3::new(2.0, 0.0, -1.0));
        assert_eq!(engine.voices[0].position, Vec3::new(2.0, 0.0, -1.0));
    }
    #[test]
    fn midi_to_hz_references() {
        assert!(approx_eq(midi_to_hz(69.0), 440.0, 0.01));
        // Middle C ≈ 261.6256 Hz
        assert!(approx_eq(midi_to_hz(60.0), 261.6256, 0.05));
        // A4 ± 12 semitones should double/halve
        assert!(approx_eq(midi_to_hz(81.0), 880.0, 0.05));
        assert!(approx_eq(midi_to_hz(57.0), 220.0, 0.05));
    }

    #[test]
    fn midi_to_hz_monotonic_and_octave_symmetry() {
        // Monotonic: increasing MIDI should not decrease Hz
        for m in 0..126 {
            let a = midi_to_hz(m as f32);
            let b = midi_to_hz((m + 1) as f32);
            assert!(b > a);
        }
        // Octave symmetry: +12 doubles within reasonable tolerance across range
        for m in 12..116 {
            let a = midi_to_hz(m as f32);
            let b = midi_to_hz((m + 12) as f32);
            let ratio = b / a;
            assert!((ratio - 2.0).abs() < 1e-3, "ratio {ratio} at m={m}");
        }
    }

    #[test]
    fn engine_tick_produces_events_with_default_params() {
        let configs = vec![
            VoiceConfig {
                color_rgb: [1.0, 0.0, 0.0],
                waveform: Waveform::Sine,
                base_position: Vec3::new(-1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 1.0, 0.0],
                waveform: Waveform::Saw,
                base_position: Vec3::new(1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 0.0, 1.0],
                waveform: Waveform::Triangle,
                base_position: Vec3::new(0.0, 0.0, -1.0),
            },
        ];
        let params = EngineParams::default();
        let mut engine = MusicEngine::new(configs, params, 12345);
        let mut out = Vec::new();
        // Advance enough simulated time over multiple ticks to very likely produce events
        let mut now = 0.0;
        for _ in 0..16 {
            engine.tick(Duration::from_millis(150), now, &mut out);
            now += 0.15;
        }
        assert!(
            !out.is_empty(),
            "expected some NoteEvent(s) to be produced over multiple ticks"
        );
        // Sanity: events have valid durations and frequencies
        for ev in &out {
            assert!(ev.duration_sec > 0.0);
            assert!(ev.frequency_hz > 0.0);
            assert!(ev.velocity >= 0.0 && ev.velocity <= 1.0);
        }
    }

    #[test]
    fn mute_and_solo_behaviour() {
        let configs = vec![
            VoiceConfig {
                color_rgb: [1.0, 0.0, 0.0],
                waveform: Waveform::Sine,
                base_position: Vec3::new(-1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 1.0, 0.0],
                waveform: Waveform::Saw,
                base_position: Vec3::new(1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 0.0, 1.0],
                waveform: Waveform::Triangle,
                base_position: Vec3::new(0.0, 0.0, -1.0),
            },
        ];
        let params = EngineParams::default();
        let mut engine = MusicEngine::new(configs, params, 7);

        // Toggle mute on voice 1
        assert!(!engine.voices[1].muted);
        engine.toggle_mute(1);
        assert!(engine.voices[1].muted);
        engine.set_voice_muted(1, false);
        assert!(!engine.voices[1].muted);

        // Solo voice 2 mutes others
        engine.toggle_solo(2);
        assert!(engine.voices[0].muted);
        assert!(engine.voices[1].muted);
        assert!(!engine.voices[2].muted);

        // Toggle solo off restores all
        engine.toggle_solo(2);
        assert!(!engine.voices[0].muted);
        assert!(!engine.voices[1].muted);
        assert!(!engine.voices[2].muted);
    }

    #[test]
    fn reseed_determinism_per_voice() {
        // Given identical configs and params, reseeding a voice with a fixed seed should
        // produce identical first scheduled events across engines.
        let configs = vec![
            VoiceConfig {
                color_rgb: [1.0, 0.0, 0.0],
                waveform: Waveform::Sine,
                base_position: Vec3::new(-1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 1.0, 0.0],
                waveform: Waveform::Saw,
                base_position: Vec3::new(1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 0.0, 1.0],
                waveform: Waveform::Triangle,
                base_position: Vec3::new(0.0, 0.0, -1.0),
            },
        ];
        let params = EngineParams::default();
        let mut a = MusicEngine::new(configs.clone(), params.clone(), 111);
        let mut b = MusicEngine::new(configs, params, 222);
        // Force same reseed for voice 1
        a.reseed_voice(1, Some(9999));
        b.reseed_voice(1, Some(9999));
        // Advance enough time to schedule a step and collect events
        let mut out_a = Vec::new();
        let mut out_b = Vec::new();
        a.tick(Duration::from_millis(300), 0.0, &mut out_a);
        b.tick(Duration::from_millis(300), 0.0, &mut out_b);
        // Filter for voice 1 events and compare first if both exist
        let ev_a = out_a.into_iter().find(|e| e.voice_index == 1);
        let ev_b = out_b.into_iter().find(|e| e.voice_index == 1);
        if let (Some(x), Some(y)) = (ev_a, ev_b) {
            assert!((x.frequency_hz - y.frequency_hz).abs() < 1e-3);
            assert!((x.duration_sec - y.duration_sec).abs() < 1e-3);
        }
    }

    #[test]
    fn tempo_change_does_not_break_mute_and_solo() {
        let configs = vec![
            VoiceConfig {
                color_rgb: [1.0, 0.0, 0.0],
                waveform: Waveform::Sine,
                base_position: Vec3::new(-1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 1.0, 0.0],
                waveform: Waveform::Saw,
                base_position: Vec3::new(1.0, 0.0, 0.0),
            },
            VoiceConfig {
                color_rgb: [0.0, 0.0, 1.0],
                waveform: Waveform::Triangle,
                base_position: Vec3::new(0.0, 0.0, -1.0),
            },
        ];
        let params = EngineParams::default();
        let mut engine = MusicEngine::new(configs, params, 7);
        // Solo voice 0 then change tempo
        engine.toggle_solo(0);
        engine.set_bpm(140.0);
        // Mute flags should still reflect solo state
        assert!(!engine.voices[0].muted);
        assert!(engine.voices[1].muted);
        assert!(engine.voices[2].muted);
        // Toggle solo off and ensure unmuted
        engine.toggle_solo(0);
        assert!(engine.voices.iter().all(|v| !v.muted));
    }
}
