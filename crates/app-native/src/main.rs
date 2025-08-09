use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wgpu::util::DeviceExt;
use winit::{event::*, event_loop::EventLoop, window::WindowBuilder};

use app_core::{EngineParams, MusicEngine, VoiceConfig, Waveform, C_MAJOR_PENTATONIC};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glam::Vec3;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    color: [f32; 4],
}

struct GpuState<'w> {
    window: &'w winit::window::Window,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'w>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl<'w> GpuState<'w> {
    async fn new(window: &'w winit::window::Window) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = unsafe { instance.create_surface(window) }?;
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

        let shader_source = r#"
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.0,  0.5),
    );
    let p = pos[idx];
    return vec4<f32>(p, 0.0, 1.0);
}

struct Uniforms { color: vec4<f32> };
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return uniforms.color;
}
"#;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                color: [0.1, 0.6, 0.9, 1.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
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

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rp"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        Ok(Self {
            window,
            instance,
            surface,
            device,
            queue,
            config,
            render_pipeline,
            uniform_buffer,
            bind_group,
        })
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render(&mut self, t: f32) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Animate color
        let color = [0.2 + 0.8 * (0.5 + 0.5 * (t).sin()), 0.4, 0.9, 1.0];
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms { color }),
        );

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
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_bind_group(0, &self.bind_group, &[]);
            rpass.draw(0..3, 0..1);
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

    // Start native audio output (sine synth driven by MusicEngine)
    let _audio_stream = start_audio_engine();

    let event_loop = EventLoop::new().expect("event loop");
    let window = WindowBuilder::new()
        .with_title("Generative Visualizer (native)")
        .build(&event_loop)
        .expect("window");

    let mut state = pollster::block_on(GpuState::new(&window)).expect("gpu");
    let start = Instant::now();

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
            Event::AboutToWait => {
                let t = start.elapsed().as_secs_f32();
                match state.render(t) {
                    Ok(_) => state.window.request_redraw(),
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.window.inner_size()),
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(_) => {}
                }
            }
            _ => {}
        })
        .unwrap();
}

// ---------------- Native audio (cpal) ----------------

#[derive(Clone)]
struct ActiveOscillator {
    frequency_hz: f32,
    amplitude: f32,
    phase: f32,     // radians
    phase_inc: f32, // radians per sample
    total_samples: u32,
    samples_emitted: u32,
    attack_samples: u32,
    release_samples: u32,
}

struct AudioState {
    sample_rate: f32,
    oscillators: Vec<ActiveOscillator>,
}

fn start_audio_engine() -> Option<cpal::Stream> {
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
        thread::Builder::new()
            .name("music-scheduler".into())
            .spawn(move || {
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
                            // Map to sine for now; extend to waveform later
                            guard.oscillators.push(ActiveOscillator {
                                frequency_hz: ev.frequency_hz,
                                amplitude: ev.velocity.min(1.0),
                                phase: 0.0,
                                phase_inc: 2.0 * std::f32::consts::PI * ev.frequency_hz / sr,
                                total_samples: total.max(1),
                                samples_emitted: 0,
                                attack_samples: attack.min(total),
                                release_samples: release.min(total),
                            });
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

fn mix_sample(oscillators: &mut Vec<ActiveOscillator>) -> f32 {
    let mut s = 0.0f32;
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
        let sample = osc.phase.sin() * amp;
        s += sample;
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
    s.tanh()
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
                let s = mix_sample(oscillators);
                for ch in 0..channels {
                    let idx = frame + ch;
                    if idx < data.len() {
                        data[idx] = s;
                    }
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
                let s = mix_sample(oscillators);
                let v = (s * i16::MAX as f32) as i16;
                for ch in 0..channels {
                    let idx = frame + ch;
                    if idx < data.len() {
                        data[idx] = v;
                    }
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
                let s = mix_sample(oscillators);
                let v = ((s * 0.5 + 0.5).clamp(0.0, 1.0) * u16::MAX as f32) as u16;
                for ch in 0..channels {
                    let idx = frame + ch;
                    if idx < data.len() {
                        data[idx] = v;
                    }
                }
                frame += channels;
            }
        },
        err_fn,
        None,
    )
}
