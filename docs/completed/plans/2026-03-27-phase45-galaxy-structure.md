# Phase 4.5: Galaxy Structure & Atmosphere -- Implementation Plan

## Overview

Replace the uniform exponential star density with a spiral galaxy model. Add four visual sky layers: Milky Way cubemap band, galactic core glow, nebula sprites, and distant galaxies. All new rendering uses screen-space effects, precomputed cubemaps, and billboard sprites for minimal per-frame cost.

## Architecture Notes

- `sa_render` does NOT depend on `sa_universe`. We will NOT add that dependency.
- The galaxy density function lives in `sa_universe::galaxy`. The game binary (`spaceaway`) calls it and passes precomputed data (cubemap pixel data, nebula list, galaxy list) to `sa_render`.
- All new shaders go in `crates/sa_render/src/shaders/`.
- New render modules: `sky.rs`, `nebula.rs` in `crates/sa_render/src/`.
- Fullscreen quads use `vertex_index` to generate positions (no vertex buffer).
- Nebula noise uses `Rng64`/seed system from `sa_universe` for determinism.

## File Map

| File | Action | Lines (est.) |
|------|--------|-------------|
| `crates/sa_universe/src/galaxy.rs` | NEW | ~250 |
| `crates/sa_universe/src/lib.rs` | MODIFY | +5 |
| `crates/sa_universe/src/sector.rs` | MODIFY | ~10 changed |
| `crates/sa_render/src/sky.rs` | NEW | ~280 |
| `crates/sa_render/src/shaders/sky.wgsl` | NEW | ~80 |
| `crates/sa_render/src/nebula.rs` | NEW | ~250 |
| `crates/sa_render/src/shaders/nebula.wgsl` | NEW | ~70 |
| `crates/sa_render/src/renderer.rs` | MODIFY | ~60 changed |
| `crates/sa_render/src/lib.rs` | MODIFY | +8 |
| `crates/spaceaway/src/main.rs` | MODIFY | ~40 changed |

---

## Task 1: Galaxy Density Model

### Step 1.1: Create `crates/sa_universe/src/galaxy.rs` -- density functions

TDD: Write tests first, then implement.

```rust
// crates/sa_universe/src/galaxy.rs

use crate::seed::{MasterSeed, Rng64};

/// Galaxy model constants.
const DISC_HALF_THICKNESS: f64 = 500.0;
const ARM_WIDTH: f64 = 2000.0;
const BULGE_RADIUS: f64 = 5000.0;
const BASE_DENSITY: f64 = 0.1;
const SPIRAL_K: f64 = 0.3;
const NUM_ARMS: usize = 2;

/// Disc component: exponential falloff from the galactic plane (y=0).
fn disc(y: f64) -> f64 {
    (-y.abs() / DISC_HALF_THICKNESS).exp()
}

/// Distance from point (x, z) to the nearest spiral arm centerline.
/// Two logarithmic spirals offset by PI.
fn arm_distance(x: f64, z: f64) -> f64 {
    let r = (x * x + z * z).sqrt().max(1.0);
    let theta = z.atan2(x);

    let mut min_dist = f64::MAX;
    for i in 0..NUM_ARMS {
        let offset = i as f64 * std::f64::consts::PI;
        // Arm centerline angle at radius r: theta_arm = k * ln(r) + offset
        let theta_arm = SPIRAL_K * r.ln() + offset;
        // Angular distance, wrapped to [-PI, PI]
        let mut d_theta = theta - theta_arm;
        d_theta = d_theta.rem_euclid(std::f64::consts::TAU);
        if d_theta > std::f64::consts::PI {
            d_theta -= std::f64::consts::TAU;
        }
        // Convert angular distance to linear distance at this radius
        let linear_dist = d_theta.abs() * r;
        min_dist = min_dist.min(linear_dist);
    }
    min_dist
}

/// Spiral arm boost: gaussian falloff from arm centerline.
fn arm_boost(x: f64, z: f64) -> f64 {
    let dist = arm_distance(x, z);
    (-dist * dist / (ARM_WIDTH * ARM_WIDTH)).exp()
}

/// Bulge: spherical core, exponential falloff from galactic center.
fn bulge(x: f64, y: f64, z: f64) -> f64 {
    let r = (x * x + y * y + z * z).sqrt();
    (-r / BULGE_RADIUS).exp()
}

/// Master galaxy density function.
/// Given (x, y, z) in light-years from galactic center, returns a density
/// multiplier in roughly [0, 2+]. Higher = more stars.
pub fn galaxy_density(x: f64, y: f64, z: f64) -> f64 {
    disc(y) * (arm_boost(x, z) + bulge(x, y, z) + BASE_DENSITY)
}

/// A nebula region in the galaxy.
#[derive(Debug, Clone)]
pub struct Nebula {
    /// Position in light-years from galactic center.
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Radius in light-years (50-500).
    pub radius: f64,
    /// Base color RGB [0..1].
    pub color: [f32; 3],
    /// Opacity [0.1..0.4].
    pub opacity: f32,
    /// Seed for procedural noise pattern.
    pub seed: u64,
}

/// Generate nebula positions near spiral arms.
/// Returns ~80 nebulae seeded deterministically from the master seed.
pub fn generate_nebulae(master: MasterSeed) -> Vec<Nebula> {
    let mut rng = Rng64::new(master.0.wrapping_mul(0xBEEF_CAFE_1234_5678));
    let count = 80;
    let mut nebulae = Vec::with_capacity(count);

    let nebula_colors: [[f32; 3]; 5] = [
        [0.9, 0.2, 0.3], // red
        [0.3, 0.4, 0.9], // blue
        [0.6, 0.2, 0.8], // purple
        [0.2, 0.8, 0.4], // green
        [0.8, 0.5, 0.2], // orange
    ];

    for i in 0..count {
        // Place near spiral arms: pick a random radius and arm angle
        let r = rng.range_f64(2000.0, 30000.0);
        let arm_idx = (rng.next_u64() % NUM_ARMS as u64) as usize;
        let arm_offset = arm_idx as f64 * std::f64::consts::PI;
        let theta_arm = SPIRAL_K * r.ln() + arm_offset;
        // Scatter around the arm centerline
        let theta_scatter = rng.range_f64(-0.3, 0.3);
        let theta = theta_arm + theta_scatter;

        let x = r * theta.cos();
        let z = r * theta.sin();
        let y = rng.range_f64(-200.0, 200.0); // near disc plane

        let radius = rng.range_f64(50.0, 500.0);
        let color_idx = (rng.next_u64() % nebula_colors.len() as u64) as usize;
        let color = nebula_colors[color_idx];
        let opacity = rng.range_f32(0.1, 0.4);
        let seed = rng.next_u64().wrapping_add(i as u64);

        nebulae.push(Nebula {
            x, y, z, radius, color, opacity, seed,
        });
    }
    nebulae
}

/// A distant galaxy visible as a faint smudge.
#[derive(Debug, Clone)]
pub struct DistantGalaxy {
    /// Direction unit vector (normalized position at extreme distance).
    pub direction: [f32; 3],
    /// Angular size in radians (small, 0.001-0.01).
    pub angular_size: f32,
    /// Brightness [0..1].
    pub brightness: f32,
    /// Ellipticity (0 = circular, 1 = very elongated). Range [0, 0.7].
    pub ellipticity: f32,
    /// Rotation angle in radians.
    pub rotation: f32,
    /// Tint color.
    pub color: [f32; 3],
}

/// Generate 20-30 distant galaxies at 1M+ ly, deterministically seeded.
pub fn generate_distant_galaxies(master: MasterSeed) -> Vec<DistantGalaxy> {
    let mut rng = Rng64::new(master.0.wrapping_mul(0xDEAD_FACE_9876_5432));
    let count = 20 + (rng.next_u64() % 11) as usize; // 20-30
    let mut galaxies = Vec::with_capacity(count);

    for _ in 0..count {
        // Random direction on unit sphere
        let theta = rng.range_f64(0.0, std::f64::consts::TAU);
        let cos_phi = rng.range_f64(-1.0, 1.0);
        let sin_phi = (1.0 - cos_phi * cos_phi).sqrt();

        let dx = (sin_phi * theta.cos()) as f32;
        let dy = (sin_phi * theta.sin()) as f32;
        let dz = cos_phi as f32;

        let angular_size = rng.range_f32(0.001, 0.008);
        let brightness = rng.range_f32(0.05, 0.25);
        let ellipticity = rng.range_f32(0.0, 0.7);
        let rotation = rng.range_f32(0.0, std::f32::consts::PI);

        // Warm white to slightly blue tint
        let tint = rng.next_f32();
        let color = if tint < 0.5 {
            [0.9, 0.85, 0.7]   // warm
        } else {
            [0.75, 0.8, 0.95]  // cool
        };

        galaxies.push(DistantGalaxy {
            direction: [dx, dy, dz],
            angular_size,
            brightness,
            ellipticity,
            rotation,
            color,
        });
    }
    galaxies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_higher_in_disc_plane() {
        let in_plane = galaxy_density(5000.0, 0.0, 5000.0);
        let above_plane = galaxy_density(5000.0, 3000.0, 5000.0);
        assert!(
            in_plane > above_plane,
            "Disc plane density ({in_plane}) should exceed above-plane ({above_plane})"
        );
    }

    #[test]
    fn density_higher_near_arm() {
        // Pick a point on the first arm centerline at r=10000
        let r = 10000.0_f64;
        let theta_arm = SPIRAL_K * r.ln();
        let on_arm_x = r * theta_arm.cos();
        let on_arm_z = r * theta_arm.sin();
        let on_arm = galaxy_density(on_arm_x, 0.0, on_arm_z);

        // Pick a point well between arms (offset by PI/2 from arm)
        let theta_off = theta_arm + std::f64::consts::FRAC_PI_2;
        let off_arm_x = r * theta_off.cos();
        let off_arm_z = r * theta_off.sin();
        let off_arm = galaxy_density(off_arm_x, 0.0, off_arm_z);

        assert!(
            on_arm > off_arm,
            "On-arm density ({on_arm}) should exceed off-arm ({off_arm})"
        );
    }

    #[test]
    fn bulge_boosts_center() {
        let center = galaxy_density(0.0, 0.0, 0.0);
        let far = galaxy_density(30000.0, 0.0, 0.0);
        assert!(
            center > far,
            "Center density ({center}) should exceed far ({far})"
        );
    }

    #[test]
    fn density_deterministic() {
        let a = galaxy_density(1234.0, 567.0, -890.0);
        let b = galaxy_density(1234.0, 567.0, -890.0);
        assert_eq!(a.to_bits(), b.to_bits());
    }

    #[test]
    fn density_never_negative() {
        let coords = [
            (0.0, 0.0, 0.0),
            (50000.0, 10000.0, -50000.0),
            (-30000.0, -5000.0, 20000.0),
            (0.0, 100000.0, 0.0),
        ];
        for (x, y, z) in coords {
            let d = galaxy_density(x, y, z);
            assert!(d >= 0.0, "Density should be non-negative at ({x},{y},{z}): {d}");
        }
    }

    #[test]
    fn density_has_base_floor() {
        // Very far from everything -- should still be > 0 due to base density
        let d = galaxy_density(100000.0, 50000.0, 100000.0);
        assert!(d > 0.0, "Should have non-zero density even in deep void: {d}");
    }

    #[test]
    fn nebulae_deterministic() {
        let a = generate_nebulae(MasterSeed(42));
        let b = generate_nebulae(MasterSeed(42));
        assert_eq!(a.len(), b.len());
        for (na, nb) in a.iter().zip(b.iter()) {
            assert_eq!(na.x.to_bits(), nb.x.to_bits());
            assert_eq!(na.seed, nb.seed);
        }
    }

    #[test]
    fn nebulae_count() {
        let nebulae = generate_nebulae(MasterSeed(42));
        assert_eq!(nebulae.len(), 80);
    }

    #[test]
    fn distant_galaxies_deterministic() {
        let a = generate_distant_galaxies(MasterSeed(42));
        let b = generate_distant_galaxies(MasterSeed(42));
        assert_eq!(a.len(), b.len());
        for (ga, gb) in a.iter().zip(b.iter()) {
            assert_eq!(ga.direction[0].to_bits(), gb.direction[0].to_bits());
        }
    }

    #[test]
    fn distant_galaxies_count_range() {
        let galaxies = generate_distant_galaxies(MasterSeed(42));
        assert!(galaxies.len() >= 20 && galaxies.len() <= 30,
            "Expected 20-30 galaxies, got {}", galaxies.len());
    }

    #[test]
    fn distant_galaxies_directions_normalized() {
        let galaxies = generate_distant_galaxies(MasterSeed(42));
        for g in &galaxies {
            let len = (g.direction[0] * g.direction[0]
                + g.direction[1] * g.direction[1]
                + g.direction[2] * g.direction[2])
                .sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "Direction not normalized: len={len}"
            );
        }
    }
}
```

### Step 1.2: Register `galaxy` module in `crates/sa_universe/src/lib.rs`

```rust
// ADD these lines to crates/sa_universe/src/lib.rs:

pub mod galaxy;

pub use galaxy::{
    galaxy_density, generate_distant_galaxies, generate_nebulae,
    DistantGalaxy, Nebula,
};
```

The full file becomes:

```rust
pub mod galaxy;
pub mod object_id;
pub mod query;
pub mod sector;
pub mod seed;
pub mod star;
pub mod system;

pub use galaxy::{galaxy_density, generate_distant_galaxies, generate_nebulae, DistantGalaxy, Nebula};
pub use object_id::ObjectId;
pub use query::{Universe, VisibleStar};
pub use sector::{Sector, SectorCoord, SECTOR_SIZE_LY};
pub use seed::{MasterSeed, Rng64, sector_hash};
pub use star::{SpectralClass, Star, generate_star};
pub use system::{Planet, PlanetType, PlanetarySystem, generate_system};
```

### Step 1.3: Modify `crates/sa_universe/src/sector.rs` to use `galaxy_density()`

Replace the `sector_density` function body. The old function used a simple exponential decay from origin. The new one calls `galaxy_density()` with world-space coordinates (in light-years).

**Old code** (lines 60-74):

```rust
fn sector_density(coord: SectorCoord) -> u32 {
    let dx = coord.x as f64;
    let dy = coord.y as f64;
    let dz = coord.z as f64;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    // Base density near center ~80 stars per sector, decaying with scale radius ~200 sectors.
    // Real stellar density is much higher — this is already sparse for gameplay.
    let base = 80.0;
    let scale_radius = 200.0;
    let density = base * (-dist / scale_radius).exp();

    // Minimum 1 star per sector to avoid empty voids everywhere
    (density as u32).max(1)
}
```

**New code:**

```rust
fn sector_density(coord: SectorCoord) -> u32 {
    // Convert sector coordinates to galactic light-year coordinates.
    // Each sector is SECTOR_SIZE_LY on a side; use the sector center.
    let x = (coord.x as f64 + 0.5) * SECTOR_SIZE_LY;
    let y = (coord.y as f64 + 0.5) * SECTOR_SIZE_LY;
    let z = (coord.z as f64 + 0.5) * SECTOR_SIZE_LY;

    let gd = crate::galaxy::galaxy_density(x, y, z);

    // Scale density to star count: peak ~80 stars/sector in densest regions.
    // galaxy_density returns roughly [0, 2+], so multiply by 40 to get ~80 at peak.
    let star_count = (gd * 40.0) as u32;

    // Minimum 1 star per sector to avoid empty voids
    star_count.max(1)
}
```

### Step 1.4: Run tests

```bash
cd /Users/dante/Projects/SpaceAway && cargo test -p sa_universe
```

Verify:
- All new galaxy tests pass (8 tests).
- Existing sector tests still pass. The `sector_density_center_higher_than_edge` test should still pass because center density (bulge) is higher than far edge.
- `generate_sector_deterministic` still passes.

---

## Task 2: Galactic Core Glow Shader

### Step 2.1: Create `crates/sa_render/src/shaders/sky.wgsl`

```wgsl
// crates/sa_render/src/shaders/sky.wgsl

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
    // Two triangles covering the screen:
    // Triangle 0: vertices 0,1,2
    // Triangle 1: vertices 2,1,3
    // Positions: (-1,-1), (1,-1), (-1,1), (1,1)
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
```

### Step 2.2: Create `crates/sa_render/src/sky.rs`

```rust
// crates/sa_render/src/sky.rs

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Uniforms for the sky shader (core glow + cubemap).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SkyUniforms {
    pub inv_view_proj: [[f32; 4]; 4],
    pub galactic_center_dir: [f32; 3],
    pub core_brightness: f32,
    pub cubemap_enabled: u32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Precomputed Milky Way cubemap data (CPU-generated, uploaded to GPU).
pub struct MilkyWayCubemap {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

/// Resolution of each cubemap face.
const CUBEMAP_SIZE: u32 = 256;

/// Face directions for cubemap generation.
/// Order: +X, -X, +Y, -Y, +Z, -Z
const FACE_DIRS: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),  // +X: right=down(-Z), up=+Y
    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),   // -X
    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),   // +Y
    ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),   // -Y
    ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),    // +Z
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]),  // -Z
];

/// Generate the cubemap pixel data on CPU.
/// `density_fn(x, y, z) -> f64` is the galaxy density function.
/// `observer` is the observer position in light-years.
/// Returns 6 faces of RGBA u8 data, each CUBEMAP_SIZE x CUBEMAP_SIZE.
pub fn generate_cubemap_data(
    density_fn: &dyn Fn(f64, f64, f64) -> f64,
    observer: [f64; 3],
) -> Vec<Vec<u8>> {
    let size = CUBEMAP_SIZE as usize;
    let num_samples = 32;
    let max_dist = 50000.0_f64;
    let step = max_dist / num_samples as f64;

    let mut faces = Vec::with_capacity(6);

    for face in 0..6 {
        let (forward, up, right) = FACE_DIRS[face];
        let mut pixels = vec![0u8; size * size * 4];

        for py in 0..size {
            for px in 0..size {
                // Map pixel to [-1, 1] on the face
                let u = (px as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                let v = (py as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                // Flip v because texture y goes top-to-bottom
                let v = -v;

                // Ray direction
                let dx = forward[0] + right[0] * u + up[0] * v;
                let dy = forward[1] + right[1] * u + up[1] * v;
                let dz = forward[2] + right[2] * u + up[2] * v;
                let len = (dx * dx + dy * dy + dz * dz).sqrt();
                let (dx, dy, dz) = (dx / len, dy / len, dz / len);

                // Integrate density along ray
                let mut accumulated = 0.0_f64;
                let mut warm_factor = 0.0_f64;

                for s in 0..num_samples {
                    let t = (s as f64 + 0.5) * step;
                    let sx = observer[0] + dx as f64 * t;
                    let sy = observer[1] + dy as f64 * t;
                    let sz = observer[2] + dz as f64 * t;

                    let d = density_fn(sx, sy, sz);
                    accumulated += d * step / max_dist;

                    // Track warmth: closer to center = warmer color
                    let r_center = (sx * sx + sy * sy + sz * sz).sqrt();
                    if r_center < 15000.0 {
                        warm_factor += d * step / max_dist;
                    }
                }

                // Map to brightness
                let brightness = (accumulated * 3.0).min(1.0);
                let warmth = (warm_factor / accumulated.max(0.001)).min(1.0);

                // Color: blend from blue-white (arm) to warm gold (center)
                let cool = [0.7_f32, 0.75, 0.9];
                let warm = [0.95_f32, 0.85, 0.65];
                let w = warmth as f32;
                let r = (cool[0] * (1.0 - w) + warm[0] * w) * brightness as f32;
                let g = (cool[1] * (1.0 - w) + warm[1] * w) * brightness as f32;
                let b = (cool[2] * (1.0 - w) + warm[2] * w) * brightness as f32;

                let idx = (py * size + px) * 4;
                pixels[idx] = (r * 255.0).min(255.0) as u8;
                pixels[idx + 1] = (g * 255.0).min(255.0) as u8;
                pixels[idx + 2] = (b * 255.0).min(255.0) as u8;
                pixels[idx + 3] = 255;
            }
        }
        faces.push(pixels);
    }
    faces
}

impl MilkyWayCubemap {
    /// Create from precomputed face data (6 faces of RGBA u8).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, faces: &[Vec<u8>]) -> Self {
        let size = CUBEMAP_SIZE;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Milky Way Cubemap"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (i, face_data) in faces.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                face_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        Self { texture, view }
    }
}

/// Renderer for the sky layers (Milky Way cubemap + galactic core glow).
pub struct SkyRenderer {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub sampler: wgpu::Sampler,
}

impl SkyRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        cubemap: &MilkyWayCubemap,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sky.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cubemap Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sky Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky Uniforms"),
            size: std::mem::size_of::<SkyUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cubemap.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Max,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            sampler,
        }
    }

    /// Rebuild the bind group after a cubemap regeneration.
    pub fn rebuild_bind_group(
        &mut self,
        device: &wgpu::Device,
        cubemap: &MilkyWayCubemap,
    ) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cubemap.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
    }
}
```

### Step 2.3: Run compilation check

```bash
cd /Users/dante/Projects/SpaceAway && cargo check -p sa_render
```

---

## Task 3: Milky Way Cubemap

Already covered in Step 2.2 above. The cubemap generation (`generate_cubemap_data`) and upload (`MilkyWayCubemap::new`) are in `sky.rs`. The shader samples the cubemap in `sky.wgsl`. The game binary is responsible for calling `generate_cubemap_data` with the density function and uploading.

No additional files needed for this task.

---

## Task 4: Nebula Sprites

### Step 4.1: Create `crates/sa_render/src/shaders/nebula.wgsl`

```wgsl
// crates/sa_render/src/shaders/nebula.wgsl

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
    return vec4<f32>(in.neb_color * alpha, alpha);
}
```

### Step 4.2: Create `crates/sa_render/src/nebula.rs`

```rust
// crates/sa_render/src/nebula.rs

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// GPU instance data for a single nebula sprite.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct NebulaInstance {
    /// Camera-relative center position.
    pub center: [f32; 3],
    /// Radius in world units (light-years, rendered as-is).
    pub radius: f32,
    /// RGB color.
    pub color: [f32; 3],
    /// Opacity [0..1].
    pub opacity: f32,
    /// Seed for noise pattern (cast to f32 for shader).
    pub seed: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Uniforms for the nebula billboard shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct NebulaUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub camera_right: [f32; 3],
    pub _pad0: f32,
    pub camera_up: [f32; 3],
    pub _pad1: f32,
}

pub struct NebulaRenderer {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub instance_buffer: wgpu::Buffer,
    pub instance_count: u32,
}

impl NebulaRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Nebula Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/nebula.wgsl").into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Nebula Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Nebula Uniforms"),
            size: std::mem::size_of::<NebulaUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Nebula Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Nebula Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Empty initial instance buffer
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Nebula Instances"),
            size: 64, // minimum size, will be replaced
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Nebula Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<NebulaInstance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 28,
                            shader_location: 3,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            offset: 32,
                            shader_location: 4,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            offset: 36,
                            shader_location: 5,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            offset: 40,
                            shader_location: 6,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            offset: 44,
                            shader_location: 7,
                            format: wgpu::VertexFormat::Float32,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Max,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group,
            instance_buffer,
            instance_count: 0,
        }
    }

    /// Update the instance buffer with new nebula data.
    pub fn update_instances(&mut self, device: &wgpu::Device, instances: &[NebulaInstance]) {
        if instances.is_empty() {
            self.instance_count = 0;
            return;
        }
        self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Nebula Instances"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.instance_count = instances.len() as u32;
    }
}
```

---

## Task 5: Distant Galaxies

### Step 5.1: Distant galaxies share the nebula renderer infrastructure

The distant galaxies use the same billboard technique but with a simpler appearance. Rather than adding a separate renderer, we reuse `NebulaRenderer` for distant galaxies. The galaxy instances are appended after nebula instances in a second draw call with the same pipeline. The fragment shader's fBm noise naturally produces a soft elliptical shape when the seed is chosen to give a smooth result, and the small angular size means the noise detail won't be visible anyway.

However, for cleaner separation, we create distinct instance data. The game binary will prepare `NebulaInstance` entries for each distant galaxy (using their direction vector as position at a large fixed distance, small radius, low opacity, and a specific seed).

**No new files needed.** The game binary converts `DistantGalaxy` data into `NebulaInstance` entries. See Task 7.

---

## Task 6: Renderer Integration

### Step 6.1: Modify `crates/sa_render/src/renderer.rs`

Add `SkyRenderer`, `NebulaRenderer` to `Renderer`. Change render order to: clear, sky (cubemap + core glow), stars, nebulae, distant galaxies, geometry.

**New `renderer.rs`:**

```rust
// crates/sa_render/src/renderer.rs

use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::mesh::{MeshMarker, MeshStore};
use crate::nebula::{NebulaRenderer, NebulaUniforms};
use crate::pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
use crate::sky::{SkyRenderer, SkyUniforms};
use crate::star_field::{StarField, StarUniforms};
use glam::{Mat4, Vec3};
use sa_core::Handle;
use wgpu::util::DeviceExt;

pub struct DrawCommand {
    pub mesh: Handle<MeshMarker>,
    pub model_matrix: Mat4,
}

/// Direction from the player toward the galactic center, plus core brightness.
pub struct GalacticCenterInfo {
    pub direction: Vec3,
    pub brightness: f32,
    pub cubemap_enabled: bool,
}

pub struct Renderer {
    pub geometry_pipeline: GeometryPipeline,
    pub star_field: StarField,
    pub mesh_store: MeshStore,
    pub sky: SkyRenderer,
    pub nebula_renderer: NebulaRenderer,
    pub galaxy_renderer: NebulaRenderer,
}

impl Renderer {
    pub fn new(
        gpu: &GpuContext,
        cubemap: &crate::sky::MilkyWayCubemap,
    ) -> Self {
        let geometry_pipeline = GeometryPipeline::new(
            &gpu.device,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
        );
        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);
        let sky = SkyRenderer::new(&gpu.device, gpu.config.format, cubemap);
        let nebula_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        let galaxy_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);

        Self {
            geometry_pipeline,
            star_field,
            mesh_store: MeshStore::new(),
            sky,
            nebula_renderer,
            galaxy_renderer,
        }
    }

    pub fn resize(&mut self, gpu: &GpuContext) {
        self.geometry_pipeline
            .resize(&gpu.device, gpu.config.width, gpu.config.height);
    }

    pub fn render_frame(
        &self,
        gpu: &GpuContext,
        camera: &Camera,
        draw_commands: &[DrawCommand],
        light_dir: Vec3,
        gc_info: &GalacticCenterInfo,
    ) {
        let aspect = gpu.aspect_ratio();
        let view_proj = camera.view_projection_matrix(aspect);
        let cam_pos = camera.position;

        // --- Geometry uniforms ---
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: light_dir.normalize().to_array(),
            _pad: 0.0,
            light_color: [1.0, 0.95, 0.9],
            _pad2: 0.0,
            ambient: [0.02, 0.02, 0.03],
            _pad3: 0.0,
        };
        gpu.queue.write_buffer(
            &self.geometry_pipeline.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        // --- Star uniforms ---
        let star_view = camera.view_matrix();
        let star_vp = camera.projection_matrix(aspect) * star_view;
        let star_uniforms = StarUniforms {
            view_proj: star_vp.to_cols_array_2d(),
            screen_height: gpu.config.height as f32,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        gpu.queue.write_buffer(
            &self.star_field.uniform_buffer,
            0,
            bytemuck::bytes_of(&star_uniforms),
        );

        // --- Sky uniforms ---
        let proj = camera.projection_matrix(aspect);
        let view = camera.view_matrix();
        let inv_view_proj = (proj * view).inverse();
        let sky_uniforms = SkyUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            galactic_center_dir: gc_info.direction.normalize().to_array(),
            core_brightness: gc_info.brightness,
            cubemap_enabled: if gc_info.cubemap_enabled { 1 } else { 0 },
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        gpu.queue.write_buffer(
            &self.sky.uniform_buffer,
            0,
            bytemuck::bytes_of(&sky_uniforms),
        );

        // --- Nebula uniforms ---
        let view_mat = camera.view_matrix();
        let camera_right = Vec3::new(view_mat.col(0).x, view_mat.col(1).x, view_mat.col(2).x);
        let camera_up = Vec3::new(view_mat.col(0).y, view_mat.col(1).y, view_mat.col(2).y);
        let nebula_uniforms = NebulaUniforms {
            view_proj: star_vp.to_cols_array_2d(),
            camera_right: camera_right.to_array(),
            _pad0: 0.0,
            camera_up: camera_up.to_array(),
            _pad1: 0.0,
        };
        gpu.queue.write_buffer(
            &self.nebula_renderer.uniform_buffer,
            0,
            bytemuck::bytes_of(&nebula_uniforms),
        );
        gpu.queue.write_buffer(
            &self.galaxy_renderer.uniform_buffer,
            0,
            bytemuck::bytes_of(&nebula_uniforms),
        );

        // --- Acquire frame ---
        let frame = match gpu.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            gpu.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Frame Encoder"),
                });

        // === Pass 1: Sky (cubemap + core glow) — rendered after clear ===
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sky Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.005,
                            g: 0.005,
                            b: 0.015,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.sky.pipeline);
            pass.set_bind_group(0, &self.sky.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        // === Pass 2: Stars + Nebulae + Galaxies + Geometry ===
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.geometry_pipeline.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // Stars
            pass.set_pipeline(&self.star_field.pipeline);
            pass.set_bind_group(0, &self.star_field.bind_group, &[]);
            pass.set_vertex_buffer(0, self.star_field.vertex_buffer.slice(..));
            pass.draw(0..6, 0..self.star_field.star_count);

            // Nebulae
            if self.nebula_renderer.instance_count > 0 {
                pass.set_pipeline(&self.nebula_renderer.pipeline);
                pass.set_bind_group(0, &self.nebula_renderer.bind_group, &[]);
                pass.set_vertex_buffer(0, self.nebula_renderer.instance_buffer.slice(..));
                pass.draw(0..6, 0..self.nebula_renderer.instance_count);
            }

            // Distant galaxies
            if self.galaxy_renderer.instance_count > 0 {
                pass.set_pipeline(&self.galaxy_renderer.pipeline);
                pass.set_bind_group(0, &self.galaxy_renderer.bind_group, &[]);
                pass.set_vertex_buffer(0, self.galaxy_renderer.instance_buffer.slice(..));
                pass.draw(0..6, 0..self.galaxy_renderer.instance_count);
            }

            // Geometry
            if !draw_commands.is_empty() {
                pass.set_pipeline(&self.geometry_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                for cmd in draw_commands {
                    if let Some(mesh) = self.mesh_store.get(cmd.mesh) {
                        let col3 = cmd.model_matrix.col(3);
                        let rebased_translation = Vec3::new(
                            (col3.x as f64 - cam_pos.x) as f32,
                            (col3.y as f64 - cam_pos.y) as f32,
                            (col3.z as f64 - cam_pos.z) as f32,
                        );
                        let mut rebased_model = cmd.model_matrix;
                        rebased_model.col_mut(3).x = rebased_translation.x;
                        rebased_model.col_mut(3).y = rebased_translation.y;
                        rebased_model.col_mut(3).z = rebased_translation.z;

                        let instance = InstanceRaw {
                            model: rebased_model.to_cols_array_2d(),
                        };
                        let instance_buffer =
                            gpu.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Instance Buffer"),
                                    contents: bytemuck::bytes_of(&instance),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, instance_buffer.slice(..));
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                    }
                }
            }
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
```

### Step 6.2: Update `crates/sa_render/src/lib.rs`

```rust
pub mod camera;
pub mod gpu;
pub mod mesh;
pub mod nebula;
pub mod pipeline;
pub mod renderer;
pub mod sky;
pub mod star_field;
pub mod vertex;

pub use camera::Camera;
pub use gpu::GpuContext;
pub use mesh::{MeshData, MeshMarker, MeshStore};
pub use nebula::{NebulaInstance, NebulaRenderer, NebulaUniforms};
pub use pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
pub use renderer::{DrawCommand, GalacticCenterInfo, Renderer};
pub use sky::{generate_cubemap_data, MilkyWayCubemap, SkyRenderer, SkyUniforms};
pub use star_field::{generate_stars, StarField, StarVertex};
pub use vertex::Vertex;
```

---

## Task 7: Game Binary Integration

### Step 7.1: Modify `crates/spaceaway/src/main.rs`

Key changes:
1. Import new types from `sa_universe` (`galaxy_density`, `generate_nebulae`, `generate_distant_galaxies`, `Nebula`, `DistantGalaxy`).
2. Import new types from `sa_render` (`GalacticCenterInfo`, `MilkyWayCubemap`, `generate_cubemap_data`, `NebulaInstance`).
3. On startup, generate the cubemap data using `galaxy_density`, create `MilkyWayCubemap`, pass to `Renderer::new`.
4. Generate nebulae and distant galaxies from the master seed, convert to `NebulaInstance` data.
5. Track cubemap regeneration (5000 ly threshold).
6. Pass `GalacticCenterInfo` to `render_frame`.

**Changes to `App` struct** -- add fields:

```rust
// New fields in App struct:
cubemap: Option<MilkyWayCubemap>,
last_cubemap_gen_pos: WorldPos,
nebulae: Vec<sa_universe::Nebula>,
distant_galaxies: Vec<sa_universe::DistantGalaxy>,
```

**Changes to `App::new()`** -- initialize new fields:

```rust
cubemap: None,
last_cubemap_gen_pos: WorldPos::ORIGIN,
nebulae: Vec::new(),
distant_galaxies: Vec::new(),
```

**Changes to `resumed()`** -- generate cubemap before creating renderer:

```rust
// After creating gpu, before creating renderer:
let cubemap_data = sa_render::generate_cubemap_data(
    &sa_universe::galaxy_density,
    [0.0, 0.0, 0.0],
);
let cubemap = MilkyWayCubemap::new(&gpu.device, &gpu.queue, &cubemap_data);
let renderer = Renderer::new(&gpu, &cubemap);

// Generate nebulae and distant galaxies
let seed = MasterSeed(42);
self.nebulae = sa_universe::generate_nebulae(seed);
self.distant_galaxies = sa_universe::generate_distant_galaxies(seed);

// Upload initial nebula instances
self.update_nebula_instances(&gpu, &renderer);
self.update_galaxy_instances(&gpu, &renderer);

self.cubemap = Some(cubemap);
self.last_cubemap_gen_pos = WorldPos::ORIGIN;
```

**New helper: convert nebulae to GPU instances:**

```rust
fn nebulae_to_instances(nebulae: &[sa_universe::Nebula], observer: WorldPos) -> Vec<NebulaInstance> {
    nebulae
        .iter()
        .filter_map(|n| {
            let dx = (n.x - observer.x) as f32;
            let dy = (n.y - observer.y) as f32;
            let dz = (n.z - observer.z) as f32;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            // Cull nebulae that are too far (>20000 ly) or behind us
            if dist > 20000.0 {
                return None;
            }
            Some(NebulaInstance {
                center: [dx, dy, dz],
                radius: n.radius as f32,
                color: n.color,
                opacity: n.opacity,
                seed: (n.seed % 10000) as f32,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
            })
        })
        .collect()
}

fn distant_galaxies_to_instances(
    galaxies: &[sa_universe::DistantGalaxy],
) -> Vec<NebulaInstance> {
    galaxies
        .iter()
        .map(|g| {
            // Place at a large distance in the direction vector so the billboard
            // is rendered on the sky dome, similar to how stars work.
            let dist = 80000.0_f32;
            NebulaInstance {
                center: [
                    g.direction[0] * dist,
                    g.direction[1] * dist,
                    g.direction[2] * dist,
                ],
                radius: g.angular_size * dist,
                color: [
                    g.color[0] * g.brightness,
                    g.color[1] * g.brightness,
                    g.color[2] * g.brightness,
                ],
                opacity: g.brightness,
                seed: (g.rotation * 1000.0) % 10000.0,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
            }
        })
        .collect()
}
```

**New helper: cubemap regeneration check:**

```rust
const CUBEMAP_REGEN_THRESHOLD: f64 = 5000.0;

fn maybe_regenerate_cubemap(&mut self) {
    let observer = self.camera.position;
    let dist = observer.distance_to(self.last_cubemap_gen_pos);
    if dist < CUBEMAP_REGEN_THRESHOLD {
        return;
    }
    let (Some(gpu), Some(renderer), Some(_)) =
        (&self.gpu, &mut self.renderer, &self.cubemap) else { return };

    let cubemap_data = sa_render::generate_cubemap_data(
        &sa_universe::galaxy_density,
        [observer.x, observer.y, observer.z],
    );
    let new_cubemap = MilkyWayCubemap::new(&gpu.device, &gpu.queue, &cubemap_data);
    renderer.sky.rebuild_bind_group(&gpu.device, &new_cubemap);
    self.cubemap = Some(new_cubemap);
    self.last_cubemap_gen_pos = observer;

    log::debug!(
        "Regenerated Milky Way cubemap at ({:.0}, {:.0}, {:.0})",
        observer.x, observer.y, observer.z,
    );
}
```

**Changes to `RedrawRequested`** -- add cubemap regen and pass galactic center info:

```rust
// After maybe_regenerate_stars(), add:
self.maybe_regenerate_cubemap();

// When calling render_frame, compute galactic center direction:
let gc_dir = {
    let cx = (0.0 - self.camera.position.x) as f32;
    let cy = (0.0 - self.camera.position.y) as f32;
    let cz = (0.0 - self.camera.position.z) as f32;
    Vec3::new(cx, cy, cz).normalize_or_zero()
};
let gc_info = GalacticCenterInfo {
    direction: if gc_dir.length() > 0.0 { gc_dir } else { Vec3::X },
    brightness: 0.5,
    cubemap_enabled: true,
};

renderer.render_frame(
    gpu,
    &self.camera,
    &commands,
    Vec3::new(0.5, -0.8, -0.3),
    &gc_info,
);
```

**Changes to `Renderer::new` call site** -- pass cubemap reference.

The full updated `main.rs` follows the patterns above. The key structural changes are:

1. `App` struct gains `cubemap`, `last_cubemap_gen_pos`, `nebulae`, `distant_galaxies` fields.
2. `resumed()` generates cubemap data before `Renderer::new`, generates nebula/galaxy lists, uploads instances.
3. `RedrawRequested` calls `maybe_regenerate_cubemap()` and passes `GalacticCenterInfo` to `render_frame`.
4. Two free functions `nebulae_to_instances` and `distant_galaxies_to_instances` handle the sa_universe-to-sa_render data bridging.

### Step 7.2: Update nebula instances when stars are regenerated

In `maybe_regenerate_stars()`, after updating the star buffer, also update nebula instances:

```rust
// At the end of maybe_regenerate_stars(), after renderer.star_field.update_star_buffer:
let nebula_instances = nebulae_to_instances(&self.nebulae, observer);
renderer.nebula_renderer.update_instances(&gpu.device, &nebula_instances);
```

Distant galaxy instances do not need updating since they are direction-based (observer-independent).

---

## Task 8: Final Verification

### Step 8.1: Full workspace build

```bash
cd /Users/dante/Projects/SpaceAway && cargo build --workspace
```

### Step 8.2: Clippy

```bash
cd /Users/dante/Projects/SpaceAway && cargo clippy --workspace -- -D warnings
```

### Step 8.3: Tests

```bash
cd /Users/dante/Projects/SpaceAway && cargo test --workspace
```

### Step 8.4: Visual verification checklist

- [ ] Stars cluster along spiral arm directions (denser in-plane, sparser above/below)
- [ ] Warm gold glow visible in the direction of galactic center (origin)
- [ ] Milky Way band visible as a diffuse glow across the sky when looking through the disc plane
- [ ] Colorful nebula sprites visible near the galactic plane
- [ ] Faint distant galaxy smudges visible scattered across the sky
- [ ] No performance regression (check FPS in title bar remains similar)
- [ ] Moving large distances (teleport to x=10000) shows different sky composition
- [ ] Cubemap regeneration fires at >5000 ly movement (check debug log)

---

## Execution Order

| Step | Task | Dependencies | Estimated Time |
|------|------|-------------|---------------|
| 1.1 | `galaxy.rs` density + nebula + galaxy generation | None | 20 min |
| 1.2 | Register in `sa_universe/lib.rs` | 1.1 | 2 min |
| 1.3 | Update `sector.rs` to use `galaxy_density` | 1.1 | 5 min |
| 1.4 | Run `sa_universe` tests | 1.1-1.3 | 3 min |
| 2.1 | `shaders/sky.wgsl` | None | 10 min |
| 2.2 | `sky.rs` (SkyRenderer + MilkyWayCubemap) | 2.1 | 20 min |
| 2.3 | Compile check `sa_render` | 2.1-2.2 | 2 min |
| 4.1 | `shaders/nebula.wgsl` | None | 10 min |
| 4.2 | `nebula.rs` (NebulaRenderer) | 4.1 | 15 min |
| 6.1 | Update `renderer.rs` | 2.2, 4.2 | 15 min |
| 6.2 | Update `sa_render/lib.rs` | 6.1 | 2 min |
| 7.1 | Update `main.rs` | 1.1-1.3, 6.1-6.2 | 20 min |
| 7.2 | Nebula instance updates in star regen | 7.1 | 5 min |
| 8.1 | Full build | All | 3 min |
| 8.2 | Clippy | 8.1 | 2 min |
| 8.3 | Tests | 8.1 | 3 min |
| 8.4 | Visual check | 8.3 | 10 min |

**Total estimated: ~2.5 hours**

---

## Risk Notes

1. **Cubemap generation time.** At 256x256x6 faces with 32 samples each, that is ~12.6M density evaluations. Each is a few trig ops. Should be well under 50ms but profile on first run.

2. **Nebula billboard depth.** Nebulae are drawn with no depth testing. If a nebula center is behind geometry, the billboard still renders in front. This is acceptable for Phase 4.5 since nebulae are atmospheric sky features, not solid objects. Depth-aware rendering can be added later.

3. **Star density change.** Replacing the old simple exponential with the galaxy model changes which sectors have more/fewer stars. Existing test `sector_density_center_higher_than_edge` should still pass because the galactic center has bulge + disc + arm density. But verify.

4. **sa_render / sa_universe boundary.** The density function is passed as `&dyn Fn(f64,f64,f64)->f64` from the game binary to `generate_cubemap_data`. This avoids adding a crate dependency from sa_render to sa_universe. The game binary already depends on both.
