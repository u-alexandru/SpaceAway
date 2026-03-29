//! HeightField collider management for terrain chunks.
//!
//! Creates rapier3d HeightField colliders for terrain chunks near the camera,
//! oriented to the local tangent plane of the sphere surface.

use std::collections::HashMap;

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;
use sa_terrain::chunk::GRID_SIZE;
use sa_terrain::ChunkKey;

use crate::ship_colliders;

/// Only create HeightField colliders for chunks within this distance (meters).
const COLLIDER_RANGE_M: f64 = 500.0;

/// Shift all collider positions when the ship moves this far from the anchor.
const ANCHOR_REBASE_THRESHOLD_M: f64 = 100.0;

/// Minimal data retained per chunk for collider management.
pub(crate) struct CachedChunk {
    pub center_f64: [f64; 3],
    pub heights: Vec<f32>,
    pub lod: u8,
}

/// Terrain collider state — manages HeightField colliders attached to a static
/// rigid body in the physics world.
pub(crate) struct TerrainColliders {
    /// Static rigid body that parents all terrain colliders.
    pub terrain_body: Option<RigidBodyHandle>,
    /// HeightField collider handles keyed by chunk.
    pub colliders: HashMap<ChunkKey, ColliderHandle>,
    /// Physics anchor position in planet-relative meters (f64).
    pub anchor_f64: [f64; 3],
    /// Chunk data retained for collider creation (heights + center).
    pub chunk_cache: HashMap<ChunkKey, CachedChunk>,
}

impl TerrainColliders {
    pub fn new() -> Self {
        Self {
            terrain_body: None,
            colliders: HashMap::new(),
            anchor_f64: [0.0; 3],
            chunk_cache: HashMap::new(),
        }
    }

    /// Remove all colliders and the terrain rigid body from physics.
    pub fn cleanup(&mut self, physics: &mut PhysicsWorld) {
        for handle in self.colliders.values() {
            physics.remove_collider(*handle);
        }
        self.colliders.clear();
        if let Some(bh) = self.terrain_body.take() {
            physics.remove_rigid_body(bh);
        }
    }

    /// Remove colliders for the given evicted chunk keys.
    pub fn remove_evicted(
        &mut self,
        physics: &mut PhysicsWorld,
        removed_keys: &[ChunkKey],
    ) {
        for key in removed_keys {
            if let Some(handle) = self.colliders.remove(key) {
                physics.remove_collider(handle);
            }
            self.chunk_cache.remove(key);
        }
    }

    /// Cache chunk data for future collider creation.
    pub fn cache_chunk(&mut self, key: ChunkKey, chunk: &sa_terrain::ChunkData) {
        self.chunk_cache.insert(key, CachedChunk {
            center_f64: chunk.center_f64,
            heights: chunk.heights.clone(),
            lod: key.lod,
        });
    }

    /// Update HeightField colliders for chunks near the camera.
    pub fn update(
        &mut self,
        physics: &mut PhysicsWorld,
        cam_rel_m: [f64; 3],
        radius_m: f64,
        max_displacement_m: f64,
    ) {
        let terrain_body = *self.terrain_body.get_or_insert_with(|| {
            let rb = RigidBodyBuilder::fixed().build();
            physics.add_rigid_body(rb)
        });

        // Anchor rebasing.
        let dx = cam_rel_m[0] - self.anchor_f64[0];
        let dy = cam_rel_m[1] - self.anchor_f64[1];
        let dz = cam_rel_m[2] - self.anchor_f64[2];
        let drift = (dx * dx + dy * dy + dz * dz).sqrt();

        if drift > ANCHOR_REBASE_THRESHOLD_M && !self.colliders.is_empty() {
            self.anchor_f64 = cam_rel_m;
            for handle in self.colliders.values() {
                physics.remove_collider(*handle);
            }
            self.colliders.clear();
        }

        // Remove out-of-range colliders.
        let keys_to_remove: Vec<ChunkKey> = self
            .colliders
            .keys()
            .filter(|key| {
                self.chunk_cache
                    .get(key)
                    .map(|c| chunk_dist(c.center_f64, cam_rel_m) > COLLIDER_RANGE_M * 1.2)
                    .unwrap_or(true)
            })
            .copied()
            .collect();

        for key in &keys_to_remove {
            if let Some(handle) = self.colliders.remove(key) {
                physics.remove_collider(handle);
            }
        }

        // Add colliders for nearby chunks without one.
        let keys_to_add: Vec<ChunkKey> = self
            .chunk_cache
            .iter()
            .filter(|(key, cached)| {
                !self.colliders.contains_key(key)
                    && chunk_dist(cached.center_f64, cam_rel_m) < COLLIDER_RANGE_M
            })
            .map(|(key, _)| *key)
            .collect();

        for key in keys_to_add {
            if let Some(cached) = self.chunk_cache.get(&key) {
                if let Some(handle) = build_heightfield(
                    physics,
                    terrain_body,
                    cached,
                    &self.anchor_f64,
                    radius_m,
                    max_displacement_m,
                ) {
                    self.colliders.insert(key, handle);
                }
            }
        }

        if !self.colliders.is_empty() {
            physics.sync_collider_positions();
            physics.update_query_pipeline();
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chunk_dist(center: [f64; 3], cam: [f64; 3]) -> f64 {
    let dx = center[0] - cam[0];
    let dy = center[1] - cam[1];
    let dz = center[2] - cam[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Build a HeightField collider for one chunk, oriented to the tangent plane.
fn build_heightfield(
    physics: &mut PhysicsWorld,
    terrain_body: RigidBodyHandle,
    cached: &CachedChunk,
    anchor: &[f64; 3],
    radius_m: f64,
    max_displacement_m: f64,
) -> Option<ColliderHandle> {
    let gs = GRID_SIZE as usize;
    if cached.heights.len() < gs * gs {
        return None;
    }

    // Heights are raw noise [0,1]. Chunk center is already displaced by avg_h * amplitude.
    // HeightField needs heights relative to center, so subtract average to avoid double-counting.
    let avg_h: f32 = cached.heights.iter().sum::<f32>() / (gs * gs) as f32;
    let heights =
        nalgebra::DMatrix::from_fn(gs, gs, |r, c| cached.heights[r * gs + c] - avg_h);

    let face_size_m = radius_m * std::f64::consts::FRAC_PI_2;
    let chunk_size_m = (face_size_m / (1u64 << cached.lod) as f64) as f32;
    let max_disp = max_displacement_m as f32;
    let scale = nalgebra::Vector3::new(chunk_size_m, max_disp, chunk_size_m);

    // Chunk center relative to anchor.
    let cx = (cached.center_f64[0] - anchor[0]) as f32;
    let cy = (cached.center_f64[1] - anchor[1]) as f32;
    let cz = (cached.center_f64[2] - anchor[2]) as f32;

    // Surface normal = normalized chunk center direction.
    let len = (cached.center_f64[0].powi(2)
        + cached.center_f64[1].powi(2)
        + cached.center_f64[2].powi(2))
    .sqrt();
    if len < 1.0 {
        return None;
    }
    let center_dir = nalgebra::Vector3::new(
        (cached.center_f64[0] / len) as f32,
        (cached.center_f64[1] / len) as f32,
        (cached.center_f64[2] / len) as f32,
    );

    // Rotate heightfield Y-up to match surface normal.
    let up = nalgebra::Vector3::y();
    let rotation = if (center_dir - up).norm() < 1e-4 {
        nalgebra::UnitQuaternion::identity()
    } else if (center_dir + up).norm() < 1e-4 {
        nalgebra::UnitQuaternion::from_axis_angle(
            &nalgebra::Vector3::x_axis(),
            std::f32::consts::PI,
        )
    } else {
        nalgebra::UnitQuaternion::rotation_between(&up, &center_dir)
            .unwrap_or_else(nalgebra::UnitQuaternion::identity)
    };

    let position = nalgebra::Isometry3::from_parts(
        nalgebra::Translation3::new(cx, cy, cz),
        rotation,
    );

    let groups = InteractionGroups::new(
        ship_colliders::TERRAIN,
        ship_colliders::PLAYER.union(ship_colliders::SHIP_HULL),
    );

    let collider = ColliderBuilder::heightfield(heights, scale)
        .collision_groups(groups)
        .position(position)
        .build();

    Some(physics.add_collider(collider, terrain_body))
}
