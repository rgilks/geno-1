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
