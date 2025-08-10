use wgpu;

/// Offscreen color targets for the render pipeline.
///
/// Contains a full-resolution HDR scene color and two half-resolution bloom
/// ping-pong textures. Views are pre-created for convenience.
///
/// - `hdr_*` hold the main scene color in Rgba16Float for post-processing.
/// - `bloom_*` are half-res buffers used for bright-pass and blur.
pub(crate) struct RenderTargets {
    pub(crate) hdr_tex: wgpu::Texture,
    pub(crate) hdr_view: wgpu::TextureView,
    pub(crate) bloom_a: wgpu::Texture,
    pub(crate) bloom_a_view: wgpu::TextureView,
    pub(crate) bloom_b: wgpu::Texture,
    pub(crate) bloom_b_view: wgpu::TextureView,
}

impl RenderTargets {
    pub(crate) fn new(
        hdr_tex: wgpu::Texture,
        hdr_view: wgpu::TextureView,
        bloom_a: wgpu::Texture,
        bloom_a_view: wgpu::TextureView,
        bloom_b: wgpu::Texture,
        bloom_b_view: wgpu::TextureView,
    ) -> Self {
        Self {
            hdr_tex,
            hdr_view,
            bloom_a,
            bloom_a_view,
            bloom_b,
            bloom_b_view,
        }
    }
}
