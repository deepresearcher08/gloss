struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> arena: array<vec4<f32>>;

@group(1) @binding(0) var samp: sampler;
@group(1) @binding(1) var t_scene: texture_2d<f32>;
@group(1) @binding(2) var t_blur: texture_2d<f32>;

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
    let unit = QUAD[vi];
    let pixel = inst.rect.xy + unit * inst.rect.zw;
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

fn shape_distance(p: vec2<f32>, in: FsIn) -> f32 {
    let half = in.shape.zw * 0.5;
    return sd_round_box(p, half, pick_radius(p, in.radii), in.params.x);
}

fn clip_coverage(world: vec2<f32>, clip: vec4<f32>, radius: f32) -> f32 {
    let center = (clip.xy + clip.zw) * 0.5;
    let half = (clip.zw - clip.xy) * 0.5;
    let d = sd_round_box(world - center, half, radius, 0.0);
    let aa = max(fwidth(d), 0.0005);
    return 1.0 - smoothstep(-aa, aa, d);
}

fn sdf_normal(p: vec2<f32>, in: FsIn) -> vec2<f32> {
    let e = 0.75;
    let dx = shape_distance(p + vec2<f32>(e, 0.0), in) - shape_distance(p - vec2<f32>(e, 0.0), in);
    let dy = shape_distance(p + vec2<f32>(0.0, e), in) - shape_distance(p - vec2<f32>(0.0, e), in);
    return normalize(vec2<f32>(dx, dy) + vec2<f32>(0.00001, 0.00001));
}

fn sample_backdrop(uv: vec2<f32>, in: FsIn, blur: f32, shift: vec2<f32>) -> vec4<f32> {
    let sharp = textureSampleLevel(t_scene, samp, uv + shift, 0.0);
    let blurred = textureSampleLevel(t_blur, samp, uv + shift, 0.0);
    let strength = clamp(blur / 24.0, 0.0, 1.0);
    return mix(sharp, blurred, strength);
}

fn saturate_color(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let lum = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(lum), c, amount);
}

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    return fract((q.x + q.y) * 43758.5453);
}

@fragment
fn fs_main(in: FsIn) -> @location(0) vec4<f32> {
    let aux_index = u32(in.params.w + 0.5);
    let aux0 = arena[aux_index * 2u];
    let aux1 = arena[aux_index * 2u + 1u];
    let flags = u32(in.params.z + 0.5);
    let edge = (flags >> 2u) & 15u;
    let world = in.pos.xy;
    let uv = world / globals.viewport;
    let center = in.shape.xy + in.shape.zw * 0.5;
    let p = world - center;
    let d = shape_distance(p, in);
    let aa = max(fwidth(d), 0.0005);
    let clip_cov = clip_coverage(world, in.clip, aux0.y);
    let inside = (1.0 - smoothstep(-aa, aa, d)) * clip_cov;
    let opacity = in.params.y;

    let n = sdf_normal(p, in);
    let refract = aux1.x;
    let dispersion = aux1.y;
    let base_offset = n * refract / globals.viewport;

    var backdrop: vec4<f32>;
    if (dispersion > 0.001) {
        let dr = sample_backdrop(uv, in, aux0.z, base_offset * (1.0 + dispersion)).r;
        let dg = sample_backdrop(uv, in, aux0.z, base_offset).g;
        let db = sample_backdrop(uv, in, aux0.z, base_offset * (1.0 - dispersion)).b;
        backdrop = vec4<f32>(dr, dg, db, 1.0);
    } else {
        backdrop = sample_backdrop(uv, in, aux0.z, base_offset);
    }

    var color = saturate_color(backdrop.rgb, aux0.w);
    color = mix(color, in.fill.rgb, in.fill.a);
    color += (hash21(world) - 0.5) * 0.02;

    var rgb = color * inside;
    var alpha = inside;

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
        let w = max(-n.y, 0.0) * f32((edge >> 0u) & 1u)
              + max(n.x, 0.0) * f32((edge >> 1u) & 1u)
              + max(n.y, 0.0) * f32((edge >> 2u) & 1u)
              + max(-n.x, 0.0) * f32((edge >> 3u) & 1u);
        let sa = clamp(rim * w * spec_intensity, 0.0, 1.0) * inside;
        rgb = rgb * (1.0 - sa) + in.spec.rgb * sa;
        alpha = alpha * (1.0 - sa) + sa;
    }

    alpha *= opacity;
    rgb *= opacity;
    return vec4<f32>(rgb, alpha);
}
