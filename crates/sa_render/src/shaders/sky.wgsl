// Sky shader: analytical Milky Way galaxy + galactic core glow
// Optimized: fused density/dust, logarithmic sample spacing, half-res friendly.

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

// --- Galaxy density model (fused emission + dust in one pass) ---

const TAU: f32 = 6.2831853;
const PI: f32 = 3.1415926;
const DISC_THICKNESS: f32 = 500.0;
const ARM_K: f32 = 0.25;
const ARM_SPACING: f32 = 1.5707963; // PI / 2
const ARM_WIDTH: f32 = 2500.0;
const ARM_WIDTH_SQ: f32 = 6250000.0; // ARM_WIDTH * ARM_WIDTH
const BULGE_SCALE: f32 = 5000.0;
const DUST_ABSORPTION: f32 = 0.00008;
const DUST_WIDTH_SQ: f32 = 2250000.0; // 1500 * 1500

// Returns vec2(emission, dust) — fused to avoid recomputing r, theta, arm distance.
fn galaxy_sample(x: f32, y: f32, z: f32) -> vec2<f32> {
    let r = max(sqrt(x * x + z * z), 1.0);
    let theta = atan2(z, x);

    // Find minimum distance to any of 4 spiral arms
    var min_arm_dist = 99999.0;
    var min_dust_dist = 99999.0;
    for (var i = 0; i < 4; i++) {
        let offset = f32(i) * ARM_SPACING;
        let theta_arm = ARM_K * log(r) + offset;
        var d_theta = theta - theta_arm;
        d_theta = d_theta - round(d_theta / TAU) * TAU;
        let linear_dist = abs(d_theta) * r;
        min_arm_dist = min(min_arm_dist, linear_dist);
        // Dust lanes: offset inward from arm center
        min_dust_dist = min(min_dust_dist, abs(linear_dist - 500.0));
    }

    // Emission: disc * (arms + bulge + background)
    let disc = exp(-abs(y) / DISC_THICKNESS);
    let arm = exp(-(min_arm_dist * min_arm_dist) / ARM_WIDTH_SQ);
    let r3d = sqrt(x * x + y * y + z * z);
    let bulge = exp(-r3d / BULGE_SCALE);
    let emission = disc * (arm + bulge + 0.01);

    // Dust: concentrated in thin disc, on inner arm edges
    let dust_disc = exp(-abs(y) / (DISC_THICKNESS * 0.5));
    let dust_arm = exp(-(min_dust_dist * min_dust_dist) / DUST_WIDTH_SQ);
    let dust = dust_disc * dust_arm;

    return vec2<f32>(emission, dust);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Reconstruct view direction from screen UV
    let ndc = vec4<f32>(in.uv, 1.0, 1.0);
    let world_dir_h = sky.inv_view_proj * ndc;
    let view_dir = normalize(world_dir_h.xyz / world_dir_h.w);

    // --- Ray-march through galaxy density ---
    // Logarithmic spacing: more samples near observer, fewer far away.
    // 8 log-spaced samples ≈ 16 uniform samples in visual quality.
    let num_samples = 8;
    let min_t: f32 = 200.0;
    let max_t: f32 = 50000.0;
    let log_min = log(min_t);
    let log_max = log(max_t);
    let log_step = (log_max - log_min) / f32(num_samples);

    var accumulated: f32 = 0.0;
    var transmittance: f32 = 1.0;
    var warm_weight: f32 = 0.0;
    var total_weight: f32 = 0.0;

    let ox = sky.observer_pos.x;
    let oy = sky.observer_pos.y;
    let oz = sky.observer_pos.z;

    for (var s = 0; s < num_samples; s++) {
        // Log-spaced sample position (center of each log interval)
        let log_t = log_min + (f32(s) + 0.5) * log_step;
        let t = exp(log_t);
        // Width of this sample interval (for proper integration weight)
        let dt = exp(log_min + f32(s + 1) * log_step) - exp(log_min + f32(s) * log_step);

        let sx = ox + view_dir.x * t;
        let sy = oy + view_dir.y * t;
        let sz = oz + view_dir.z * t;

        let sample = galaxy_sample(sx, sy, sz);
        let emission = sample.x;
        let dust = sample.y;

        // Beer-Lambert absorption
        transmittance *= exp(-dust * dt * DUST_ABSORPTION);

        let contribution = emission * transmittance * dt / max_t;
        accumulated += contribution;

        // Warmth: weight by proximity to galactic center
        let r_center = sqrt(sx * sx + sy * sy + sz * sz);
        let bulge_weight = exp(-r_center / BULGE_SCALE);
        warm_weight += contribution * bulge_weight;
        total_weight += contribution;
    }

    // Two-part curve: smoothstep kills low values, pow preserves arm contrast
    let fade = smoothstep(0.03, 0.15, accumulated);
    let detail = pow(accumulated * 1.5, 0.7);
    let brightness = min(fade * detail, 0.85);

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
