use glam::Vec3;
use rand::prelude::*;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub enum Waveform {
    Sine,
    Square,
    Saw,
    Triangle,
}

#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub color_rgb: [f32; 3],
    pub waveform: Waveform,
    pub base_position: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct NoteEvent {
    pub voice_index: usize,
    pub frequency_hz: f32,
    pub velocity: f32,
    pub start_time_sec: f64,
    pub duration_sec: f32,
}

#[derive(Clone, Debug)]
pub struct VoiceState {
    pub position: Vec3,
    pub muted: bool,
}

#[derive(Clone, Debug)]
pub struct EngineParams {
    pub bpm: f32,
    pub scale: &'static [i32],
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            bpm: 110.0,
            scale: C_MAJOR_PENTATONIC,
        }
    }
}

pub const C_MAJOR_PENTATONIC: &[i32] = &[0, 2, 4, 7, 9, 12];

pub struct MusicEngine {
    pub voices: Vec<VoiceState>,
    pub configs: Vec<VoiceConfig>,
    pub params: EngineParams,
    rngs: Vec<StdRng>,
    solo_index: Option<usize>,
    beat_accum: f64,
}

impl MusicEngine {
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

    pub fn set_bpm(&mut self, bpm: f32) {
        self.params.bpm = bpm;
    }

    pub fn toggle_mute(&mut self, voice_index: usize) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.muted = !v.muted;
        }
    }

    pub fn set_voice_muted(&mut self, voice_index: usize, muted: bool) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.muted = muted;
        }
    }

    pub fn set_voice_position(&mut self, voice_index: usize, pos: Vec3) {
        if let Some(v) = self.voices.get_mut(voice_index) {
            v.position = pos;
        }
    }

    pub fn reseed_voice(&mut self, voice_index: usize, seed: Option<u64>) {
        if let Some(r) = self.rngs.get_mut(voice_index) {
            let new_seed = seed.unwrap_or_else(|| r.gen());
            *r = StdRng::seed_from_u64(new_seed);
        }
    }

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

    pub fn tick(&mut self, dt: Duration, now_sec: f64, out_events: &mut Vec<NoteEvent>) {
        let seconds_per_beat = 60.0 / self.params.bpm as f64;
        self.beat_accum += dt.as_secs_f64();
        while self.beat_accum >= seconds_per_beat / 2.0 {
            // eighth notes grid
            self.beat_accum -= seconds_per_beat / 2.0;
            self.schedule_step(now_sec, out_events);
        }
    }

    fn schedule_step(&mut self, now_sec: f64, out_events: &mut Vec<NoteEvent>) {
        for (i, voice) in self.voices.iter().enumerate() {
            if voice.muted {
                continue;
            }
            // Probability to trigger per eighth note varies per voice
            let prob = match i {
                0 => 0.4,
                1 => 0.6,
                _ => 0.3,
            };
            if self.rngs[i].gen::<f32>() < prob {
                let degree = *self.params.scale.choose(&mut self.rngs[i]).unwrap_or(&0);
                let octave = match i {
                    0 => -1,
                    1 => 0,
                    _ => 1,
                };
                let midi = 60 + degree + octave * 12; // around middle C
                let freq = midi_to_hz(midi as f32);
                let vel = 0.4 + self.rngs[i].gen::<f32>() * 0.6;
                let dur = match i {
                    0 => 0.4,
                    1 => 0.25,
                    _ => 0.6,
                } + self.rngs[i].gen::<f32>() * 0.2;
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
}

pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * (2.0_f32).powf((midi - 69.0) / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
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
}
