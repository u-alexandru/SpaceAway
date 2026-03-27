// Sky shader: Milky Way cubemap + galactic core glow

struct SkyUniforms {
    inv_view_proj: mat4x4<f32>,
    galactic_center_dir: vec3<f32>,
    core_brightness: f32,
    cubemap_enabled: u32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0)
var<uniform> sky: SkyUniforms;

@group(0) @binding(1)
var cubemap_texture: texture_cube<f32>;

@group(0) @binding(2)
var cubemap_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Fullscreen quad from vertex_index: 0,1,2 and 2,1,3
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );
    var indices = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let idx = indices[vid];
    let pos = positions[idx];

    var out: VertexOutput;
    out.position = vec4<f32>(pos, 0.999, 1.0);
    out.uv = pos;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct view direction from screen UV
    let ndc = vec4<f32>(in.uv, 1.0, 1.0);
    let world_dir_h = sky.inv_view_proj * ndc;
    let view_dir = normalize(world_dir_h.xyz / world_dir_h.w);

    var color = vec3<f32>(0.0);

    // --- Milky Way cubemap ---
    if sky.cubemap_enabled == 1u {
        let cubemap_color = textureSample(cubemap_texture, cubemap_sampler, view_dir).rgb;
        color += cubemap_color;
    }

    // --- Galactic core glow ---
    let gc_dir = normalize(sky.galactic_center_dir);
    let cos_angle = dot(view_dir, gc_dir);
    // Convert to angle for gaussian falloff
    let angle = acos(clamp(cos_angle, -1.0, 1.0));
    let spread = 0.4; // radians
    let glow = sky.core_brightness * exp(-angle * angle / (spread * spread));
    let core_color = vec3<f32>(0.95, 0.85, 0.6) * glow;
    color += core_color;

    return vec4<f32>(color, 1.0);
}
