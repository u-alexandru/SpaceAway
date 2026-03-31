//! Fixed-resolution 7×7 collision grid independent of visual LOD.
//!
//! Produces `GridUpdate` (chunks to add/remove with 33×33 height data).
//! The integration layer in spaceaway converts these into rapier colliders.

use std::collections::HashMap;

use crate::config::{
    CELLS, COLLISION_GRID_HYSTERESIS, COLLISION_GRID_SIZE, COLLISION_LOD_OFFSET,
    COLLISION_MAX_CHUNK_WIDTH_M, GRID_SIZE,
};
use crate::cube_sphere::{cube_to_sphere, CubeFace};
use crate::heightmap::{make_terrain_noise, make_warp_noise, sample_height};
use crate::quadtree::max_lod_levels;
use crate::{ChunkKey, TerrainConfig};

/// Compute the collision LOD level for a planet.
///
/// Picks the finest LOD whose chunk width is ≤ `COLLISION_MAX_CHUNK_WIDTH_M`,
/// offset by `COLLISION_LOD_OFFSET` from the visual max LOD.
pub fn collision_lod(max_lod: u8, radius_m: f64) -> u8 {
    // Start from max_lod - offset and search coarser if chunk is still too wide.
    let start = max_lod.saturating_sub(COLLISION_LOD_OFFSET);
    for lod in (0..=start).rev() {
        let width = face_size_at_lod(radius_m, lod);
        if width <= COLLISION_MAX_CHUNK_WIDTH_M {
            return lod;
        }
    }
    // If even the finest LOD is too wide, use the finest available.
    start
}

/// Chunk width (in meters) at a given LOD level.
///
/// At LOD `lod`, the face is divided into `2^lod` tiles per edge.
/// Each tile spans `2 * radius / 2^lod` meters on the sphere surface
/// (approximation using great-circle arc ≈ chord for small tiles).
pub fn face_size_at_lod(radius_m: f64, lod: u8) -> f64 {
    let tiles = (1u64 << lod) as f64;
    // Arc length of one tile: radius * (2 / tiles) ≈ face chord
    // The cube face subtends ~pi/2 radians, so arc = radius * pi/2 / tiles
    // but the linear approximation 2*radius/tiles matches the visual system.
    2.0 * radius_m / tiles
}

/// Map a unit sphere direction back to the dominant cube face and UV coords.
///
/// Inverse of `cube_to_sphere` (approximate — exact inverse of the analytic
/// projection is not closed-form, but the dominant-axis approach gives the
/// correct face and close-enough UVs for grid indexing).
pub fn sphere_to_cube_face(dir: [f64; 3]) -> (CubeFace, f64, f64) {
    let ax = dir[0].abs();
    let ay = dir[1].abs();
    let az = dir[2].abs();

    if ax >= ay && ax >= az {
        if dir[0] > 0.0 {
            // PosX: (1, v, -u) → u = -z/x, v = y/x
            let u = -dir[2] / dir[0];
            let v = dir[1] / dir[0];
            (CubeFace::PosX, u, v)
        } else {
            // NegX: (-1, v, u) → u = z/(-x), v = y/(-x)
            let u = dir[2] / (-dir[0]);
            let v = dir[1] / (-dir[0]);
            (CubeFace::NegX, u, v)
        }
    } else if ay >= ax && ay >= az {
        if dir[1] > 0.0 {
            // PosY: (u, 1, -v) → u = x/y, v = -z/y
            let u = dir[0] / dir[1];
            let v = -dir[2] / dir[1];
            (CubeFace::PosY, u, v)
        } else {
            // NegY: (u, -1, v) → u = x/(-y), v = z/(-y)
            let u = dir[0] / (-dir[1]);
            let v = dir[2] / (-dir[1]);
            (CubeFace::NegY, u, v)
        }
    } else if dir[2] > 0.0 {
        // PosZ: (u, v, 1) → u = x/z, v = y/z
        let u = dir[0] / dir[2];
        let v = dir[1] / dir[2];
        (CubeFace::PosZ, u, v)
    } else {
        // NegZ: (-u, v, -1) → u = -x/(-z) = x/z, v = y/(-z)
        let u = dir[0] / dir[2]; // dir[2] < 0, so x/z = -x/|z|
        let v = dir[1] / (-dir[2]);
        (CubeFace::NegZ, u, v)
    }
}

/// Chunks added and removed in a single frame update.
#[derive(Debug, Default)]
pub struct GridUpdate {
    /// Newly visible chunks with their 33×33 height samples.
    pub added: Vec<(ChunkKey, Vec<f32>)>,
    /// Chunks that left the grid and should be removed.
    pub removed: Vec<ChunkKey>,
}

/// Fixed-resolution 7×7 collision grid centered on the player.
pub struct CollisionGrid {
    /// The collision LOD level (fixed for the planet's lifetime).
    pub col_lod: u8,
    /// Currently active chunks with their height data.
    pub active_chunks: HashMap<ChunkKey, Vec<f32>>,
    /// Last grid center (face, tile_x, tile_y) used for hysteresis.
    last_center: Option<(CubeFace, u32, u32)>,
}

impl CollisionGrid {
    /// Create a new collision grid for the given planet config.
    pub fn new(config: &TerrainConfig) -> Self {
        let max_lod = max_lod_levels(config.radius_m * 1.57);
        let col_lod = collision_lod(max_lod, config.radius_m);
        Self {
            col_lod,
            active_chunks: HashMap::new(),
            last_center: None,
        }
    }

    /// Update the grid based on player position (planet-relative meters).
    ///
    /// Returns chunks to add/remove. Only triggers re-centering when the
    /// player has moved more than `COLLISION_GRID_HYSTERESIS` chunk widths.
    pub fn update(&mut self, player_pos: [f64; 3], config: &TerrainConfig) -> GridUpdate {
        // Normalise player position to unit sphere direction.
        let len = (player_pos[0] * player_pos[0]
            + player_pos[1] * player_pos[1]
            + player_pos[2] * player_pos[2])
            .sqrt();
        if len < 1.0 {
            return GridUpdate::default();
        }
        let dir = [player_pos[0] / len, player_pos[1] / len, player_pos[2] / len];

        let (face, u, v) = sphere_to_cube_face(dir);

        // Convert UV [-1,1] to tile coordinates at collision LOD.
        let tiles = 1u32 << self.col_lod;
        let tile_x = ((u + 1.0) * 0.5 * tiles as f64).floor() as u32;
        let tile_y = ((v + 1.0) * 0.5 * tiles as f64).floor() as u32;
        let tile_x = tile_x.min(tiles - 1);
        let tile_y = tile_y.min(tiles - 1);

        // Hysteresis: skip re-centering if movement is small.
        if let Some((last_face, lx, ly)) = self.last_center
            && last_face == face
        {
            let dx = (tile_x as f64 - lx as f64).abs();
            let dy = (tile_y as f64 - ly as f64).abs();
            if dx < COLLISION_GRID_HYSTERESIS && dy < COLLISION_GRID_HYSTERESIS {
                return GridUpdate::default();
            }
        }

        self.last_center = Some((face, tile_x, tile_y));

        // Build the desired 7×7 set of chunk keys.
        let half = (COLLISION_GRID_SIZE / 2) as i64;
        let mut desired: HashMap<ChunkKey, ()> = HashMap::new();

        for dy in -half..=half {
            for dx in -half..=half {
                let cx = tile_x as i64 + dx;
                let cy = tile_y as i64 + dy;

                // Skip chunks outside the face boundary.
                if cx < 0 || cx >= tiles as i64 || cy < 0 || cy >= tiles as i64 {
                    continue;
                }

                let key = ChunkKey {
                    face: face as u8,
                    lod: self.col_lod,
                    x: cx as u32,
                    y: cy as u32,
                };
                desired.insert(key, ());
            }
        }

        let mut update = GridUpdate::default();

        // Remove chunks no longer in the desired set.
        let old_keys: Vec<ChunkKey> = self.active_chunks.keys().copied().collect();
        for key in old_keys {
            if !desired.contains_key(&key) {
                self.active_chunks.remove(&key);
                update.removed.push(key);
            }
        }

        // Add new chunks not yet in the active set.
        for &key in desired.keys() {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.active_chunks.entry(key)
            {
                let heights = generate_collision_heights(key, config);
                update.added.push((key, heights.clone()));
                e.insert(heights);
            }
        }

        update
    }
}

/// Generate 33×33 height samples for a collision chunk.
///
/// Uses the same noise pipeline as the visual terrain so collision
/// surfaces match the rendered terrain exactly.
pub fn generate_collision_heights(key: ChunkKey, config: &TerrainConfig) -> Vec<f32> {
    let face = CubeFace::ALL[key.face as usize];
    let tiles = 1u32 << key.lod;
    let tile_size = 2.0 / tiles as f64;
    let u_start = -1.0 + key.x as f64 * tile_size;
    let v_start = -1.0 + key.y as f64 * tile_size;

    let noise = make_terrain_noise(config.noise_seed);
    let warp = make_warp_noise(config.noise_seed);
    let freq_scale = 2.0;
    let amplitude = config.radius_m * config.displacement_fraction as f64;

    let n = GRID_SIZE as usize;
    let mut heights = Vec::with_capacity(n * n);

    for row in 0..n {
        for col in 0..n {
            let u = u_start + col as f64 / CELLS as f64 * tile_size;
            let v = v_start + row as f64 / CELLS as f64 * tile_size;

            let dir = cube_to_sphere(face, u, v);
            let h = sample_height(&noise, &warp, dir, freq_scale);

            // Store absolute radius (meters from planet center) so the
            // integration layer can build colliders in world space.
            // Must match chunk.rs: centered around radius.
            let r = config.radius_m + (h as f64 - 0.5) * amplitude;
            heights.push(r as f32);
        }
    }

    heights
}

#[cfg(test)]
mod tests {
    use super::*;
    use sa_universe::PlanetSubType;

    fn test_config() -> TerrainConfig {
        TerrainConfig {
            radius_m: 6_371_000.0,
            noise_seed: 42,
            sub_type: PlanetSubType::Temperate,
            displacement_fraction: 0.02,
        }
    }

    #[test]
    fn collision_lod_produces_reasonable_chunk_width() {
        let config = test_config();
        let max_lod = crate::quadtree::max_lod_levels(config.radius_m * 1.57);
        let col_lod = collision_lod(max_lod, config.radius_m);
        let chunk_width = face_size_at_lod(config.radius_m, col_lod);
        assert!(
            chunk_width <= 200.0,
            "chunk width {chunk_width}m exceeds 200m"
        );
        assert!(chunk_width >= 10.0, "chunk width {chunk_width}m too small");
    }

    #[test]
    fn new_grid_starts_empty() {
        let config = test_config();
        let grid = CollisionGrid::new(&config);
        assert!(grid.active_chunks.is_empty());
    }

    #[test]
    fn first_update_generates_chunks() {
        let config = test_config();
        let mut grid = CollisionGrid::new(&config);
        let player_pos = [0.0, 0.0, config.radius_m];
        let update = grid.update(player_pos, &config);
        assert!(!update.added.is_empty());
        // May be less than 49 if near face edge, but should be substantial
        assert!(
            update.added.len() >= 25,
            "expected at least 25 chunks, got {}",
            update.added.len()
        );
    }

    #[test]
    fn small_movement_does_not_trigger_update() {
        let config = test_config();
        let mut grid = CollisionGrid::new(&config);
        let pos = [0.0, 0.0, config.radius_m];
        grid.update(pos, &config);
        let pos2 = [1.0, 0.0, config.radius_m];
        let update2 = grid.update(pos2, &config);
        assert!(update2.added.is_empty());
        assert!(update2.removed.is_empty());
    }

    #[test]
    fn heights_are_33x33() {
        let config = test_config();
        let mut grid = CollisionGrid::new(&config);
        let pos = [0.0, 0.0, config.radius_m];
        let update = grid.update(pos, &config);
        for (_key, heights) in &update.added {
            assert_eq!(heights.len(), (GRID_SIZE * GRID_SIZE) as usize);
        }
    }
}
