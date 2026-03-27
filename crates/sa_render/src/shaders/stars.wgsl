struct StarUniforms {
    view_proj: mat4x4<f32>,
    screen_height: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
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
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(
    vertex: VertexInput,
    @builtin(vertex_index) vid: u32,
) -> VertexOutput {
    // Each star is 6 vertices (2 triangles forming a quad).
    // The star index is vid / 6, the corner is vid % 6.
    let corner = vid % 6u;

    // Quad corners: 0,1,2 and 3,4,5 (two triangles)
    var offsets: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );

    let offset = offsets[corner];

    // Project star center to clip space
    let world_pos = vec4<f32>(vertex.position * 90000.0, 1.0);
    let clip = uniforms.view_proj * world_pos;

    // Star size in pixels: brighter stars are larger (1.5 to 4px radius)
    let base_size = mix(1.5, 4.0, vertex.brightness);
    // Convert pixel size to clip-space offset
    let pixel_to_clip = vec2<f32>(2.0 / uniforms.screen_height);

    var out: VertexOutput;
    out.clip_position = clip;
    // Offset in clip space (after perspective divide, so multiply by w)
    out.clip_position.x += offset.x * base_size * pixel_to_clip.x * clip.w;
    out.clip_position.y += offset.y * base_size * pixel_to_clip.y * clip.w;
    out.star_color = vertex.color;
    out.star_brightness = vertex.brightness;
    out.uv = offset;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Soft circle falloff — makes stars round with a glow
    let dist = length(in.uv);
    if dist > 1.0 {
        discard;
    }
    // Smooth falloff: bright center, soft edge
    let alpha = smoothstep(1.0, 0.3, dist);
    let glow = alpha * in.star_brightness;
    return vec4<f32>(in.star_color * glow, glow);
}
