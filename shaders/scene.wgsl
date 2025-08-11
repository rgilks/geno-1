// Copy exists under app-web for bundling via core module include_str!
// (Content pulled from former app-core/shaders/scene.wgsl)
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

// Cheap hash and helpers for micro-detail
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

@fragment
fn fs_main(inf: VsOut) -> @location(0) vec4<f32> {
    // Local in [-0.5,0.5] → map to unit disk coordinates
    let p = inf.local * 2.0;
    let r = length(p);
    let alpha = 1.0 - smoothstep(0.48, 0.5, r); // soft circle mask
    if alpha <= 0.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Fake sphere normal from signed distance to unit circle
    let zz = sqrt(max(1.0 - clamp(r, 0.0, 1.0) * clamp(r, 0.0, 1.0), 0.0));
    let n = normalize(vec3<f32>(p, zz));
    let light1 = normalize(vec3<f32>(-0.4, 0.2, 0.9));
    let light2 = normalize(vec3<f32>(0.6, -0.3, 0.7));
    let base_col = inf.color.rgb;

    // Diffuse + warm/cool split lighting
    let diff1 = max(dot(n, light1), 0.0);
    let diff2 = max(dot(n, light2), 0.0);
    var col = base_col * (0.25 + 0.9 * diff1) + vec3<f32>(0.9, 0.95, 1.0) * (0.15 * diff2);

    // Specular highlights
    let view = vec3<f32>(0.0, 0.0, 1.0);
    let h1 = normalize(light1 + view);
    let spec = pow(max(dot(n, h1), 0.0), 64.0);
    col += vec3<f32>(1.0) * (0.25 * spec);

    // Pulse-driven inner bloom and moving ring
    let pulse = clamp(inf.pulse, 0.0, 1.5);
    let core = smoothstep(0.55, 0.0, r) * (0.6 + 1.8 * pulse);
    let ring = exp(-40.0 * pow(r - (0.25 + 0.2 * pulse), 2.0)) * (0.3 + 1.2 * pulse);
    col *= 0.6 + core;
    col += base_col * ring;

    // Iridescent rim
    let rim = pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 3.0);
    col += vec3<f32>(0.9, 0.8, 1.2) * rim * (0.4 + 0.8 * pulse);

    // Micro sparkle texture
    let g = hash(p * 120.0) - 0.5;
    col += (0.02 + 0.08 * pulse) * g;

    // Outer halo beyond the mask for smoother edges (relies on HDR post bloom)
    let halo = smoothstep(0.55, 0.45, r) * (0.4 + 1.5 * pulse);
    col += base_col * halo;

    return vec4<f32>(col, alpha * inf.color.a);
}


