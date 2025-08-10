use app_core::{BASE_SCALE, SCALE_PULSE_MULTIPLIER};
use glam::{Mat4, Vec3, Vec4};
use web_sys as web;
// wgpu types are used below
use wgpu;

pub struct RenderTargets {
    pub hdr_tex: wgpu::Texture,
    pub hdr_view: wgpu::TextureView,
    pub bloom_a: wgpu::Texture,
    pub bloom_a_view: wgpu::TextureView,
    pub bloom_b: wgpu::Texture,
    pub bloom_b_view: wgpu::TextureView,
}

impl RenderTargets {
    pub fn new(
        hdr_tex: wgpu::Texture,
        hdr_view: wgpu::TextureView,
        bloom_a: wgpu::Texture,
        bloom_a_view: wgpu::TextureView,
        bloom_b: wgpu::Texture,
        bloom_b_view: wgpu::TextureView,
    ) -> Self {
        Self { hdr_tex, hdr_view, bloom_a, bloom_a_view, bloom_b, bloom_b_view }
    }
}

#[inline]
pub fn screen_to_world_ray(
    canvas: &web::HtmlCanvasElement,
    sx: f32,
    sy: f32,
    camera_z: f32,
) -> (Vec3, Vec3) {
    let width = canvas.width() as f32;
    let height = canvas.height() as f32;
    let ndc_x = (2.0 * sx / width) - 1.0;
    let ndc_y = 1.0 - (2.0 * sy / height);
    let aspect = width / height.max(1.0);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
    let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, camera_z), Vec3::ZERO, Vec3::Y);
    let inv = (proj * view).inverse();
    let p_near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let p_far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let _p0: Vec3 = p_near.truncate() / p_near.w;
    let p1: Vec3 = p_far.truncate() / p_far.w;
    let ro = Vec3::new(0.0, 0.0, camera_z);
    let rd = (p1 - ro).normalize();
    (ro, rd)
}

// ===================== WebGPU state (moved from lib.rs) =====================

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct VoicePacked {
    pos_pulse: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct WavesUniforms {
    resolution: [f32; 2],
    time: f32,
    ambient: f32,
    voices: [VoicePacked; 3],
    swirl_uv: [f32; 2],
    swirl_strength: f32,
    swirl_active: f32,
    ripple_uv: [f32; 2],
    ripple_t0: f32,
    ripple_amp: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PostUniforms {
    resolution: [f32; 2],
    time: f32,
    ambient: f32,
    blur_dir: [f32; 2],
    bloom_strength: f32,
    threshold: f32,
}

pub struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    // Waves full-screen layer
    waves_pipeline: wgpu::RenderPipeline,
    waves_uniform_buffer: wgpu::Buffer,
    waves_bind_group: wgpu::BindGroup,
    // Post-processing resources
    targets: RenderTargets,
    linear_sampler: wgpu::Sampler,

    #[allow(dead_code)]
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
    // Click/tap ripple state
    ripple_uv: [f32; 2],
    ripple_t0: f32,
    ripple_amp: f32,
}

impl<'a> GpuState<'a> {
    pub async fn new(canvas: &'a web::HtmlCanvasElement, camera_z: f32) -> anyhow::Result<Self> {
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
            waves_pipeline,
            waves_uniform_buffer,
            waves_bind_group,
            targets: RenderTargets::new(hdr_tex, hdr_view, bloom_a, bloom_a_view, bloom_b, bloom_b_view),
            linear_sampler,
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
            cam_eye: Vec3::new(0.0, 0.0, camera_z),
            cam_target: Vec3::ZERO,
            time_accum: 0.0,
            ambient_energy: 0.0,
            swirl_uv: [0.5, 0.5],
            swirl_strength: 0.0,
            swirl_active: 0.0,
            ripple_uv: [0.5, 0.5],
            ripple_t0: -1.0,
            ripple_amp: 0.0,
        })
    }

    pub fn set_ambient_clear(&mut self, energy01: f32) {
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

    pub fn set_camera(&mut self, eye: Vec3, target: Vec3) {
        self.cam_eye = eye;
        self.cam_target = target;
    }

    pub fn set_swirl(&mut self, uv: [f32; 2], strength: f32, active: bool) {
        self.swirl_uv = uv;
        self.swirl_strength = strength;
        self.swirl_active = if active { 1.0 } else { 0.0 };
    }

    pub fn set_ripple(&mut self, uv: [f32; 2], amp: f32) {
        self.ripple_uv = uv;
        self.ripple_amp = amp.clamp(0.0, 1.5);
        // Anchor ripple start to current accumulated time so shader can compute age
        self.ripple_t0 = self.time_accum;
    }

    pub fn resize_if_needed(&mut self, width: u32, height: u32) {
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
            self.targets.hdr_tex = self.device.create_texture(&wgpu::TextureDescriptor {
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
            self.targets.hdr_view = self
                .targets.hdr_tex
                .create_view(&wgpu::TextureViewDescriptor::default());
            let bw = (width.max(1) / 2).max(1);
            let bh = (height.max(1) / 2).max(1);
            let bloom_format = wgpu::TextureFormat::Rgba16Float;
            self.targets.bloom_a = self.device.create_texture(&wgpu::TextureDescriptor {
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
            self.targets.bloom_b = self.device.create_texture(&wgpu::TextureDescriptor {
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
            self.targets.bloom_a_view = self
                .targets.bloom_a
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.targets.bloom_b_view = self
                .targets.bloom_b
                .create_view(&wgpu::TextureViewDescriptor::default());

            // Rebuild bind groups that reference these views
            self.rebuild_post_bind_groups();
        }
    }

    pub fn render(
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
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.hdr_view,
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
            let pack = |i: usize| VoicePacked {
                pos_pulse: [
                    positions[i].x,
                    positions[i].y,
                    positions[i].z,
                    ((scales[i] - BASE_SCALE).max(0.0) / SCALE_PULSE_MULTIPLIER).clamp(0.0, 1.5),
                ],
                color: colors[i].to_array(),
            };
            let w = WavesUniforms {
                resolution: [self.width as f32, self.height as f32],
                time: self.time_accum,
                ambient: self.ambient_energy,
                voices: [pack(0), pack(1), pack(2)],
                swirl_uv: [
                    self.swirl_uv[0].clamp(0.0, 1.0),
                    self.swirl_uv[1].clamp(0.0, 1.0),
                ],
                swirl_strength: if self.swirl_active > 0.5 { 1.4 } else { 0.0 },
                swirl_active: self.swirl_active,
                ripple_uv: self.ripple_uv,
                ripple_t0: self.ripple_t0,
                ripple_amp: self.ripple_amp,
            };
            self.queue
                .write_buffer(&self.waves_uniform_buffer, 0, bytemuck::bytes_of(&w));
            rpass.set_pipeline(&self.waves_pipeline);
            rpass.set_bind_group(0, &self.waves_bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }

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
        self.blit(
            &mut encoder,
            "bright_pass",
            &self.bloom_a_view,
            wgpu::Color::BLACK,
            &self.bright_pipeline,
            &self.bg_hdr,
            None,
        );

        // Pass 3: blur horizontal bloom_a -> bloom_b
        post.blur_dir = [1.0, 0.0];
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));
        self.blit(
            &mut encoder,
            "blur_h",
            &self.bloom_b_view,
            wgpu::Color::BLACK,
            &self.blur_pipeline,
            &self.bg_from_bloom_a,
            None,
        );

        // Pass 4: blur vertical bloom_b -> bloom_a
        post.blur_dir = [0.0, 1.0];
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));
        self.blit(
            &mut encoder,
            "blur_v",
            &self.bloom_a_view,
            wgpu::Color::BLACK,
            &self.blur_pipeline,
            &self.bg_from_bloom_b,
            None,
        );

        // Pass 5: composite to swapchain
        post.blur_dir = [0.0, 0.0];
        self.queue
            .write_buffer(&self.post_uniform_buffer, 0, bytemuck::bytes_of(&post));
        self.blit(
            &mut encoder,
            "composite",
            &view,
            self.clear_color,
            &self.composite_pipeline,
            &self.bg_hdr,
            Some(&self.bg_bloom_a_only),
        );

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

impl<'a> GpuState<'a> {
    fn rebuild_post_bind_groups(&mut self) {
        // bg sampling HDR scene
        self.bg_hdr = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_hdr"),
            layout: &self.post_bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.targets.hdr_view),
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
        // bg sampling bloom_a
        self.bg_from_bloom_a = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_from_bloom_a"),
            layout: &self.post_bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.targets.bloom_a_view),
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
        // bg sampling bloom_b
        self.bg_from_bloom_b = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_from_bloom_b"),
            layout: &self.post_bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.targets.bloom_b_view),
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
        // group1 variants (no uniforms)
        self.bg_bloom_a_only = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg_bloom_a_only"),
            layout: &self.post_bgl1,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.targets.bloom_a_view),
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
                        resource: wgpu::BindingResource::TextureView(&self.targets.bloom_b_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        });
    }

    fn blit(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        target: &wgpu::TextureView,
        clear: wgpu::Color,
        pipeline: &wgpu::RenderPipeline,
        bg0: &wgpu::BindGroup,
        bg1: Option<&wgpu::BindGroup>,
    ) {
        let mut r = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        r.set_pipeline(pipeline);
        r.set_bind_group(0, bg0, &[]);
        if let Some(g1) = bg1 {
            r.set_bind_group(1, g1, &[]);
        }
        r.draw(0..3, 0..1);
        drop(r);
    }
}
