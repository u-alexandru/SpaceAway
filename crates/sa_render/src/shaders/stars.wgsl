struct StarUniforms {
    view_proj: mat4x4<f32>,
    screen_height: f32,
    screen_width: f32,
    beta: f32,            // speed as fraction of c (0.0-0.99)
    streak_factor: f32,   // star streak length in pixels (0-300)
    velocity_dir: vec3<f32>,   // normalized velocity direction
    flash_intensity: f32,      // additive white flash (0-1)
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
    @location(2) uv: vec2<f32>,          // x = along streak, y = across
    @location(3) streak_amount: f32,     // 0 = circular, 1 = full streak
};

// Project a world-space direction to NDC, returning xy. w must be > 0.
fn project_dir(dir: vec3<f32>) -> vec2<f32> {
    let c = uniforms.view_proj * vec4<f32>(dir * 90000.0, 1.0);
    return c.xy / c.w;
}

@vertex
fn vs_main(
    vertex: VertexInput,
    @builtin(vertex_index) vid: u32,
) -> VertexOutput {
    // ---- quad corner layout (same winding as original) ----
    // uv.x = along streak axis, uv.y = across streak axis
    var along_offsets: array<f32, 6> = array<f32, 6>(
        -1.0,  1.0,  1.0,
        -1.0,  1.0, -1.0,
    );
    var across_offsets: array<f32, 6> = array<f32, 6>(
        -1.0, -1.0,  1.0,
        -1.0,  1.0,  1.0,
    );
    let corner = vid % 6u;
    let along_off = along_offsets[corner];
    let across_off = across_offsets[corner];

    // ---- Relativistic aberration ----
    var star_dir = normalize(vertex.position);

    if uniforms.beta > 0.01 {
        let v = normalize(uniforms.velocity_dir);
        let cos_theta = dot(star_dir, v);
        // Perpendicular component of star_dir relative to velocity
        let perp = star_dir - cos_theta * v;
        let perp_len = length(perp);

        // Aberrated cos(theta')
        let cos_tp = (cos_theta - uniforms.beta) / (1.0 - uniforms.beta * cos_theta);
        let sin_tp = sqrt(max(0.0, 1.0 - cos_tp * cos_tp));

        if perp_len > 1e-6 {
            // Reconstruct aberrated direction
            let perp_hat = perp / perp_len;
            star_dir = normalize(cos_tp * v + sin_tp * perp_hat);
        } else {
            // Parallel or anti-parallel: stays on axis, just flip if needed
            star_dir = select(-v, v, cos_theta > 0.0);
        }
    }

    // ---- Project star center to clip space ----
    let world_pos = vec4<f32>(star_dir * 90000.0, 1.0);
    let clip = uniforms.view_proj * world_pos;

    // Convert to NDC; snap center to pixel grid (reduces flicker)
    let ndc = clip.xy / clip.w;
    let pixel = vec2<f32>(ndc.x * uniforms.screen_width * 0.5, ndc.y * uniforms.screen_height * 0.5);
    let snapped_pixel = round(pixel);
    let snapped_ndc = vec2<f32>(
        snapped_pixel.x / (uniforms.screen_width * 0.5),
        snapped_pixel.y / (uniforms.screen_height * 0.5),
    );

    // ---- Star base size ----
    let size_px = mix(1.0, 2.5, vertex.brightness);

    // ---- Doppler shift ----
    // Use original (pre-aberration) angle for Doppler computation
    let orig_dir = normalize(vertex.position);
    let v_norm = select(vec3<f32>(0.0, 0.0, 1.0), normalize(uniforms.velocity_dir), uniforms.beta > 0.01);
    let cos_theta_orig = dot(orig_dir, v_norm);

    let denom_d = max(1e-6, 1.0 - uniforms.beta * cos_theta_orig);
    let numer_d = max(0.0, 1.0 + uniforms.beta * cos_theta_orig);
    let doppler = sqrt(numer_d / denom_d);

    // Shift color: forward = blue, rear = red
    let doppler_fwd = vec3<f32>(0.7, 0.85, 1.3);
    let doppler_bwd = vec3<f32>(1.3, 0.85, 0.7);
    var shifted_color = vertex.color;
    if uniforms.beta > 0.01 {
        let t = clamp((doppler - 1.0) * 2.0, -1.0, 1.0);
        let shift_tgt = select(doppler_bwd, doppler_fwd, t >= 0.0);
        shifted_color = vertex.color * mix(vec3<f32>(1.0), shift_tgt, abs(t) * 0.6);
    }

    // Relativistic beaming: stars ahead appear brighter
    let beaming = select(1.0, doppler * doppler, uniforms.beta > 0.01);
    let boosted_brightness = clamp(vertex.brightness * beaming, 0.0, 1.0);

    // ---- Streak geometry ----
    let streak_amount = clamp(uniforms.streak_factor / 300.0, 0.0, 1.0);

    // Project velocity direction to screen to find streak axis
    let center_ndc = snapped_ndc;
    let offset_ndc = project_dir(star_dir + uniforms.velocity_dir * 0.01);
    var streak_screen = offset_ndc - center_ndc;
    let streak_screen_len = length(streak_screen);
    var tangent_ndc = vec2<f32>(1.0, 0.0); // fallback
    if streak_screen_len > 1e-7 {
        tangent_ndc = streak_screen / streak_screen_len;
    }
    let normal_ndc = vec2<f32>(-tangent_ndc.y, tangent_ndc.x);

    // sin(theta) modulation: stars near velocity axis streak less
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta_orig * cos_theta_orig));

    // Streak length in NDC units
    let streak_len_px = uniforms.streak_factor * sin_theta;
    let streak_len_ndc = streak_len_px / uniforms.screen_height * 2.0;

    // Width stays 1-2 pixels
    let width_ndc = max(1.0, size_px * 0.5) / uniforms.screen_height * 2.0;

    // For non-streaked mode we use the original square offset
    let square_ndc = vec2<f32>(
        across_off * size_px / uniforms.screen_width * 2.0,
        along_off  * size_px / uniforms.screen_height * 2.0,
    );

    // For streaked mode: tangent carries streak length, normal carries width
    // along_off in [-1,1]: negative = tail, positive = head
    let streak_offset_ndc = tangent_ndc * (along_off * streak_len_ndc * 0.5)
                          + normal_ndc   * (across_off * width_ndc * 0.5);

    let final_offset_ndc = mix(square_ndc, streak_offset_ndc, streak_amount);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(
        (snapped_ndc.x + final_offset_ndc.x) * clip.w,
        (snapped_ndc.y + final_offset_ndc.y) * clip.w,
        clip.z,
        clip.w,
    );
    out.star_color = shifted_color;
    out.star_brightness = boosted_brightness;
    out.uv = vec2<f32>(along_off, across_off);
    out.streak_amount = streak_amount;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var intensity: f32;

    if in.streak_amount > 0.001 {
        // Elongated streak falloff
        let along  = in.uv.x;  // -1 = tail, +1 = head
        let across = in.uv.y;

        // Width falloff — thin perpendicular profile
        let width_fade = 1.0 - smoothstep(0.0, 1.0, abs(across));

        // Asymmetric along-axis falloff: bright head, fading tail
        var along_fade: f32;
        if along >= 0.0 {
            // Head side: stays bright close to center
            along_fade = 1.0 - smoothstep(0.0, 1.0, along);
        } else {
            // Tail side: fade out more aggressively
            along_fade = 1.0 - smoothstep(0.0, 1.0, -along * 0.7);
        }

        let streak_intensity = width_fade * along_fade * in.star_brightness;

        // Circular falloff for non-streaked appearance
        let dist = length(in.uv);
        let circ_core = 1.0 - smoothstep(0.0, 1.0, dist);
        let circ_intensity = circ_core * in.star_brightness;

        // Blend between circle and streak based on streak_amount
        intensity = mix(circ_intensity, streak_intensity, in.streak_amount);

        if intensity <= 0.0 {
            discard;
        }
    } else {
        // Original circular rendering (unchanged)
        let dist = length(in.uv);
        if dist > 1.0 {
            discard;
        }
        intensity = (1.0 - smoothstep(0.0, 1.0, dist)) * in.star_brightness;
    }

    let base_color = in.star_color * intensity;
    let flash_color = vec3<f32>(uniforms.flash_intensity);
    let final_color = base_color + flash_color;
    let final_alpha = clamp(intensity + uniforms.flash_intensity, 0.0, 1.0);

    return vec4<f32>(final_color, final_alpha);
}
