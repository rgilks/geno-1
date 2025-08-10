//! Visual-side state types shared with the web frontend.
//!
//! These types intentionally avoid referencing platform-specific APIs and are
//! suitable for use on both native and web targets. The web frontend consumes
//! them to build camera matrices and to drive simple, audio-reactive pulses.

use glam::{Mat4, Vec3};

/// Simple right-handed camera description with perspective projection.
#[derive(Clone, Debug)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub aspect: f32,
    pub fovy_radians: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    /// Compute the clip-space projection matrix.
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fovy_radians, self.aspect, self.znear, self.zfar)
    }
    /// Compute the view matrix that transforms world to view space.
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }
}

/// Small value container used by the renderer to represent a note-driven pulse.
///
/// The `amount` should typically be in the \[0, 1\] range where 0 is idle and
/// 1 is a full pulse. The renderer can map this to scale/emissive intensity.
#[derive(Clone, Debug, Default)]
pub struct VisualNotePulse {
    pub amount: f32,
}
