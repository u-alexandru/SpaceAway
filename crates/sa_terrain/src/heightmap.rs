//! Noise sampling: fastnoise-lite fBm + domain warp → terrain height.

use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};

/// Create the terrain noise generator for a planet.
pub fn make_terrain_noise(seed: u64) -> FastNoiseLite {
    let mut noise = FastNoiseLite::with_seed(seed as i32);
    noise.set_noise_type(Some(NoiseType::OpenSimplex2));
    noise.set_fractal_type(Some(FractalType::FBm));
    noise.set_fractal_octaves(Some(5));
    noise.set_fractal_lacunarity(Some(2.0));
    noise.set_fractal_gain(Some(0.5));
    noise.set_frequency(Some(1.0));
    noise
}

/// Create the domain warp noise generator.
pub fn make_warp_noise(seed: u64) -> FastNoiseLite {
    let mut warp = FastNoiseLite::with_seed(seed.wrapping_add(1337) as i32);
    warp.set_noise_type(Some(NoiseType::OpenSimplex2));
    warp.set_frequency(Some(0.5));
    warp
}

/// Sample terrain height at a sphere-surface point.
///
/// `dir`: unit direction vector on sphere (from cube_to_sphere).
/// `freq_scale`: frequency multiplier (controls feature size relative to planet).
///
/// Returns height in [0, 1] range.
pub fn sample_height(
    noise: &FastNoiseLite,
    warp: &FastNoiseLite,
    dir: [f64; 3],
    freq_scale: f64,
) -> f32 {
    let x = dir[0] * freq_scale;
    let y = dir[1] * freq_scale;
    let z = dir[2] * freq_scale;

    // Domain warping: offset sample position by warp noise
    let warp_strength = 0.3;
    let wx = warp.get_noise_3d(x, y, z) as f64 * warp_strength;
    let wy = warp.get_noise_3d(x + 100.0, y + 100.0, z + 100.0) as f64 * warp_strength;
    let wz = warp.get_noise_3d(x + 200.0, y + 200.0, z + 200.0) as f64 * warp_strength;

    // Sample fBm at warped position
    let raw = noise.get_noise_3d(x + wx, y + wy, z + wz);

    // Map from [-1, 1] to [0, 1]
    (raw * 0.5 + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_in_range() {
        let noise = make_terrain_noise(42);
        let warp = make_warp_noise(42);
        for i in 0..100 {
            let angle = i as f64 * 0.1;
            let dir = [angle.cos(), angle.sin(), 0.3];
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            let dir = [dir[0] / len, dir[1] / len, dir[2] / len];
            let h = sample_height(&noise, &warp, dir, 2.0);
            assert!((0.0..=1.0).contains(&h), "height out of range: {h}");
        }
    }

    #[test]
    fn deterministic_same_seed() {
        let n1 = make_terrain_noise(42);
        let w1 = make_warp_noise(42);
        let n2 = make_terrain_noise(42);
        let w2 = make_warp_noise(42);
        let dir = [0.577, 0.577, 0.577];
        let h1 = sample_height(&n1, &w1, dir, 2.0);
        let h2 = sample_height(&n2, &w2, dir, 2.0);
        assert!((h1 - h2).abs() < 1e-6, "same seed should produce same height");
    }

    #[test]
    fn different_seeds_differ() {
        let n1 = make_terrain_noise(42);
        let w1 = make_warp_noise(42);
        let n2 = make_terrain_noise(999);
        let w2 = make_warp_noise(999);
        let dir = [0.0, 0.0, 1.0];
        let h1 = sample_height(&n1, &w1, dir, 2.0);
        let h2 = sample_height(&n2, &w2, dir, 2.0);
        assert!((h1 - h2).abs() > 0.001, "different seeds should produce different heights");
    }
}
