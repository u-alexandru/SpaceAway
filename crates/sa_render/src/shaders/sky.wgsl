// Sky shader: analytical Milky Way galaxy + galactic core glow
// Replaces cubemap sampling with per-pixel ray-marched galaxy density.

struct SkyUniforms {
    inv_view_proj: mat4x4<f32>,
    galactic_center_dir: vec3<f32>,
    core_brightness: f32,
    observer_pos: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0)
var<uniform> sky: SkyUniforms;

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

// --- Galaxy density model (analytical, evaluated per sample) ---

const TAU: f32 = 6.2831853;
const PI: f32 = 3.1415926;
const DISC_THICKNESS: f32 = 500.0;
const ARM_K: f32 = 0.25;
const ARM_SPACING: f32 = 1.5707963; // PI / 2
const ARM_WIDTH: f32 = 2500.0;
const BULGE_SCALE: f32 = 5000.0;
const DUST_ABSORPTION: f32 = 0.00008;

fn galaxy_density(x: f32, y: f32, z: f32) -> f32 {
    // Disc: exponential falloff from galactic plane
    let disc = exp(-abs(y) / DISC_THICKNESS);

    // Spiral arms: 4 logarithmic arms
    let r = max(sqrt(x * x + z * z), 1.0);
    let theta = atan2(z, x);

    var min_arm_dist = 99999.0;
    for (var i = 0; i < 4; i++) {
        let offset = f32(i) * ARM_SPACING;
        let theta_arm = ARM_K * log(r) + offset;
        var d_theta = theta - theta_arm;
        // Wrap to [-PI, PI]
        d_theta = d_theta - round(d_theta / TAU) * TAU;
        let linear_dist = abs(d_theta) * r;
        min_arm_dist = min(min_arm_dist, linear_dist);
    }

    let arm = exp(-(min_arm_dist * min_arm_dist) / (ARM_WIDTH * ARM_WIDTH));

    // Bulge: spherical exponential
    let r3d = sqrt(x * x + y * y + z * z);
    let bulge = exp(-r3d / BULGE_SCALE);

    // Base density very low — empty space should produce no light
    return disc * (arm + bulge + 0.01);
}

fn dust_density(x: f32, y: f32, z: f32) -> f32 {
    // Dust concentrated in disc plane and on inner edges of arms
    let disc = exp(-abs(y) / (DISC_THICKNESS * 0.5));
    let r = max(sqrt(x * x + z * z), 1.0);
    let theta = atan2(z, x);

    var min_arm_dist = 99999.0;
    for (var i = 0; i < 4; i++) {
        let offset = f32(i) * ARM_SPACING;
        let theta_arm = ARM_K * log(r) + offset;
        var d_theta = theta - theta_arm;
        d_theta = d_theta - round(d_theta / TAU) * TAU;
        // Offset inward for dust lanes (inner edge of arms)
        let linear_dist = abs(d_theta) * r - 500.0;
        min_arm_dist = min(min_arm_dist, abs(linear_dist));
    }

    let arm_dust = exp(-(min_arm_dist * min_arm_dist) / (1500.0 * 1500.0));
    return disc * arm_dust;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct view direction from screen UV
    let ndc = vec4<f32>(in.uv, 1.0, 1.0);
    let world_dir_h = sky.inv_view_proj * ndc;
    let view_dir = normalize(world_dir_h.xyz / world_dir_h.w);

    // --- Ray-march through galaxy density ---
    let num_samples = 16;
    let max_dist: f32 = 50000.0;
    let step_size = max_dist / f32(num_samples);

    var accumulated: f32 = 0.0;
    var transmittance: f32 = 1.0;
    var warm_weight: f32 = 0.0;
    var total_weight: f32 = 0.0;

    let ox = sky.observer_pos.x;
    let oy = sky.observer_pos.y;
    let oz = sky.observer_pos.z;

    for (var s = 0; s < num_samples; s++) {
        let t = (f32(s) + 0.5) * step_size;
        let sx = ox + view_dir.x * t;
        let sy = oy + view_dir.y * t;
        let sz = oz + view_dir.z * t;

        let emission = galaxy_density(sx, sy, sz);
        let dust = dust_density(sx, sy, sz);

        // Beer-Lambert absorption
        transmittance *= exp(-dust * step_size * DUST_ABSORPTION);

        let contribution = emission * transmittance * step_size / max_dist;
        accumulated += contribution;

        // Warmth: weight by proximity to galactic center (bulge region)
        let r_center = sqrt(sx * sx + sy * sy + sz * sz);
        let bulge_weight = exp(-r_center / BULGE_SCALE);
        warm_weight += contribution * bulge_weight;
        total_weight += contribution;
    }

    // Gentle brightness curve: no hard threshold, just a soft pow.
    // Low density directions fade to near-zero naturally.
    // The pow(0.5) = sqrt gives a smooth ramp: faint glow at low densities,
    // bright at high densities, no hard edge.
    let brightness = min(pow(accumulated * 2.5, 0.5), 0.85);

    // Warmth ratio
    var warmth: f32 = 0.0;
    if total_weight > 0.001 {
        warmth = clamp(warm_weight / total_weight, 0.0, 1.0);
    }

    // Color: silvery blue-white, warm gold near bulge
    let cool = vec3<f32>(0.72, 0.78, 0.95);
    let warm_col = vec3<f32>(1.0, 0.90, 0.65);
    let w = pow(warmth, 3.0);
    var color = mix(cool, warm_col, w) * brightness;

    // --- Galactic core glow ---
    let gc_dir = normalize(sky.galactic_center_dir);
    let cos_angle = dot(view_dir, gc_dir);
    let angle = acos(clamp(cos_angle, -1.0, 1.0));
    let spread: f32 = 0.4;
    let glow = sky.core_brightness * exp(-angle * angle / (spread * spread));
    let core_color = vec3<f32>(0.95, 0.85, 0.6) * glow;
    color += core_color;

    return vec4<f32>(color, 1.0);
}
