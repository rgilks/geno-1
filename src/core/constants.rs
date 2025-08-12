use glam::Vec3;

// Shared visual/audio tuning constants used by the web frontend.

// Scene layout
pub const SPREAD: f32 = 1.8; // scales engine-space positions to world-space
pub const Z_OFFSET: Vec3 = Vec3::new(0.0, 0.0, -4.0); // world-space offset applied to all markers

// Interaction
pub const PICK_SPHERE_RADIUS: f32 = 0.8; // ray-sphere radius for picking
pub const ENGINE_DRAG_MAX_RADIUS: f32 = 3.0; // max engine-space radius when dragging
