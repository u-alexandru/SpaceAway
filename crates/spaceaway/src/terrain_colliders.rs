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
        // On first call, initialize anchor so that planet-relative positions
        // map correctly into the existing rapier coordinate space.
        //
        // The ship is already at some rapier position (e.g. (60, -59, -30))
        // from prior physics. cam_rel_m is the ship/camera's true planet-
        // relative position. The anchor must satisfy:
        //   ship_rapier_pos = cam_rel_m - anchor
        // so:
        //   anchor = cam_rel_m - ship_rapier_pos
        //
        // This ensures colliders placed at (chunk - anchor) end up at the
        // correct rapier position relative to the ship body.
        if self.terrain_body.is_none() {
            let ship_rapier = rebase_bodies
                .ship
                .and_then(|h| physics.rigid_body_set.get(h))
                .map(|b| *b.translation())
                .unwrap_or(nalgebra::Vector3::zeros());
            self.anchor_f64 = [
                cam_rel_m[0] - ship_rapier.x as f64,
                cam_rel_m[1] - ship_rapier.y as f64,
                cam_rel_m[2] - ship_rapier.z as f64,
            ];
            log::info!(
                "Collision anchor init: ship_rapier=({:.1},{:.1},{:.1}), \
                 cam_rel_m=({:.0},{:.0},{:.0}), anchor=({:.0},{:.0},{:.0})",
                ship_rapier.x, ship_rapier.y, ship_rapier.z,
                cam_rel_m[0], cam_rel_m[1], cam_rel_m[2],
                self.anchor_f64[0], self.anchor_f64[1], self.anchor_f64[2],
            );
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
    // Use tu × normal (not normal × tu) to get a right-handed frame.
    // normal × tu gives a left-handed frame (det = -1) for some faces,
    // producing an improper rotation that shrinks the collider's AABB
    // and creates gaps between adjacent heightfields.
    let tv_ortho = tu.cross(&normal).normalize();

    let rotation = nalgebra::UnitQuaternion::from_rotation_matrix(
        &nalgebra::Rotation3::from_matrix_unchecked(nalgebra::Matrix3::from_columns(&[
            tu, normal, tv_ortho,
        ])),
    );

    // Rapier HeightField: local Y=0 is at the isometry position,
    // heights go upward. With [0,1] rescaled values, Y=0 corresponds
    // to min_h and Y=height_range to max_h. We need Y=0 at min_h's
    // radial position, so offset from avg_r (where cx/cy/cz sits)
    // down to min_h along the surface normal.
    let h_offset = (min_h as f64 - avg_r) as f32;
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

    log::debug!(
        "HF collider: face={} lod={} x={} y={}, \
         avg_r={:.0}, min_h={:.0}, max_h={:.0}, range={:.1}, \
         rapier_pos=({:.1},{:.1},{:.1}), chunk_size={:.0}",
        key.face, key.lod, key.x, key.y,
        avg_r, min_h, max_h, height_range,
        final_cx, final_cy, final_cz, chunk_size_m,
    );

    let collider = ColliderBuilder::heightfield(height_matrix_scaled, scale)
        .collision_groups(groups)
        .friction(0.8)
        .position(position)
        .build();

    Some(physics.add_collider(collider, terrain_body))
}
