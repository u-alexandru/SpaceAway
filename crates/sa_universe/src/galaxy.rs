// crates/sa_universe/src/galaxy.rs

use crate::seed::{MasterSeed, Rng64};

/// Galaxy model constants based on measured Milky Way parameters.
/// Sources: Wikipedia Milky Way, ESA Gaia data, Vallée 2017 spiral arm survey.
const DISC_HALF_THICKNESS: f64 = 500.0; // thin disc scale height (measured: 300-400 pc ≈ 1000-1300 ly, but visually thinner works better)
const ARM_WIDTH: f64 = 2500.0; // spiral arm width in ly
const BULGE_RADIUS: f64 = 5000.0; // central bulge radius
const BASE_DENSITY: f64 = 0.08; // inter-arm baseline
const SPIRAL_K: f64 = 0.25; // tighter spiral (~13° pitch angle)
const NUM_ARMS: usize = 4; // 4 major arms (Milky Way has 4)
/// Dust absorption coefficient along spiral arm inner edges.
const DUST_STRENGTH: f64 = 0.4;

/// Disc component: exponential falloff from the galactic plane (y=0).
fn disc(y: f64) -> f64 {
    (-y.abs() / DISC_HALF_THICKNESS).exp()
}

/// Distance from point (x, z) to the nearest spiral arm centerline,
/// and also returns signed angular offset (negative = inside/toward center).
/// Four logarithmic spirals offset by PI/2 each.
fn arm_info(x: f64, z: f64) -> (f64, f64) {
    let r = (x * x + z * z).sqrt().max(1.0);
    let theta = z.atan2(x);

    let mut min_dist = f64::MAX;
    let mut min_signed = 0.0_f64;
    let arm_spacing = std::f64::consts::TAU / NUM_ARMS as f64;
    for i in 0..NUM_ARMS {
        let offset = i as f64 * arm_spacing;
        let theta_arm = SPIRAL_K * r.ln() + offset;
        let mut d_theta = theta - theta_arm;
        d_theta = d_theta.rem_euclid(std::f64::consts::TAU);
        if d_theta > std::f64::consts::PI {
            d_theta -= std::f64::consts::TAU;
        }
        let linear_dist = d_theta.abs() * r;
        if linear_dist < min_dist {
            min_dist = linear_dist;
            min_signed = d_theta * r; // negative = trailing edge (inside)
        }
    }
    (min_dist, min_signed)
}

/// Distance from point (x, z) to the nearest spiral arm centerline.
fn arm_distance(x: f64, z: f64) -> f64 {
    arm_info(x, z).0
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

/// Dust density at a point. Dust concentrates on the inner (trailing)
/// edges of spiral arms, in the disc plane. Used for Beer-Lambert
/// absorption when ray-marching the Milky Way cubemap.
pub fn dust_density(x: f64, y: f64, z: f64) -> f64 {
    let (_dist, signed) = arm_info(x, z);
    let dist = _dist;
    // Dust is on the inner edge of arms (signed < 0) and within ~1500 ly
    let edge_factor = if signed < 0.0 {
        (-dist * dist / (1500.0 * 1500.0)).exp() * DUST_STRENGTH
    } else {
        (-dist * dist / (1000.0 * 1000.0)).exp() * DUST_STRENGTH * 0.3
    };
    // Only in the disc plane
    let disc_factor = (-y.abs() / (DISC_HALF_THICKNESS * 0.6)).exp();
    edge_factor * disc_factor
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

        // Pick a point well between arms (offset by PI/4 from arm — midpoint between 4 arms)
        let theta_off = theta_arm + std::f64::consts::FRAC_PI_4;
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
