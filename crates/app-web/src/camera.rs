use glam::{Mat4, Vec3, Vec4};
use web_sys as web;

#[inline]
pub fn screen_to_world_ray(
    canvas: &web::HtmlCanvasElement,
    sx: f32,
    sy: f32,
    camera_z: f32,
) -> (Vec3, Vec3) {
    let width = canvas.width() as f32;
    let height = canvas.height() as f32;
    let ndc_x = (2.0 * sx / width) - 1.0;
    let ndc_y = 1.0 - (2.0 * sy / height);
    let aspect = width / height.max(1.0);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
    let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, camera_z), Vec3::ZERO, Vec3::Y);
    let inv = (proj * view).inverse();
    let p_near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
    let p_far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
    let _p0: Vec3 = p_near.truncate() / p_near.w;
    let p1: Vec3 = p_far.truncate() / p_far.w;
    let ro = Vec3::new(0.0, 0.0, camera_z);
    let rd = (p1 - ro).normalize();
    (ro, rd)
}


