pub mod constants;
pub mod music;
pub mod state;
pub static SCENE_WGSL: &str = include_str!("../shaders/scene.wgsl");

pub use constants::*;
pub use music::*;
pub use state::*;
