//! HeightField collider management for terrain chunks.
//!
//! Uses `CollisionGrid` from `sa_terrain` for fixed-LOD collision chunks
//! independent of visual LOD. The grid produces height data; this module
//! converts it into rapier3d HeightField colliders.

use std::collections::HashMap;

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;
use sa_terrain::collision_grid::{CollisionGrid, GridUpdate};
use sa_terrain::config::{COLLISION_REBASE_THRESHOLD_M, GRID_SIZE};
use sa_terrain::{ChunkKey, TerrainConfig};

use crate::ship_colliders;

/// Handles for all bodies that must be shifted during an anchor rebase.
/// Collected from main.rs and passed into the rebase function.
///
/// Note: the interior body is NOT included — it always stays at the rapier
/// origin (the player controller subtracts/adds ship_position). Shifting it
/// during rebase would be immediately undone by the interior sync code.
pub struct RebaseBodies {
    pub ship: Option<RigidBodyHandle>,
    pub player: Option<RigidBodyHandle>,
}

/// Terrain collider state — manages HeightField colliders attached to a static
/// rigid body in the physics world, driven by the `CollisionGrid`.
pub struct TerrainColliders {
    /// Static rigid body that parents all terrain colliders.
    pub terrain_body: Option<RigidBodyHandle>,
    /// HeightField collider handles keyed by chunk.
    pub colliders: HashMap<ChunkKey, ColliderHandle>,
    /// Physics anchor position in planet-relative meters (f64).
    pub anchor_f64: [f64; 3],
    /// Lazily initialized collision grid.
    collision_grid: Option<CollisionGrid>,
}

impl Default for TerrainColliders {
    fn default() -> Self {
        Self::new()
    }
}

impl TerrainColliders {
    pub fn new() -> Self {
        Self {
            terrain_body: None,
            colliders: HashMap::new(),
            anchor_f64: [0.0; 3],
            collision_grid: None,
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
        self.collision_grid = None;
    }

    /// Force an immediate anchor rebase to the current ship position,
    /// regardless of drift threshold. Ensures the rapier origin is fresh
    /// when collision first activates.
    pub fn force_rebase(
        &mut self,
        physics: &mut PhysicsWorld,
        rebase_bodies: &RebaseBodies,
    ) {
        let ship_rapier_pos = rebase_bodies
            .ship
            .and_then(|h| physics.rigid_body_set.get(h))
            .map(|b| *b.translation())
            .unwrap_or(nalgebra::Vector3::zeros());

        let drift = (ship_rapier_pos.x * ship_rapier_pos.x
            + ship_rapier_pos.y * ship_rapier_pos.y
            + ship_rapier_pos.z * ship_rapier_pos.z)
            .sqrt();

        if drift < 0.001 {
            return; // already at origin
        }

        let shift = -ship_rapier_pos;

        self.anchor_f64[0] += ship_rapier_pos.x as f64;
        self.anchor_f64[1] += ship_rapier_pos.y as f64;
        self.anchor_f64[2] += ship_rapier_pos.z as f64;

        for handle in [rebase_bodies.ship, rebase_bodies.player]
            .iter()
            .flatten()
        {
            if let Some(body) = physics.rigid_body_set.get_mut(*handle) {
                let t = body.translation();
                body.set_translation(
                    nalgebra::Vector3::new(
                        t.x + shift.x,
                        t.y + shift.y,
                        t.z + shift.z,
                    ),
                    true,
                );
            }
        }

        for handle in self.colliders.values() {
            if let Some(coll) = physics.collider_set.get_mut(*handle)
                && let Some(pos) = coll.position_wrt_parent()
            {
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

        physics.sync_collider_positions();
        physics.update_query_pipeline();

        log::info!(
            "Force rebase on collision entry: shift=({:.0},{:.0},{:.0})",
            shift.x, shift.y, shift.z,
        );
    }

    /// Update collision grid and manage HeightField colliders.
    ///
    /// Lazily creates the `CollisionGrid` on first call. Each frame, asks
    /// the grid for chunks to add/remove and converts them to rapier
    /// HeightField colliders.
    #[profiling::function]
    pub fn update_collision_grid(
        &mut self,
        cam_rel_m: [f64; 3],
        config: &TerrainConfig,
        physics: &mut PhysicsWorld,
        rebase_bodies: &RebaseBodies,
    ) {
        // On first call, initialize anchor to the camera position.
        if self.terrain_body.is_none() {
            self.anchor_f64 = cam_rel_m;
        }

        let terrain_body = *self.terrain_body.get_or_insert_with(|| {
            let rb = RigidBodyBuilder::fixed().build();
            physics.add_rigid_body(rb)
        });

        // Lazy init collision grid.
        let grid = self
            .collision_grid
            .get_or_insert_with(|| CollisionGrid::new(config));

        let GridUpdate { added, removed } = grid.update(cam_rel_m, config);

        // Remove colliders for chunks that left the grid.
        for key in &removed {
            if let Some(handle) = self.colliders.remove(key) {
                physics.remove_collider(handle);
            }
        }

        // Add colliders for newly visible chunks.
        for (key, heights) in &added {
            if let Some(handle) = build_heightfield_from_grid(
                physics,
                terrain_body,
                *key,
                heights,
                &self.anchor_f64,
                config.radius_m,
            ) {
                self.colliders.insert(*key, handle);
            }
        }

        // Anchor rebase: shift all bodies when ship drifts too far.
        let ship_rapier_pos = rebase_bodies
            .ship
            .and_then(|h| physics.rigid_body_set.get(h))
            .map(|b| *b.translation())
            .unwrap_or(nalgebra::Vector3::zeros());

        let ship_drift = (ship_rapier_pos.x * ship_rapier_pos.x
            + ship_rapier_pos.y * ship_rapier_pos.y
            + ship_rapier_pos.z * ship_rapier_pos.z)
            .sqrt();

        if ship_drift > COLLISION_REBASE_THRESHOLD_M as f32 {
            let shift = -ship_rapier_pos;

            self.anchor_f64[0] += ship_rapier_pos.x as f64;
            self.anchor_f64[1] += ship_rapier_pos.y as f64;
            self.anchor_f64[2] += ship_rapier_pos.z as f64;

            // Shift rigid bodies (ship + player).
            for handle in [rebase_bodies.ship, rebase_bodies.player]
                .iter()
                .flatten()
            {
                if let Some(body) = physics.rigid_body_set.get_mut(*handle) {
                    let t = body.translation();
                    body.set_translation(
                        nalgebra::Vector3::new(
                            t.x + shift.x,
                            t.y + shift.y,
                            t.z + shift.z,
                        ),
                        true,
                    );
                }
            }

            // Shift HeightField colliders.
            for handle in self.colliders.values() {
                if let Some(coll) = physics.collider_set.get_mut(*handle)
                    && let Some(pos) = coll.position_wrt_parent()
                {
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

            physics.sync_collider_positions();
            physics.update_query_pipeline();
        }

        if !self.colliders.is_empty() && (!added.is_empty() || !removed.is_empty()) {
            physics.sync_collider_positions();
            physics.update_query_pipeline();
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a HeightField collider from collision grid heights (absolute radii).
///
/// The collision grid stores heights as absolute radius values (meters from
/// planet center). We compute the chunk center direction, position, and
/// tangent frame from the chunk key's face/UV coordinates.
fn build_heightfield_from_grid(
    physics: &mut PhysicsWorld,
    terrain_body: RigidBodyHandle,
    key: ChunkKey,
    heights: &[f32],
    anchor: &[f64; 3],
    radius_m: f64,
) -> Option<ColliderHandle> {
    let gs = GRID_SIZE as usize;
    if heights.len() < gs * gs {
        return None;
    }

    // Heights are absolute radii. Compute average radius for centering.
    let avg_r: f64 = heights.iter().map(|h| *h as f64).sum::<f64>() / (gs * gs) as f64;

    // Chunk geometry from cube face coordinates.
    let face = sa_terrain::cube_sphere::CubeFace::ALL[key.face as usize];
    let tiles = 1u64 << key.lod;
    let u_center = -1.0 + (2.0 * key.x as f64 + 1.0) / tiles as f64;
    let v_center = -1.0 + (2.0 * key.y as f64 + 1.0) / tiles as f64;

    // Compute chunk center on sphere at the average radius.
    let center_dir = sa_terrain::cube_sphere::cube_to_sphere(face, u_center, v_center);
    let cx_world = center_dir[0] * avg_r;
    let cy_world = center_dir[1] * avg_r;
    let cz_world = center_dir[2] * avg_r;

    // Position relative to anchor.
    let cx = (cx_world - anchor[0]) as f32;
    let cy = (cy_world - anchor[1]) as f32;
    let cz = (cz_world - anchor[2]) as f32;

    // Chunk physical size: arc length at this LOD.
    let face_size_m = radius_m * std::f64::consts::FRAC_PI_2;
    let chunk_size_m = (face_size_m / tiles as f64) as f32;

    // Height scale: max deviation from average determines the HeightField
    // Y scale. Use the actual range to get precise collision.
    let min_h = heights.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_h = heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let height_range = (max_h - min_h).max(1.0); // prevent degenerate zero thickness

    // Rescale height matrix to [0,1] range for rapier (it multiplies by scale.y).
    let height_matrix_scaled = nalgebra::DMatrix::from_fn(gs, gs, |r, c| {
        (heights[r * gs + c] - min_h) / height_range
    });

    let scale = nalgebra::Vector3::new(chunk_size_m, height_range, chunk_size_m);

    // Tangent frame from cube face UV axes.
    let eps = 0.001;
    let u_dir_f64 =
        sa_terrain::cube_sphere::cube_to_sphere(face, u_center + eps, v_center);
    let v_dir_f64 =
        sa_terrain::cube_sphere::cube_to_sphere(face, u_center, v_center + eps);

    let tu = nalgebra::Vector3::new(
        (u_dir_f64[0] - center_dir[0]) as f32,
        (u_dir_f64[1] - center_dir[1]) as f32,
        (u_dir_f64[2] - center_dir[2]) as f32,
    )
    .normalize();
    let tv = nalgebra::Vector3::new(
        (v_dir_f64[0] - center_dir[0]) as f32,
        (v_dir_f64[1] - center_dir[1]) as f32,
        (v_dir_f64[2] - center_dir[2]) as f32,
    )
    .normalize();

    let normal = tu.cross(&tv).normalize();
    let tv_ortho = normal.cross(&tu).normalize();

    let rotation = nalgebra::UnitQuaternion::from_rotation_matrix(
        &nalgebra::Rotation3::from_matrix_unchecked(nalgebra::Matrix3::from_columns(&[
            tu, normal, tv_ortho,
        ])),
    );

    // Offset the center vertically: rapier HeightField center is at
    // scale.y * 0.5 above the base. We placed center at avg_r, but with
    // the [0,1] rescaled matrix, the base is at min_h and top at max_h.
    // The HeightField center in local Y is at (min_h + max_h) / 2.
    // We need to offset from avg_r to (min_h + max_h) / 2 along the normal.
    let mid_h = (min_h as f64 + max_h as f64) * 0.5;
    let h_offset = (mid_h - avg_r) as f32;
    let final_cx = cx + normal.x * h_offset;
    let final_cy = cy + normal.y * h_offset;
    let final_cz = cz + normal.z * h_offset;

    let position = nalgebra::Isometry3::from_parts(
        nalgebra::Translation3::new(final_cx, final_cy, final_cz),
        rotation,
    );

    let groups = InteractionGroups::new(
        ship_colliders::TERRAIN,
        ship_colliders::PLAYER
            .union(ship_colliders::SHIP_HULL)
            .union(ship_colliders::SHIP_EXTERIOR),
    );

    let collider = ColliderBuilder::heightfield(height_matrix_scaled, scale)
        .collision_groups(groups)
        .friction(0.8)
        .position(position)
        .build();

    Some(physics.add_collider(collider, terrain_body))
}
