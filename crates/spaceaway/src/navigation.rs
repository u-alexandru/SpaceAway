//! Navigation: nearby star markers, lock-on targeting, gravity well detection.
#![allow(dead_code)]

use sa_math::WorldPos;
use sa_universe::sector::{SectorCoord, generate_sector};
use sa_universe::seed::MasterSeed;
use sa_universe::{ObjectId, SpectralClass};

use crate::constants::{AU_IN_LY, GRAVITY_WELL_AU, MAX_NEARBY_STARS, NEARBY_STAR_RANGE_LY};

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
                        if dist < NEARBY_STAR_RANGE_LY && dist > 0.01 {
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
        stars.truncate(MAX_NEARBY_STARS);
        self.nearby_stars = stars;
    }

    /// Lock on to a star by index in nearby_stars.
    pub fn lock_target(&mut self, index: usize) {
        if let Some(star) = self.nearby_stars.get(index) {
            self.locked_target = Some(star.clone());
        }
    }

    /// Lock on to a specific NavStar directly (e.g. a planet).
    pub fn lock_star(&mut self, star: NavStar) {
        self.locked_target = Some(star);
    }

    /// Clear the current lock-on target.
    pub fn clear_target(&mut self) {
        self.locked_target = None;
    }

    /// Check if the observer is within a star's gravity well (~50 AU).
    /// Returns the star to drop at, if any. Checks locked target first,
    /// then all nearby stars.
    ///
    /// IMPORTANT: The caller must NOT call this while an ActiveSystem is loaded.
    /// Otherwise it will re-trigger on the current system's star every frame.
    /// Call only during warp when no system is active.
    pub fn check_gravity_well(&self, galactic_pos: WorldPos) -> Option<&NavStar> {
        let drop_distance_ly = GRAVITY_WELL_AU * AU_IN_LY;

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

    /// Lock the star most aligned with the camera forward direction.
    /// Only considers stars within a ~45° cone (dot > 0.7).
    pub fn lock_nearest_to_crosshair(&mut self, camera_forward: [f32; 3], galactic_pos: WorldPos) {
        let fwd = [camera_forward[0] as f64, camera_forward[1] as f64, camera_forward[2] as f64];
        let fwd_len = (fwd[0]*fwd[0] + fwd[1]*fwd[1] + fwd[2]*fwd[2]).sqrt();
        if fwd_len < 1e-10 { return; }
        let fwd = [fwd[0]/fwd_len, fwd[1]/fwd_len, fwd[2]/fwd_len];

        let mut best_dot = 0.0; // forward hemisphere — pick best match ahead of you
        let mut best_idx = None;

        for (i, star) in self.nearby_stars.iter().enumerate() {
            let dx = star.galactic_pos.x - galactic_pos.x;
            let dy = star.galactic_pos.y - galactic_pos.y;
            let dz = star.galactic_pos.z - galactic_pos.z;
            let len = (dx*dx + dy*dy + dz*dz).sqrt();
            if len < 0.01 { continue; }
            let dir = [dx/len, dy/len, dz/len];
            let dot = fwd[0]*dir[0] + fwd[1]*dir[1] + fwd[2]*dir[2];
            if dot > best_dot {
                best_dot = dot;
                best_idx = Some(i);
            }
        }

        if let Some(idx) = best_idx {
            let star = &self.nearby_stars[idx];
            log::info!("Lock-on: {} (dot={:.3}, dist={:.2}ly, dir=[{:.2},{:.2},{:.2}], cam_fwd=[{:.2},{:.2},{:.2}])",
                star.catalog_name, best_dot, star.distance_ly,
                {let d=star.galactic_pos.x-galactic_pos.x; let l=(d*d+(star.galactic_pos.y-galactic_pos.y).powi(2)+(star.galactic_pos.z-galactic_pos.z).powi(2)).sqrt(); d/l},
                {let d=star.galactic_pos.y-galactic_pos.y; let l=((star.galactic_pos.x-galactic_pos.x).powi(2)+d*d+(star.galactic_pos.z-galactic_pos.z).powi(2)).sqrt(); d/l},
                {let d=star.galactic_pos.z-galactic_pos.z; let l=((star.galactic_pos.x-galactic_pos.x).powi(2)+(star.galactic_pos.y-galactic_pos.y).powi(2)+d*d).sqrt(); d/l},
                fwd[0], fwd[1], fwd[2],
            );
            self.locked_target = Some(self.nearby_stars[idx].clone());
        } else {
            log::warn!("No star in forward hemisphere ({} nearby stars, fwd=[{:.2},{:.2},{:.2}])",
                self.nearby_stars.len(), fwd[0], fwd[1], fwd[2]);
        }
    }

    /// Predictive gravity well check: does the line segment from pos_before to pos_after
    /// pass within 50 AU of any star? Returns the star and the exact drop position.
    pub fn check_gravity_well_predictive(
        &self,
        pos_before: WorldPos,
        pos_after: WorldPos,
    ) -> Option<(NavStar, WorldPos)> {
        let well_radius = GRAVITY_WELL_AU * AU_IN_LY;

        // Segment vector
        let seg = [
            pos_after.x - pos_before.x,
            pos_after.y - pos_before.y,
            pos_after.z - pos_before.z,
        ];
        let seg_len_sq = seg[0]*seg[0] + seg[1]*seg[1] + seg[2]*seg[2];
        if seg_len_sq < 1e-30 { return None; }

        let mut closest_t = f64::MAX;
        let mut hit_star = None;
        let mut hit_pos = pos_after;

        // Check locked target first, then all nearby stars
        let targets: Vec<&NavStar> = self.locked_target.iter()
            .chain(self.nearby_stars.iter())
            .collect();

        for star in targets {
            // Vector from segment start to star
            let to_star = [
                star.galactic_pos.x - pos_before.x,
                star.galactic_pos.y - pos_before.y,
                star.galactic_pos.z - pos_before.z,
            ];

            // Project star onto segment: t = dot(to_star, seg) / dot(seg, seg)
            let dot = to_star[0]*seg[0] + to_star[1]*seg[1] + to_star[2]*seg[2];
            let t = (dot / seg_len_sq).clamp(0.0, 1.0);

            // Closest point on segment to star
            let closest = [
                pos_before.x + seg[0] * t,
                pos_before.y + seg[1] * t,
                pos_before.z + seg[2] * t,
            ];

            let dx = closest[0] - star.galactic_pos.x;
            let dy = closest[1] - star.galactic_pos.y;
            let dz = closest[2] - star.galactic_pos.z;
            let dist = (dx*dx + dy*dy + dz*dz).sqrt();

            if dist < well_radius && t < closest_t {
                closest_t = t;
                // Place ship at well_radius from star, on the approach side
                let approach_dir = [
                    pos_before.x - star.galactic_pos.x,
                    pos_before.y - star.galactic_pos.y,
                    pos_before.z - star.galactic_pos.z,
                ];
                let a_len = (approach_dir[0]*approach_dir[0]
                    + approach_dir[1]*approach_dir[1]
                    + approach_dir[2]*approach_dir[2]).sqrt();
                if a_len > 1e-10 {
                    hit_pos = WorldPos::new(
                        star.galactic_pos.x + approach_dir[0]/a_len * well_radius,
                        star.galactic_pos.y + approach_dir[1]/a_len * well_radius,
                        star.galactic_pos.z + approach_dir[2]/a_len * well_radius,
                    );
                }
                hit_star = Some(star.clone());
            }
        }

        hit_star.map(|star| (star, hit_pos))
    }

    /// Check if the ship will pass through a gravity well within `lookahead` ly.
    /// Returns the star if a proximity alert should fire.
    pub fn check_proximity_warning(
        &self,
        pos: WorldPos,
        velocity_dir: [f64; 3],
        lookahead_ly: f64,
    ) -> Option<&NavStar> {
        let well_radius = GRAVITY_WELL_AU * AU_IN_LY;

        let future_pos = WorldPos::new(
            pos.x + velocity_dir[0] * lookahead_ly,
            pos.y + velocity_dir[1] * lookahead_ly,
            pos.z + velocity_dir[2] * lookahead_ly,
        );

        for star in &self.nearby_stars {
            let seg = [future_pos.x - pos.x, future_pos.y - pos.y, future_pos.z - pos.z];
            let seg_len_sq = seg[0]*seg[0] + seg[1]*seg[1] + seg[2]*seg[2];
            if seg_len_sq < 1e-30 { continue; }

            let to_star = [
                star.galactic_pos.x - pos.x,
                star.galactic_pos.y - pos.y,
                star.galactic_pos.z - pos.z,
            ];
            let dot = to_star[0]*seg[0] + to_star[1]*seg[1] + to_star[2]*seg[2];
            let t = (dot / seg_len_sq).clamp(0.0, 1.0);

            let closest = [pos.x + seg[0]*t, pos.y + seg[1]*t, pos.z + seg[2]*t];
            let dx = closest[0] - star.galactic_pos.x;
            let dy = closest[1] - star.galactic_pos.y;
            let dz = closest[2] - star.galactic_pos.z;
            let dist = (dx*dx + dy*dy + dz*dz).sqrt();

            if dist < well_radius {
                return Some(star);
            }
        }
        None
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

    #[test]
    fn lock_nearest_to_crosshair_picks_aimed_star() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
        if nav.nearby_stars.len() >= 2 {
            // Aim at the first star
            let star = &nav.nearby_stars[0];
            let dx = (star.galactic_pos.x - 5.0) as f32;
            let dy = (star.galactic_pos.y) as f32;
            let dz = (star.galactic_pos.z - 5.0) as f32;
            let len = (dx*dx + dy*dy + dz*dz).sqrt();
            let fwd = [dx/len, dy/len, dz/len];
            nav.lock_nearest_to_crosshair(fwd, WorldPos::new(5.0, 0.0, 5.0));
            assert!(nav.locked_target.is_some());
        }
    }

    #[test]
    fn predictive_gravity_well_catches_flythrough() {
        let mut nav = Navigation::new(MasterSeed(42));
        nav.update_nearby(WorldPos::ORIGIN);
        if let Some(star) = nav.nearby_stars.first() {
            // Fly a segment that passes through the star
            let before = WorldPos::new(
                star.galactic_pos.x - 0.1, star.galactic_pos.y, star.galactic_pos.z,
            );
            let after = WorldPos::new(
                star.galactic_pos.x + 0.1, star.galactic_pos.y, star.galactic_pos.z,
            );
            let result = nav.check_gravity_well_predictive(before, after);
            assert!(result.is_some(), "should detect flythrough");
        }
    }
}
