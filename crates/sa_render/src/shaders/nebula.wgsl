struct NebulaUniforms {
    view_proj: mat4x4<f32>,
    camera_right: vec3<f32>,
    _pad0: f32,
    camera_up: vec3<f32>,
    _pad1: f32,
};

@group(0) @binding(0)
var<uniform> nebula: NebulaUniforms;

struct InstanceInput {
    @location(0) center: vec3<f32>,
    @location(1) radius: f32,
    @location(2) color: vec3<f32>,
    @location(3) opacity: f32,
    @location(4) seed: f32,
    @location(5) _inst_pad0: f32,
    @location(6) _inst_pad1: f32,
    @location(7) _inst_pad2: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) neb_color: vec3<f32>,
    @location(2) neb_opacity: f32,
    @location(3) neb_seed: f32,
};

@vertex
fn vs_main(
    inst: InstanceInput,
    @builtin(vertex_index) vid: u32,
) -> VertexOutput {
    var offsets = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    let offset = offsets[vid % 6u];

    // Billboard: expand quad in camera-facing plane
    let world_pos = inst.center
        + nebula.camera_right * offset.x * inst.radius
        + nebula.camera_up * offset.y * inst.radius;

    var out: VertexOutput;
    out.position = nebula.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = offset;
    out.neb_color = inst.color;
    out.neb_opacity = inst.opacity;
    out.neb_seed = inst.seed;
    return out;
}

// Simple hash for noise
fn hash(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

// Value noise
fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = hash(i);
    let b = hash(i + vec2<f32>(1.0, 0.0));
    let c = hash(i + vec2<f32>(0.0, 1.0));
    let d = hash(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// fBm: 3 octaves of value noise
fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var freq_p = p;
    for (var i = 0; i < 3; i++) {
        value += amplitude * noise(freq_p);
        freq_p *= 2.0;
        amplitude *= 0.5;
    }
    return value;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.uv);
    if dist > 1.0 {
        discard;
    }

    // Soft radial falloff
    let radial = 1.0 - smoothstep(0.0, 1.0, dist);

    // fBm noise for cloudy shape, offset by seed
    let seed_offset = vec2<f32>(in.neb_seed * 17.3, in.neb_seed * 31.7);
    let n = fbm(in.uv * 3.0 + seed_offset);

    let alpha = radial * n * in.neb_opacity;
    // Non-premultiplied output — blend mode is SrcAlpha/OneMinusSrcAlpha
    return vec4<f32>(in.neb_color, alpha);
}
