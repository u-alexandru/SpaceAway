use crate::object_id::ObjectId;
use crate::seed::{MasterSeed, Rng64, sector_hash};
use crate::star::{Star, generate_star};
use sa_math::WorldPos;
use serde::{Deserialize, Serialize};

/// Sector size in light-years per side.
pub const SECTOR_SIZE_LY: f64 = 10.0;

/// Integer sector coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectorCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectorCoord {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Convert a WorldPos (in light-years) to the sector it falls in.
    pub fn from_world_pos(pos: WorldPos) -> Self {
        Self {
            x: (pos.x / SECTOR_SIZE_LY).floor() as i32,
            y: (pos.y / SECTOR_SIZE_LY).floor() as i32,
            z: (pos.z / SECTOR_SIZE_LY).floor() as i32,
        }
    }

    /// World-space origin (minimum corner) of this sector.
    pub fn world_origin(self) -> WorldPos {
        WorldPos::new(
            self.x as f64 * SECTOR_SIZE_LY,
            self.y as f64 * SECTOR_SIZE_LY,
            self.z as f64 * SECTOR_SIZE_LY,
        )
    }
}

/// A star placed within a sector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedStar {
    pub id: ObjectId,
    pub position: WorldPos,
    pub star: Star,
}

/// A generated sector containing placed stars.
#[derive(Debug, Clone)]
pub struct Sector {
    pub coord: SectorCoord,
    pub stars: Vec<PlacedStar>,
}

/// Compute star density for a sector based on distance from galactic center.
/// The galaxy layer: exponential falloff from center.
/// Returns approximate number of stars per sector.
fn sector_density(coord: SectorCoord) -> u32 {
    let dx = coord.x as f64;
    let dy = coord.y as f64;
    let dz = coord.z as f64;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    // Base density near center ~20 stars per sector, decaying with scale radius ~200 sectors.
    let base = 20.0;
    let scale_radius = 200.0;
    let density = base * (-dist / scale_radius).exp();

    // Minimum 1 star per sector to avoid empty voids everywhere
    (density as u32).max(1)
}

/// 3D Poisson disk sampling (Bridson's algorithm) within the sector cube.
/// Returns positions in [0, SECTOR_SIZE_LY]^3.
fn poisson_disk_3d(rng: &mut Rng64, count: u32, min_distance: f64) -> Vec<[f64; 3]> {
    let size = SECTOR_SIZE_LY;
    let max_attempts = 30;
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(count as usize);
    let mut active: Vec<usize> = Vec::new();

    // First point: random in the cube
    let p0 = [
        rng.next_f64() * size,
        rng.next_f64() * size,
        rng.next_f64() * size,
    ];
    points.push(p0);
    active.push(0);

    while !active.is_empty() && points.len() < count as usize {
        // Pick a random active point
        let active_idx = (rng.next_u64() % active.len() as u64) as usize;
        let parent = points[active[active_idx]];
        let mut found = false;

        for _ in 0..max_attempts {
            // Random point in spherical shell [min_distance, 2*min_distance]
            let r = min_distance * (1.0 + rng.next_f64());
            let theta = rng.next_f64() * std::f64::consts::TAU;
            let phi = (rng.next_f64() * 2.0 - 1.0).acos();

            let dx = r * phi.sin() * theta.cos();
            let dy = r * phi.sin() * theta.sin();
            let dz = r * phi.cos();

            let candidate = [parent[0] + dx, parent[1] + dy, parent[2] + dz];

            // Check bounds
            if candidate[0] < 0.0 || candidate[0] > size
                || candidate[1] < 0.0 || candidate[1] > size
                || candidate[2] < 0.0 || candidate[2] > size
            {
                continue;
            }

            // Check distance to all existing points (brute force, fine for <256 stars)
            let too_close = points.iter().any(|p| {
                let d0 = p[0] - candidate[0];
                let d1 = p[1] - candidate[1];
                let d2 = p[2] - candidate[2];
                (d0 * d0 + d1 * d1 + d2 * d2) < min_distance * min_distance
            });

            if !too_close {
                active.push(points.len());
                points.push(candidate);
                found = true;
                break;
            }
        }

        if !found {
            active.swap_remove(active_idx);
        }
    }

    points
}

/// Generate all stars in a sector, deterministically from the master seed.
pub fn generate_sector(master: MasterSeed, coord: SectorCoord) -> Sector {
    let hash = sector_hash(master, coord.x, coord.y, coord.z);
    let mut rng = Rng64::new(hash);
    let density = sector_density(coord);

    // Minimum distance between stars scales inversely with density
    let min_dist = SECTOR_SIZE_LY / (density as f64 + 1.0).sqrt();

    let positions = poisson_disk_3d(&mut rng, density, min_dist);
    let origin = coord.world_origin();

    let stars = positions
        .iter()
        .enumerate()
        .map(|(i, local_pos)| {
            let id = ObjectId::star_id(
                coord.x as i16,
                coord.y as i16,
                coord.z as i16,
                i as u8,
            );
            // Each star gets a unique seed derived from the sector hash and its index
            let star_seed = hash.wrapping_add(i as u64).wrapping_mul(0x517CC1B727220A95);
            let star = generate_star(star_seed);
            let position = WorldPos::new(
                origin.x + local_pos[0],
                origin.y + local_pos[1],
                origin.z + local_pos[2],
            );
            PlacedStar { id, position, star }
        })
        .collect();

    Sector { coord, stars }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_coord_from_world_pos() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(15.0, -5.0, 25.0));
        assert_eq!(coord, SectorCoord::new(1, -1, 2));
    }

    #[test]
    fn sector_coord_from_world_pos_exact_boundary() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(10.0, 0.0, 0.0));
        assert_eq!(coord, SectorCoord::new(1, 0, 0));
    }

    #[test]
    fn sector_coord_from_world_pos_negative() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(-0.1, -10.1, 0.0));
        assert_eq!(coord, SectorCoord::new(-1, -2, 0));
    }

    #[test]
    fn sector_world_origin() {
        let origin = SectorCoord::new(1, -1, 2).world_origin();
        assert!((origin.x - 10.0).abs() < 1e-10);
        assert!((origin.y - (-10.0)).abs() < 1e-10);
        assert!((origin.z - 20.0).abs() < 1e-10);
    }

    #[test]
    fn sector_density_center_higher_than_edge() {
        let center = sector_density(SectorCoord::new(0, 0, 0));
        let edge = sector_density(SectorCoord::new(1000, 0, 0));
        assert!(
            center >= edge,
            "Center density ({center}) should be >= edge density ({edge})"
        );
    }

    #[test]
    fn poisson_disk_returns_points() {
        let mut rng = Rng64::new(42);
        let pts = poisson_disk_3d(&mut rng, 20, 1.0);
        assert!(!pts.is_empty(), "Should return at least some points");
    }

    #[test]
    fn poisson_disk_minimum_distance() {
        let mut rng = Rng64::new(42);
        let min_dist = 1.5;
        let pts = poisson_disk_3d(&mut rng, 15, min_dist);
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let dx = pts[i][0] - pts[j][0];
                let dy = pts[i][1] - pts[j][1];
                let dz = pts[i][2] - pts[j][2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!(
                    dist >= min_dist * 0.99,
                    "Points {i} and {j} too close: {dist} < {min_dist}"
                );
            }
        }
    }

    #[test]
    fn poisson_disk_points_in_bounds() {
        let mut rng = Rng64::new(99);
        let pts = poisson_disk_3d(&mut rng, 30, 1.0);
        for (i, p) in pts.iter().enumerate() {
            for d in 0..3 {
                assert!(
                    (0.0..=SECTOR_SIZE_LY).contains(&p[d]),
                    "Point {i} dim {d} out of bounds: {}",
                    p[d]
                );
            }
        }
    }

    #[test]
    fn generate_sector_deterministic() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(5, -3, 1);
        let a = generate_sector(seed, coord);
        let b = generate_sector(seed, coord);
        assert_eq!(a.stars.len(), b.stars.len());
        for (sa, sb) in a.stars.iter().zip(b.stars.iter()) {
            assert_eq!(sa.id, sb.id);
            assert_eq!(sa.position, sb.position);
            assert_eq!(sa.star.mass.to_bits(), sb.star.mass.to_bits());
        }
    }

    #[test]
    fn generate_sector_stars_inside_sector() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(2, -1, 0);
        let sector = generate_sector(seed, coord);
        let origin = coord.world_origin();
        for ps in &sector.stars {
            assert!(ps.position.x >= origin.x && ps.position.x <= origin.x + SECTOR_SIZE_LY);
            assert!(ps.position.y >= origin.y && ps.position.y <= origin.y + SECTOR_SIZE_LY);
            assert!(ps.position.z >= origin.z && ps.position.z <= origin.z + SECTOR_SIZE_LY);
        }
    }

    #[test]
    fn generate_sector_has_valid_object_ids() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(3, 4, 5);
        let sector = generate_sector(seed, coord);
        for (i, ps) in sector.stars.iter().enumerate() {
            assert_eq!(ps.id.sector_x(), coord.x as i16);
            assert_eq!(ps.id.sector_y(), coord.y as i16);
            assert_eq!(ps.id.sector_z(), coord.z as i16);
            assert_eq!(ps.id.system(), i as u8);
            assert_eq!(ps.id.body(), 0);
        }
    }
}
