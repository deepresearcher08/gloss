@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var t_scene: texture_2d<f32>;

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

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    return fract((q.x + q.y) * 43758.5453);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(t_scene));
    let color = textureSample(t_scene, samp, in.pos.xy / size);
    let noise = (hash21(in.pos.xy) - 0.5) * (1.0 / 255.0);
    return vec4<f32>(color.rgb + vec3<f32>(noise), color.a);
}
