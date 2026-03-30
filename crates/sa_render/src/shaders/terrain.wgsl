// Terrain shader with CDLOD vertex morphing.
// Shares uniforms with the geometry pipeline but has a different vertex
// layout: adds morph_target (location 7) and morph_factor (location 8).

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
    @location(8) morph_factor: f32,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(7) morph_target: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: Instance) -> VertexOutput {
    // CDLOD vertex morphing: blend position toward parent-LOD position.
    // At morph_factor=0: full detail. At morph_factor=1: matches parent LOD.
    let morphed_pos = mix(vertex.position, vertex.morph_target, instance.morph_factor);

    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    let world_pos = model * vec4<f32>(morphed_pos, 1.0);

    // Cofactor matrix for correct normal transform under non-uniform scale
    let col0 = model[0].xyz;
    let col1 = model[1].xyz;
    let col2 = model[2].xyz;
    let cofactor0 = cross(col1, col2);
    let cofactor1 = cross(col2, col0);
    let cofactor2 = cross(col0, col1);
    let world_normal = normalize(
        cofactor0 * vertex.normal.x + cofactor1 * vertex.normal.y + cofactor2 * vertex.normal.z
    );

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * world_pos;
    out.color = vertex.color;
    out.world_normal = world_normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let adjusted_n = select(-n, n, front_facing);
    let l = normalize(-uniforms.light_dir);
    let ndotl = max(dot(adjusted_n, l), 0.0);
    let diffuse = uniforms.light_color * ndotl;
    let color = in.color * (uniforms.ambient + diffuse);
    return vec4<f32>(color, 1.0);
}
