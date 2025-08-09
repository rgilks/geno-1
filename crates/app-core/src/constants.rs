use glam::Vec3;

// Shared visual/audio tuning constants used by both web and native frontends.

// Scene layout
pub const SPREAD: f32 = 1.8; // scales engine-space positions to world-space
pub const Z_OFFSET: [f32; 3] = [0.0, 0.0, -4.0]; // world-space offset applied to all markers

// Visual sizing
pub const BASE_SCALE: f32 = 1.6; // idle marker size
pub const SCALE_PULSE_MULTIPLIER: f32 = 0.4; // how much a full pulse enlarges a marker

// Interaction
pub const PICK_SPHERE_RADIUS: f32 = 0.8; // ray-sphere radius for picking
pub const ENGINE_DRAG_MAX_RADIUS: f32 = 3.0; // max engine-space radius when dragging

// Default palette and positions for the three voices
pub const DEFAULT_VOICE_COLORS: [[f32; 3]; 3] = [
    [0.9, 0.3, 0.3], // red-ish
    [0.3, 0.9, 0.4], // green-ish
    [0.3, 0.5, 0.9], // blue-ish
];

pub const DEFAULT_VOICE_POSITIONS: [[f32; 3]; 3] =
    [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]];

#[inline]
pub fn z_offset_vec3() -> Vec3 {
    Vec3::new(Z_OFFSET[0], Z_OFFSET[1], Z_OFFSET[2])
}
