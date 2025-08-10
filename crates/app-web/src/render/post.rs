use wgpu;

pub(crate) struct PostResources {
    pub(crate) bgl0: wgpu::BindGroupLayout, // tex+sampler+uniform
    pub(crate) bgl1: wgpu::BindGroupLayout, // tex+sampler
    pub(crate) uniform_buffer: wgpu::Buffer,
    pub(crate) bright_pipeline: wgpu::RenderPipeline,
    pub(crate) blur_pipeline: wgpu::RenderPipeline,
    pub(crate) composite_pipeline: wgpu::RenderPipeline,
}

pub(crate) fn create_post_resources(
    device: &wgpu::Device,
    post_shader: &wgpu::ShaderModule,
    bloom_format: wgpu::TextureFormat,
    swap_format: wgpu::TextureFormat,
) -> PostResources {
    let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("post_bgl0"),
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
            wgpu::BindGroupLayoutEntry {
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
    let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("post_uniforms"),
        size: std::mem::size_of::<super::PostUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pl_bright_blur = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pl_post_0"),
        bind_group_layouts: &[&bgl0],
        push_constant_ranges: &[],
    });
    let pl_composite = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pl_post_comp"),
        bind_group_layouts: &[&bgl0, &bgl1],
        push_constant_ranges: &[],
    });
    let bright_pipeline = super::helpers::make_post_pipeline(
        device,
        &pl_bright_blur,
        post_shader,
        "fs_bright",
        bloom_format,
        None,
    );
    let blur_pipeline = super::helpers::make_post_pipeline(
        device,
        &pl_bright_blur,
        post_shader,
        "fs_blur",
        bloom_format,
        None,
    );
    let composite_pipeline = super::helpers::make_post_pipeline(
        device,
        &pl_composite,
        post_shader,
        "fs_composite",
        swap_format,
        Some(wgpu::BlendState::REPLACE),
    );

    PostResources {
        bgl0,
        bgl1,
        uniform_buffer,
        bright_pipeline,
        blur_pipeline,
        composite_pipeline,
    }
}

pub(crate) fn blit(
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
