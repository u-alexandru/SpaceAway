// Fullscreen blit shader: samples a texture and outputs additively.
// Used to upscale the half-resolution sky texture to the main framebuffer.

@group(0) @binding(0)
var sky_texture: texture_2d<f32>;
@group(0) @binding(1)
var sky_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

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
    // Convert clip-space [-1,1] to UV [0,1], flip Y for texture coords
    out.uv = vec2<f32>(pos.x * 0.5 + 0.5, 1.0 - (pos.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(sky_texture, sky_sampler, in.uv);
    return color;
}
