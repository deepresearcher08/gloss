struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> arena: array<vec4<f32>>;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) border: vec4<f32>,
    @location(4) spec: vec4<f32>,
    @location(5) params: vec4<f32>,
    @location(6) gradient: vec4<f32>,
    @location(7) clip: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) shape: vec4<f32>,
    @location(1) @interpolate(flat) radii: vec4<f32>,
    @location(2) @interpolate(flat) fill: vec4<f32>,
    @location(3) @interpolate(flat) border: vec4<f32>,
    @location(4) @interpolate(flat) spec: vec4<f32>,
    @location(5) @interpolate(flat) params: vec4<f32>,
    @location(6) @interpolate(flat) gradient: vec4<f32>,
    @location(7) @interpolate(flat) clip: vec4<f32>,
};

const QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    var out: VsOut;
    let aux_index = u32(inst.params.w + 0.5);
    let aux0 = arena[aux_index * 2u];
    let aux1 = arena[aux_index * 2u + 1u];
    let flags = u32(inst.params.z + 0.5);
    let kind = flags & 3u;
    var lo = inst.rect.xy;
    var hi = inst.rect.xy + inst.rect.zw;
    if (kind == 1u) {
        let blur = aux0.z;
        let spread = aux0.w;
        let offset = aux1.zw;
        let extent = vec2<f32>(blur + max(spread, 0.0));
        lo = lo + offset - extent;
        hi = hi + offset + extent;
    }
    let unit = QUAD[vi];
    let pixel = lo + unit * (hi - lo);
    out.pos = vec4<f32>(
        pixel.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - pixel.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
    out.shape = inst.rect;
    out.radii = inst.radii;
    out.fill = inst.fill;
    out.border = inst.border;
    out.spec = inst.spec;
    out.params = inst.params;
    out.gradient = inst.gradient;
    out.clip = inst.clip;
    return out;
}

struct FsIn {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) shape: vec4<f32>,
    @location(1) @interpolate(flat) radii: vec4<f32>,
    @location(2) @interpolate(flat) fill: vec4<f32>,
    @location(3) @interpolate(flat) border: vec4<f32>,
    @location(4) @interpolate(flat) spec: vec4<f32>,
    @location(5) @interpolate(flat) params: vec4<f32>,
    @location(6) @interpolate(flat) gradient: vec4<f32>,
    @location(7) @interpolate(flat) clip: vec4<f32>,
};

fn pick_radius(p: vec2<f32>, r: vec4<f32>) -> f32 {
    if (p.x < 0.0) {
        if (p.y < 0.0) { return r.x; }
        return r.w;
    }
    if (p.y < 0.0) { return r.y; }
    return r.z;
}

fn sd_round_box(p: vec2<f32>, half: vec2<f32>, radius: f32, smoothing: f32) -> f32 {
    let r = clamp(radius, 0.0, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(r, r);
    let base = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    if (smoothing <= 0.001 || r <= 0.001) {
        return base;
    }
    if (q.x > 0.0 && q.y > 0.0) {
        let n = mix(2.0, 5.0, clamp(smoothing, 0.0, 1.0));
        let u = q / max(r, 0.001);
        let s = pow(u.x, n) + pow(u.y, n);
        let f = pow(s, 1.0 / n) - 1.0;
        let gx = pow(s, 1.0 / n - 1.0) * pow(u.x, n - 1.0);
        let gy = pow(s, 1.0 / n - 1.0) * pow(u.y, n - 1.0);
        return max(r, 0.001) * f / max(length(vec2<f32>(gx, gy)), 0.0001);
    }
    return base;
}

fn shape_distance(p: vec2<f32>, in: FsIn, inflate: f32) -> f32 {
    let half = in.shape.zw * 0.5 + vec2<f32>(inflate);
    return sd_round_box(p, half, pick_radius(p, in.radii) + inflate, in.params.x);
}

fn clip_coverage(world: vec2<f32>, clip: vec4<f32>, radius: f32) -> f32 {
    let center = (clip.xy + clip.zw) * 0.5;
    let half = (clip.zw - clip.xy) * 0.5;
    let d = sd_round_box(world - center, half, radius, 0.0);
    let aa = max(fwidth(d), 0.0005);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn stop_color(base: u32, index: u32) -> vec4<f32> {
    return arena[(base + index) * 2u];
}

fn stop_offset(base: u32, index: u32) -> f32 {
    return arena[(base + index) * 2u + 1u].x;
}

fn gradient_color(in: FsIn, p: vec2<f32>) -> vec4<f32> {
    let count = u32(in.gradient.w + 0.5);
    let base = u32(in.gradient.z + 0.5);
    if (count == 0u) {
        return in.fill;
    }
    if (count == 1u) {
        return stop_color(base, 0u);
    }
    let half = in.shape.zw * 0.5;
    var t: f32;
    if (in.gradient.x < 1.5) {
        let angle = in.gradient.y * 3.14159265 / 180.0;
        let dir = vec2<f32>(cos(angle), sin(angle));
        t = clamp(dot(p / half, dir) * 0.5 + 0.5, 0.0, 1.0);
    } else {
        t = clamp(length(p) / max(half.x, half.y), 0.0, 1.0);
    }
    var color = stop_color(base, 0u);
    for (var i: u32 = 1u; i < count; i = i + 1u) {
        let a = stop_offset(base, i - 1u);
        let b = stop_offset(base, i);
        if (t >= a) {
            let span = max(b - a, 0.00001);
            let local_t = clamp((t - a) / span, 0.0, 1.0);
            color = mix(stop_color(base, i - 1u), stop_color(base, i), local_t);
        }
    }
    return color;
}

fn sdf_normal(p: vec2<f32>, in: FsIn) -> vec2<f32> {
    let e = 0.75;
    let dx = shape_distance(p + vec2<f32>(e, 0.0), in, 0.0) - shape_distance(p - vec2<f32>(e, 0.0), in, 0.0);
    let dy = shape_distance(p + vec2<f32>(0.0, e), in, 0.0) - shape_distance(p - vec2<f32>(0.0, e), in, 0.0);
    return normalize(vec2<f32>(dx, dy) + vec2<f32>(0.00001, 0.00001));
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aux_index = u32(in.params.w + 0.5);
    let aux0 = arena[aux_index * 2u];
    let aux1 = arena[aux_index * 2u + 1u];
    let flags = u32(in.params.z + 0.5);
    let kind = flags & 3u;
    let edge = (flags >> 2u) & 15u;
    let clip_radius = aux0.y;
    let world = in.pos.xy;
    let center = in.shape.xy + in.shape.zw * 0.5 + aux1.zw;
    let p = world - center;
    var clip_cov = 1.0;
    if ((flags & 64u) == 0u) {
        clip_cov = clip_coverage(world, in.clip, clip_radius);
    }
    let opacity = in.params.y;

    if (kind == 1u) {
        let spread = max(aux0.w, 0.0);
        let blur = max(aux0.z, 0.001);
        let d = shape_distance(p, in, spread);
        let coverage = 1.0 - smoothstep(0.0, blur, max(d, 0.0));
        let a = in.fill.a * coverage * opacity * clip_cov;
        return vec4<f32>(in.fill.rgb * a, a);
    }

    let d = shape_distance(p, in, 0.0);
    let aa = max(fwidth(d), 0.0005);
    var inside = 1.0 - smoothstep(-aa, aa, d);

    if (kind == 2u) {
        let spread = max(aux0.w, 0.0);
        let blur = max(aux0.z, 0.001);
        let depth = max(-d, 0.0);
        let coverage = (1.0 - smoothstep(0.0, blur, depth)) * inside;
        let a = in.fill.a * coverage * opacity * clip_cov;
        return vec4<f32>(in.fill.rgb * a, a);
    }

    inside *= clip_cov;
    var fill = in.fill;
    if (in.gradient.x > 0.5) {
        fill = gradient_color(in, p);
    }

    var rgb = fill.rgb * fill.a;
    var alpha = fill.a;

    let border_width = in.border.w;
    if (border_width > 0.0 && in.border.a > 0.0) {
        let inner = 1.0 - smoothstep(-aa, aa, d + border_width);
        let band = max(inside - inner * clip_cov, 0.0);
        let ba = in.border.a * band;
        rgb = rgb * (1.0 - ba) + in.border.rgb * ba;
        alpha = alpha * (1.0 - ba) + ba;
    }

    let spec_width = max(aux0.x, 0.0);
    let spec_intensity = in.spec.a;
    if (spec_width > 0.0 && spec_intensity > 0.0) {
        let rim = clamp(1.0 - abs(d + spec_width * 0.5) / spec_width, 0.0, 1.0);
        let n = sdf_normal(p, in);
        let w = max(-n.y, 0.0) * f32((edge >> 0u) & 1u)
              + max(n.x, 0.0) * f32((edge >> 1u) & 1u)
              + max(n.y, 0.0) * f32((edge >> 2u) & 1u)
              + max(-n.x, 0.0) * f32((edge >> 3u) & 1u);
        let sa = clamp(rim * w * spec_intensity, 0.0, 1.0) * inside;
        rgb = rgb * (1.0 - sa) + in.spec.rgb * sa;
        alpha = alpha * (1.0 - sa) + sa;
    }

    alpha *= opacity * inside;
    rgb *= opacity * inside;
    return vec4<f32>(rgb, alpha);
}
