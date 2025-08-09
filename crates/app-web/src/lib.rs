#![cfg(target_arch = "wasm32")]
use app_core::{EngineParams, MusicEngine, VoiceConfig, Waveform, C_MAJOR_PENTATONIC};
use glam::{Mat4, Vec3, Vec4};
use instant::Instant;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys as web;

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

    // Prepare a clone for use inside the click closure
    let canvas_for_click = canvas.clone();

    // On first click, start audio graph and scheduling + WebGPU renderer
    {
        let closure = Closure::wrap(Box::new(move || {
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
                let mut engine = MusicEngine::new(
                    voice_configs,
                    EngineParams {
                        bpm: 110.0,
                        scale: C_MAJOR_PENTATONIC,
                    },
                    42,
                );

                // Per-voice master gains -> destination
                let mut voice_gains: Vec<web::GainNode> = Vec::new();
                let mut voice_panners: Vec<web::PannerNode> = Vec::new();
                for v in 0..engine.voices.len() {
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
                    let pos = engine.voices[v].position;
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
                let pulses = Rc::new(RefCell::new(vec![0.0_f32; engine.voices.len()]));

                // Scheduler + renderer loop
                let mut last_instant = Instant::now();
                let mut note_events = Vec::new();
                let pulses_tick = pulses.clone();
                let tick = Closure::wrap(Box::new(move || {
                    let now = Instant::now();
                    let dt = now - last_instant;
                    last_instant = now;
                    let dt_sec = dt.as_secs_f32();

                    let audio_time = audio_ctx.current_time();
                    note_events.clear();
                    engine.tick(dt, audio_time, &mut note_events);

                    {
                        let mut ps = pulses_tick.borrow_mut();
                        for ev in &note_events {
                            ps[ev.voice_index] = (ps[ev.voice_index] + ev.velocity as f32).min(1.5);
                        }
                        for p in ps.iter_mut() {
                            *p = (*p - dt_sec * 1.5).max(0.0);
                        }
                        for i in 0..voice_panners.len() {
                            let pos = engine.voices[i].position;
                            voice_panners[i].set_position(pos.x as f64, pos.y as f64, pos.z as f64);
                        }
                        let positions = [
                            engine.voices[0].position,
                            engine.voices[1].position,
                            engine.voices[2].position,
                        ];
                        let colors = [
                            Vec4::from((Vec3::from(engine.configs[0].color_rgb), 1.0)),
                            Vec4::from((Vec3::from(engine.configs[1].color_rgb), 1.0)),
                            Vec4::from((Vec3::from(engine.configs[2].color_rgb), 1.0)),
                        ];
                        let scales = [1.0 + ps[0] * 0.6, 1.0 + ps[1] * 0.6, 1.0 + ps[2] * 0.6];
                        if let Err(e) = gpu.render(&positions, &colors, &scales) {
                            log::error!("render error: {:?}", e);
                        }
                    }

                    for ev in &note_events {
                        let src = match web::OscillatorNode::new(&audio_ctx) {
                            Ok(s) => s,
                            Err(_) => continue,
                        };
                        match engine.configs[ev.voice_index].waveform {
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
                }) as Box<dyn FnMut()>);

                let _ = web::window()
                    .unwrap()
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        tick.as_ref().unchecked_ref(),
                        16,
                    );
                tick.forget();
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
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
}

struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
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
                    required_limits: wgpu::Limits::downlevel_defaults(),
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
struct VsOut { @builtin(position) pos: vec4<f32> };
struct Uniforms { mvp: mat4x4<f32>, color: vec4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var positions = array<vec2<f32>,3>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.0,  0.5)
    );
    let p = positions[idx];
    var out: VsOut;
    out.pos = u.mvp * vec4<f32>(p, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> { return u.color; }
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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

    fn mvp_for(&self, pos: Vec3, scale: f32) -> [[f32; 4]; 4] {
        let aspect = self.width as f32 / self.height as f32;
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 3.0), Vec3::ZERO, Vec3::Y);
        let model = Mat4::from_scale(Vec3::splat(scale)) * Mat4::from_translation(pos);
        (proj * view * model).to_cols_array_2d()
    }

    fn draw_instance(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        mvp: [[f32; 4]; 4],
        color: [f32; 4],
    ) {
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms { mvp, color }),
        );
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rpass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
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
        rpass.draw(0..3, 0..1);
    }

    fn render(
        &mut self,
        positions: &[Vec3; 3],
        colors: &[Vec4; 3],
        scales: &[f32; 3],
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
        // Draw three instances in separate passes (simple)
        for i in 0..3 {
            let mvp = self.mvp_for(positions[i], scales[i]);
            let color = colors[i].to_array();
            self.draw_instance(&mut encoder, &view, mvp, color);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
