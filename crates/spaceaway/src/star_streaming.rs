//! Sector-based star streaming with per-sector fade transitions.
//!
//! Instead of regenerating all stars at once (causing visible popping),
//! this system tracks which sectors are loaded and smoothly fades
//! new sectors in and old sectors out as the observer moves.
//!
//! - Sectors entering the query radius: fade in over FADE_DURATION
//! - Sectors leaving the query radius: fade out over FADE_DURATION
//! - Sectors staying in range: untouched (zero cost)
//! - Result: smooth, continuous sky with no visible transitions

use sa_render::star_field::StarVertex;
use sa_universe::sector::{generate_sector, PlacedStar, SectorCoord};
use sa_universe::seed::MasterSeed;
use sa_math::WorldPos;
use std::collections::{HashMap, HashSet};

/// Fade duration in seconds (0.5s = fast but smooth).
const FADE_DURATION: f32 = 0.5;
/// Query radius in sectors (4 = 9×9×9 = 729 sectors).
const QUERY_RADIUS: i32 = 4;
/// Minimum brightness to include a star (culls dim stars).
const MIN_BRIGHTNESS: f32 = 0.32;

struct CachedSector {
    stars: Vec<PlacedStar>,
    /// Fade factor: 0.0 (invisible) to 1.0 (fully visible).
    fade: f32,
    /// True when sector is being removed (fading out).
    fading_out: bool,
}

pub struct StarStreaming {
    sectors: HashMap<SectorCoord, CachedSector>,
    last_observer_sector: Option<SectorCoord>,
    seed: MasterSeed,
    /// True when any sector is mid-fade (needs buffer rebuild each frame).
    any_fading: bool,
}

impl StarStreaming {
    pub fn new(seed: MasterSeed) -> Self {
        Self {
            sectors: HashMap::new(),
            last_observer_sector: None,
            seed,
            any_fading: false,
        }
    }

    /// Call every frame. Returns true if the star vertex buffer needs rebuilding.
    pub fn update(&mut self, observer: WorldPos, dt: f32) -> bool {
        let current_sector = SectorCoord::from_world_pos(observer);
        let sector_changed = self.last_observer_sector != Some(current_sector);

        if sector_changed {
            self.last_observer_sector = Some(current_sector);

            // Compute desired sector set
            let desired: HashSet<SectorCoord> = desired_sectors(current_sector)
                .into_iter()
                .collect();

            // Add new sectors (fade in)
            for &coord in &desired {
                if !self.sectors.contains_key(&coord) {
                    let sector = generate_sector(self.seed, coord);
                    self.sectors.insert(coord, CachedSector {
                        stars: sector.stars,
                        fade: 0.0,
                        fading_out: false,
                    });
                } else if let Some(s) = self.sectors.get_mut(&coord) {
                    // Sector was fading out but is back in range — cancel fadeout
                    s.fading_out = false;
                }
            }

            // Mark sectors no longer in range for fadeout
            for (coord, sector) in self.sectors.iter_mut() {
                if !desired.contains(coord) && !sector.fading_out {
                    sector.fading_out = true;
                }
            }
        }

        // Update fades
        let fade_speed = 1.0 / FADE_DURATION;
        self.any_fading = false;

        for sector in self.sectors.values_mut() {
            if sector.fading_out {
                sector.fade = (sector.fade - dt * fade_speed).max(0.0);
                if sector.fade > 0.0 {
                    self.any_fading = true;
                }
            } else if sector.fade < 1.0 {
                sector.fade = (sector.fade + dt * fade_speed).min(1.0);
                if sector.fade < 1.0 {
                    self.any_fading = true;
                }
            }
        }

        // Remove fully faded-out sectors
        self.sectors.retain(|_, s| !(s.fading_out && s.fade <= 0.0));

        // Buffer rebuild needed if sectors changed or any fade in progress
        sector_changed || self.any_fading
    }

    /// Build star vertices from all cached sectors relative to the current observer.
    pub fn build_vertices(&self, observer: WorldPos) -> Vec<StarVertex> {
        let mut vertices = Vec::with_capacity(self.sectors.len() * 40);

        for sector in self.sectors.values() {
            if sector.fade <= 0.0 {
                continue;
            }

            for placed in &sector.stars {
                let dx = (placed.position.x - observer.x) as f32;
                let dy = (placed.position.y - observer.y) as f32;
                let dz = (placed.position.z - observer.z) as f32;

                let dist_sq = dx * dx + dy * dy + dz * dz;
                let luminosity = placed.star.luminosity;

                let apparent = if dist_sq > 0.01 {
                    luminosity / (1.0 + dist_sq * 0.005)
                } else {
                    luminosity
                };

                let log_apparent = (1.0 + apparent * 200.0).ln();
                let brightness = (log_apparent / 10.0 + 0.30).clamp(0.30, 1.0);

                // Apply sector fade
                let faded = brightness * sector.fade;
                if faded < MIN_BRIGHTNESS {
                    continue;
                }

                // Normalize to direction on unit sphere
                let len = dist_sq.sqrt();
                if len < 0.001 {
                    continue;
                }

                vertices.push(StarVertex {
                    position: [dx / len, dy / len, dz / len],
                    brightness: faded,
                    color: placed.star.color,
                    _pad: 0.0,
                });
            }
        }

        vertices
    }

    /// Force immediate load of all sectors around observer (no fade, instant).
    /// Used on teleport or initial load.
    pub fn force_load(&mut self, observer: WorldPos) {
        self.sectors.clear();
        let current_sector = SectorCoord::from_world_pos(observer);
        self.last_observer_sector = Some(current_sector);

        for coord in desired_sectors(current_sector) {
            let sector = generate_sector(self.seed, coord);
            self.sectors.insert(coord, CachedSector {
                stars: sector.stars,
                fade: 1.0,
                fading_out: false,
            });
        }
        self.any_fading = false;
    }

    /// Number of currently loaded sectors (for debug).
    pub fn sector_count(&self) -> usize {
        self.sectors.len()
    }
}

/// Compute the set of sectors that should be loaded around the observer.
fn desired_sectors(center: SectorCoord) -> Vec<SectorCoord> {
    let mut sectors = Vec::with_capacity(
        ((QUERY_RADIUS * 2 + 1).pow(3)) as usize,
    );
    for dx in -QUERY_RADIUS..=QUERY_RADIUS {
        for dy in -QUERY_RADIUS..=QUERY_RADIUS {
            for dz in -QUERY_RADIUS..=QUERY_RADIUS {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_load_populates_sectors() {
        let mut streaming = StarStreaming::new(MasterSeed(42));
        streaming.force_load(WorldPos::ORIGIN);
        let expected = (QUERY_RADIUS * 2 + 1).pow(3) as usize;
        assert_eq!(streaming.sector_count(), expected);
    }

    #[test]
    fn force_load_all_fully_visible() {
        let mut streaming = StarStreaming::new(MasterSeed(42));
        streaming.force_load(WorldPos::ORIGIN);
        let verts = streaming.build_vertices(WorldPos::ORIGIN);
        assert!(!verts.is_empty(), "should have visible stars");
    }

    #[test]
    fn update_same_position_no_change() {
        let mut streaming = StarStreaming::new(MasterSeed(42));
        streaming.force_load(WorldPos::ORIGIN);
        let changed = streaming.update(WorldPos::ORIGIN, 1.0 / 60.0);
        assert!(!changed, "no change when observer hasn't moved sectors");
    }

    #[test]
    fn update_new_sector_triggers_fade() {
        let mut streaming = StarStreaming::new(MasterSeed(42));
        streaming.force_load(WorldPos::ORIGIN);
        // Move to a new sector
        let far = WorldPos::new(100.0, 0.0, 0.0);
        let changed = streaming.update(far, 1.0 / 60.0);
        assert!(changed, "moving to new sector should trigger rebuild");
    }

    #[test]
    fn vertices_deterministic() {
        let mut s1 = StarStreaming::new(MasterSeed(42));
        let mut s2 = StarStreaming::new(MasterSeed(42));
        s1.force_load(WorldPos::ORIGIN);
        s2.force_load(WorldPos::ORIGIN);
        let v1 = s1.build_vertices(WorldPos::ORIGIN);
        let v2 = s2.build_vertices(WorldPos::ORIGIN);
        assert_eq!(v1.len(), v2.len());
    }
}
