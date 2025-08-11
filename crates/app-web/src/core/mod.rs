pub mod constants;
pub mod music;
pub mod state;

pub use constants::*;
pub use music::*;
pub use state::*;

// Shaders bundled as string constants
pub static SCENE_WGSL: &str = include_str!("../../shaders/scene.wgsl");
pub static POST_WGSL: &str = include_str!("../../shaders/post.wgsl");
pub static WAVES_WGSL: &str = include_str!("../../shaders/waves.wgsl");
