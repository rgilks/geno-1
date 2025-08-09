// Audio-reactive ribbon/heightfield aesthetic rendered in a single fullscreen pass.
// Inspired by smooth velvet waves with golden accents.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Voice {
    // xyz position (x,z used), w = pulse (0..1.5)
    pos_pulse: vec4<f32>,
    // rgb base, a unused
    color: vec4<f32>,
};

struct WaveUniforms {
    resolution: vec2<f32>,
    time: f32,
    ambient: f32,
    voices: array<Voice, 3>,
    swirl_uv: vec2<f32>,
    swirl_strength: f32,
    swirl_active: f32,
};

@group(0) @binding(0) var<uniform> u: WaveUniforms;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> VsOut {
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

fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

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

// --- Wireframe helpers and layered waves version ---
fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn wireframe_bg(p: vec2<f32>) -> f32 {
    var d = 1e9;
    let centers = array<vec2<f32>, 3>(
        vec2<f32>(0.62, 0.28),
        vec2<f32>(0.80, 0.44),
        vec2<f32>(0.70, 0.72),
    );
    let radii = array<f32, 3>(0.28, 0.18, 0.12);
    for (var i = 0; i < 3; i = i + 1) {
        d = min(d, abs(length(p - centers[i]) - radii[i]));
    }
    d = min(d, sd_segment(p, vec2<f32>(0.55, 0.20), vec2<f32>(0.85, 0.50)));
    d = min(d, sd_segment(p, vec2<f32>(0.62, 0.28), vec2<f32>(0.80, 0.44)));
    d = min(d, sd_segment(p, vec2<f32>(0.72, 0.70), vec2<f32>(0.86, 0.52)));
    return d;
}

@fragment
fn fs_waves(inp: VsOut) -> @location(0) vec4<f32> {
    let uv = inp.uv;
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let cuv0 = (uv - 0.5) * vec2<f32>(aspect, 1.0);
    let t = u.time;

    // Background wireframe
    let gold = vec3<f32>(1.00, 0.86, 0.46);
    let d = wireframe_bg(uv);
    var col = vec3<f32>(0.04, 0.055, 0.10);
    let line = smoothstep(0.012, 0.002, d);
    // Make wireframe more visible; it will bloom subtly
    col += gold * line * (0.30 + 0.60 * u.ambient);

    // Three layered waves with parallax
    for (var L = 0; L < 3; L = L + 1) {
        let depth = f32(L);
        let par = mix(0.65, 1.25, depth / 2.0);
        var cuv = cuv0 * par + vec2<f32>(0.0, -0.10 * depth);
        // Swirl displacement driven by pointer position.
        // Always active; strength is provided by CPU and can be small when idle.
        let c = (u.swirl_uv - 0.5) * vec2<f32>(aspect, 1.0) * par;
        let v = cuv - c;
        let r = length(v);
        let ang = u.swirl_strength * 2.5 * exp(-1.8 * r);
        let cs = cos(ang);
        let sn = sin(ang);
        let rot = vec2<f32>(v.x * cs - v.y * sn, v.x * sn + v.y * cs);
        cuv = c + rot;
        // Displace coordinates by nearby voices so dragging clearly affects visuals
        var disp = vec2<f32>(0.0);
        for (var i = 0; i < 3; i = i + 1) {
            let v = u.voices[i];
            let p = vec2<f32>(v.pos_pulse.x, v.pos_pulse.z) * 0.33;
            let d = distance(cuv, p);
            let dir = normalize(cuv - p);
            let pulse = clamp(v.pos_pulse.w, 0.0, 1.5);
            let str = (0.12 + 0.45 * pulse) * exp(-1.8 * d);
            disp += dir * str;
        }
        cuv += disp;
        let tt = t * (0.30 + 0.08 * depth);
        let amp = mix(1.0, 2.2, depth / 2.0);
        var h = 0.0;
        // Broader ribbons along X with mild Y slant
        h += amp * (1.05 * sin((6.0 + 1.0 * depth) * cuv.x - 1.2 * tt));
        h += amp * (0.65 * sin((9.0 + 1.5 * depth) * cuv.x + 0.8 * tt + 0.7 * cuv.y));
        // Envelope so center has more curvature
        h *= (1.0 - 0.25 * abs(cuv.y));
        // Add velvet noise
        h += 0.35 * fbm(cuv * 2.4 + vec2<f32>(0.22 * tt, -0.16 * tt));
        for (var i = 0; i < 3; i = i + 1) {
            let v = u.voices[i];
            let p = vec2<f32>(v.pos_pulse.x, v.pos_pulse.z) * 0.33;
            let dd = distance(cuv, p);
            let pulse = clamp(v.pos_pulse.w, 0.0, 1.5);
            h += (0.65 + 0.9 * pulse) * exp(-2.2 * dd) * sin(14.0 * dd - 2.0 * tt);
            h += 0.22 * (1.0 / (1.0 + 6.0 * dd)) * sin(7.0 * (cuv.x - p.x) + 1.5 * tt);
        }
        let e = 0.002;
        let hx = h - (0.55 * sin(6.0 * (cuv.x - e) - 1.4 * tt) + 0.35 * sin(10.0 * (cuv.x - e) + 0.9 * tt + 0.8 * cuv.y) + 0.25 * fbm((cuv - vec2<f32>(e, 0.0)) * 2.5 + vec2<f32>(0.2 * tt, -0.15 * tt)));
        let hy = h - (0.55 * sin(6.0 * cuv.x - 1.4 * tt) + 0.35 * sin(10.0 * cuv.x + 0.9 * tt + 0.8 * (cuv.y - e)) + 0.25 * fbm((cuv - vec2<f32>(0.0, e)) * 2.5 + vec2<f32>(0.2 * tt, -0.15 * tt)));
        let n = normalize(vec3<f32>(hx, hy, e));
        let l1 = normalize(vec3<f32>(-0.4, 0.3, 0.85));
        let l2 = normalize(vec3<f32>(0.6, -0.2, 0.75));
        let diff = 0.65 * max(dot(n, l1), 0.0) + 0.35 * max(dot(n, l2), 0.0);
        let base = mix(vec3<f32>(0.03, 0.04, 0.08), vec3<f32>(0.12, 0.14, 0.26), diff + 0.15 * u.ambient);
        let cool = vec3<f32>(0.18, 0.45, 1.05);
        let warm = vec3<f32>(1.08, 0.86, 0.40);
        let k = clamp(0.5 + 1.1 * h, 0.0, 1.0);
        var lay = base + mix(cool * 0.45, warm * 0.55, k);
        let stripes = smoothstep(0.45, 0.5, abs(fract(h * 8.0) - 0.5));
        lay += (1.0 - stripes) * gold * (0.18 + 0.30 * u.ambient);
        let view = vec3<f32>(0.0, 0.0, 1.0);
        let h1 = normalize(l1 + view);
        lay += vec3<f32>(1.0) * (0.18 * pow(max(dot(n, h1), 0.0), 72.0));
        // Emissive crest glow captured by bloom
        let crest = smoothstep(0.84, 0.98, k);
        lay += gold * crest * (0.75 + 1.4 * u.ambient);
        // Visible voice focus points
        for (var i = 0; i < 3; i = i + 1) {
            let p = vec2<f32>(u.voices[i].pos_pulse.x, u.voices[i].pos_pulse.z) * 0.33;
            let dd = distance(cuv, p);
            lay += gold * exp(-40.0 * dd * dd) * 0.45;
        }
        let a = mix(0.55, 0.28, depth / 2.0);
        col = col * (1.0 - a) + lay * a;
    }

    let s = hash2(cuv0 * 600.0 + t);
    col += (step(0.992, s) * (s - 0.992) * 240.0) * gold * (0.35 + 0.55 * u.ambient);

    return vec4<f32>(col, 1.0);
}


