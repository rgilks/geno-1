use glam::Vec3;

#[derive(Default, Clone, Copy)]
pub struct MouseState {
    pub x: f32,
    pub y: f32,
    pub down: bool,
}
#[derive(Default, Clone, Copy)]
pub struct DragState {
    pub active: bool,
    pub voice: usize,
    pub plane_z_world: f32,
}
#[inline]
pub fn ray_sphere(ray_origin: Vec3, ray_dir: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ray_origin - center;
    let b = oc.dot(ray_dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    (t >= 0.0).then_some(t)
}
