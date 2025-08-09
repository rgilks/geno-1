struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local: vec2<f32>,
    @location(2) pulse: f32,
};
struct Uniforms {
    view_proj: mat4x4<f32>
};
@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(
    @location(0) v_pos: vec2<f32>,
    @location(1) i_pos: vec3<f32>,
    @location(2) i_scale: f32,
    @location(3) i_color: vec4<f32>,
    @location(4) i_pulse: f32,
) -> VsOut {
    let local_scaled = vec4<f32>(v_pos * i_scale, 0.0, 1.0);
    let world = vec4<f32>(i_pos, 1.0) + local_scaled;
    var out: VsOut;
    out.pos = u.view_proj * world;
    out.color = i_color;
    out.local = v_pos; // unscaled local for shape mask
    out.pulse = i_pulse;
    return out;
}

@fragment
fn fs_main(inf: VsOut) -> @location(0) vec4<f32> {
  // Circular mask within the quad (unit circle of radius 0.5)
    let r = length(inf.local);
    let shape_alpha = 1.0 - smoothstep(0.48, 0.5, r);

  // Emissive pulse boosts brightness subtly
    let emissive = 0.7 * clamp(inf.pulse, 0.0, 1.5);
    let rgb = inf.color.rgb * (1.0 + emissive);
    return vec4<f32>(rgb, shape_alpha * inf.color.a);
}


