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
            scale: &C_MAJOR_PENTATONIC,
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
