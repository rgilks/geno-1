use crate::constants::*;
use crate::core::{MusicEngine, Waveform};
use crate::input;
use crate::render;
use glam::Vec3;
use instant::Instant;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys as web;

use crate::constants::CAMERA_Z;

pub struct FrameContext<'a> {
    pub engine: Rc<RefCell<MusicEngine>>,
    pub paused: Rc<RefCell<bool>>,
    pub pulses: Rc<RefCell<Vec<f32>>>,
    pub hover_index: Rc<RefCell<Option<usize>>>,

    pub canvas: web::HtmlCanvasElement,
    pub mouse: Rc<RefCell<input::MouseState>>,

    pub audio_ctx: web::AudioContext,
    pub listener: web::AudioListener,
    pub voice_gains: Rc<Vec<web::GainNode>>,
    pub delay_sends: Rc<Vec<web::GainNode>>,
    pub reverb_sends: Rc<Vec<web::GainNode>>,
    pub voice_panners: Vec<web::PannerNode>,

    pub reverb_wet: web::GainNode,
    pub delay_wet: web::GainNode,
    pub delay_feedback: web::GainNode,
    pub sat_pre: web::GainNode,
    pub sat_wet: web::GainNode,
    pub sat_dry: web::GainNode,

    pub analyser: Option<web::AnalyserNode>,
    pub analyser_buf: Rc<RefCell<Vec<f32>>>,

    pub gpu: Option<render::GpuState<'a>>,
    pub queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>>,

    pub last_instant: Instant,
    pub prev_uv: [f32; 2],
    pub swirl_energy: f32,
    pub swirl_pos: [f32; 2],
    pub swirl_vel: [f32; 2],
    pub swirl_initialized: bool,
    pub pulse_energy: [f32; 3],
}

impl<'a> FrameContext<'a> {
    pub fn frame(&mut self) {
        let now = Instant::now();
        let dt = now - self.last_instant;
        self.last_instant = now;
        let dt_sec = dt.as_secs_f32();

        let audio_time = self.audio_ctx.current_time();
        let mut note_events = Vec::new();
        if !*self.paused.borrow() {
            self.engine.borrow_mut().tick(dt, &mut note_events);
        }

        {
            let pulses_copy: Vec<f32> = {
                let mut pulses_ref = self.pulses.borrow_mut();
                let n = pulses_ref.len().min(3);
                for ev in &note_events {
                    if ev.voice_index < n {
                        self.pulse_energy[ev.voice_index] =
                            (self.pulse_energy[ev.voice_index] + ev.velocity as f32).min(1.8);
                    }
                }
                smooth_pulses(&mut pulses_ref, &mut self.pulse_energy, dt_sec);
                pulses_ref.clone()
            }; // drop pulses_ref here

            // Swirl input and energy (no RefCell borrow active)
            let ms = self.mouse.borrow();
            let uv = input::mouse_uv(&self.canvas, &ms);
            let mouse_down = ms.down;
            drop(ms);
            self.update_swirl(uv, dt_sec, mouse_down);

            // Global FX modulation
            apply_global_fx_swirl(
                &self.reverb_wet,
                &self.delay_wet,
                &self.delay_feedback,
                &self.sat_pre,
                &self.sat_wet,
                &self.sat_dry,
                self.swirl_energy,
                uv,
            );

            // Per-voice audio positioning and sends
            for i in 0..self.voice_panners.len() {
                let pos = self.engine.borrow().voices[i].position;
                self.voice_panners[i].position_x().set_value(pos.x as f32);
                self.voice_panners[i].position_y().set_value(pos.y as f32);
                self.voice_panners[i].position_z().set_value(pos.z as f32);
                let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
                let mut d_amt = (D_SEND_BASE + D_SEND_SPAN * pos.x.abs().min(1.0)).clamp(0.0, 1.0);
                let mut r_amt = (R_SEND_BASE
                    + R_SEND_SPAN * (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0))
                .clamp(0.0, R_SEND_CLAMP_MAX);
                let boost = 1.0 + SEND_BOOST_COEFF * self.swirl_energy;
                d_amt = (d_amt * boost).clamp(0.0, D_SEND_CLAMP_MAX);
                r_amt = (r_amt * boost).clamp(0.0, R_SEND_CLAMP_MAX);
                self.delay_sends[i].gain().set_value(d_amt);
                self.reverb_sends[i].gain().set_value(r_amt);
                let lvl = (LEVEL_BASE
                    + LEVEL_SPAN * (1.0 - (dist / DIST_NORM_DIVISOR).clamp(0.0, 1.0)))
                    as f32;
                self.voice_gains[i].gain().set_value(lvl);
            }

            // Optional analyser-driven ambient energy
            if let Some(a) = &self.analyser {
                let bins = a.frequency_bin_count() as usize;
                {
                    let mut buf = self.analyser_buf.borrow_mut();
                    if buf.len() != bins {
                        buf.resize(bins, 0.0);
                    }
                    a.get_float_frequency_data(&mut buf);
                }
                let mut sum = 0.0f32;
                let take = (bins.min(16)) as u32;
                for i in 0..take {
                    let v = self.analyser_buf.borrow()[i as usize];
                    let lin = ((v + 100.0) / 100.0).clamp(0.0, 1.0);
                    sum += lin;
                }
                let avg = sum / take as f32;
                let n = pulses_copy.len().min(3);
                {
                    // update both self.pulses and local copy
                    let mut pulses_ref = self.pulses.borrow_mut();
                    for i in 0..n {
                        pulses_ref[i] = (pulses_ref[i] + avg * 0.05).min(1.5);
                    }
                }
                if let Some(g) = &mut self.gpu {
                    g.set_ambient_clear(avg * 0.9);
                }
            }

            // Voice positions are now only used for audio spatialization and wave displacement

            // Camera + listener
            let cam_eye = Vec3::new(0.0, 0.0, CAMERA_Z);
            let cam_target = Vec3::ZERO;
            update_listener_to_camera(&self.listener, cam_eye, cam_target);

            if let Some(g) = &mut self.gpu {
                g.set_camera(cam_eye, cam_target);
                if let Some(uvr) = self.queued_ripple_uv.borrow_mut().take() {
                    g.set_ripple(uvr, 1.0);
                }
                let speed_norm = ((self.swirl_vel[0] * self.swirl_vel[0]
                    + self.swirl_vel[1] * self.swirl_vel[1])
                    .sqrt()
                    / 1.0)
                    .clamp(0.0, 1.0);
                let strength = 0.28 + 0.85 * self.swirl_energy + 0.15 * speed_norm;
                g.set_swirl(self.swirl_pos, strength, true);
                let w = self.canvas.width();
                let h = self.canvas.height();
                g.resize_if_needed(w, h);
                if let Err(e) = g.render(dt_sec) {
                    log::error!("render error: {:?}", e);
                }
            }
        }

        if !*self.paused.borrow() {
            for ev in &note_events {
                let src = match web::OscillatorNode::new(&self.audio_ctx) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                match self.engine.borrow().configs[ev.voice_index].waveform {
                    Waveform::Sine => src.set_type(web::OscillatorType::Sine),
                    // Waveform::Square => src.set_type(web::OscillatorType::Square),
                    Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
                    Waveform::Triangle => src.set_type(web::OscillatorType::Triangle),
                }
                src.frequency().set_value(ev.frequency_hz);
                let gain = match web::GainNode::new(&self.audio_ctx) {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                gain.gain().set_value(0.0);
                let t0 = audio_time + 0.01;
                _ = gain
                    .gain()
                    .linear_ramp_to_value_at_time(ev.velocity as f32, t0 + 0.02);
                _ = gain
                    .gain()
                    .linear_ramp_to_value_at_time(0.0_f32, t0 + ev.duration_sec as f64);
                _ = src.connect_with_audio_node(&gain);
                _ = gain.connect_with_audio_node(&self.voice_gains[ev.voice_index]);
                _ = gain.connect_with_audio_node(&self.delay_sends[ev.voice_index]);
                _ = gain.connect_with_audio_node(&self.reverb_sends[ev.voice_index]);
                _ = src.start_with_when(t0);
                _ = src.stop_with_when(t0 + ev.duration_sec as f64 + 0.02);
            }
        }
    }
}

impl<'a> FrameContext<'a> {
    fn update_swirl(&mut self, uv: [f32; 2], dt_sec: f32, mouse_down: bool) {
        step_inertial_swirl(
            &mut self.swirl_initialized,
            &mut self.swirl_pos,
            &mut self.swirl_vel,
            uv,
            dt_sec,
        );
        let du = uv[0] - self.prev_uv[0];
        let dv = uv[1] - self.prev_uv[1];
        let pointer_speed = ((du * du + dv * dv).sqrt() / (dt_sec + 1e-5)).min(POINTER_SPEED_MAX);
        let swirl_speed =
            (self.swirl_vel[0] * self.swirl_vel[0] + self.swirl_vel[1] * self.swirl_vel[1]).sqrt();
        let target = ((pointer_speed * SWIRL_TARGET_WEIGHT_POINTER)
            + (swirl_speed * SWIRL_TARGET_WEIGHT_VELOCITY)
            + if mouse_down {
                SWIRL_TARGET_CLICK_BONUS
            } else {
                0.0
            })
        .clamp(0.0, 1.0);
        self.swirl_energy = (1.0 - SWIRL_ENERGY_BLEND_ALPHA) * self.swirl_energy
            + SWIRL_ENERGY_BLEND_ALPHA * target;
        self.prev_uv = uv;
    }
}

#[inline]
fn smooth_pulses(pulses: &mut [f32], pulse_energy: &mut [f32; 3], dt_sec: f32) {
    let n = pulses.len().min(3);
    let energy_decay = (-dt_sec * PULSE_ENERGY_DECAY_PER_SEC).exp();
    for i in 0..n {
        pulse_energy[i] *= energy_decay;
    }
    let tau_up = PULSE_RISE_TAU_SEC;
    let tau_down = PULSE_FALL_TAU_SEC;
    let alpha_up = 1.0 - (-dt_sec / tau_up).exp();
    let alpha_down = 1.0 - (-dt_sec / tau_down).exp();
    for i in 0..n {
        let target = pulse_energy[i].clamp(0.0, 1.5);
        let alpha = if target > pulses[i] {
            alpha_up
        } else {
            alpha_down
        };
        pulses[i] += (target - pulses[i]) * alpha;
    }
}

pub async fn init_gpu(canvas: &web::HtmlCanvasElement) -> Option<render::GpuState<'static>> {
    // leak a canvas clone to satisfy 'static lifetime for surface
    let leaked_canvas = Box::leak(Box::new(canvas.clone()));
    match render::GpuState::new(leaked_canvas, CAMERA_Z).await {
        Ok(g) => Some(g),
        Err(e) => {
            log::error!("WebGPU init error: {:?}", e);
            None
        }
    }
}

pub fn start_loop(frame_ctx: Rc<RefCell<FrameContext<'static>>>) {
    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let tick_clone = tick.clone();
    let frame_ctx_tick = frame_ctx.clone();
    *tick.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        frame_ctx_tick.borrow_mut().frame();
        if let Some(w) = web::window() {
            _ = w.request_animation_frame(
                tick_clone
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unchecked_ref(),
            );
        }
    }) as Box<dyn FnMut()>));
    if let Some(w) = web::window() {
        _ = w.request_animation_frame(tick.borrow().as_ref().unwrap().as_ref().unchecked_ref());
    }
}

// --- helpers private to frame ---
fn step_inertial_swirl(
    initialized: &mut bool,
    swirl_pos: &mut [f32; 2],
    swirl_vel: &mut [f32; 2],
    target_uv: [f32; 2],
    dt_sec: f32,
) {
    if !*initialized {
        *swirl_pos = target_uv;
        swirl_vel[0] = 0.0;
        swirl_vel[1] = 0.0;
        *initialized = true;
        return;
    }
    let omega = SWIRL_OMEGA;
    let k = omega * omega;
    let c = 2.0 * omega * SWIRL_DAMPING_RATIO;
    let dx = target_uv[0] - swirl_pos[0];
    let dy = target_uv[1] - swirl_pos[1];
    let ax = k * dx - c * swirl_vel[0];
    let ay = k * dy - c * swirl_vel[1];
    swirl_vel[0] += ax * dt_sec;
    swirl_vel[1] += ay * dt_sec;
    let mut nx = swirl_pos[0] + swirl_vel[0] * dt_sec;
    let mut ny = swirl_pos[1] + swirl_vel[1] * dt_sec;
    let sdx = nx - swirl_pos[0];
    let sdy = ny - swirl_pos[1];
    let step = (sdx * sdx + sdy * sdy).sqrt();
    let max_step = SWIRL_MAX_STEP_PER_SEC * dt_sec;
    if step > max_step {
        let inv = 1.0 / (step + 1e-6);
        nx = swirl_pos[0] + sdx * inv * max_step;
        ny = swirl_pos[1] + sdy * inv * max_step;
    }
    swirl_pos[0] = nx.clamp(0.0, 1.0);
    swirl_pos[1] = ny.clamp(0.0, 1.0);
}

fn apply_global_fx_swirl(
    reverb_wet: &web::GainNode,
    delay_wet: &web::GainNode,
    delay_feedback: &web::GainNode,
    sat_pre: &web::GainNode,
    sat_wet: &web::GainNode,
    sat_dry: &web::GainNode,
    swirl_energy: f32,
    uv: [f32; 2],
) {
    _ = reverb_wet
        .gain()
        .set_value(FX_REVERB_BASE + FX_REVERB_SPAN * swirl_energy);
    let echo = (uv[0] - uv[1]).abs();
    let delay_wet_val =
        (FX_DELAY_WET_BASE + FX_DELAY_WET_SWIRL * swirl_energy + FX_DELAY_WET_ECHO * echo)
            .clamp(0.0, 1.0);
    let delay_fb_val =
        (FX_DELAY_FB_BASE + FX_DELAY_FB_SWIRL * swirl_energy + FX_DELAY_FB_ECHO * echo)
            .clamp(0.0, 0.95);
    _ = delay_wet.gain().set_value(delay_wet_val);
    _ = delay_feedback.gain().set_value(delay_fb_val);
    let fizz = ((uv[0] + uv[1]) * 0.5).clamp(0.0, 1.0);
    let drive = (FX_SAT_DRIVE_MIN
        + (FX_SAT_DRIVE_MAX - FX_SAT_DRIVE_MIN) * ((fizz - 0.25).clamp(0.0, 1.0)))
    .clamp(FX_SAT_DRIVE_MIN, FX_SAT_DRIVE_MAX);
    _ = sat_pre.gain().set_value(drive);
    let wet = (FX_SAT_WET_BASE + FX_SAT_WET_SPAN * fizz).clamp(0.0, 1.0);
    _ = sat_wet.gain().set_value(wet);
    _ = sat_dry.gain().set_value(1.0 - wet);
}

fn update_listener_to_camera(listener: &web::AudioListener, cam_eye: Vec3, cam_target: Vec3) {
    let fwd = (cam_target - cam_eye).normalize();
    listener.set_position(cam_eye.x as f64, cam_eye.y as f64, cam_eye.z as f64);
    _ = listener.set_orientation(fwd.x as f64, fwd.y as f64, fwd.z as f64, 0.0, 1.0, 0.0);
}
