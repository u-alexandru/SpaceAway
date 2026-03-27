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

struct Instance {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
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
    let world_normal = normalize((model * vec4<f32>(vertex.normal, 0.0)).xyz);

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * world_pos;
    out.color = vertex.color;
    out.world_normal = world_normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let l = normalize(-uniforms.light_dir);
    let ndotl = max(dot(n, l), 0.0);
    let diffuse = uniforms.light_color * ndotl;
    let color = in.color * (uniforms.ambient + diffuse);
    return vec4<f32>(color, 1.0);
}
