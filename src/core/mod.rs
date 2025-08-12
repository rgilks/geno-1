pub mod music;

pub use music::*;

// Shaders bundled as string constants
pub static POST_WGSL: &str = include_str!("../../shaders/post.wgsl");
pub static WAVES_WGSL: &str = include_str!("../../shaders/waves.wgsl");
