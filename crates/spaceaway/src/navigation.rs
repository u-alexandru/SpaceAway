//! Navigation: nearby star markers, lock-on targeting, gravity well detection.

use sa_math::WorldPos;
use sa_universe::sector::{SectorCoord, generate_sector};
use sa_universe::seed::MasterSeed;
use sa_universe::{ObjectId, SpectralClass};

/// A nearby star visible for navigation.
#[derive(Clone)]
pub struct NavStar {
    pub id: ObjectId,
    pub galactic_pos: WorldPos,
    pub catalog_name: String,
    pub distance_ly: f64,
    pub color: [f32; 3],
    pub spectral_class: SpectralClass,
    pub luminosity: f32,
}

/// A saved bookmark.
#[derive(Clone)]
pub struct Bookmark {
    pub star_id: ObjectId,
    pub galactic_pos: WorldPos,
    pub catalog_name: String,
    pub nickname: Option<String>,
}

/// Navigation state.
pub struct Navigation {
    /// Nearby stars for window markers (sorted by distance).
    pub nearby_stars: Vec<NavStar>,
    /// Currently locked target (if any).
    pub locked_target: Option<NavStar>,
    /// Bookmarked systems.
    pub bookmarks: Vec<Bookmark>,
    seed: MasterSeed,
    last_update_sector: Option<SectorCoord>,
}

impl Navigation {
    pub fn new(seed: MasterSeed) -> Self {
        Self {
            nearby_stars: Vec::new(),
            locked_target: None,
            bookmarks: Vec::new(),
            seed,
            last_update_sector: None,
        }
    }

    /// Update nearby star list. Only re-queries when observer changes sector.
    /// Call every frame — it early-returns if sector hasn't changed.
    pub fn update_nearby(&mut self, galactic_pos: WorldPos) {
        let current_sector = SectorCoord::from_world_pos(galactic_pos);
        if self.last_update_sector == Some(current_sector) {
            // Still in same sector — just update distances (cheap)
            for star in &mut self.nearby_stars {
                star.distance_ly = galactic_pos.distance_to(star.galactic_pos);
            }
            self.nearby_stars
                .sort_by(|a, b| a.distance_ly.partial_cmp(&b.distance_ly).unwrap());
            return;
        }
        self.last_update_sector = Some(current_sector);

        // Query sectors within radius 5 (11x11x11 = 1331 sectors, covers ~50 ly)
        let mut stars = Vec::new();
        for dx in -5..=5 {
            for dy in -5..=5 {
                for dz in -5..=5 {
                    let coord = SectorCoord::new(
                        current_sector.x + dx,
                        current_sector.y + dy,
                        current_sector.z + dz,
                    );
                    let sector = generate_sector(self.seed, coord);
                    for placed in &sector.stars {
                        let dist = galactic_pos.distance_to(placed.position);
                        if dist < 50.0 && dist > 0.01 {
                            // within 50 ly, not self
                            stars.push(NavStar {
                                id: placed.id,
                                galactic_pos: placed.position,
                                catalog_name: catalog_name_from_id(placed.id),
                                distance_ly: dist,
                                color: placed.star.color,
                                spectral_class: placed.star.spectral_class,
                                luminosity: placed.star.luminosity,
                            });
                        }
                    }
                }
            }
        }

        stars.sort_by(|a, b| a.distance_ly.partial_cmp(&b.distance_ly).unwrap());
        stars.truncate(15); // Keep closest 15
        self.nearby_stars = stars;
    }

    /// Lock on to a star by index in nearby_stars.
    pub fn lock_target(&mut self, index: usize) {
        if let Some(star) = self.nearby_stars.get(index) {
            self.locked_target = Some(star.clone());
        }
    }

    /// Clear the current lock-on target.
    pub fn clear_target(&mut self) {
        self.locked_target = None;
    }

    /// Check if the observer is within a star's gravity well (~50 AU).
    /// Returns the star to drop at, if any. Checks locked target first,
    /// then all nearby stars.
    pub fn check_gravity_well(&self, galactic_pos: WorldPos) -> Option<&NavStar> {
        let au_in_ly: f64 = 1.581e-5; // 1 AU in light-years
        let drop_distance_ly = 50.0 * au_in_ly; // 50 AU

        // Check locked target first
        if let Some(target) = &self.locked_target {
            let dist = galactic_pos.distance_to(target.galactic_pos);
            if dist < drop_distance_ly {
                return Some(target);
            }
        }

        // Check all nearby stars
        for star in &self.nearby_stars {
            let dist = galactic_pos.distance_to(star.galactic_pos);
            if dist < drop_distance_ly {
                return Some(star);
            }
        }

        None
    }

    /// Add a bookmark for a star.
    pub fn add_bookmark(&mut self, star: &NavStar, nickname: Option<String>) {
        self.bookmarks.push(Bookmark {
            star_id: star.id,
            galactic_pos: star.galactic_pos,
            catalog_name: star.catalog_name.clone(),
            nickname,
        });
    }

    /// Remove a bookmark by index.
    pub fn remove_bookmark(&mut self, index: usize) {
        if index < self.bookmarks.len() {
            self.bookmarks.remove(index);
        }
    }

    /// Compute distance and ETA to locked target.
    /// Returns (distance_ly, eta_seconds) or None if no target.
    pub fn target_eta(&self, galactic_pos: WorldPos, speed_ly_s: f64) -> Option<(f64, f64)> {
        let target = self.locked_target.as_ref()?;
        let dist = galactic_pos.distance_to(target.galactic_pos);
        let eta = if speed_ly_s > 1e-20 {
            dist / speed_ly_s
        } else {
            f64::INFINITY
        };
        Some((dist, eta))
    }
}

/// Generate a catalog name from an ObjectId.
pub fn catalog_name_from_id(id: ObjectId) -> String {
    format!(
        "SEC {:04}.{:04} / S-{:03}",
        id.sector_x().unsigned_abs(),
        id.sector_z().unsigned_abs(),
        id.system()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_name_format() {
        let id = ObjectId::pack(42, 0, 17, 0, 3, 0);
        let name = catalog_name_from_id(id);
        assert!(name.contains("SEC"), "name={name}");
        assert!(name.contains("S-003"), "name={name}");
    }

    #[test]
    fn navigation_starts_empty() {
        let nav = Navigation::new(MasterSeed(42));
        assert!(nav.nearby_stars.is_empty());
        assert!(nav.locked_target.is_none());
    }

    #[test]
    fn update_nearby_finds_stars() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
        assert!(!nav.nearby_stars.is_empty(), "should find nearby stars");
        assert!(nav.nearby_stars.len() <= 15, "should cap at 15");
    }

    #[test]
    fn nearby_stars_sorted_by_distance() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
        for w in nav.nearby_stars.windows(2) {
            assert!(
                w[0].distance_ly <= w[1].distance_ly,
                "stars should be sorted by distance"
            );
        }
    }

    #[test]
    fn lock_and_clear_target() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
        if !nav.nearby_stars.is_empty() {
            nav.lock_target(0);
            assert!(nav.locked_target.is_some());
            nav.clear_target();
            assert!(nav.locked_target.is_none());
        }
    }

    #[test]
    fn gravity_well_detects_nearby_star() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
        if let Some(star) = nav.nearby_stars.first() {
            // Place observer right on top of the star
            let hit = nav.check_gravity_well(star.galactic_pos);
            assert!(
                hit.is_some(),
                "should detect gravity well when on top of star"
            );
        }
    }

    #[test]
    fn target_eta_calculation() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
        if !nav.nearby_stars.is_empty() {
            nav.lock_target(0);
            let pos = WorldPos::new(5.0, 0.0, 5.0);
            let result = nav.target_eta(pos, 0.1); // 0.1 ly/s
            assert!(result.is_some());
            let (dist, eta) = result.unwrap();
            assert!(dist > 0.0);
            assert!(eta > 0.0);
        }
    }
}
