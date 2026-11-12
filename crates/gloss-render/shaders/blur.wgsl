@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var t_source: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

const TRI: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(3.0, -1.0),
    vec2<f32>(-1.0, 3.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(TRI[vi], 0.0, 1.0);
    return out;
}

fn kawase(uv: vec2<f32>, src: vec2<f32>) -> vec4<f32> {
    let o = 0.5 / src;
    var c = textureSample(t_source, samp, uv) * 4.0;
    c += textureSample(t_source, samp, uv + vec2<f32>(-o.x, -o.y));
    c += textureSample(t_source, samp, uv + vec2<f32>(o.x, -o.y));
    c += textureSample(t_source, samp, uv + vec2<f32>(-o.x, o.y));
    c += textureSample(t_source, samp, uv + vec2<f32>(o.x, o.y));
    return c / 8.0;
}

@fragment
fn down(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let src = vec2<f32>(textureDimensions(t_source));
    let dst = max(src * 0.5, vec2<f32>(1.0));
    return kawase(pos.xy / dst, src);
}

@fragment
fn up(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let src = vec2<f32>(textureDimensions(t_source));
    let dst = src * 2.0;
    return kawase(pos.xy / dst, src);
}
