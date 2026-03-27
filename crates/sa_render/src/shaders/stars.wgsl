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
    let corner = vid % 6u;

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

    // Convert to NDC to snap center to pixel grid (reduces flicker)
    let ndc = clip.xy / clip.w;
    let pixel = ndc * uniforms.screen_height * 0.5;
    let snapped_pixel = round(pixel);
    let snapped_ndc = snapped_pixel / (uniforms.screen_height * 0.5);

    // Star size: bright stars get a slightly larger quad.
    // Dim stars stay at 1px (crisp point), bright ones up to 2.5px.
    let size_px = mix(1.0, 2.5, vertex.brightness);
    let pixel_size = size_px / uniforms.screen_height * 2.0;

    var out: VertexOutput;
    out.clip_position = vec4<f32>(
        (snapped_ndc.x + offset.x * pixel_size) * clip.w,
        (snapped_ndc.y + offset.y * pixel_size) * clip.w,
        clip.z,
        clip.w,
    );
    out.star_color = vertex.color;
    out.star_brightness = vertex.brightness;
    out.uv = offset;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Hard-edged circle for small stars, slight softness for bright ones.
    let dist = length(in.uv);
    if dist > 1.0 {
        discard;
    }

    // Crisp center with minimal falloff — keeps stars sharp
    let core = 1.0 - smoothstep(0.0, 1.0, dist);
    let intensity = core * in.star_brightness;

    return vec4<f32>(in.star_color * intensity, intensity);
}
