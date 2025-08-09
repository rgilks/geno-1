#![cfg(target_arch = "wasm32")]
use app_core::{EngineParams, MusicEngine, VoiceConfig, Waveform, C_MAJOR_PENTATONIC};
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

    // On first click, start audio graph and scheduling + WebGPU renderer
    static STARTED: AtomicBool = AtomicBool::new(false);
    {
        let closure = Closure::wrap(Box::new(move || {
            if STARTED.swap(true, Ordering::SeqCst) {
                log::warn!("[gesture] start already triggered; ignoring extra click");
                return;
            }
            // Run async startup in response to user gesture
            let canvas_for_click = canvas_for_click.clone();
            spawn_local(async move {
                // Build AudioContext
                let audio_ctx = match web::AudioContext::new() {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        log::error!("AudioContext error: {:?}", e);
                        return;
                    }
                };
                let listener = audio_ctx.listener();
                listener.set_position(0.0, 0.0, 1.5);

                // Music engine
                let voice_configs = vec![
                    VoiceConfig {
                        color_rgb: [0.9, 0.3, 0.3],
                        waveform: Waveform::Sine,
                        base_position: Vec3::new(-1.0, 0.0, 0.0),
                    },
                    VoiceConfig {
                        color_rgb: [0.3, 0.9, 0.4],
                        waveform: Waveform::Saw,
                        base_position: Vec3::new(1.0, 0.0, 0.0),
                    },
                    VoiceConfig {
                        color_rgb: [0.3, 0.5, 0.9],
                        waveform: Waveform::Triangle,
                        base_position: Vec3::new(0.0, 0.0, -1.0),
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

                // Per-voice master gains -> destination
                let mut voice_gains: Vec<web::GainNode> = Vec::new();
                let mut voice_panners: Vec<web::PannerNode> = Vec::new();
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
                    gain.gain().set_value(0.2);
                    if let Err(e) = gain.connect_with_audio_node(&panner) {
                        log::error!("connect error: {:?}", e);
                        return;
                    }
                    if let Err(e) = panner.connect_with_audio_node(&audio_ctx.destination()) {
                        log::error!("connect error: {:?}", e);
                        return;
                    }
                    voice_gains.push(gain);
                    voice_panners.push(panner);
                }

                // Initialize WebGPU (leak a canvas clone to satisfy 'static lifetime for surface)
                let leaked_canvas = Box::leak(Box::new(canvas_for_click.clone()));
                let mut gpu = match GpuState::new(leaked_canvas).await {
                    Ok(g) => g,
                    Err(e) => {
                        log::error!("WebGPU init error: {:?}", e);
                        return;
                    }
                };

                // Visual pulses per voice
                let pulses = Rc::new(RefCell::new(vec![0.0_f32; engine.borrow().voices.len()]));

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
                }
                let mouse_state = Rc::new(RefCell::new(MouseState::default()));
                let hover_index = Rc::new(RefCell::new(None::<usize>));
                let drag_state = Rc::new(RefCell::new(DragState::default()));

                // Helper: compute ray from screen to world
                let project_to_ray = {
                    let canvas = canvas_for_click.clone();
                    move |sx: f32, sy: f32| -> (Vec3, Vec3) {
                        let width = canvas.width() as f32;
                        let height = canvas.height() as f32;
                        let ndc_x = (2.0 * sx / width) - 1.0;
                        let ndc_y = 1.0 - (2.0 * sy / height);
                        let aspect = width / height.max(1.0);
                        let proj =
                            Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
                        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, Vec3::Y);
                        let inv = (proj * view).inverse();
                        let p_near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
                        let p_far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
                        let p0: Vec3 = p_near.truncate() / p_near.w;
                        let p1: Vec3 = p_far.truncate() / p_far.w;
                        let dir = (p1 - p0).normalize();
                        (p0, dir)
                    }
                };

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

                // Screen -> canvas coords
                let to_canvas_coords = {
                    let canvas = canvas_for_click.clone();
                    move |client_x: f32, client_y: f32| -> Vec2 {
                        let rect = canvas.get_bounding_client_rect();
                        // Convert client (CSS px) to canvas internal pixel coords
                        let x_css = client_x - rect.left() as f32;
                        let y_css = client_y - rect.top() as f32;
                        let sx = (x_css / rect.width() as f32) * canvas.width() as f32;
                        let sy = (y_css / rect.height() as f32) * canvas.height() as f32;
                        Vec2::new(sx, sy)
                    }
                };

                // Mouse move: hover + drag
                {
                    let mouse_state_m = mouse_state.clone();
                    let hover_m = hover_index.clone();
                    let drag_m = drag_state.clone();
                    let engine_m = engine.clone();
                    let canvas = canvas_for_click.clone();
                    let closure = Closure::wrap(Box::new(move |ev: web::MouseEvent| {
                        let pos = to_canvas_coords(ev.client_x() as f32, ev.client_y() as f32);
                        {
                            let mut ms = mouse_state_m.borrow_mut();
                            ms.x = pos.x;
                            ms.y = pos.y;
                        }
                        // Compute hover or drag update
                        let (ro, rd) = project_to_ray(pos.x, pos.y);
                        let mut best = None::<(usize, f32)>;
                        let spread = 1.8f32;
                        let z_offset = Vec3::new(0.0, 0.0, -4.0);
                        for (i, v) in engine_m.borrow().voices.iter().enumerate() {
                            let center_world = v.position * spread + z_offset;
                            if let Some(t) = ray_sphere(ro, rd, center_world, 0.8) {
                                if t >= 0.0 {
                                    match best {
                                        Some((_, bt)) if t >= bt => {}
                                        _ => best = Some((i, t)),
                                    }
                                }
                            }
                        }
                        if drag_m.borrow().active {
                            // Drag to XZ plane (y = 0)
                            let n = Vec3::Y;
                            let plane_p = Vec3::ZERO;
                            let denom = n.dot(rd);
                            if denom.abs() > 1e-4 {
                                let t = n.dot(plane_p - ro) / denom;
                                if t >= 0.0 {
                                    let hit_world = ro + rd * t;
                                    let eng_pos = (hit_world - z_offset) / spread;
                                    let mut eng = engine_m.borrow_mut();
                                    let vi = drag_m.borrow().voice;
                                    eng.set_voice_position(
                                        vi,
                                        Vec3::new(eng_pos.x, 0.0, eng_pos.z),
                                    );
                                    log::info!(
                                        "[drag] voice {} -> world=({:.2},{:.2},{:.2}) engine=({:.2},{:.2},{:.2})",
                                        vi, hit_world.x, hit_world.y, hit_world.z, eng_pos.x, 0.0, eng_pos.z
                                    );
                                }
                            }
                        } else {
                            *hover_m.borrow_mut() = best.map(|(i, _)| i);
                        }
                    }) as Box<dyn FnMut(_)>);
                    canvas
                        .add_event_listener_with_callback(
                            "mousemove",
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
                    let canvas = canvas_for_click.clone();
                    let closure = Closure::wrap(Box::new(move |ev: web::MouseEvent| {
                        if let Some(i) = *hover_m.borrow() {
                            let mut ds = drag_m.borrow_mut();
                            ds.active = true;
                            ds.voice = i;
                            log::info!("[mouse] begin drag on voice {}", i);
                        }
                        mouse_m.borrow_mut().down = true;
                        ev.prevent_default();
                    }) as Box<dyn FnMut(_)>);
                    canvas
                        .add_event_listener_with_callback(
                            "mousedown",
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
                    let canvas = canvas_for_click.clone();
                    let closure = Closure::wrap(Box::new(move |ev: web::MouseEvent| {
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
                        mouse_m.borrow_mut().down = false;
                        ev.prevent_default();
                    }) as Box<dyn FnMut(_)>);
                    canvas
                        .add_event_listener_with_callback(
                            "mouseup",
                            closure.as_ref().unchecked_ref(),
                        )
                        .ok();
                    closure.forget();
                }

                // Scheduler + renderer loop driven by requestAnimationFrame
                let mut last_instant = Instant::now();
                let mut note_events = Vec::new();
                let pulses_tick = pulses.clone();
                let engine_tick = engine.clone();
                let hover_tick = hover_index.clone();
                let canvas_for_tick = canvas_for_click.clone();
                let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
                let tick_clone = tick.clone();
                *tick.borrow_mut() = Some(Closure::wrap(Box::new(move || {
                    let now = Instant::now();
                    let dt = now - last_instant;
                    last_instant = now;
                    let dt_sec = dt.as_secs_f32();

                    let audio_time = audio_ctx.current_time();
                    note_events.clear();
                    engine_tick
                        .borrow_mut()
                        .tick(dt, audio_time, &mut note_events);

                    {
                        let mut ps = pulses_tick.borrow_mut();
                        for ev in &note_events {
                            ps[ev.voice_index] = (ps[ev.voice_index] + ev.velocity as f32).min(1.5);
                        }
                        for p in ps.iter_mut() {
                            *p = (*p - dt_sec * 1.5).max(0.0);
                        }
                        for i in 0..voice_panners.len() {
                            let pos = engine_tick.borrow().voices[i].position;
                            voice_panners[i].set_position(pos.x as f64, pos.y as f64, pos.z as f64);
                        }
                        let e_ref = engine_tick.borrow();
                        let z_offset = Vec3::new(0.0, 0.0, -4.0);
                        let spread = 1.8f32;
                        let positions: Vec<Vec3> = vec![
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
                        let scales: Vec<f32> =
                            vec![1.6 + ps[0] * 0.4, 1.6 + ps[1] * 0.4, 1.6 + ps[2] * 0.4];

                        // Keep WebGPU surface sized to canvas backing size
                        let w = canvas_for_tick.width();
                        let h = canvas_for_tick.height();
                        gpu.resize_if_needed(w, h);
                        if let Err(e) = gpu.render(&positions, &colors, &scales) {
                            log::error!("render error: {:?}", e);
                        }
                    }

                    for ev in &note_events {
                        let src = match web::OscillatorNode::new(&audio_ctx) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        match engine_tick.borrow().configs[ev.voice_index].waveform {
                            Waveform::Sine => src.set_type(web::OscillatorType::Sine),
                            Waveform::Square => src.set_type(web::OscillatorType::Square),
                            Waveform::Saw => src.set_type(web::OscillatorType::Sawtooth),
                            Waveform::Triangle => src.set_type(web::OscillatorType::Triangle),
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
                        let _ = gain
                            .gain()
                            .linear_ramp_to_value_at_time(0.0_f32, t0 + ev.duration_sec as f64);

                        let _ = src.connect_with_audio_node(&gain);
                        let _ = gain.connect_with_audio_node(&voice_gains[ev.voice_index]);

                        let _ = src.start_with_when(t0);
                        let _ = src.stop_with_when(t0 + ev.duration_sec as f64 + 0.02);
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
                }) as Box<dyn FnMut()>));
                if let Some(w) = web::window() {
                    let _ = w.request_animation_frame(
                        tick.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
                    );
                }
            });
        }) as Box<dyn FnMut()>);
        canvas
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
            .map_err(|e| anyhow::anyhow!(format!("{:?}", e)))?;
        closure.forget();
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

struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    quad_vb: wgpu::Buffer,
    instance_vb: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
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
        let format = caps.formats[0];
        let mut config = wgpu::SurfaceConfiguration {
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

        let shader_src = r#"
struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) color: vec4<f32>,
  @location(1) local: vec2<f32>,
  @location(2) pulse: f32,
};
struct Uniforms { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(
  @location(0) v_pos: vec2<f32>,
  @location(1) i_pos: vec3<f32>,
  @location(2) i_scale: f32,
  @location(3) i_color: vec4<f32>,
  @location(4) i_pulse: f32,
) -> VsOut {
  let local_scaled = vec4<f32>(v_pos * i_scale, 0.0, 1.0);
  let world = vec4<f32>(i_pos, 1.0) + local_scaled;
  var out: VsOut;
  out.pos = u.view_proj * world;
  out.color = i_color;
  out.local = v_pos; // unscaled local for shape mask
  out.pulse = i_pulse;
  return out;
}

@fragment
fn fs_main(inf: VsOut) -> @location(0) vec4<f32> {
  // Circular mask within the quad (unit circle of radius 0.5)
  let r = length(inf.local);
  let shape_alpha = 1.0 - smoothstep(0.48, 0.5, r);

  // Emissive pulse boosts brightness subtly
  let emissive = 0.7 * clamp(inf.pulse, 0.0, 1.5);
  let rgb = inf.color.rgb * (1.0 + emissive);
  return vec4<f32>(rgb, shape_alpha * inf.color.a);
}
"#;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
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
        // Instance buffer (capacity for 32 instances)
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_vb"),
            size: (std::mem::size_of::<InstanceData>() * 32) as u64,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
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

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
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
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            pipeline,
            uniform_buffer,
            quad_vb,
            instance_vb,
            bind_group,
            format,
            width,
            height,
        })
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
        }
    }

    fn view_proj(&self) -> [[f32; 4]; 4] {
        let aspect = self.width as f32 / self.height as f32;
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        // Camera a bit back to see three markers comfortably
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y);
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
            &self.uniform_buffer,
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

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rpass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.03,
                        g: 0.04,
                        b: 0.08,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_vertex_buffer(0, self.quad_vb.slice(..));
        rpass.set_vertex_buffer(1, self.instance_vb.slice(..));
        rpass.draw(0..6, 0..(positions.len() as u32));
        drop(rpass);
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
