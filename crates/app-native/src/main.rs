use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;
use winit::{event::*, event_loop::EventLoop, window::WindowBuilder};

use app_core::{
    EngineParams, MusicEngine, VoiceConfig, Waveform, BASE_SCALE, C_MAJOR_PENTATONIC,
    DEFAULT_VOICE_COLORS, DEFAULT_VOICE_POSITIONS, SPREAD,
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

#[derive(Default)]
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

        // Build instance data from shared state
        let mut vis = self.shared.lock().unwrap();
        // decay pulses
        let dt_sec = dt.as_secs_f32();
        for p in vis.pulses.iter_mut() {
            *p = (*p - dt_sec * 1.5).max(0.0);
        }
        let z_offset = app_core::z_offset_vec3();
        let spread = SPREAD;
        let positions = [
            vis.positions[0] * spread + z_offset,
            vis.positions[1] * spread + z_offset,
            vis.positions[2] * spread + z_offset,
        ];
        let scales = [
            BASE_SCALE + vis.pulses[0] * app_core::SCALE_PULSE_MULTIPLIER,
            BASE_SCALE + vis.pulses[1] * app_core::SCALE_PULSE_MULTIPLIER,
            BASE_SCALE + vis.pulses[2] * app_core::SCALE_PULSE_MULTIPLIER,
        ];
        let mut instances: Vec<InstanceData> = Vec::with_capacity(3);
        for i in 0..3 {
            instances.push(InstanceData {
                pos: positions[i].to_array(),
                scale: scales[i],
                color: vis.colors[i].to_array(),
                pulse: vis.pulses[i],
            });
        }
        drop(vis);
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

    // Start native audio output (sine synth driven by MusicEngine)
    let _audio_stream = start_audio_engine(Arc::clone(&shared_state));

    let event_loop = EventLoop::new().expect("event loop");
    let window = WindowBuilder::new()
        .with_title("Generative Visualizer (native)")
        .build(&event_loop)
        .expect("window");

    let mut state =
        pollster::block_on(GpuState::new(&window, Arc::clone(&shared_state))).expect("gpu");
    let _start = Instant::now();

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
            Event::AboutToWait => match state.render() {
                Ok(_) => state.window.request_redraw(),
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

fn start_audio_engine(shared_vis: Arc<Mutex<VisState>>) -> Option<cpal::Stream> {
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
                let mut engine = MusicEngine::new(
                    voice_configs,
                    EngineParams {
                        bpm: 110.0,
                        scale: C_MAJOR_PENTATONIC,
                    },
                    42,
                );
                let start_instant = Instant::now();
                let mut last = start_instant;
                let mut events = Vec::new();
                loop {
                    let now = Instant::now();
                    let dt = now - last;
                    last = now;
                    let now_sec = start_instant.elapsed().as_secs_f64();
                    events.clear();
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
                            let pan = (pos_x / 1.5).clamp(-1.0, 1.0); // -1 left .. 1 right
                            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4; // 0..pi/2
                            let left_gain = angle.cos();
                            let right_gain = angle.sin();
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
                        let mut vis = vis_clone.lock().unwrap();
                        for ev in &events {
                            let i = ev.voice_index.min(2);
                            vis.pulses[i] = (vis.pulses[i] + ev.velocity).min(1.5);
                        }
                    }
                    std::thread::sleep(Duration::from_millis(15));
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
        WaveKind::Square => if phase.sin() >= 0.0 { 1.0 } else { -1.0 },
        WaveKind::Saw => {
            // Map phase 0..2PI to -1..1
            let t = phase / (2.0 * std::f32::consts::PI);
            (2.0 * (t - t.floor())) * 2.0 - 1.0
        }
        WaveKind::Triangle => {
            // Triangle from saw
            let saw = {
                let t = phase / (2.0 * std::f32::consts::PI);
                (2.0 * (t - t.floor())) * 2.0 - 1.0
            };
            (2.0 / std::f32::consts::PI) * (saw.asin())
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
    (left.tanh(), right.tanh())
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
                let (l, r) = mix_sample_stereo(oscillators);
                if channels >= 2 {
                    if frame + 0 < data.len() {
                        data[frame + 0] = l;
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
                let (l, r) = mix_sample_stereo(oscillators);
                let vl = (l * i16::MAX as f32) as i16;
                let vr = (r * i16::MAX as f32) as i16;
                if channels >= 2 {
                    if frame + 0 < data.len() {
                        data[frame + 0] = vl;
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
                let (l, r) = mix_sample_stereo(oscillators);
                let vl = (((l * 0.5 + 0.5).clamp(0.0, 1.0)) * u16::MAX as f32) as u16;
                let vr = (((r * 0.5 + 0.5).clamp(0.0, 1.0)) * u16::MAX as f32) as u16;
                if channels >= 2 {
                    if frame + 0 < data.len() {
                        data[frame + 0] = vl;
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
