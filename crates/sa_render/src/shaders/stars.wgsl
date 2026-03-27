struct StarUniforms {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: StarUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) brightness: f32,
    @location(2) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) star_color: vec3<f32>,
    @location(1) star_brightness: f32,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    let pos = vec4<f32>(vertex.position * 90000.0, 1.0);
    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * pos;
    out.star_color = vertex.color;
    out.star_brightness = vertex.brightness;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.star_color * in.star_brightness, 1.0);
}
