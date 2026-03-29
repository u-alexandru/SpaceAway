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
    pub face: u8,
    pub grid_x: u32,
    pub grid_y: u32,
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
            face: key.face,
            grid_x: key.x,
            grid_y: key.y,
        });
    }

    /// Update HeightField colliders for chunks near the camera.
    pub fn update(
        &mut self,
        physics: &mut PhysicsWorld,
        cam_rel_m: [f64; 3],
        radius_m: f64,
        max_displacement_m: f64,
        visible_keys: &std::collections::HashSet<ChunkKey>,
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
            // Shift existing collider positions instead of destroying them.
            // This preserves collision during high-speed descent.
            let old_anchor = self.anchor_f64;
            self.anchor_f64 = cam_rel_m;
            let shift = nalgebra::Vector3::new(
                (old_anchor[0] - cam_rel_m[0]) as f32,
                (old_anchor[1] - cam_rel_m[1]) as f32,
                (old_anchor[2] - cam_rel_m[2]) as f32,
            );
            for handle in self.colliders.values() {
                if let Some(coll) = physics.collider_set.get_mut(*handle) {
                    if let Some(pos) = coll.position_wrt_parent() {
                        let new_pos = nalgebra::Isometry3::from_parts(
                            nalgebra::Translation3::new(
                                pos.translation.x + shift.x,
                                pos.translation.y + shift.y,
                                pos.translation.z + shift.z,
                            ),
                            pos.rotation,
                        );
                        coll.set_position_wrt_parent(new_pos);
                    }
                }
            }
            physics.sync_collider_positions();
            physics.update_query_pipeline();
        }

        // Remove colliders that are out-of-range OR no longer in the visible set.
        // The visible set check prevents overlapping LOD colliders.
        let keys_to_remove: Vec<ChunkKey> = self
            .colliders
            .keys()
            .filter(|key| {
                !visible_keys.contains(key)
                    || self.chunk_cache
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

        // Add colliders only for chunks in the CURRENT visible set that are nearby.
        // This prevents overlapping colliders from different LOD levels for the
        // same area (coarse + fine LOD chunks both in cache near a LOD boundary).
        // Minimum LOD for collision: chunks must be fine enough that the
        // flat HeightField approximation is accurate. At LOD 10, chunk covers
        // ~10km — above this, curvature error is too large for reliable collision.
        let min_collider_lod: u8 = 10;

        let keys_to_add: Vec<ChunkKey> = self
            .chunk_cache
            .iter()
            .filter(|(key, cached)| {
                !self.colliders.contains_key(key)
                    && visible_keys.contains(key)
                    && key.lod >= min_collider_lod
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
    // Clamp min displacement to 1m to prevent degenerate zero-thickness HeightField.
    let max_disp = (max_displacement_m as f32).max(1.0);
    let scale = nalgebra::Vector3::new(chunk_size_m, max_disp, chunk_size_m);

    // Chunk center relative to anchor.
    let cx = (cached.center_f64[0] - anchor[0]) as f32;
    let cy = (cached.center_f64[1] - anchor[1]) as f32;
    let cz = (cached.center_f64[2] - anchor[2]) as f32;

    // Compute tangent frame from the cube face UV axes.
    // HeightField X-axis must align with the chunk's U direction,
    // HeightField Z-axis must align with the chunk's V direction,
    // HeightField Y-axis (height) must align with the surface normal.
    //
    // We sample two nearby sphere points to get tangent vectors,
    // then build a rotation matrix from the resulting frame.
    let subdivs = 1u64 << cached.lod;
    let u_center = -1.0 + (2.0 * cached.grid_x as f64 + 1.0) / subdivs as f64;
    let v_center = -1.0 + (2.0 * cached.grid_y as f64 + 1.0) / subdivs as f64;
    let face = sa_terrain::cube_sphere::CubeFace::ALL[cached.face as usize];

    let eps = 0.001;
    let center_dir_f64 = sa_terrain::cube_sphere::cube_to_sphere(face, u_center, v_center);
    let u_dir_f64 = sa_terrain::cube_sphere::cube_to_sphere(face, u_center + eps, v_center);
    let v_dir_f64 = sa_terrain::cube_sphere::cube_to_sphere(face, u_center, v_center + eps);

    // Tangent U = normalize(u_dir - center_dir)
    let tu = nalgebra::Vector3::new(
        (u_dir_f64[0] - center_dir_f64[0]) as f32,
        (u_dir_f64[1] - center_dir_f64[1]) as f32,
        (u_dir_f64[2] - center_dir_f64[2]) as f32,
    ).normalize();
    // Tangent V = normalize(v_dir - center_dir)
    let tv = nalgebra::Vector3::new(
        (v_dir_f64[0] - center_dir_f64[0]) as f32,
        (v_dir_f64[1] - center_dir_f64[1]) as f32,
        (v_dir_f64[2] - center_dir_f64[2]) as f32,
    ).normalize();
    // Gram-Schmidt orthogonalize: tu and tv from finite differences are
    // not exactly orthogonal due to cube-to-sphere distortion.
    let normal = tu.cross(&tv).normalize();
    let tv_ortho = normal.cross(&tu).normalize();

    // Build rotation: columns are the axes of the rotated frame.
    // HeightField local X → tu (chunk U axis)
    // HeightField local Y → normal (outward from sphere)
    // HeightField local Z → tv_ortho (chunk V axis, orthogonalized)
    let rotation = nalgebra::UnitQuaternion::from_rotation_matrix(
        &nalgebra::Rotation3::from_matrix_unchecked(
            nalgebra::Matrix3::from_columns(&[tu, normal, tv_ortho])
        )
    );

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
