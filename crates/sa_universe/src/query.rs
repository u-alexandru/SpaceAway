use crate::object_id::ObjectId;
use crate::sector::{SectorCoord, generate_sector};
use crate::seed::MasterSeed;
use sa_math::WorldPos;

/// A star ready for rendering: position relative to observer, plus visual data.
#[derive(Debug, Clone)]
pub struct VisibleStar {
    pub id: ObjectId,
    /// Position relative to the observer (camera-space, in light-years).
    pub relative_pos: [f32; 3],
    pub brightness: f32,
    pub color: [f32; 3],
}

/// Top-level universe handle. Holds the master seed and provides queries.
pub struct Universe {
    pub seed: MasterSeed,
}

impl Universe {
    pub fn new(seed: MasterSeed) -> Self {
        Self { seed }
    }

    /// Return all sectors within `radius` sectors of the given position.
    pub fn nearby_sectors(&self, pos: WorldPos, radius: i32) -> Vec<SectorCoord> {
        let center = SectorCoord::from_world_pos(pos);
        let mut sectors = Vec::new();
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    sectors.push(SectorCoord::new(
                        center.x + dx,
                        center.y + dy,
                        center.z + dz,
                    ));
                }
            }
        }
        sectors
    }

    /// Query all visible stars within `radius` sectors of `observer_pos`.
    /// Returns stars with positions relative to the observer for rendering.
    pub fn visible_stars(&self, observer_pos: WorldPos, radius: i32) -> Vec<VisibleStar> {
        let sectors = self.nearby_sectors(observer_pos, radius);
        let mut visible = Vec::new();

        for coord in sectors {
            let sector = generate_sector(self.seed, coord);
            for placed in &sector.stars {
                let dx = (placed.position.x - observer_pos.x) as f32;
                let dy = (placed.position.y - observer_pos.y) as f32;
                let dz = (placed.position.z - observer_pos.z) as f32;

                // Distance-based brightness attenuation
                let dist_sq = dx * dx + dy * dy + dz * dz;
                let attenuation = if dist_sq > 0.01 {
                    1.0 / (1.0 + dist_sq * 0.001)
                } else {
                    1.0
                };
                let brightness = (placed.star.brightness * attenuation).clamp(0.01, 1.0);

                visible.push(VisibleStar {
                    id: placed.id,
                    relative_pos: [dx, dy, dz],
                    brightness,
                    color: placed.star.color,
                });
            }
        }

        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sector::SECTOR_SIZE_LY;

    #[test]
    fn nearby_sectors_at_origin() {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 1);
        // Radius 1 around sector (0,0,0): should be 3x3x3 = 27 sectors
        assert_eq!(sectors.len(), 27);
    }

    #[test]
    fn nearby_sectors_contains_current() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(5.0, 5.0, 5.0); // sector (0,0,0)
        let sectors = uni.nearby_sectors(pos, 1);
        assert!(
            sectors.contains(&SectorCoord::new(0, 0, 0)),
            "Should contain the sector the observer is in"
        );
    }

    #[test]
    fn nearby_sectors_radius_zero() {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 0);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0], SectorCoord::new(0, 0, 0));
    }

    #[test]
    fn nearby_sectors_offset_position() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(15.0, 0.0, 0.0); // sector (1,0,0)
        let sectors = uni.nearby_sectors(pos, 0);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0], SectorCoord::new(1, 0, 0));
    }

    #[test]
    fn visible_stars_deterministic() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(5.0, 5.0, 5.0);
        let a = uni.visible_stars(pos, 1);
        let b = uni.visible_stars(pos, 1);
        assert_eq!(a.len(), b.len());
        for (sa, sb) in a.iter().zip(b.iter()) {
            assert_eq!(sa.id, sb.id);
            assert_eq!(sa.relative_pos[0].to_bits(), sb.relative_pos[0].to_bits());
        }
    }

    #[test]
    fn visible_stars_have_valid_color() {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, 1);
        for s in &stars {
            for c in &s.color {
                assert!((0.0..=1.0).contains(c), "Color out of range: {c}");
            }
            assert!(s.brightness > 0.0 && s.brightness <= 1.0);
        }
    }

    #[test]
    fn visible_stars_returns_nonempty() {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, 2);
        assert!(!stars.is_empty(), "Should find at least some stars");
    }

    #[test]
    fn visible_stars_relative_positions() {
        let uni = Universe::new(MasterSeed(42));
        let observer = WorldPos::new(50.0, 50.0, 50.0);
        let stars = uni.visible_stars(observer, 1);
        // Relative positions should be small (within a few sector sizes)
        let max_dist = (3.0 * SECTOR_SIZE_LY * SECTOR_SIZE_LY * 4.0).sqrt() as f32;
        for s in &stars {
            let d = (s.relative_pos[0].powi(2)
                + s.relative_pos[1].powi(2)
                + s.relative_pos[2].powi(2))
            .sqrt();
            assert!(
                d < max_dist * 2.0,
                "Star too far from observer: {d} ly"
            );
        }
    }
}
