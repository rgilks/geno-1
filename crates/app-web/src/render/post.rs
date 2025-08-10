use wgpu;

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
