pub mod constants;
pub mod music;
pub mod state;
pub static SCENE_WGSL: &str = include_str!("../shaders/scene.wgsl");
pub static POST_WGSL: &str = include_str!("../shaders/post.wgsl");
pub static WAVES_WGSL: &str = include_str!("../shaders/waves.wgsl");

pub use constants::*;
pub use music::*;
pub use state::*;
