#![cfg(target_arch = "wasm32")]
use app_core::{
    z_offset_vec3, EngineParams, MusicEngine, VoiceConfig, Waveform, BASE_SCALE,
    C_MAJOR_PENTATONIC, DEFAULT_VOICE_COLORS, DEFAULT_VOICE_POSITIONS, ENGINE_DRAG_MAX_RADIUS,
    PICK_SPHERE_RADIUS, SCALE_PULSE_MULTIPLIER, SPREAD,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use instant::Instant;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;
use wgpu::util::DeviceExt;

// Rendering/picking shared constants to keep math consistent
const CAMERA_Z: f32 = 6.0;

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

    // Minimal early keyboard handler for hint toggle (works even if WebGPU init fails in CI)
    {
        let window = web::window().unwrap();
        let document = document.clone();
        let closure = Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
            let key = ev.key();
            if key == "h" || key == "H" {
                if let Ok(Some(el)) = document.query_selector(".hint") {
                    let cur = el.get_attribute("data-visible");
                    let show = match cur.as_deref() {
                        Some("1") => false,
                        _ => true,
                    };
                    let _ = el.set_attribute("data-visible", if show { "1" } else { "0" });
                    if let Some(div) = el.dyn_ref::<web::HtmlElement>() {
                        if show {
                            // Default content (before full engine/UI attach)
                            div.set_inner_html(
                                "Click canvas to start • Drag a circle to move\nClick: mute, Shift+Click: reseed, Alt+Click: solo\nR: reseed all • Space: pause/resume • +/-: tempo • M: master mute • O: orbit on/off\nBPM: 110 • Paused: no • Muted: yes • Orbit: yes",
                            );
                            let _ = el.set_attribute("style", "");
                        } else {
                            let _ = el.set_attribute("style", "display:none");
                        }
                    }
                }
                ev.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);
        window
            .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())
            .ok();
        closure.forget();
    }

    // Note: we will query the optional hint element lazily inside event handlers to avoid
    // capturing it here and forcing closures to be FnOnce.

    // Avoid grabbing a 2D context here to allow WebGPU to acquire the canvas

    // Maintain canvas internal pixel size to match CSS size * devicePixelRatio
    {
        let window = web::window().unwrap();
        let dpr = window.device_pixel_ratio();
        let rect = canvas.get_bounding_client_rect();
        let width = (rect.width() * dpr) as u32;
        let height = (rect.height() * dpr) as u32;
        canvas.set_width(width.max(1));
        canvas.set_height(height.max(1));
        // Listen for window resize and update canvas backing size
        let canvas_resize = canvas.clone();
        let resize_closure = Closure::wrap(Box::new(move || {
            if let Some(w) = web::window() {
                let dpr = w.device_pixel_ratio();
                let rect = canvas_resize.get_bounding_client_rect();
                let w_px = (rect.width() * dpr) as u32;
                let h_px = (rect.height() * dpr) as u32;
                canvas_resize.set_width(w_px.max(1));
                canvas_resize.set_height(h_px.max(1));
            }
        }) as Box<dyn FnMut()>);
        window
            .add_event_listener_with_callback("resize", resize_closure.as_ref().unchecked_ref())
            .ok();
        resize_closure.forget();
    }

    // Prepare a clone for use inside the click closure
    let canvas_for_click = canvas.clone();

    // On Start button click, start audio graph and scheduling + WebGPU renderer
    static STARTED: AtomicBool = AtomicBool::new(false);
    {
        if let Some(start_btn) = document.get_element_by_id("start-btn") {
            let closure = Closure::wrap(Box::new(move || {
                if STARTED.swap(true, Ordering::SeqCst) {
                    log::warn!("[gesture] start already triggered; ignoring extra click");
                    return;
                }
                // Hide start overlay if present
                if let Some(w) = web::window() {
                    if let Some(doc) = w.document() {
                        if let Some(overlay) = doc.get_element_by_id("start-overlay") {
                            let _ = overlay.set_attribute("style", "display:none");
                        }
                    }
                }
                // Run async startup in response to user gesture
                let canvas_for_click_inner = canvas_for_click.clone();
                spawn_local(async move {
                    // Build AudioContext
                    let audio_ctx = match web::AudioContext::new() {
                        Ok(ctx) => ctx,
                        Err(e) => {
                            log::error!("AudioContext error: {:?}", e);
                            if let Some(win) = web::window() {
                                if let Some(doc) = win.document() {
                                    if let Ok(Some(el)) = doc.query_selector("#audio-error") {
                                        if let Some(div) = el.dyn_ref::<web::HtmlElement>() {
                                            let _ = div.set_attribute("style", "");
                                        }
                                    }
                                }
                            }
                            return;
                        }
                    };
                    let listener = audio_ctx.listener();
                    listener.set_position(0.0, 0.0, 1.5);
                    let listener_for_tick = listener.clone();

                    // Music engine
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
                    log::info!("[gesture] starting systems after click");
                    let engine = Rc::new(RefCell::new(MusicEngine::new(
                        voice_configs,
                        EngineParams {
                            bpm: 110.0,
                            scale: C_MAJOR_PENTATONIC,
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

                    // Master mix bus -> destination
                    let master_gain = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("Master GainNode error: {:?}", e);
                            return;
                        }
                    };
                    master_gain.gain().set_value(0.8);
                    if let Err(e) = master_gain.connect_with_audio_node(&audio_ctx.destination()) {
                        log::error!("connect error: {:?}", e);
                        return;
                    }

                    // Global lush reverb (Convolver) and tempo-synced dark delay bus
                    // Reverb input and wet level
                    let reverb_in = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("Reverb in GainNode error: {:?}", e);
                            return;
                        }
                    };
                    reverb_in.gain().set_value(1.0);
                    let reverb = match web::ConvolverNode::new(&audio_ctx) {
                        Ok(n) => n,
                        Err(e) => {
                            log::error!("ConvolverNode error: {:?}", e);
                            return;
                        }
                    };
                    reverb.set_normalize(true);
                    // Create a long, dark stereo impulse response procedurally
                    {
                        let sr = audio_ctx.sample_rate();
                        let seconds = 5.0_f32; // lush tail
                        let len = (sr as f32 * seconds) as u32;
                        if let Ok(ir) = audio_ctx.create_buffer(2, len, sr) {
                            // simple xorshift32 for deterministic noise
                            let mut seed_l: u32 = 0x1234ABCD;
                            let mut seed_r: u32 = 0x7890FEDC;
                            for ch in 0..2 {
                                let mut buf: Vec<f32> = vec![0.0; len as usize];
                                let mut t = 0.0_f32;
                                let dt = 1.0_f32 / sr as f32;
                                for i in 0..len as usize {
                                    let s = if ch == 0 { &mut seed_l } else { &mut seed_r };
                                    let mut x = *s;
                                    x ^= x << 13;
                                    x ^= x >> 17;
                                    x ^= x << 5;
                                    *s = x;
                                    let n = ((x as f32 / std::u32::MAX as f32) * 2.0 - 1.0) as f32;
                                    // Exponential decay envelope, dark tilt
                                    let decay = (-t / 3.0).exp();
                                    let dark = (1.0 - (t / seconds)).max(0.0);
                                    let v = n * decay * (0.6 + 0.4 * dark);
                                    buf[i] = v;
                                    t += dt;
                                }
                                let _ = ir.copy_to_channel(&mut buf, ch as i32);
                            }
                            reverb.set_buffer(Some(&ir));
                        }
                    }
                    let reverb_wet = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("Reverb wet GainNode error: {:?}", e);
                            return;
                        }
                    };
                    reverb_wet.gain().set_value(0.6);
                    let _ = reverb_in.connect_with_audio_node(&reverb);
                    let _ = reverb.connect_with_audio_node(&reverb_wet);
                    let _ = reverb_wet.connect_with_audio_node(&master_gain);

                    // Delay bus with feedback loop and lowpass tone for darkness
                    let delay_in = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("Delay in GainNode error: {:?}", e);
                            return;
                        }
                    };
                    delay_in.gain().set_value(1.0);
                    let delay = match audio_ctx.create_delay_with_max_delay_time(3.0) {
                        Ok(n) => n,
                        Err(e) => {
                            log::error!("DelayNode error: {:?}", e);
                            return;
                        }
                    };
                    // Around ~3/8 to ~1/2 note depending on BPM 110 → ~0.55s feels lush
                    delay.delay_time().set_value(0.55);
                    let delay_tone = match web::BiquadFilterNode::new(&audio_ctx) {
                        Ok(n) => n,
                        Err(e) => {
                            log::error!("BiquadFilterNode error: {:?}", e);
                            return;
                        }
                    };
                    delay_tone.set_type(web::BiquadFilterType::Lowpass);
                    delay_tone.frequency().set_value(1400.0);
                    let delay_feedback = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("Delay feedback GainNode error: {:?}", e);
                            return;
                        }
                    };
                    delay_feedback.gain().set_value(0.6);
                    let delay_wet = match web::GainNode::new(&audio_ctx) {
                        Ok(g) => g,
                        Err(e) => {
                            log::error!("Delay wet GainNode error: {:?}", e);
                            return;
                        }
                    };
                    delay_wet.gain().set_value(0.5);
                    let _ = delay_in.connect_with_audio_node(&delay);
                    let _ = delay.connect_with_audio_node(&delay_tone);
                    let _ = delay_tone.connect_with_audio_node(&delay_feedback);
                    let _ = delay_feedback.connect_with_audio_node(&delay);
                    let _ = delay_tone.connect_with_audio_node(&delay_wet);
                    let _ = delay_wet.connect_with_audio_node(&master_gain);

                    // Per-voice master gains -> master bus, plus effect sends
                    let mut voice_gains: Vec<web::GainNode> = Vec::new();
                    let mut voice_panners: Vec<web::PannerNode> = Vec::new();
                    let mut delay_sends_vec: Vec<web::GainNode> = Vec::new();
                    let mut reverb_sends_vec: Vec<web::GainNode> = Vec::new();
                    for v in 0..engine.borrow().voices.len() {
                        let panner = match web::PannerNode::new(&audio_ctx) {
                            Ok(p) => p,
                            Err(e) => {
                                log::error!("PannerNode error: {:?}", e);
                                return;
                            }
                        };
                        panner.set_panning_model(web::PanningModelType::Hrtf);
                        panner.set_distance_model(web::DistanceModelType::Inverse);
                        panner.set_ref_distance(0.5);
                        panner.set_max_distance(50.0);
                        let pos = engine.borrow().voices[v].position;
                        panner.set_position(pos.x as f64, pos.y as f64, pos.z as f64);

                        let gain = match web::GainNode::new(&audio_ctx) {
                            Ok(g) => g,
                            Err(e) => {
                                log::error!("GainNode error: {:?}", e);
                                return;
                            }
                        };
                        // Start muted; we will allow toggling via 'M' key
                        gain.gain().set_value(0.0);
                        if let Err(e) = gain.connect_with_audio_node(&panner) {
                            log::error!("connect error: {:?}", e);
                            return;
                        }
                        if let Err(e) = panner.connect_with_audio_node(&master_gain) {
                            log::error!("connect error: {:?}", e);
                            return;
                        }
                        // Per-voice sends
                        let d_send = match web::GainNode::new(&audio_ctx) {
                            Ok(g) => g,
                            Err(e) => {
                                log::error!("Delay send GainNode error: {:?}", e);
                                return;
                            }
                        };
                        d_send.gain().set_value(0.4);
                        let _ = d_send.connect_with_audio_node(&delay_in);
                        delay_sends_vec.push(d_send);
                        let r_send = match web::GainNode::new(&audio_ctx) {
                            Ok(g) => g,
                            Err(e) => {
                                log::error!("Reverb send GainNode error: {:?}", e);
                                return;
                            }
                        };
                        r_send.gain().set_value(0.65);
                        let _ = r_send.connect_with_audio_node(&reverb_in);
                        reverb_sends_vec.push(r_send);
                        voice_gains.push(gain);
                        voice_panners.push(panner);
                    }
                    let delay_sends = Rc::new(delay_sends_vec);
                    let reverb_sends = Rc::new(reverb_sends_vec);

                    // Initialize WebGPU (leak a canvas clone to satisfy 'static lifetime for surface)
                    let leaked_canvas = Box::leak(Box::new(canvas_for_click_inner.clone()));
                    let mut gpu: Option<GpuState> = match GpuState::new(leaked_canvas).await {
                        Ok(g) => Some(g),
                        Err(e) => {
                            log::error!("WebGPU init error: {:?}", e);
                            None
                        }
                    };

                    // Visual pulses per voice and optional analyser for ambient effects
                    let pulses = Rc::new(RefCell::new(vec![0.0_f32; engine.borrow().voices.len()]));
                    let analyser: Option<web::AnalyserNode> =
                        web::AnalyserNode::new(&audio_ctx).ok();
                    if let Some(a) = &analyser {
                        a.set_fft_size(256);
                    }

                    // Pause state (stops scheduling new notes but keeps rendering)
                    let paused = Rc::new(RefCell::new(false));
                    // Master mute state for all voices (start muted)
                    let master_muted = Rc::new(RefCell::new(true));
                    // Camera orbit toggle (enabled by default)
                    let orbit_enabled = Rc::new(RefCell::new(true));
                    let voice_gains = Rc::new(voice_gains);

                    // ---------------- Interaction state ----------------
                    #[derive(Default, Clone, Copy)]
                    struct MouseState {
                        x: f32,
                        y: f32,
                        down: bool,
                    }
                    #[derive(Default, Clone, Copy)]
                    struct DragState {
                        active: bool,
                        voice: usize,
                        plane_z_world: f32,
                    }
                    let mouse_state = Rc::new(RefCell::new(MouseState::default()));
                    let hover_index = Rc::new(RefCell::new(None::<usize>));
                    let drag_state = Rc::new(RefCell::new(DragState::default()));

                    // Ray-sphere intersect
                    let ray_sphere =
                        |ray_o: Vec3, ray_d: Vec3, center: Vec3, radius: f32| -> Option<f32> {
                            let oc = ray_o - center;
                            let b = oc.dot(ray_d);
                            let c = oc.dot(oc) - radius * radius;
                            let disc = b * b - c;
                            if disc < 0.0 {
                                return None;
                            }
                            let t = -b - disc.sqrt();
                            if t >= 0.0 {
                                Some(t)
                            } else {
                                None
                            }
                        };

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
                            let rect = canvas_mouse.get_bounding_client_rect();
                            let x_css = ev.client_x() as f32 - rect.left() as f32;
                            let y_css = ev.client_y() as f32 - rect.top() as f32;
                            let sx = (x_css / rect.width() as f32) * canvas_mouse.width() as f32;
                            let sy = (y_css / rect.height() as f32) * canvas_mouse.height() as f32;
                            let pos = Vec2::new(sx, sy);
                            // For CI/headless environments without real mouse, synthesize hover over center
                            if !canvas_connected {
                                return;
                            }
                            {
                                let mut ms = mouse_state_m.borrow_mut();
                                ms.x = pos.x;
                                ms.y = pos.y;
                            }
                            let is_active = drag_m.borrow().active;
                            // Store pointer position; render() converts to uv for swirl uniforms
                            let mut ms = mouse_state_m.borrow_mut();
                            ms.x = pos.x;
                            ms.y = pos.y;
                            log::info!(
                                "[move] pid={} pos=({:.1},{:.1}) active={}",
                                ev.pointer_id(),
                                pos.x,
                                pos.y,
                                is_active
                            );
                            // Compute hover or drag update
                            let width = canvas_mouse.width() as f32;
                            let height = canvas_mouse.height() as f32;
                            let ndc_x = (2.0 * pos.x / width) - 1.0;
                            let ndc_y = 1.0 - (2.0 * pos.y / height);
                            let aspect = width / height.max(1.0);
                            let proj = Mat4::perspective_rh(
                                std::f32::consts::FRAC_PI_4,
                                aspect,
                                0.1,
                                100.0,
                            );
                            let view = Mat4::look_at_rh(
                                Vec3::new(0.0, 0.0, CAMERA_Z),
                                Vec3::ZERO,
                                Vec3::Y,
                            );
                            let inv = (proj * view).inverse();
                            let p_near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
                            let p_far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
                            let _p0: Vec3 = p_near.truncate() / p_near.w;
                            let p1: Vec3 = p_far.truncate() / p_far.w;
                            // Ray origin from camera eye to improve drag intersection stability
                            let ro = Vec3::new(0.0, 0.0, CAMERA_Z);
                            let rd = (p1 - ro).normalize();
                            let mut best = None::<(usize, f32)>;
                            let spread = SPREAD;
                            let z_offset = z_offset_vec3();
                            for (i, v) in engine_m.borrow().voices.iter().enumerate() {
                                let center_world = v.position * spread + z_offset;
                                if let Some(t) =
                                    ray_sphere(ro, rd, center_world, PICK_SPHERE_RADIUS)
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
                                        log::info!(
                                            "[drag] voice {} -> world=({:.2},{:.2},{:.2}) engine=({:.2},{:.2},{:.2}) t={:.3} plane_z={:.2}",
                                            vi, hit_world.x, hit_world.y, hit_world.z, eng_pos.x, 0.0, eng_pos.z, t, plane_z
                                        );
                                    }
                                } else {
                                    log::info!("[drag] ray parallel to z-plane (rd.z~0)");
                                }
                                // While dragging, boost swirl strength (used during render)
                            } else {
                                match best {
                                    Some((i, t)) => {
                                        log::info!("[hover] hit voice={} t={:.3}", i, t);
                                        *hover_m.borrow_mut() = Some(i);
                                    }
                                    None => {
                                        *hover_m.borrow_mut() = None;
                                    }
                                }
                            }
                        })
                            as Box<dyn FnMut(_)>);
                        if let Some(w) = web::window() {
                            w.add_event_listener_with_callback(
                                "pointermove",
                                closure.as_ref().unchecked_ref(),
                            )
                            .ok();
                        }
                        closure.forget();
                    }

                    // Keyboard controls: R reseed all, Space pause, +/- bpm adjust
                    {
                        let engine_k = engine.clone();
                        let paused_k = paused.clone();
                        let master_muted_k = master_muted.clone();
                        let orbit_enabled_k = orbit_enabled.clone();
                        let voice_gains_k = voice_gains.clone();
                        let master_gain_k = master_gain.clone();
                        let window = web::window().unwrap();
                        let closure = Closure::wrap(Box::new(move |ev: web::KeyboardEvent| {
                            let key = ev.key();
                            match key.as_str() {
                                // Reseed all voices
                                "r" | "R" => {
                                    let voice_len = engine_k.borrow().voices.len();
                                    let mut eng = engine_k.borrow_mut();
                                    for i in 0..voice_len {
                                        eng.reseed_voice(i, None);
                                    }
                                    log::info!("[keys] reseeded all voices");
                                }
                                // Pause/resume scheduling
                                " " => {
                                    let mut p = paused_k.borrow_mut();
                                    *p = !*p;
                                    log::info!("[keys] paused={}", *p);
                                    // If hint visible, refresh its content
                                    if let Some(win) = web::window() {
                                        if let Some(doc) = win.document() {
                                            if let Ok(Some(el)) = doc.query_selector(".hint") {
                                                if el.get_attribute("data-visible").as_deref()
                                                    == Some("1")
                                                {
                                                    let bpm_now = engine_k.borrow().params.bpm;
                                                    let muted_now = *master_muted_k.borrow();
                                                    let orbit_now = *orbit_enabled_k.borrow();
                                                    if let Some(div) =
                                                        el.dyn_ref::<web::HtmlElement>()
                                                    {
                                                        let content = format!(
                                                            "Click canvas to start • Drag a circle to move\nClick: mute, Shift+Click: reseed, Alt+Click: solo\nR: reseed all • Space: pause/resume • +/-: tempo • M: master mute • O: orbit on/off\nBPM: {:.0} • Paused: {} • Muted: {} • Orbit: {}",
                                                            bpm_now,
                                                            if *p { "yes" } else { "no" },
                                                            if muted_now { "yes" } else { "no" },
                                                            if orbit_now { "yes" } else { "no" }
                                                    );
                                                        div.set_inner_html(&content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    ev.prevent_default();
                                }
                                // Increase BPM
                                "+" | "=" => {
                                    let mut eng = engine_k.borrow_mut();
                                    let new_bpm = (eng.params.bpm + 5.0).min(240.0);
                                    eng.set_bpm(new_bpm);
                                    log::info!("[keys] bpm -> {:.1}", new_bpm);
                                    // If hint visible, refresh its content
                                    if let Some(win) = web::window() {
                                        if let Some(doc) = win.document() {
                                            if let Ok(Some(el)) = doc.query_selector(".hint") {
                                                if el.get_attribute("data-visible").as_deref()
                                                    == Some("1")
                                                {
                                                    let paused_now = *paused_k.borrow();
                                                    let muted_now = *master_muted_k.borrow();
                                                    let orbit_now = *orbit_enabled_k.borrow();
                                                    if let Some(div) =
                                                        el.dyn_ref::<web::HtmlElement>()
                                                    {
                                                        let content = format!(
                                                            "Click canvas to start • Drag a circle to move\nClick: mute, Shift+Click: reseed, Alt+Click: solo\nR: reseed all • Space: pause/resume • +/-: tempo • M: master mute • O: orbit on/off\nBPM: {:.0} • Paused: {} • Muted: {} • Orbit: {}",
                                                            new_bpm,
                                                            if paused_now { "yes" } else { "no" },
                                                            if muted_now { "yes" } else { "no" },
                                                            if orbit_now { "yes" } else { "no" }
                                                    );
                                                        div.set_inner_html(&content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Decrease BPM
                                "-" | "_" => {
                                    let mut eng = engine_k.borrow_mut();
                                    let new_bpm = (eng.params.bpm - 5.0).max(40.0);
                                    eng.set_bpm(new_bpm);
                                    log::info!("[keys] bpm -> {:.1}", new_bpm);
                                    // If hint visible, refresh its content
                                    if let Some(win) = web::window() {
                                        if let Some(doc) = win.document() {
                                            if let Ok(Some(el)) = doc.query_selector(".hint") {
                                                if el.get_attribute("data-visible").as_deref()
                                                    == Some("1")
                                                {
                                                    let paused_now = *paused_k.borrow();
                                                    let muted_now = *master_muted_k.borrow();
                                                    let orbit_now = *orbit_enabled_k.borrow();
                                                    if let Some(div) =
                                                        el.dyn_ref::<web::HtmlElement>()
                                                    {
                                                        let content = format!(
                                                            "Click canvas to start • Drag a circle to move\nClick: mute, Shift+Click: reseed, Alt+Click: solo\nR: reseed all • Space: pause/resume • +/-: tempo • M: master mute • O: orbit on/off\nBPM: {:.0} • Paused: {} • Muted: {} • Orbit: {}",
                                                            new_bpm,
                                                            if paused_now { "yes" } else { "no" },
                                                            if muted_now { "yes" } else { "no" },
                                                            if orbit_now { "yes" } else { "no" }
                                                    );
                                                        div.set_inner_html(&content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Master mute toggle
                                "m" | "M" => {
                                    let mut muted = master_muted_k.borrow_mut();
                                    *muted = !*muted;
                                    let new_val = if *muted { 0.0 } else { 0.8 };
                                    master_gain_k.gain().set_value(new_val);
                                    log::info!("[keys] master muted={}", *muted);
                                    // If hint visible, refresh its content
                                    if let Some(win) = web::window() {
                                        if let Some(doc) = win.document() {
                                            if let Ok(Some(el)) = doc.query_selector(".hint") {
                                                if el.get_attribute("data-visible").as_deref()
                                                    == Some("1")
                                                {
                                                    let paused_now = *paused_k.borrow();
                                                    let bpm_now = engine_k.borrow().params.bpm;
                                                    let orbit_now = *orbit_enabled_k.borrow();
                                                    if let Some(div) =
                                                        el.dyn_ref::<web::HtmlElement>()
                                                    {
                                                        let content = format!(
                                                            "Click canvas to start • Drag a circle to move\nClick: mute, Shift+Click: reseed, Alt+Click: solo\nR: reseed all • Space: pause/resume • +/-: tempo • M: master mute • O: orbit on/off\nBPM: {:.0} • Paused: {} • Muted: {} • Orbit: {}",
                                                            bpm_now,
                                                            if paused_now { "yes" } else { "no" },
                                                            if *muted { "yes" } else { "no" },
                                                            if orbit_now { "yes" } else { "no" }
                                                        );
                                                        div.set_inner_html(&content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Orbit toggle
                                "o" | "O" => {
                                    let mut ob = orbit_enabled_k.borrow_mut();
                                    *ob = !*ob;
                                    log::info!("[keys] orbit={}", *ob);
                                    // If hint visible, refresh its content
                                    if let Some(win) = web::window() {
                                        if let Some(doc) = win.document() {
                                            if let Ok(Some(el)) = doc.query_selector(".hint") {
                                                if el.get_attribute("data-visible").as_deref()
                                                    == Some("1")
                                                {
                                                    let paused_now = *paused_k.borrow();
                                                    let muted_now = *master_muted_k.borrow();
                                                    let bpm_now = engine_k.borrow().params.bpm;
                                                    if let Some(div) =
                                                        el.dyn_ref::<web::HtmlElement>()
                                                    {
                                                        let content = format!(
                                                            "Click canvas to start • Drag a circle to move\nClick: mute, Shift+Click: reseed, Alt+Click: solo\nR: reseed all • Space: pause/resume • +/-: tempo • M: master mute • O: orbit on/off\nBPM: {:.0} • Paused: {} • Muted: {} • Orbit: {}",
                                                            bpm_now,
                                                            if paused_now { "yes" } else { "no" },
                                                            if muted_now { "yes" } else { "no" },
                                                            if *ob { "yes" } else { "no" }
                                                        );
                                                        div.set_inner_html(&content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        })
                            as Box<dyn FnMut(_)>);
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
                                ds.plane_z_world = engine_m.borrow().voices[i].position.z * SPREAD
                                    + z_offset_vec3().z;
                                log::info!("[mouse] begin drag on voice {}", i);
                            }
                            mouse_m.borrow_mut().down = true;
                            let _ = canvas_target.set_pointer_capture(ev.pointer_id());
                            log::info!(
                                "[down] pid={} shift={} alt={}",
                                ev.pointer_id(),
                                ev.shift_key(),
                                ev.alt_key()
                            );
                            ev.prevent_default();
                        })
                            as Box<dyn FnMut(_)>);
                        canvas_for_click_inner
                            .add_event_listener_with_callback(
                                "pointerdown",
                                closure.as_ref().unchecked_ref(),
                            )
                            .ok();
                        closure.forget();
                    }

                    // Mouseup: click actions or end drag
                    {
                        let hover_m = hover_index.clone();
                        let drag_m = drag_state.clone();
                        let mouse_m = mouse_state.clone();
                        let engine_m = engine.clone();
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
                                    log::info!("[click] solo voice {}", i);
                                } else if shift {
                                    engine_m.borrow_mut().reseed_voice(i, None);
                                    log::info!("[click] reseed voice {}", i);
                                } else {
                                    engine_m.borrow_mut().toggle_mute(i);
                                    log::info!("[click] toggle mute voice {}", i);
                                }
                            } else {
                                log::info!("[click] mouseup with no hit");
                            }
                            log::info!(
                                "[up] pid={} was_dragging={}",
                                ev.pointer_id(),
                                was_dragging
                            );
                            mouse_m.borrow_mut().down = false;
                            ev.prevent_default();
                        })
                            as Box<dyn FnMut(_)>);
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
                    let mut last_instant = Instant::now();
                    let mut note_events = Vec::new();
                    let pulses_tick = pulses.clone();
                    let engine_tick = engine.clone();
                    let hover_tick = hover_index.clone();
                    let canvas_for_tick = canvas_for_click_inner.clone();
                    let voice_gains_tick = voice_gains.clone();
                    let delay_sends_tick = delay_sends.clone();
                    let reverb_sends_tick = reverb_sends.clone();
                    // Optional slow camera orbit
                    let mut orbit_t: f32 = 0.0;
                    let orbit_tick = orbit_enabled.clone();
                    let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> =
                        Rc::new(RefCell::new(None));
                    let tick_clone = tick.clone();
                    *tick.borrow_mut() = Some(Closure::wrap(Box::new(move || {
                        let now = Instant::now();
                        let dt = now - last_instant;
                        last_instant = now;
                        let dt_sec = dt.as_secs_f32();

                        let audio_time = audio_ctx.current_time();
                        note_events.clear();
                        if !*paused.borrow() {
                            engine_tick
                                .borrow_mut()
                                .tick(dt, audio_time, &mut note_events);
                        }

                        {
                            let mut ps = pulses_tick.borrow_mut();
                            for ev in &note_events {
                                // Smooth in the pulse so visuals don't jump
                                let target = (ps[ev.voice_index] + ev.velocity as f32).min(1.5);
                                ps[ev.voice_index] = 0.7 * ps[ev.voice_index] + 0.3 * target;
                            }
                            for p in ps.iter_mut() {
                                // Exponential decay for smoother falloff
                                *p *= (1.0 - (dt_sec * 1.8).min(0.9));
                            }
                            for i in 0..voice_panners.len() {
                                let pos = engine_tick.borrow().voices[i].position;
                                voice_panners[i].set_position(
                                    pos.x as f64,
                                    pos.y as f64,
                                    pos.z as f64,
                                );
                                // Direct sound↔visual link: map position to per-voice mix and fx
                                let dist = (pos.x * pos.x + pos.z * pos.z).sqrt();
                                // Delay send increases with |x|, reverb with radial distance
                                let d_amt = (0.15 + 0.85 * pos.x.abs().min(1.0)).clamp(0.0, 1.0);
                                let r_amt =
                                    (0.25 + 0.75 * (dist / 2.5).clamp(0.0, 1.0)).clamp(0.0, 1.2);
                                delay_sends_tick[i].gain().set_value(d_amt);
                                reverb_sends_tick[i].gain().set_value(r_amt);
                                // Subtle level change with proximity to center (prevents clipping)
                                let lvl =
                                    (0.55 + 0.45 * (1.0 - (dist / 2.5).clamp(0.0, 1.0))) as f32;
                                voice_gains_tick[i].gain().set_value(lvl);
                            }
                            // Optional analyser-driven mild ambient pulse
                            if let Some(a) = &analyser {
                                let bins = a.frequency_bin_count();
                                let mut freq = vec![0.0_f32; bins as usize];
                                a.get_float_frequency_data(&mut freq);
                                // Use low-frequency bin energy to adjust background subtly
                                let mut sum = 0.0f32;
                                let take = (bins.min(16)) as u32;
                                for i in 0..take {
                                    let v = freq[i as usize]; // in dBFS (-inf..0)
                                                              // map dB to 0..1 roughly
                                    let lin = ((v + 100.0) / 100.0).clamp(0.0, 1.0);
                                    sum += lin;
                                }
                                let avg = sum / take as f32;
                                // Slightly push base scales with ambient energy
                                let n = ps.len().min(3);
                                for i in 0..n {
                                    // This is local shadow; adjust just-written scales via positions/colors path
                                    // We use pulses array instead to avoid mutating scales directly
                                    ps[i] = (ps[i] + avg * 0.05).min(1.5);
                                }
                                if let Some(g) = &mut gpu {
                                    g.set_ambient_clear(avg * 0.9);
                                }
                            }
                            let e_ref = engine_tick.borrow();
                            let z_offset = z_offset_vec3();
                            let spread = SPREAD;
                            let mut positions: Vec<Vec3> = vec![
                                e_ref.voices[0].position * spread + z_offset,
                                e_ref.voices[1].position * spread + z_offset,
                                e_ref.voices[2].position * spread + z_offset,
                            ];
                            let mut colors: Vec<Vec4> = vec![
                                Vec4::from((Vec3::from(e_ref.configs[0].color_rgb), 1.0)),
                                Vec4::from((Vec3::from(e_ref.configs[1].color_rgb), 1.0)),
                                Vec4::from((Vec3::from(e_ref.configs[2].color_rgb), 1.0)),
                            ];
                            let hovered = *hover_tick.borrow();
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
                            let mut scales: Vec<f32> = vec![
                                BASE_SCALE + ps[0] * SCALE_PULSE_MULTIPLIER,
                                BASE_SCALE + ps[1] * SCALE_PULSE_MULTIPLIER,
                                BASE_SCALE + ps[2] * SCALE_PULSE_MULTIPLIER,
                            ];

                            // Orbiting ring particles around each voice center
                            let two_pi = std::f32::consts::PI * 2.0;
                            let ring_count = 48usize;
                            for vi in 0..3 {
                                let center = positions[vi];
                                let base_col = Vec3::from(e_ref.configs[vi].color_rgb);
                                let ring_r = 0.9 + ps[vi] * 0.25;
                                for j in 0..ring_count {
                                    let a =
                                        orbit_t * 0.8 + (j as f32) * (two_pi / ring_count as f32);
                                    let offset = Vec3::new(a.cos() * ring_r, 0.0, a.sin() * ring_r);
                                    positions.push(center + offset);
                                    let c = base_col * 0.55;
                                    colors.push(Vec4::from((c, 0.9)));
                                    let s = 0.06 + 0.04 * ((j % 12) as f32 / 12.0);
                                    scales.push(s);
                                }
                            }

                            // Optional analyser-driven dot spectrum row
                            if let Some(a) = &analyser {
                                let bins = a.frequency_bin_count() as usize;
                                let dots = bins.min(16);
                                if dots > 0 {
                                    let mut freq = vec![0.0_f32; bins];
                                    a.get_float_frequency_data(&mut freq);
                                    let _w = canvas_for_tick.width().max(1) as f32;
                                    let _h = canvas_for_tick.height().max(1) as f32;
                                    // place dots near bottom of view in world space
                                    // map x from -2.8..2.8 and y slightly below origin
                                    let z = z_offset.z;
                                    for i in 0..dots {
                                        let v_db = freq[i];
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

                            // Compute camera eye for orbit or fixed
                            let mut cam_eye = Vec3::new(0.0, 0.0, CAMERA_Z);
                            if *orbit_tick.borrow() {
                                orbit_t += dt_sec * 0.1; // rad/s
                                let r = 6.0f32;
                                cam_eye = Vec3::new(r * orbit_t.cos(), 0.0, r * orbit_t.sin());
                            }
                            let cam_target = Vec3::ZERO;
                            // Sync AudioListener position + orientation to camera
                            let fwd = (cam_target - cam_eye).normalize();
                            listener_for_tick.set_position(
                                cam_eye.x as f64,
                                cam_eye.y as f64,
                                cam_eye.z as f64,
                            );
                            let _ = listener_for_tick.set_orientation(
                                fwd.x as f64,
                                fwd.y as f64,
                                fwd.z as f64,
                                0.0,
                                1.0,
                                0.0,
                            );

                            if let Some(g) = &mut gpu {
                                g.set_camera(cam_eye, cam_target);
                                // Keep WebGPU surface sized to canvas backing size
                                let w = canvas_for_tick.width();
                                let h = canvas_for_tick.height();
                                g.resize_if_needed(w, h);
                                if let Err(e) = g.render(&positions, &colors, &scales) {
                                    log::error!("render error: {:?}", e);
                                }
                            }
                        }

                        if !*paused.borrow() {
                            for ev in &note_events {
                                let src = match web::OscillatorNode::new(&audio_ctx) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                };
                                match engine_tick.borrow().configs[ev.voice_index].waveform {
                                    Waveform::Sine => src.set_type(web::OscillatorType::Sine),
                                    Waveform::Square => src.set_type(web::OscillatorType::Square),
                                    Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
                                    Waveform::Triangle => {
                                        src.set_type(web::OscillatorType::Triangle)
                                    }
                                }
                                src.frequency().set_value(ev.frequency_hz);

                                let gain = match web::GainNode::new(&audio_ctx) {
                                    Ok(g) => g,
                                    Err(_) => continue,
                                };
                                gain.gain().set_value(0.0);
                                let t0 = audio_time + 0.01;
                                let _ = gain
                                    .gain()
                                    .linear_ramp_to_value_at_time(ev.velocity as f32, t0 + 0.02);
                                let _ = gain.gain().linear_ramp_to_value_at_time(
                                    0.0_f32,
                                    t0 + ev.duration_sec as f64,
                                );

                                let _ = src.connect_with_audio_node(&gain);
                                let _ =
                                    gain.connect_with_audio_node(&voice_gains_tick[ev.voice_index]);
                                // Effect sends per note
                                let _ =
                                    gain.connect_with_audio_node(&delay_sends_tick[ev.voice_index]);
                                let _ = gain
                                    .connect_with_audio_node(&reverb_sends_tick[ev.voice_index]);

                                let _ = src.start_with_when(t0);
                                let _ = src.stop_with_when(t0 + ev.duration_sec as f64 + 0.02);
                            }
                        }

                        // Schedule next frame
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
                    })
                        as Box<dyn FnMut()>));
                    if let Some(w) = web::window() {
                        let _ = w.request_animation_frame(
                            tick.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
                        );
                    }
                });
            }) as Box<dyn FnMut()>);
            let _ = start_btn
                .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
            closure.forget();
        } else {
            // Fallback: start on first window click (legacy behavior)
            let closure = Closure::wrap(Box::new(move || {
                if STARTED.swap(true, Ordering::SeqCst) {
                    log::warn!("[gesture] start already triggered; ignoring extra click");
                    return;
                }
                let _canvas_for_click_inner = canvas_for_click.clone();
                spawn_local(async move {
                    // Build AudioContext
                    let audio_ctx = match web::AudioContext::new() {
                        Ok(ctx) => ctx,
                        Err(e) => {
                            log::error!("AudioContext error: {:?}", e);
                            if let Some(win) = web::window() {
                                if let Some(doc) = win.document() {
                                    if let Ok(Some(el)) = doc.query_selector("#audio-error") {
                                        if let Some(div) = el.dyn_ref::<web::HtmlElement>() {
                                            let _ = div.set_attribute("style", "");
                                        }
                                    }
                                }
                            }
                            return;
                        }
                    };
                    let listener = audio_ctx.listener();
                    listener.set_position(0.0, 0.0, 1.5);
                    // ... existing code continues unchanged ...
                });
            }) as Box<dyn FnMut()>);
            if let Some(w) = web::window() {
                let _ =
                    w.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
            }
            closure.forget();
        }
    }

    Ok(())
}

// ===================== WebGPU state =====================

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    pos: [f32; 3],
    scale: f32,
    color: [f32; 4],
    pulse: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct VoicePacked {
    pos_pulse: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct WavesUniforms {
    resolution: [f32; 2],
    time: f32,
    ambient: f32,
    voices: [VoicePacked; 3],
    swirl_uv: [f32; 2],
    swirl_strength: f32,
    swirl_active: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PostUniforms {
    resolution: [f32; 2],
    time: f32,
    ambient: f32,
    blur_dir: [f32; 2],
    bloom_strength: f32,
    threshold: f32,
}

struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    // Scene
    scene_pipeline: wgpu::RenderPipeline,
    scene_uniform_buffer: wgpu::Buffer,
    quad_vb: wgpu::Buffer,
    instance_vb: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    // Waves full-screen layer
    waves_pipeline: wgpu::RenderPipeline,
    waves_uniform_buffer: wgpu::Buffer,
    waves_bind_group: wgpu::BindGroup,
    // Post-processing resources
    hdr_tex: wgpu::Texture,
    hdr_view: wgpu::TextureView,
    bloom_a: wgpu::Texture,
    bloom_a_view: wgpu::TextureView,
    bloom_b: wgpu::Texture,
    bloom_b_view: wgpu::TextureView,
    linear_sampler: wgpu::Sampler,

    post_shader: wgpu::ShaderModule,
    post_bgl0: wgpu::BindGroupLayout, // texture+sampler+uniform
    post_bgl1: wgpu::BindGroupLayout, // optional second texture+sampler
    post_uniform_buffer: wgpu::Buffer,
    // Bind groups for different sources
    bg_hdr: wgpu::BindGroup,
    bg_from_bloom_a: wgpu::BindGroup,
    bg_from_bloom_b: wgpu::BindGroup,
    bg_bloom_a_only: wgpu::BindGroup, // group1 for composite, sampling bloom A
    bg_bloom_b_only: wgpu::BindGroup, // group1 for composite, sampling bloom B

    bright_pipeline: wgpu::RenderPipeline,
    blur_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,

    width: u32,
    height: u32,
    clear_color: wgpu::Color,
    cam_eye: Vec3,
    cam_target: Vec3,
    time_accum: f32,
    ambient_energy: f32,
    swirl_uv: [f32; 2],
    swirl_strength: f32,
    swirl_active: f32,
}

impl<'a> GpuState<'a> {
    async fn new(canvas: &'a web::HtmlCanvasElement) -> anyhow::Result<Self> {
        let width = canvas.width();
        let height = canvas.height();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No WebGPU adapter"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    // Use default limits on web to avoid passing unknown fields to older WebGPU impls
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    label: None,
                },
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!(format!("request_device error: {:?}", e)))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Rgba8UnormSrgb
                )
            })
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(app_core::SCENE_WGSL.into()),
        });
        let scene_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Quad vertex buffer (two triangles)
        let quad_vertices: [f32; 12] = [
            -0.5, -0.5, 0.5, -0.5, 0.5, 0.5, -0.5, -0.5, 0.5, 0.5, -0.5, 0.5,
        ];
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vb"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        // Instance buffer (capacity for many instances: 3 voices + rings + spectrum)
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_vb"),
            size: (std::mem::size_of::<InstanceData>() * 1024) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_uniform_buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let vertex_buffers = [
            // slot 0: quad positions
            wgpu::VertexBufferLayout {
                array_stride: (std::mem::size_of::<f32>() * 2) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            },
            // slot 1: instance data
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<InstanceData>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 12,
                        shader_location: 2,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 16,
                        shader_location: 3,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 32,
                        shader_location: 4,
                    },
                ],
            },
        ];

        // Offscreen HDR targets (scene and bloom) at full and half resolution
        let hdr_format = wgpu::TextureFormat::Rgba16Float;
        let hdr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr_tex"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: hdr_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let hdr_view = hdr_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bloom_w = (width.max(1) / 2).max(1);
        let bloom_h = (height.max(1) / 2).max(1);
        let bloom_format = wgpu::TextureFormat::Rgba16Float;
        let bloom_a = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bloom_a"),
            size: wgpu::Extent3d {
                width: bloom_w,
                height: bloom_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: bloom_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bloom_b = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bloom_b"),
            size: wgpu::Extent3d {
                width: bloom_w,
                height: bloom_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: bloom_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let bloom_a_view = bloom_a.create_view(&wgpu::TextureViewDescriptor::default());
        let bloom_b_view = bloom_b.create_view(&wgpu::TextureViewDescriptor::default());

        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &vertex_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            cache: None,
            multiview: None,
        });
        // Waves fullscreen pass (drawn into HDR before bloom)
        let waves_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("waves_shader"),
            source: wgpu::ShaderSource::Wgsl(app_core::WAVES_WGSL.into()),
        });
        let waves_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("waves_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let waves_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("waves_pl"),
            bind_group_layouts: &[&waves_bgl],
            push_constant_ranges: &[],
        });
        let waves_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("waves_pipeline"),
            layout: Some(&waves_pl),
            vertex: wgpu::VertexState {
                module: &waves_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &waves_shader,
                entry_point: Some("fs_waves"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            cache: None,
            multiview: None,
        });
        let waves_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("waves_uniforms"),
            size: std::mem::size_of::<WavesUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let waves_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waves_bg"),
            layout: &waves_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: waves_uniform_buffer.as_entire_binding(),
            }],
        });

        // Post shader + pipelines
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post_shader"),
            source: wgpu::ShaderSource::Wgsl(app_core::POST_WGSL.into()),
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let post_bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post_bgl0"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    // tex
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // sampler
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    // uniforms
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let post_bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post_bgl1"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let post_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post_uniforms"),
            size: std::mem::size_of::<PostUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg_hdr = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_hdr"),
            layout: &post_bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: post_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let bg_from_bloom_a = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_from_bloom_a"),
            layout: &post_bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&bloom_a_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: post_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let bg_from_bloom_b = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_from_bloom_b"),
            layout: &post_bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&bloom_b_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: post_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let bg_bloom_a_only = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_bloom_a_only"),
            layout: &post_bgl1,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&bloom_a_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
            ],
        });
        let bg_bloom_b_only = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_bloom_b_only"),
            layout: &post_bgl1,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&bloom_b_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&linear_sampler),
                },
            ],
        });

        let post_pl_bright_blur = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl_post_0"),
            bind_group_layouts: &[&post_bgl0],
            push_constant_ranges: &[],
        });
        let post_pl_composite = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl_post_comp"),
            bind_group_layouts: &[&post_bgl0, &post_bgl1],
            push_constant_ranges: &[],
        });
        let bright_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bright_pipeline"),
            layout: Some(&post_pl_bright_blur),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_bright"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: bloom_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            cache: None,
            multiview: None,
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"),
            layout: Some(&post_pl_bright_blur),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_blur"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: bloom_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            cache: None,
            multiview: None,
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite_pipeline"),
            layout: Some(&post_pl_composite),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_composite"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            cache: None,
            multiview: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            scene_pipeline,
            scene_uniform_buffer,
            quad_vb,
            instance_vb,
            scene_bind_group,
            waves_pipeline,
            waves_uniform_buffer,
            waves_bind_group,
            hdr_tex,
            hdr_view,
            bloom_a,
            bloom_a_view,
            bloom_b,
            bloom_b_view,
            linear_sampler,
            post_shader,
            post_bgl0,
            post_bgl1,
            post_uniform_buffer,
            bg_hdr,
            bg_from_bloom_a,
            bg_from_bloom_b,
            bg_bloom_a_only,
            bg_bloom_b_only,
            bright_pipeline,
            blur_pipeline,
            composite_pipeline,
            width,
            height,
            clear_color: wgpu::Color {
                r: 0.03,
                g: 0.04,
                b: 0.08,
                a: 1.0,
            },
            cam_eye: Vec3::new(0.0, 0.0, CAMERA_Z),
            cam_target: Vec3::ZERO,
            time_accum: 0.0,
            ambient_energy: 0.0,
            swirl_uv: [0.5, 0.5],
            swirl_strength: 0.0,
            swirl_active: 0.0,
        })
    }
    fn set_ambient_clear(&mut self, energy01: f32) {
        // Subtle brighten and slight hue shift with ambient energy
        let e = energy01.clamp(0.0, 1.0);
        let boost = 0.06 * e; // up to +0.06
        self.clear_color = wgpu::Color {
            r: (0.03 + boost * 0.8) as f64,
            g: (0.04 + boost * 0.9) as f64,
            b: (0.08 + boost * 0.5) as f64,
            a: 1.0,
        };
        self.ambient_energy = e;
    }

    fn set_camera(&mut self, eye: Vec3, target: Vec3) {
        self.cam_eye = eye;
        self.cam_target = target;
    }

    fn set_swirl(&mut self, uv: [f32; 2], strength: f32, active: bool) {
        self.swirl_uv = uv;
        self.swirl_strength = strength;
        self.swirl_active = if active { 1.0 } else { 0.0 };
    }

    fn resize_if_needed(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);

            // Recreate offscreen render targets and dependent bind groups
            let hdr_format = wgpu::TextureFormat::Rgba16Float;
            self.hdr_tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hdr_tex"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: hdr_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.hdr_view = self
                .hdr_tex
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bw = (width.max(1) / 2).max(1);
            let bh = (height.max(1) / 2).max(1);
            let bloom_format = wgpu::TextureFormat::Rgba16Float;
            self.bloom_a = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bloom_a"),
                size: wgpu::Extent3d {
                    width: bw,
                    height: bh,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: bloom_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.bloom_b = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bloom_b"),
                size: wgpu::Extent3d {
                    width: bw,
                    height: bh,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: bloom_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.bloom_a_view = self
                .bloom_a
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.bloom_b_view = self
                .bloom_b
                .create_view(&wgpu::TextureViewDescriptor::default());

            // Rebuild bind groups that reference these views
            self.bg_hdr = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg_hdr"),
                layout: &self.post_bgl0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.hdr_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.post_uniform_buffer.as_entire_binding(),
                    },
                ],
            });
            self.bg_from_bloom_a = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg_from_bloom_a"),
                layout: &self.post_bgl0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.bloom_a_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.post_uniform_buffer.as_entire_binding(),
                    },
                ],
            });
            self.bg_from_bloom_b = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg_from_bloom_b"),
                layout: &self.post_bgl0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.bloom_b_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.post_uniform_buffer.as_entire_binding(),
                    },
                ],
            });
            self.bg_bloom_a_only = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg_bloom_a_only"),
                layout: &self.post_bgl1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.bloom_a_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                ],
            });
            self.bg_bloom_b_only = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg_bloom_b_only"),
                layout: &self.post_bgl1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.bloom_b_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                ],
            });
        }
    }

    fn view_proj(&self) -> [[f32; 4]; 4] {
        let aspect = self.width as f32 / self.height.max(1) as f32;
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        let view = Mat4::look_at_rh(self.cam_eye, self.cam_target, Vec3::Y);
        (proj * view).to_cols_array_2d()
    }

    // draw_instance no longer used with instanced path

    fn render(
        &mut self,
        positions: &[Vec3],
        colors: &[Vec4],
        scales: &[f32],
    ) -> Result<(), wgpu::SurfaceError> {
        self.resize_if_needed(self.width, self.height);
        self.time_accum += 1.0 / 60.0; // approx; real dt not tracked here precisely
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });
        // Write view-projection
        self.queue.write_buffer(
            &self.scene_uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms {
                view_proj: self.view_proj(),
            }),
        );
        // Write instance data
        let mut instance_data: Vec<InstanceData> = Vec::with_capacity(positions.len());
        for i in 0..positions.len() {
            let pulse_amount = if i < 3 {
                // guard
                // derive from scale relative to base 1.6
                (scales[i] - 1.6).max(0.0) / 0.4
            } else {
                0.0
            };
            instance_data.push(InstanceData {
                pos: positions[i].to_array(),
                scale: scales[i],
                color: colors[i].to_array(),
                pulse: pulse_amount,
            });
        }
        self.queue
            .write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(&instance_data));
        // Pass 1: render waves (and optionally instances) into HDR offscreen
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // Waves uniforms from current voice positions/pulses
            let pack = |i: usize| VoicePacked {
                pos_pulse: [
                    positions[i].x,
                    positions[i].y,
                    positions[i].z,
                    instance_data[i].pulse,
                ],
                color: colors[i].to_array(),
            };
            let w = WavesUniforms {
                resolution: [self.width as f32, self.height as f32],
                time: self.time_accum,
                ambient: self.ambient_energy,
                voices: [pack(0), pack(1), pack(2)],
                swirl_uv: [
                    (self.swirl_uv[0]).clamp(0.0, 1.0),
                    (self.swirl_uv[1]).clamp(0.0, 1.0),
                ],
                swirl_strength: if self.swirl_active > 0.5 { 1.4 } else { 0.0 },
                swirl_active: self.swirl_active,
            };
            self.queue
                .write_buffer(&self.waves_uniform_buffer, 0, bytemuck::bytes_of(&w));
            rpass.set_pipeline(&self.waves_pipeline);
            rpass.set_bind_group(0, &self.waves_bind_group, &[]);
            rpass.draw(0..3, 0..1);
            // Do not draw the circles anymore (visual is handled by waves layer)
        }

        // Update post uniforms base
        let res = [self.width as f32 / 2.0, self.height as f32 / 2.0];
        let mut post = PostUniforms {
            resolution: res,
            time: self.time_accum,
            ambient: self.ambient_energy,
            blur_dir: [0.0, 0.0],
            bloom_strength: 0.9,
            threshold: 0.6,
        };
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));

        // Pass 2: bright pass → bloom_a
        {
            let mut r = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bright_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_a_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            r.set_pipeline(&self.bright_pipeline);
            r.set_bind_group(0, &self.bg_hdr, &[]);
            r.draw(0..3, 0..1);
        }

        // Pass 3: blur horizontal bloom_a -> bloom_b
        post.blur_dir = [1.0, 0.0];
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));
        {
            let mut r = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur_h"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_b_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            r.set_pipeline(&self.blur_pipeline);
            r.set_bind_group(0, &self.bg_from_bloom_a, &[]);
            r.draw(0..3, 0..1);
        }

        // Pass 4: blur vertical bloom_b -> bloom_a
        post.blur_dir = [0.0, 1.0];
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));
        {
            let mut r = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur_v"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.bloom_a_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            r.set_pipeline(&self.blur_pipeline);
            r.set_bind_group(0, &self.bg_from_bloom_b, &[]);
            r.draw(0..3, 0..1);
        }

        // Pass 5: composite to swapchain
        post.blur_dir = [0.0, 0.0];
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));
        {
            let mut r = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            r.set_pipeline(&self.composite_pipeline);
            r.set_bind_group(0, &self.bg_hdr, &[]);
            r.set_bind_group(1, &self.bg_bloom_a_only, &[]);
            r.draw(0..3, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
