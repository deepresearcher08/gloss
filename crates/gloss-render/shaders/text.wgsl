struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

@group(1) @binding(0) var samp: sampler;
@group(1) @binding(1) var t_atlas: texture_2d<f32>;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) clip: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) clip: vec4<f32>,
    @location(3) world: vec2<f32>,
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
    out.uv = mix(inst.uv.xy, inst.uv.zw, unit);
    out.color = inst.color;
    out.clip = inst.clip;
    out.world = pixel;
    return out;
}

fn clip_coverage(world: vec2<f32>, clip: vec4<f32>) -> f32 {
    let edge = min(world - clip.xy, clip.zw - world);
    let aa = max(fwidth(edge), vec2<f32>(0.0005));
    return smoothstep(-aa.x, aa.x, edge.x) * smoothstep(-aa.y, aa.y, edge.y);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(t_atlas, samp, in.uv).r * clip_coverage(in.world, in.clip);
    let a = in.color.a * coverage;
    return vec4<f32>(in.color.rgb * a, a);
}
