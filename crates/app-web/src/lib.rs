#![cfg(target_arch = "wasm32")]
use app_core::{
    midi_to_hz, z_offset_vec3, EngineParams, MusicEngine, VoiceConfig, Waveform, AEOLIAN,
    BASE_SCALE, C_MAJOR_PENTATONIC, DEFAULT_VOICE_COLORS, DEFAULT_VOICE_POSITIONS, DORIAN,
    ENGINE_DRAG_MAX_RADIUS, IONIAN, LOCRIAN, LYDIAN, MIXOLYDIAN, PHRYGIAN, PICK_SPHERE_RADIUS,
    SCALE_PULSE_MULTIPLIER, SPREAD,
};
use glam::{Vec3, Vec4};
use instant::Instant;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;
// (DeviceExt no longer needed; legacy vertex buffers removed)

mod audio;
mod dom;
mod events;
mod frame;
mod input;
mod overlay;
mod render;
// ui module removed; overlay is controlled directly from here

// Rendering/picking shared constants to keep math consistent
const CAMERA_Z: f32 = 6.0;
fn wire_canvas_resize(canvas: &web::HtmlCanvasElement) {
    dom::sync_canvas_backing_size(canvas);
    let canvas_resize = canvas.clone();
    let resize_closure = Closure::wrap(Box::new(move || {
        dom::sync_canvas_backing_size(&canvas_resize);
    }) as Box<dyn FnMut()>);
    if let Some(window) = web::window() {
        let _ = window.add_event_listener_with_callback(
            "resize",
            resize_closure.as_ref().unchecked_ref(),
        );
    }
    resize_closure.forget();
}

struct InitParts {
    audio_ctx: web::AudioContext,
    listener_for_tick: web::AudioListener,
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
}

async fn build_audio_and_engine(document: web::Document) -> anyhow::Result<InitParts> {
    let audio_ctx = web::AudioContext::new().map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let _ = audio_ctx.resume();
    let listener = audio_ctx.listener();
    listener.set_position(0.0, 0.0, 1.5);

    let voice_configs = vec![
        VoiceConfig {
            color_rgb: DEFAULT_VOICE_COLORS[0],
            waveform: Waveform::Sine,
            base_position: Vec3::from(DEFAULT_VOICE_POSITIONS[0]),
        },
        VoiceConfig {
            color_rgb: DEFAULT_VOICE_COLORS[1],
            waveform: Waveform::Saw,
            base_position: Vec3::from(DEFAULT_VOICE_POSITIONS[1]),
        },
        VoiceConfig {
            color_rgb: DEFAULT_VOICE_COLORS[2],
            waveform: Waveform::Triangle,
            base_position: Vec3::from(DEFAULT_VOICE_POSITIONS[2]),
        },
    ];
    let engine = Rc::new(RefCell::new(MusicEngine::new(
        voice_configs,
        EngineParams {
            bpm: 110.0,
            scale: C_MAJOR_PENTATONIC,
            root_midi: 60,
        },
        42,
    )));
    {
        let e = engine.borrow();
        log::info!(
            "[engine] voices={} pos0=({:.2},{:.2},{:.2}) pos1=({:.2},{:.2},{:.2}) pos2=({:.2},{:.2},{:.2})",
            e.voices.len(),
            e.voices[0].position.x, e.voices[0].position.y, e.voices[0].position.z,
            e.voices[1].position.x, e.voices[1].position.y, e.voices[1].position.z,
            e.voices[2].position.x, e.voices[2].position.y, e.voices[2].position.z
        );
    }
    let paused = Rc::new(RefCell::new(true));
    Ok(InitParts {
        audio_ctx,
        listener_for_tick: listener,
        engine,
        paused,
    })
}

fn wire_overlay_buttons(audio_ctx: &web::AudioContext, paused: &Rc<RefCell<bool>>) {
    if let Some(doc2) = dom::window_document() {
        let paused_ok = paused.clone();
        let audio_ok = audio_ctx.clone();
        dom::add_click_listener(&doc2, "overlay-ok", move || {
            *paused_ok.borrow_mut() = false;
            let _ = audio_ok.resume();
            if let Some(w2) = web::window() {
                if let Some(d2) = w2.document() {
                    overlay::hide(&d2);
                }
            }
        });

        let paused_close = paused.clone();
        let audio_close = audio_ctx.clone();
        dom::add_click_listener(&doc2, "overlay-close", move || {
            *paused_close.borrow_mut() = false;
            let _ = audio_close.resume();
            if let Some(w2) = web::window() {
                if let Some(d2) = w2.document() {
                    overlay::hide(&d2);
                }
            }
        });
    }
}
const ANALYSER_FFT_SIZE: u32 = 256;

// helpers moved to dom.rs and events.rs

// Pointer helpers moved to input.rs

// Critically-damped (slightly underdamped) spring toward target UV
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
    let omega = 1.1_f32;
    let k = omega * omega;
    let c = 2.0 * omega * 0.5;
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
    let max_step = 0.50_f32 * dt_sec;
    if step > max_step {
        let inv = 1.0 / (step + 1e-6);
        nx = swirl_pos[0] + sdx * inv * max_step;
        ny = swirl_pos[1] + sdy * inv * max_step;
    }
    swirl_pos[0] = nx.clamp(0.0, 1.0);
    swirl_pos[1] = ny.clamp(0.0, 1.0);
}

// Apply global audio FX modulation based on swirl and pointer state
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
    let _ = reverb_wet.gain().set_value(0.35 + 0.65 * swirl_energy);
    let echo = (uv[0] - uv[1]).abs();
    let delay_wet_val = (0.15 + 0.55 * swirl_energy + 0.30 * echo).clamp(0.0, 1.0);
    let delay_fb_val = (0.35 + 0.35 * swirl_energy + 0.25 * echo).clamp(0.0, 0.95);
    let _ = delay_wet.gain().set_value(delay_wet_val);
    let _ = delay_feedback.gain().set_value(delay_fb_val);
    let fizz = ((uv[0] + uv[1]) * 0.5).clamp(0.0, 1.0);
    let drive = (0.6 + 2.4 * fizz).clamp(0.2, 3.0);
    let _ = sat_pre.gain().set_value(drive);
    let wet = (0.15 + 0.85 * fizz).clamp(0.0, 1.0);
    let _ = sat_wet.gain().set_value(wet);
    let _ = sat_dry.gain().set_value(1.0 - wet);
}

// Keep the AudioListener aligned to the camera
fn update_listener_to_camera(listener: &web::AudioListener, cam_eye: Vec3, cam_target: Vec3) {
    let fwd = (cam_target - cam_eye).normalize();
    listener.set_position(cam_eye.x as f64, cam_eye.y as f64, cam_eye.z as f64);
    let _ = listener.set_orientation(fwd.x as f64, fwd.y as f64, fwd.z as f64, 0.0, 1.0, 0.0);
}

// (waveshaper curve builder moved inside audio module)

// ===================== Frame context =====================
struct FrameContext<'a> {
    // Core state
    engine: Rc<RefCell<MusicEngine>>,
    paused: Rc<RefCell<bool>>,
    pulses: Rc<RefCell<Vec<f32>>>,
    hover_index: Rc<RefCell<Option<usize>>>,

    // Canvas and input
    canvas: web::HtmlCanvasElement,
    mouse: Rc<RefCell<input::MouseState>>,

    // Audio routing
    audio_ctx: web::AudioContext,
    listener: web::AudioListener,
    voice_gains: Rc<Vec<web::GainNode>>,
    delay_sends: Rc<Vec<web::GainNode>>,
    reverb_sends: Rc<Vec<web::GainNode>>,
    voice_panners: Vec<web::PannerNode>,

    // Global FX controls
    reverb_wet: web::GainNode,
    delay_wet: web::GainNode,
    delay_feedback: web::GainNode,
    sat_pre: web::GainNode,
    sat_wet: web::GainNode,
    sat_dry: web::GainNode,

    // Optional analyser
    analyser: Option<web::AnalyserNode>,
    analyser_buf: Rc<RefCell<Vec<f32>>>,

    // Renderer
    gpu: Option<render::GpuState<'a>>,
    queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>>,

    // Time + visual dynamics
    last_instant: Instant,
    prev_uv: [f32; 2],
    swirl_energy: f32,
    swirl_pos: [f32; 2],
    swirl_vel: [f32; 2],
    swirl_initialized: bool,
    pulse_energy: [f32; 3],
}

impl<'a> FrameContext<'a> {
    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now - self.last_instant;
        self.last_instant = now;
        let dt_sec = dt.as_secs_f32();

        let audio_time = self.audio_ctx.current_time();
        let mut note_events = Vec::new();
        if !*self.paused.borrow() {
            self.engine
                .borrow_mut()
                .tick(dt, audio_time, &mut note_events);
        }

        {
            let mut pulses = self.pulses.borrow_mut();
            let n = pulses.len().min(3);
            for ev in &note_events {
                if ev.voice_index < n {
                    self.pulse_energy[ev.voice_index] =
                        (self.pulse_energy[ev.voice_index] + ev.velocity as f32).min(1.8);
                }
            }
            let energy_decay = (-dt_sec * 1.6).exp();
            for i in 0..n {
                self.pulse_energy[i] *= energy_decay;
            }
            let tau_up = 0.10_f32;
            let tau_down = 0.45_f32;
            let alpha_up = 1.0 - (-dt_sec / tau_up).exp();
            let alpha_down = 1.0 - (-dt_sec / tau_down).exp();
            for i in 0..n {
                let target = self.pulse_energy[i].clamp(0.0, 1.5);
                let alpha = if target > pulses[i] {
                    alpha_up
                } else {
                    alpha_down
                };
                pulses[i] += (target - pulses[i]) * alpha;
            }

            // Swirl input
            let ms = self.mouse.borrow();
            let uv = input::mouse_uv(&self.canvas, &ms);
            step_inertial_swirl(
                &mut self.swirl_initialized,
                &mut self.swirl_pos,
                &mut self.swirl_vel,
                uv,
                dt_sec,
            );
            let du = uv[0] - self.prev_uv[0];
            let dv = uv[1] - self.prev_uv[1];
            let pointer_speed = ((du * du + dv * dv).sqrt() / (dt_sec + 1e-5)).min(10.0);
            let swirl_speed = (self.swirl_vel[0] * self.swirl_vel[0]
                + self.swirl_vel[1] * self.swirl_vel[1])
                .sqrt();
            let target =
                ((pointer_speed * 0.2) + (swirl_speed * 0.35) + if ms.down { 0.5 } else { 0.0 })
                    .clamp(0.0, 1.0);
            drop(ms);
            self.swirl_energy = 0.85 * self.swirl_energy + 0.15 * target;
            self.prev_uv = uv;

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
                let mut d_amt = (0.15 + 0.85 * pos.x.abs().min(1.0)).clamp(0.0, 1.0);
                let mut r_amt = (0.25 + 0.75 * (dist / 2.5).clamp(0.0, 1.0)).clamp(0.0, 1.2);
                let boost = 1.0 + 0.8 * self.swirl_energy;
                d_amt = (d_amt * boost).clamp(0.0, 1.2);
                r_amt = (r_amt * boost).clamp(0.0, 1.5);
                self.delay_sends[i].gain().set_value(d_amt);
                self.reverb_sends[i].gain().set_value(r_amt);
                let lvl = (0.55 + 0.45 * (1.0 - (dist / 2.5).clamp(0.0, 1.0))) as f32;
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
                let n = pulses.len().min(3);
                for i in 0..n {
                    pulses[i] = (pulses[i] + avg * 0.05).min(1.5);
                }
                if let Some(g) = &mut self.gpu {
                    g.set_ambient_clear(avg * 0.9);
                }
            }

            // Build instance buffers for renderer
            let e_ref = self.engine.borrow();
            let z_offset = z_offset_vec3();
            let spread = SPREAD;
            let ring_count = 48usize;
            let mut positions: Vec<Vec3> = Vec::with_capacity(3 + ring_count * 3 + 16);
            positions.push(e_ref.voices[0].position * spread + z_offset);
            positions.push(e_ref.voices[1].position * spread + z_offset);
            positions.push(e_ref.voices[2].position * spread + z_offset);
            let mut colors: Vec<Vec4> = Vec::with_capacity(3 + ring_count * 3 + 16);
            colors.push(Vec4::from((Vec3::from(e_ref.configs[0].color_rgb), 1.0)));
            colors.push(Vec4::from((Vec3::from(e_ref.configs[1].color_rgb), 1.0)));
            colors.push(Vec4::from((Vec3::from(e_ref.configs[2].color_rgb), 1.0)));
            let hovered = *self.hover_index.borrow();
            for i in 0..3 {
                if e_ref.voices[i].muted {
                    colors[i].x *= 0.35;
                    colors[i].y *= 0.35;
                    colors[i].z *= 0.35;
                    colors[i].w = 1.0;
                }
                if hovered == Some(i) {
                    colors[i].x = (colors[i].x * 1.4).min(1.0);
                    colors[i].y = (colors[i].y * 1.4).min(1.0);
                    colors[i].z = (colors[i].z * 1.4).min(1.0);
                }
            }
            let mut scales: Vec<f32> = Vec::with_capacity(3 + ring_count * 3 + 16);
            scales.push(BASE_SCALE + pulses[0] * SCALE_PULSE_MULTIPLIER);
            scales.push(BASE_SCALE + pulses[1] * SCALE_PULSE_MULTIPLIER);
            scales.push(BASE_SCALE + pulses[2] * SCALE_PULSE_MULTIPLIER);

            if let Some(a) = &self.analyser {
                let bins = a.frequency_bin_count() as usize;
                let dots = bins.min(16);
                if dots > 0 {
                    {
                        let mut buf = self.analyser_buf.borrow_mut();
                        if buf.len() != bins {
                            buf.resize(bins, 0.0);
                        }
                        a.get_float_frequency_data(&mut buf);
                    }
                    let z = z_offset.z;
                    for i in 0..dots {
                        let v_db = self.analyser_buf.borrow()[i];
                        let lin = ((v_db + 100.0) / 100.0).clamp(0.0, 1.0);
                        let x = -2.8 + (i as f32) * (5.6 / (dots as f32 - 1.0));
                        let y = -1.8;
                        positions.push(Vec3::new(x, y, z));
                        let c = Vec3::new(0.25 + 0.5 * lin, 0.6 + 0.3 * lin, 0.9);
                        colors.push(Vec4::from((c, 0.95)));
                        scales.push(0.18 + lin * 0.35);
                    }
                }
            }

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
                if let Err(e) = g.render(&positions, &colors, &scales) {
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
                    Waveform::Square => src.set_type(web::OscillatorType::Square),
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
                let _ = gain
                    .gain()
                    .linear_ramp_to_value_at_time(ev.velocity as f32, t0 + 0.02);
                let _ = gain
                    .gain()
                    .linear_ramp_to_value_at_time(0.0_f32, t0 + ev.duration_sec as f64);
                let _ = src.connect_with_audio_node(&gain);
                let _ = gain.connect_with_audio_node(&self.voice_gains[ev.voice_index]);
                let _ = gain.connect_with_audio_node(&self.delay_sends[ev.voice_index]);
                let _ = gain.connect_with_audio_node(&self.reverb_sends[ev.voice_index]);
                let _ = src.start_with_when(t0);
                let _ = src.stop_with_when(t0 + ev.duration_sec as f64 + 0.02);
            }
        }
    }
}

// Pick nearest voice by comparing UV x to normalized voice x positions
fn nearest_index_by_uvx(normalized_voice_xs: &[f32], uvx: f32) -> usize {
    let mut best_i = 0usize;
    let mut best_dx = f32::MAX;
    for (i, vx) in normalized_voice_xs.iter().enumerate() {
        let dx = (uvx - *vx).abs();
        if dx < best_dx {
            best_dx = dx;
            best_i = i;
        }
    }
    best_i
}

// Fire a simple one-shot oscillator routed through a voice's gain and sends
fn trigger_one_shot(
    audio_ctx: &web::AudioContext,
    waveform: Waveform,
    frequency_hz: f32,
    velocity: f32,
    duration_sec: f64,
    voice_gain: &web::GainNode,
    delay_send: &web::GainNode,
    reverb_send: &web::GainNode,
) {
    if let Ok(src) = web::OscillatorNode::new(audio_ctx) {
        match waveform {
            Waveform::Sine => src.set_type(web::OscillatorType::Sine),
            Waveform::Square => src.set_type(web::OscillatorType::Square),
            Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
            Waveform::Triangle => src.set_type(web::OscillatorType::Triangle),
        }
        src.frequency().set_value(frequency_hz);
        if let Ok(g) = web::GainNode::new(audio_ctx) {
            g.gain().set_value(0.0);
            let now = audio_ctx.current_time();
            let t0 = now + 0.005;
            let _ = g.gain().linear_ramp_to_value_at_time(velocity, t0 + 0.02);
            let _ = g
                .gain()
                .linear_ramp_to_value_at_time(0.0, t0 + duration_sec);
            let _ = src.connect_with_audio_node(&g);
            let _ = g.connect_with_audio_node(voice_gain);
            let _ = g.connect_with_audio_node(delay_send);
            let _ = g.connect_with_audio_node(reverb_send);
            let _ = src.start_with_when(t0);
            let _ = src.stop_with_when(t0 + duration_sec + 0.05);
        }
    }
}

// analyser creation moved to audio::create_analyser

// global keydown moved to events.rs

// Create a GainNode with an initial value; logs on failure and returns None
// create_gain moved to audio.rs

// (use overlay::hide instead of local helper)

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).ok();
    log::info!("app-web starting");

    spawn_local(async move {
        if let Err(e) = init().await {
            log::error!("init error: {:?}", e);
        }
    });
    Ok(())
}

async fn init() -> anyhow::Result<()> {
    let window = web::window().ok_or_else(|| anyhow::anyhow!("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| anyhow::anyhow!("no document"))?;

    let canvas_el = document
        .get_element_by_id("app-canvas")
        .ok_or_else(|| anyhow::anyhow!("missing #app-canvas"))?;
    let canvas: web::HtmlCanvasElement = canvas_el
        .dyn_into::<web::HtmlCanvasElement>()
        .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;

    // Note: start overlay is handled below (toggle with 'h') once audio is initialized.

    // Avoid grabbing a 2D context here to allow WebGPU to acquire the canvas

    // Maintain canvas internal pixel size to match CSS size * devicePixelRatio
    wire_canvas_resize(&canvas);

    // Prepare a clone for use inside the click closure
    let canvas_for_click = canvas.clone();

    // Start audio graph and scheduling + WebGPU renderer immediately; show overlay until OK/close
    static STARTED: AtomicBool = AtomicBool::new(false);
    {
        if STARTED.swap(true, Ordering::SeqCst) == false {
            let canvas_for_click_inner = canvas_for_click.clone();
            spawn_local(async move {
                let InitParts {
                    audio_ctx,
                    listener_for_tick,
                    engine,
                    paused,
                } = match build_audio_and_engine(document.clone()).await {
                    Ok(p) => p,
                    Err(_) => return,
                };

                wire_overlay_buttons(&audio_ctx, &paused);
                events::wire_overlay_toggle_h(&document);

                // FX buses
                let fx = match audio::build_fx_buses(&audio_ctx) {
                    Ok(f) => f,
                    Err(_) => return,
                };
                let master_gain = fx.master_gain.clone();
                let sat_pre = fx.sat_pre.clone();
                let sat_wet = fx.sat_wet.clone();
                let sat_dry = fx.sat_dry.clone();
                let reverb_in = fx.reverb_in.clone();
                let reverb_wet = fx.reverb_wet.clone();
                let delay_in = fx.delay_in.clone();
                let delay_feedback = fx.delay_feedback.clone();
                let delay_wet = fx.delay_wet.clone();

                // Per-voice master gains -> master bus, plus effect sends
                let initial_positions: Vec<Vec3> =
                    engine.borrow().voices.iter().map(|v| v.position).collect();
                let routing = match audio::wire_voices(
                    &audio_ctx,
                    &initial_positions,
                    &master_gain,
                    &delay_in,
                    &reverb_in,
                ) {
                    Ok(r) => r,
                    Err(_) => return,
                };
                let delay_sends = Rc::new(routing.delay_sends);
                let reverb_sends = Rc::new(routing.reverb_sends);
                let voice_panners = routing.voice_panners;
                let voice_gains = Rc::new(routing.voice_gains);

                // Initialize WebGPU (leak a canvas clone to satisfy 'static lifetime for surface)
                let leaked_canvas = Box::leak(Box::new(canvas_for_click_inner.clone()));
                let gpu: Option<render::GpuState> =
                    match render::GpuState::new(leaked_canvas, CAMERA_Z).await {
                        Ok(g) => Some(g),
                        Err(e) => {
                            log::error!("WebGPU init error: {:?}", e);
                            None
                        }
                    };

                // Visual pulses per voice and optional analyser for ambient effects
                let pulses = Rc::new(RefCell::new(vec![0.0_f32; engine.borrow().voices.len()]));
                let (analyser, analyser_buf) = audio::create_analyser(&audio_ctx);

                // Queued ripple UV from pointer taps (read by render tick)
                let queued_ripple_uv: Rc<RefCell<Option<[f32; 2]>>> = Rc::new(RefCell::new(None));

                // ---------------- Interaction state ----------------
                let mouse_state = Rc::new(RefCell::new(input::MouseState::default()));
                let hover_index = Rc::new(RefCell::new(None::<usize>));
                let drag_state = Rc::new(RefCell::new(input::DragState::default()));

                // Screen -> canvas coords inline helper

                // Mouse move: hover + drag
                {
                    let mouse_state_m = mouse_state.clone();
                    let hover_m = hover_index.clone();
                    let drag_m = drag_state.clone();
                    let engine_m = engine.clone();
                    let canvas_mouse = canvas_for_click_inner.clone();
                    let canvas_connected = canvas_mouse.is_connected();
                    let closure = Closure::wrap(Box::new(move |ev: web::PointerEvent| {
                        let pos = input::pointer_canvas_px(&ev, &canvas_mouse);
                        // For CI/headless environments without real mouse, synthesize hover over center
                        if !canvas_connected {
                            return;
                        }
                        {
                            // Store pointer position; render() converts to uv for swirl uniforms
                            let mut ms = mouse_state_m.borrow_mut();
                            ms.x = pos.x;
                            ms.y = pos.y;
                        }
                        // Compute hover or drag update
                        let (ro, rd) =
                            render::screen_to_world_ray(&canvas_mouse, pos.x, pos.y, CAMERA_Z);
                        let mut best = None::<(usize, f32)>;
                        let spread = SPREAD;
                        let z_offset = z_offset_vec3();
                        for (i, v) in engine_m.borrow().voices.iter().enumerate() {
                            let center_world = v.position * spread + z_offset;
                            if let Some(t) =
                                input::ray_sphere(ro, rd, center_world, PICK_SPHERE_RADIUS)
                            {
                                if t >= 0.0 {
                                    match best {
                                        Some((_, bt)) if t >= bt => {}
                                        _ => best = Some((i, t)),
                                    }
                                }
                            }
                        }
                        if drag_m.borrow().active {
                            // Drag on plane z = constant (locked at mousedown)
                            let plane_z = drag_m.borrow().plane_z_world;
                            if rd.z.abs() > 1e-6 {
                                let t = (plane_z - ro.z) / rd.z;
                                if t >= 0.0 {
                                    let hit_world = ro + rd * t;
                                    let mut eng_pos = (hit_world - z_offset_vec3()) / SPREAD;
                                    // Clamp drag radius to avoid losing objects
                                    let max_r = ENGINE_DRAG_MAX_RADIUS; // engine-space radius
                                    let len =
                                        (eng_pos.x * eng_pos.x + eng_pos.z * eng_pos.z).sqrt();
                                    if len > max_r {
                                        let scale = max_r / len;
                                        eng_pos.x *= scale;
                                        eng_pos.z *= scale;
                                    }
                                    let mut eng = engine_m.borrow_mut();
                                    let vi = drag_m.borrow().voice;
                                    eng.set_voice_position(
                                        vi,
                                        Vec3::new(eng_pos.x, 0.0, eng_pos.z),
                                    );
                                    // noisy drag debug log removed
                                }
                            } else {
                                // noisy drag-parallel debug log removed
                            }
                            // While dragging, boost swirl strength (used during render)
                        } else {
                            match best {
                                Some((i, _t)) => {
                                    *hover_m.borrow_mut() = Some(i);
                                }
                                None => {
                                    *hover_m.borrow_mut() = None;
                                }
                            }
                        }
                    }) as Box<dyn FnMut(_)>);
                    if let Some(w) = web::window() {
                        w.add_event_listener_with_callback(
                            "pointermove",
                            closure.as_ref().unchecked_ref(),
                        )
                        .ok();
                    }
                    closure.forget();
                }

                // Keyboard controls: R reseed all, Space pause, +/- bpm adjust, ArrowUp/Down volume, F/Escape fullscreen
                {
                    let engine_k = engine.clone();
                    let paused_k = paused.clone();
                    let canvas_k = canvas_for_click_inner.clone();
                    let master_gain_k = master_gain.clone();
                    let window = web::window().unwrap();
                    let closure = Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                        events::handle_global_keydown(
                            &ev,
                            &engine_k,
                            &paused_k,
                            &master_gain_k,
                            &canvas_k,
                        );
                    }) as Box<dyn FnMut(_)>);
                    window
                        .add_event_listener_with_callback(
                            "keydown",
                            closure.as_ref().unchecked_ref(),
                        )
                        .ok();
                    closure.forget();
                }

                // Mousedown: begin drag if over a voice
                {
                    let hover_m = hover_index.clone();
                    let drag_m = drag_state.clone();
                    let mouse_m = mouse_state.clone();
                    let engine_m = engine.clone();
                    let canvas_target = canvas_for_click_inner.clone();
                    let closure = Closure::wrap(Box::new(move |ev: web::PointerEvent| {
                        if let Some(i) = *hover_m.borrow() {
                            let mut ds = drag_m.borrow_mut();
                            ds.active = true;
                            ds.voice = i;
                            ds.plane_z_world =
                                engine_m.borrow().voices[i].position.z * SPREAD + z_offset_vec3().z;
                            log::info!("[mouse] begin drag on voice {}", i);
                        }
                        mouse_m.borrow_mut().down = true;
                        let _ = canvas_target.set_pointer_capture(ev.pointer_id());
                        // noisy pointer down debug log removed
                        ev.prevent_default();
                    }) as Box<dyn FnMut(_)>);
                    canvas_for_click_inner
                        .add_event_listener_with_callback(
                            "pointerdown",
                            closure.as_ref().unchecked_ref(),
                        )
                        .ok();
                    closure.forget();
                }

                // Mouseup: click actions or end drag; also trigger background tap note+ripple
                {
                    let hover_m = hover_index.clone();
                    let drag_m = drag_state.clone();
                    let mouse_m = mouse_state.clone();
                    let engine_m = engine.clone();
                    let voice_gains_click = voice_gains.clone();
                    let delay_sends_click = delay_sends.clone();
                    let reverb_sends_click = reverb_sends.clone();
                    let canvas_click = canvas_for_click_inner.clone();
                    let audio_ctx_click = audio_ctx.clone();
                    let ripple_queue = queued_ripple_uv.clone();
                    let closure = Closure::wrap(Box::new(move |ev: web::PointerEvent| {
                        let was_dragging = drag_m.borrow().active;
                        if was_dragging {
                            drag_m.borrow_mut().active = false;
                        } else if let Some(i) = *hover_m.borrow() {
                            // Click without drag: modifiers
                            let shift = ev.shift_key();
                            let alt = ev.alt_key();
                            if alt {
                                engine_m.borrow_mut().toggle_solo(i);
                                // noisy click debug log removed
                            } else if shift {
                                engine_m.borrow_mut().reseed_voice(i, None);
                                // noisy click debug log removed
                            } else {
                                engine_m.borrow_mut().toggle_mute(i);
                                // noisy click debug log removed
                            }
                        } else {
                            // Background click: synth one-shot via WebAudio and request a ripple
                            let [uvx, uvy] = input::pointer_canvas_uv(&ev, &canvas_click);
                            if uvx.is_finite() && uvy.is_finite() {
                                let midi = 60.0 + uvx * 24.0;
                                let freq = midi_to_hz(midi as f32);
                                let vel = (0.35 + 0.65 * uvy) as f32;
                                // Precompute normalized voice x for nearest-voice pick
                                let eng = engine_m.borrow();
                                let norm_xs: Vec<f32> = eng
                                    .voices
                                    .iter()
                                    .map(|v| (v.position.x / 3.0).clamp(-1.0, 1.0) * 0.5 + 0.5)
                                    .collect();
                                let best_i = input::nearest_index_by_uvx(&norm_xs, uvx);
                                let dur = 0.35 + 0.25 * (1.0 - uvy as f64);
                                let wf = eng.configs[best_i].waveform;
                                drop(eng);
                                audio::trigger_one_shot(
                                    &audio_ctx_click,
                                    wf,
                                    freq,
                                    vel,
                                    dur,
                                    &voice_gains_click[best_i],
                                    &delay_sends_click[best_i],
                                    &reverb_sends_click[best_i],
                                );
                                *ripple_queue.borrow_mut() = Some([uvx, uvy]);
                            }
                        }
                        // noisy pointer up debug log removed
                        mouse_m.borrow_mut().down = false;
                        ev.prevent_default();
                    }) as Box<dyn FnMut(_)>);
                    if let Some(w) = web::window() {
                        w.add_event_listener_with_callback(
                            "pointerup",
                            closure.as_ref().unchecked_ref(),
                        )
                        .ok();
                    }
                    closure.forget();
                }

                // Scheduler + renderer loop driven by requestAnimationFrame
                let frame_ctx = Rc::new(RefCell::new(frame::FrameContext {
                    engine: engine.clone(),
                    paused: paused.clone(),
                    pulses: pulses.clone(),
                    hover_index: hover_index.clone(),
                    canvas: canvas_for_click_inner.clone(),
                    mouse: mouse_state.clone(),
                    audio_ctx: audio_ctx.clone(),
                    listener: listener_for_tick.clone(),
                    voice_gains: voice_gains.clone(),
                    delay_sends: delay_sends.clone(),
                    reverb_sends: reverb_sends.clone(),
                    voice_panners,
                    reverb_wet: reverb_wet.clone(),
                    delay_wet: delay_wet.clone(),
                    delay_feedback: delay_feedback.clone(),
                    sat_pre: sat_pre.clone(),
                    sat_wet: sat_wet.clone(),
                    sat_dry: sat_dry.clone(),
                    analyser: analyser.clone(),
                    analyser_buf: analyser_buf.clone(),
                    gpu,
                    queued_ripple_uv: queued_ripple_uv.clone(),
                    last_instant: Instant::now(),
                    prev_uv: [0.5, 0.5],
                    swirl_energy: 0.0,
                    swirl_pos: [0.5, 0.5],
                    swirl_vel: [0.0, 0.0],
                    swirl_initialized: false,
                    pulse_energy: [0.0, 0.0, 0.0],
                }));
                let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
                let tick_clone = tick.clone();
                let frame_ctx_tick = frame_ctx.clone();
                *tick.borrow_mut() = Some(Closure::wrap(Box::new(move || {
                    frame_ctx_tick.borrow_mut().frame();
                    if let Some(w) = web::window() {
                        let _ = w.request_animation_frame(
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
                    let _ = w.request_animation_frame(
                        tick.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
                    );
                }
            });
        }
    }

    Ok(())
}

// (local GpuState definition removed; use `render::GpuState` exclusively)
