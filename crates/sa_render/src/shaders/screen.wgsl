// Screen pipeline: renders a textured quad for in-world monitors.
// Bind group 0: view/projection uniforms (shared with geometry pipeline).
// Bind group 1: screen texture + sampler.

struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    _pad: f32,
    light_color: vec3<f32>,
    _pad2: f32,
    ambient: vec3<f32>,
    _pad3: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(1) @binding(0)
var screen_texture: texture_2d<f32>;
@group(1) @binding(1)
var screen_sampler: sampler;

struct Instance {
    @location(2) model_0: vec4<f32>,
    @location(3) model_1: vec4<f32>,
    @location(4) model_2: vec4<f32>,
    @location(5) model_3: vec4<f32>,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: Instance) -> VertexOutput {
    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    let world_pos = model * vec4<f32>(vertex.position, 1.0);

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * world_pos;
    out.uv = vertex.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(screen_texture, screen_sampler, in.uv);
}
