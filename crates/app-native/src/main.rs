use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;
use winit::{event::*, event_loop::EventLoop, window::WindowBuilder};

use app_core::{
    z_offset_vec3, EngineParams, MusicEngine, VoiceConfig, Waveform, BASE_SCALE,
    C_MAJOR_PENTATONIC, DEFAULT_VOICE_COLORS, DEFAULT_VOICE_POSITIONS, PICK_SPHERE_RADIUS, SPREAD,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glam::{Mat4, Vec3, Vec4};

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

#[derive(Default, Clone)]
struct VisState {
    positions: [Vec3; 3],
    colors: [Vec4; 3],
    pulses: [f32; 3],
}

struct GpuState<'w> {
    window: &'w winit::window::Window,
    surface: wgpu::Surface<'w>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    quad_vb: wgpu::Buffer,
    instance_vb: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    last_frame: Instant,
    shared: Arc<Mutex<VisState>>,
    // Local snapshot to render when shared state is locked by audio thread
    last_vis_snapshot: VisState,
}

impl<'w> GpuState<'w> {
    async fn new(
        window: &'w winit::window::Window,
        shared: Arc<Mutex<VisState>>,
    ) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No GPU adapter"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    label: None,
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps.formats[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader_source: &str = app_core::SCENE_WGSL;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Quad vertices for two triangles
        let quad_vertices: [f32; 12] = [
            -0.5, -0.5, 0.5, -0.5, 0.5, 0.5, -0.5, -0.5, 0.5, 0.5, -0.5, 0.5,
        ];
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vb"),
            contents: bytemuck::cast_slice(&quad_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_vb"),
            size: (std::mem::size_of::<InstanceData>() * 32) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[&bind_group_layout],
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

        // Take an initial snapshot of visual state (non-blocking best-effort)
        let initial_snapshot = shared.lock().map(|v| v.clone()).unwrap_or_default();

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            uniform_buffer,
            quad_vb,
            instance_vb,
            bind_group,
            width: size.width,
            height: size.height,
            last_frame: Instant::now(),
            shared,
            last_vis_snapshot: initial_snapshot,
        })
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.width = new_size.width;
        self.height = new_size.height;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn view_proj(&self) -> [[f32; 4]; 4] {
        let aspect = self.width as f32 / self.height as f32;
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y);
        (proj * view).to_cols_array_2d()
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let now = Instant::now();
        let dt = now - self.last_frame;
        self.last_frame = now;

        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Update uniforms (view-proj)
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms {
                view_proj: self.view_proj(),
            }),
        );

        // Build instance data from shared state without blocking the render thread.
        // If the mutex is held by the audio scheduler, render using the last snapshot.
        let dt_sec = dt.as_secs_f32();
        let vis_local: VisState = if let Ok(mut vis) = self.shared.try_lock() {
            // Decay pulses in shared state and copy snapshot
            for p in vis.pulses.iter_mut() {
                *p = (*p - dt_sec * 1.5).max(0.0);
            }
            let snapshot = vis.clone();
            self.last_vis_snapshot = snapshot.clone();
            snapshot
        } else {
            // Decay locally; avoid writing back to shared state
            for p in self.last_vis_snapshot.pulses.iter_mut() {
                *p = (*p - dt_sec * 1.5).max(0.0);
            }
            self.last_vis_snapshot.clone()
        };

        let z_offset = app_core::z_offset_vec3();
        let spread = SPREAD;
        let positions = [
            vis_local.positions[0] * spread + z_offset,
            vis_local.positions[1] * spread + z_offset,
            vis_local.positions[2] * spread + z_offset,
        ];
        let scales = [
            BASE_SCALE + vis_local.pulses[0] * app_core::SCALE_PULSE_MULTIPLIER,
            BASE_SCALE + vis_local.pulses[1] * app_core::SCALE_PULSE_MULTIPLIER,
            BASE_SCALE + vis_local.pulses[2] * app_core::SCALE_PULSE_MULTIPLIER,
        ];
        let mut instances: Vec<InstanceData> = Vec::with_capacity(3);
        for i in 0..3 {
            instances.push(InstanceData {
                pos: positions[i].to_array(),
                scale: scales[i],
                color: vis_local.colors[i].to_array(),
                pulse: vis_local.pulses[i],
            });
        }
        self.queue
            .write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(&instances));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rpass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.02,
                            b: 0.04,
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
            rpass.draw(0..6, 0..3);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    // Shared visual state between scheduler and renderer
    let shared_state = Arc::new(Mutex::new(VisState {
        positions: [
            Vec3::from(DEFAULT_VOICE_POSITIONS[0]),
            Vec3::from(DEFAULT_VOICE_POSITIONS[1]),
            Vec3::from(DEFAULT_VOICE_POSITIONS[2]),
        ],
        colors: [
            Vec4::new(
                DEFAULT_VOICE_COLORS[0][0],
                DEFAULT_VOICE_COLORS[0][1],
                DEFAULT_VOICE_COLORS[0][2],
                1.0,
            ),
            Vec4::new(
                DEFAULT_VOICE_COLORS[1][0],
                DEFAULT_VOICE_COLORS[1][1],
                DEFAULT_VOICE_COLORS[1][2],
                1.0,
            ),
            Vec4::new(
                DEFAULT_VOICE_COLORS[2][0],
                DEFAULT_VOICE_COLORS[2][1],
                DEFAULT_VOICE_COLORS[2][2],
                1.0,
            ),
        ],
        pulses: [0.0, 0.0, 0.0],
    }));

    // Build shared music engine (used by audio thread and input)
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
    let engine = Arc::new(Mutex::new(MusicEngine::new(
        voice_configs,
        EngineParams {
            bpm: 110.0,
            scale: C_MAJOR_PENTATONIC,
        },
        42,
    )));

    // Start native audio output (synth driven by MusicEngine)
    let _audio_stream = start_audio_engine(Arc::clone(&shared_state), Arc::clone(&engine));

    let event_loop = EventLoop::new().expect("event loop");
    let window = WindowBuilder::new()
        .with_title("Generative Visualizer (native)")
        .build(&event_loop)
        .expect("window");

    let mut state =
        pollster::block_on(GpuState::new(&window, Arc::clone(&shared_state))).expect("gpu");
    let _start = Instant::now();

    let mut frames_left: Option<u32> = if std::env::var("SMOKE_TEST").ok().as_deref() == Some("1") {
        Some(120)
    } else {
        None
    };

    // Hover state for simple parity (highlight only)
    let mut hover: Option<usize> = None;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                ..
            } => state.resize(size),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => elwt.exit(),
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                let sz = state.window.inner_size();
                let (w, h) = (sz.width.max(1) as f32, sz.height.max(1) as f32);
                let x = position.x as f32;
                let y = position.y as f32;
                // Build pick ray
                let ndc_x = (2.0 * x / w) - 1.0;
                let ndc_y = 1.0 - (2.0 * y / h);
                let aspect = w / h;
                let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
                let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y);
                let inv = (proj * view).inverse();
                let p_near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
                let p_far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
                let _p0: Vec3 = p_near.truncate() / p_near.w;
                let p1: Vec3 = p_far.truncate() / p_far.w;
                let ro = Vec3::new(0.0, 0.0, 6.0);
                let rd = (p1 - ro).normalize();
                // Intersect against shared positions
                let z_off = z_offset_vec3();
                let spread = SPREAD;
                let mut best: Option<(usize, f32)> = None;
                {
                    let vis = state.shared.lock().unwrap();
                    for (i, pos) in vis.positions.iter().enumerate() {
                        let center_world = (*pos) * spread + z_off;
                        // ray-sphere
                        let oc = ro - center_world;
                        let b = oc.dot(rd);
                        let c = oc.dot(oc) - PICK_SPHERE_RADIUS * PICK_SPHERE_RADIUS;
                        let disc = b * b - c;
                        if disc < 0.0 {
                            continue;
                        }
                        let t = -b - disc.sqrt();
                        if t >= 0.0 {
                            match best {
                                Some((_, bt)) if t >= bt => {}
                                _ => best = Some((i, t)),
                            }
                        }
                    }
                }
                let new_hover = best.map(|(i, _)| i);
                if new_hover != hover {
                    // update colors to highlight hovered voice
                    let mut vis = state.shared.lock().unwrap();
                    // restore all to base first then apply hover brighten for determinism
                    for (i, base) in DEFAULT_VOICE_COLORS.iter().enumerate() {
                        vis.colors[i] = Vec4::new(base[0], base[1], base[2], 1.0);
                    }
                    if let Some(i) = new_hover {
                        vis.colors[i].x = (vis.colors[i].x * 1.4).min(1.0);
                        vis.colors[i].y = (vis.colors[i].y * 1.4).min(1.0);
                        vis.colors[i].z = (vis.colors[i].z * 1.4).min(1.0);
                    }
                    hover = new_hover;
                }
            }
            Event::AboutToWait => match state.render() {
                Ok(_) => {
                    state.window.request_redraw();
                    if let Some(ref mut n) = frames_left {
                        if *n == 0 {
                            elwt.exit();
                        } else {
                            *n -= 1;
                        }
                    }
                }
                Err(wgpu::SurfaceError::Lost) => state.resize(state.window.inner_size()),
                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                Err(_) => {}
            },
            _ => {}
        })
        .unwrap();
}

// ---------------- Native audio (cpal) ----------------

#[derive(Clone, Copy)]
enum WaveKind {
    Sine,
    Square,
    Saw,
    Triangle,
}

#[derive(Clone)]
struct ActiveOscillator {
    amplitude: f32,
    phase: f32,     // radians
    phase_inc: f32, // radians per sample
    total_samples: u32,
    samples_emitted: u32,
    attack_samples: u32,
    release_samples: u32,
    wave: WaveKind,
    left_gain: f32,
    right_gain: f32,
}

struct AudioState {
    sample_rate: f32,
    oscillators: Vec<ActiveOscillator>,
}

fn compute_equal_power_gains(pos_x_engine: f32) -> (f32, f32) {
    // Map engine-space X (roughly -1..1 typical) into pan -1..1
    let pan = (pos_x_engine / 1.5).clamp(-1.0, 1.0);
    // Equal-power panning
    let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4; // 0..pi/2
    (angle.cos(), angle.sin())
}

fn start_audio_engine(
    shared_vis: Arc<Mutex<VisState>>,
    shared_engine: Arc<Mutex<MusicEngine>>,
) -> Option<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let config = device.default_output_config().ok()?;
    let sample_rate = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    let state = Arc::new(Mutex::new(AudioState {
        sample_rate,
        oscillators: Vec::new(),
    }));

    // Scheduler thread producing notes using MusicEngine
    {
        let state_clone = Arc::clone(&state);
        let vis_clone = Arc::clone(&shared_vis);
        thread::Builder::new()
            .name("music-scheduler".into())
            .spawn(move || {
                let shared = shared_engine.clone();
                let mut engine = {
                    let guard = shared.lock().unwrap();
                    // Rebuild a local engine snapshot from shared configs/params/state
                    let mut e = MusicEngine::new(guard.configs.clone(), guard.params.clone(), 42);
                    e.voices = guard.voices.clone();
                    e
                };
                let start_instant = Instant::now();
                let mut last = start_instant;
                let mut events = Vec::new();
                loop {
                    let now = Instant::now();
                    let dt = now - last;
                    last = now;
                    let now_sec = start_instant.elapsed().as_secs_f64();
                    events.clear();
                    // Pull latest voice state from shared engine to reflect input changes
                    {
                        if let Ok(guard) = shared.lock() {
                            engine.voices = guard.voices.clone();
                        }
                    }
                    engine.tick(dt, now_sec, &mut events);

                    if !events.is_empty() {
                        let mut guard = state_clone.lock().unwrap();
                        for ev in &events {
                            let sr = guard.sample_rate;
                            let total = (ev.duration_sec * sr) as u32;
                            let attack = (0.02 * sr) as u32;
                            let release = (0.02 * sr) as u32;
                            // Determine waveform for this voice
                            let wave = match engine.configs[ev.voice_index].waveform {
                                Waveform::Sine => WaveKind::Sine,
                                Waveform::Square => WaveKind::Square,
                                Waveform::Saw => WaveKind::Saw,
                                Waveform::Triangle => WaveKind::Triangle,
                            };
                            // Stereo pan from voice X position (engine-space)
                            let pos_x = engine.voices[ev.voice_index].position.x;
                            let (left_gain, right_gain) = compute_equal_power_gains(pos_x);
                            guard.oscillators.push(ActiveOscillator {
                                amplitude: ev.velocity.min(1.0),
                                phase: 0.0,
                                phase_inc: 2.0 * std::f32::consts::PI * ev.frequency_hz / sr,
                                total_samples: total.max(1),
                                samples_emitted: 0,
                                attack_samples: attack.min(total),
                                release_samples: release.min(total),
                                wave,
                                left_gain,
                                right_gain,
                            });
                        }
                        drop(guard);
                        // Kick visual pulses
                        // Try to update visual pulses without blocking; if busy, skip this tick
                        if let Ok(mut vis) = vis_clone.try_lock() {
                            for ev in &events {
                                let i = ev.voice_index.min(2);
                                vis.pulses[i] = (vis.pulses[i] + ev.velocity).min(1.5);
                            }
                        }
                    }
                    // Small sleep to limit CPU without inducing long stalls
                    std::thread::sleep(Duration::from_millis(8));
                }
            })
            .ok()?;
    }

    let err_fn = |err| eprintln!("audio stream error: {err}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream_f32(
            &device,
            &config.into(),
            channels,
            Arc::clone(&state),
            err_fn,
        )
        .ok()?,
        cpal::SampleFormat::I16 => build_stream_i16(
            &device,
            &config.into(),
            channels,
            Arc::clone(&state),
            err_fn,
        )
        .ok()?,
        cpal::SampleFormat::U16 => build_stream_u16(
            &device,
            &config.into(),
            channels,
            Arc::clone(&state),
            err_fn,
        )
        .ok()?,
        _ => return None,
    };

    stream.play().ok()?;
    Some(stream)
}

fn render_wave_sample(phase: f32, wave: WaveKind) -> f32 {
    match wave {
        WaveKind::Sine => phase.sin(),
        WaveKind::Square => {
            if phase.sin() >= 0.0 {
                1.0
            } else {
                -1.0
            }
        }
        WaveKind::Saw => {
            // Map phase 0..2PI to -1..1
            let t = phase / (2.0 * std::f32::consts::PI);
            2.0 * (t - t.floor()) - 1.0
        }
        WaveKind::Triangle => {
            // Triangle using arcsin(sin) identity, normalized to [-1, 1]
            (2.0 / std::f32::consts::PI) * (phase.sin().asin())
        }
    }
}

fn mix_sample_stereo(oscillators: &mut Vec<ActiveOscillator>) -> (f32, f32) {
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    let mut i = 0usize;
    while i < oscillators.len() {
        let osc = &mut oscillators[i];
        // envelope
        let n = osc.samples_emitted;
        let a = if n < osc.attack_samples {
            n as f32 / osc.attack_samples.max(1) as f32
        } else if n > (osc.total_samples.saturating_sub(osc.release_samples)) {
            let rel_n = n.saturating_sub(osc.total_samples - osc.release_samples);
            1.0 - (rel_n as f32 / osc.release_samples.max(1) as f32)
        } else {
            1.0
        };
        let amp = osc.amplitude * a;
        let raw = render_wave_sample(osc.phase, osc.wave) * amp;
        // equal-power stereo distribution
        left += raw * osc.left_gain;
        right += raw * osc.right_gain;
        osc.phase += osc.phase_inc;
        if osc.phase > 2.0 * std::f32::consts::PI {
            osc.phase -= 2.0 * std::f32::consts::PI;
        }
        osc.samples_emitted += 1;
        if osc.samples_emitted >= osc.total_samples {
            oscillators.swap_remove(i);
            continue;
        }
        i += 1;
    }
    // Return linear mix; master saturation is applied downstream
    (left, right)
}

fn saturate_sample_arctan(input: f32, drive: f32) -> f32 {
    // Soft, analog-like symmetrical arctan curve
    (2.0 / std::f32::consts::PI) * (drive * input).atan()
}

fn apply_master_saturation(left: f32, right: f32) -> (f32, f32) {
    // Tuned for subtle warmth and gentle compression
    let drive = 1.6f32; // input drive into shaper
    let wet = 0.35f32; // wet mix amount
    let pre_gain = 0.9f32; // headroom before shaping
    let post_gain = 1.05f32; // slight makeup gain

    let l_in = left * pre_gain;
    let r_in = right * pre_gain;
    let l_sat = saturate_sample_arctan(l_in, drive);
    let r_sat = saturate_sample_arctan(r_in, drive);
    let l_out = (wet * l_sat + (1.0 - wet) * l_in) * post_gain;
    let r_out = (wet * r_sat + (1.0 - wet) * r_in) * post_gain;
    (l_out.clamp(-1.0, 1.0), r_out.clamp(-1.0, 1.0))
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    state: Arc<Mutex<AudioState>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    device.build_output_stream(
        config,
        move |data: &mut [f32], _| {
            let mut guard = state.lock().unwrap();
            let oscillators = &mut guard.oscillators;
            let mut frame = 0usize;
            while frame < data.len() {
                let (l_raw, r_raw) = mix_sample_stereo(oscillators);
                let (l, r) = apply_master_saturation(l_raw, r_raw);
                if channels >= 2 {
                    if frame < data.len() {
                        data[frame] = l;
                    }
                    if frame + 1 < data.len() {
                        data[frame + 1] = r;
                    }
                } else if frame < data.len() {
                    data[frame] = 0.5 * (l + r);
                }
                frame += channels;
            }
        },
        err_fn,
        None,
    )
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    state: Arc<Mutex<AudioState>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    device.build_output_stream(
        config,
        move |data: &mut [i16], _| {
            let mut guard = state.lock().unwrap();
            let oscillators = &mut guard.oscillators;
            let mut frame = 0usize;
            while frame < data.len() {
                let (l_raw, r_raw) = mix_sample_stereo(oscillators);
                let (l, r) = apply_master_saturation(l_raw, r_raw);
                let vl = (l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                let vr = (r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                if channels >= 2 {
                    if frame < data.len() {
                        data[frame] = vl;
                    }
                    if frame + 1 < data.len() {
                        data[frame + 1] = vr;
                    }
                } else if frame < data.len() {
                    data[frame] = ((vl as i32 + vr as i32) / 2) as i16;
                }
                frame += channels;
            }
        },
        err_fn,
        None,
    )
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    state: Arc<Mutex<AudioState>>,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    device.build_output_stream(
        config,
        move |data: &mut [u16], _| {
            let mut guard = state.lock().unwrap();
            let oscillators = &mut guard.oscillators;
            let mut frame = 0usize;
            while frame < data.len() {
                let (l_raw, r_raw) = mix_sample_stereo(oscillators);
                let (l, r) = apply_master_saturation(l_raw, r_raw);
                let vl = (((l * 0.5 + 0.5).clamp(0.0, 1.0)) * u16::MAX as f32) as u16;
                let vr = (((r * 0.5 + 0.5).clamp(0.0, 1.0)) * u16::MAX as f32) as u16;
                if channels >= 2 {
                    if frame < data.len() {
                        data[frame] = vl;
                    }
                    if frame + 1 < data.len() {
                        data[frame + 1] = vr;
                    }
                } else if frame < data.len() {
                    let mix = ((vl as u32 + vr as u32) / 2) as u16;
                    data[frame] = mix;
                }
                frame += channels;
            }
        },
        err_fn,
        None,
    )
}
