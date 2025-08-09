// Fullscreen post-processing: HDR bright pass, separable blur, composite with
// filmic tonemapping, vignette, gentle chroma shift and film grain.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Common uniforms used by all post passes
struct PostUniforms {
    resolution: vec2<f32>,
    time: f32,
    ambient: f32,
    // For blur
    blur_dir: vec2<f32>,
    bloom_strength: f32,
    threshold: f32,
};

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var<uniform> u_post: PostUniforms;

// Optional second texture (used by composite for blurred bloom)
@group(1) @binding(0) var blur_tex: texture_2d<f32>;
@group(1) @binding(1) var blur_sampler: sampler;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> VsOut {
    // Fullscreen triangle (no vertex buffer)
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(pos[vid], 0.0, 1.0);
    out.uv = uv[vid];
    return out;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// BRIGHT PASS: keep only highlights above threshold
@fragment
fn fs_bright(inp: VsOut) -> @location(0) vec4<f32> {
    let col = textureSample(hdr_tex, hdr_sampler, inp.uv).rgb;
    let thr = u_post.threshold;
    let l = luminance(col);
    let k = max(l - thr, 0.0);
    let outc = col * (k / max(l, 1e-5));
    return vec4<f32>(outc, 1.0);
}

// BLUR PASS: simple 9-tap gaussian along blur_dir
@fragment
fn fs_blur(inp: VsOut) -> @location(0) vec4<f32> {
    let texel = u_post.blur_dir / u_post.resolution;
    let w0 = 0.05;
    let w1 = 0.09;
    let w2 = 0.12;
    let w3 = 0.15;
    var acc: vec3<f32> = vec3<f32>(0.0);
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 3.0).rgb * w0;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 2.0).rgb * w1;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 1.0).rgb * w2;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv).rgb * w3;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 1.0).rgb * w2;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 2.0).rgb * w1;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 3.0).rgb * w0;
    return vec4<f32>(acc, 1.0);
}

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn vignette(uv: vec2<f32>) -> f32 {
    let r = length(uv - 0.5);
    return smoothstep(0.95, 0.45, r);
}

fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Simple FBM using sin/cos mixing for soft, wispy noise
fn fbm(p: vec2<f32>) -> f32 {
    var a = 0.0;
    var b = 0.5;
    var f = p;
    for (var i = 0; i < 5; i = i + 1) {
        a += b * sin(f.x) * cos(f.y);
        f *= 2.17;
        b *= 0.55;
    }
    return a;
}

// COMPOSITE: tone-map HDR + add bloom + color grading and grain
@fragment
fn fs_composite(inp: VsOut) -> @location(0) vec4<f32> {
    var base = textureSample(hdr_tex, hdr_sampler, inp.uv).rgb;
    let bloom = textureSample(blur_tex, blur_sampler, inp.uv).rgb * u_post.bloom_strength;
    base += bloom;

    // Subtle hue warp based on ambient and time
    let t = u_post.time * 0.15;
    let ambient = u_post.ambient;
    let hue = vec3<f32>(sin(t * 1.2) + 1.0, sin(t * 1.5 + 2.0) + 1.0, sin(t * 1.8 + 4.0) + 1.0) * 0.05 * ambient;
    base *= (vec3<f32>(1.0) + hue);

    // Darken exposure slightly before tonemapping to give a deeper look
    base *= 0.9;

    // Tonemap
    var mapped = aces_tonemap(base);

    // Contrast and gamma to increase punch without blowing highlights
    let contrast = 0.15; // positive increases contrast
    mapped = clamp((mapped - vec3<f32>(0.5)) * (1.0 + contrast) + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    mapped = pow(mapped, vec3<f32>(1.07));

    // Stronger vignette for moodier edges
    let vig = vignette(inp.uv);
    mapped *= mix(1.0, 0.75, vig);

    // Smoky darkening using low-frequency FBM modulated by radius
    let uv = inp.uv;
    let r = length(uv - 0.5);
    let smokeField = 0.5 + 0.5 * fbm(uv * 2.6 + vec2<f32>(u_post.time * 0.05, -u_post.time * 0.04));
    let smokeField2 = 0.5 + 0.5 * fbm((uv.yx + vec2<f32>(0.17, -0.09)) * 3.1 + vec2<f32>(-u_post.time * 0.035, u_post.time * 0.045));
    let smoke = clamp(0.5 * smokeField + 0.5 * smokeField2, 0.0, 1.0);
    let radial = smoothstep(0.2, 0.95, r);
    let smokeStrength = 0.18; // overall intensity
    let k = smokeStrength * radial * smoke;
    // Darken multiplicatively; tiny bluish tint in the darkening
    let smokeTint = vec3<f32>(0.03, 0.04, 0.06);
    mapped = mapped * (1.0 - k) + smokeTint * (k * 0.25);

    // Film grain
    let noise = hash2(inp.uv * u_post.resolution + u_post.time);
    mapped += (noise - 0.5) * 0.022;

    // Slight desaturation for a smokier palette
    let luma = luminance(mapped);
    mapped = mix(vec3<f32>(luma), mapped, 0.9);

    return vec4<f32>(mapped, 1.0);
}


