use glam::{Mat4, Vec3};

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
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fovy_radians, self.aspect, self.znear, self.zfar)
    }
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }
}

#[derive(Clone, Debug, Default)]
pub struct VisualNotePulse {
    pub amount: f32,
}
