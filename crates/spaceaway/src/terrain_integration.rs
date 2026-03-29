//! Terrain integration: activates CDLOD terrain when camera approaches a
//! landable planet, streams chunks, uploads meshes to GPU, computes gravity,
//! and manages HeightField colliders for physics.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use sa_core::Handle;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use sa_render::{DrawCommand, MeshData, MeshMarker, MeshStore, Vertex};
use sa_terrain::quadtree::{max_lod_levels, select_visible_nodes};
use sa_terrain::streaming::ChunkStreaming;
use sa_terrain::{ChunkKey, TerrainConfig};
use sa_universe::PlanetSubType;

use crate::solar_system::ActiveSystem;
use crate::terrain_colliders::TerrainColliders;

/// Light-years to meters conversion factor.
const LY_TO_M: f64 = 9.461e15;

/// Terrain activates when camera is within this multiple of the planet radius.
/// At 2.0× radius the initial LOD is coarse (LOD 0-2 panels), but streaming
/// fills finer chunks within seconds as the player approaches. This wide zone
/// ensures terrain activates before the player reaches the icosphere surface,
/// even at cruise speeds (1c ≈ 4,800 km/frame).
const ACTIVATE_RADIUS_MULT: f64 = 2.0;

/// Terrain deactivates with hysteresis to prevent toggling. The 0.5× gap
/// between activation (2.0×) and deactivation (2.5×) is wide enough that
/// orbital drift and cruise overshoot don't cause rapid toggling.
const DEACTIVATE_RADIUS_MULT: f64 = 2.5;

// ---------------------------------------------------------------------------
// TerrainFrameResult
// ---------------------------------------------------------------------------

/// Per-frame output from the terrain manager.
pub struct TerrainFrameResult {
    /// Draw commands for all visible terrain chunks (pre-rebased).
    pub draw_commands: Vec<DrawCommand>,
    /// Body index to hide in the solar system renderer (the icosphere).
    pub hidden_body_index: Option<usize>,
    /// Blended gravity state (ship <-> planet transition).
    pub gravity: Option<sa_terrain::gravity::GravityState>,
}

// ---------------------------------------------------------------------------
// TerrainManager
// ---------------------------------------------------------------------------

/// Owns the streaming pipeline and GPU mesh handles for one planet's terrain.
pub struct TerrainManager {
    streaming: ChunkStreaming,
    config: TerrainConfig,
    /// GPU handles and chunk centers keyed by chunk.
    /// The center_f64 is the actual displaced chunk center (not the quadtree node center).
    gpu_meshes: HashMap<ChunkKey, (Handle<MeshMarker>, [f64; 3])>,
    /// Planet center in light-years (updated each frame from orbital position).
    planet_center_ly: WorldPos,
    /// Body index in the ActiveSystem (for icosphere hiding).
    body_index: usize,
    /// Maximum LOD level for this planet.
    max_lod: u8,
    /// Maximum terrain displacement in meters.
    max_displacement_m: f64,
    /// Planet surface gravity in m/s^2.
    surface_gravity_ms2: f32,
    /// Collider management state.
    col: TerrainColliders,
    /// Frame counter for periodic diagnostics.
    diag_frame: u64,
}

impl TerrainManager {
    /// Create a new terrain manager for a planet.
    pub fn new(
        config: TerrainConfig,
        planet_center_ly: WorldPos,
        body_index: usize,
        surface_gravity_ms2: f32,
    ) -> Self {
        let face_size_m = config.radius_m * std::f64::consts::FRAC_PI_2;
        let max_lod = max_lod_levels(face_size_m);
        let max_displacement_m =
            config.radius_m * config.displacement_fraction as f64;

        let streaming = ChunkStreaming::new(config.clone());

        Self {
            streaming,
            config,
            gpu_meshes: HashMap::new(),
            planet_center_ly,
            body_index,
            max_lod,
            max_displacement_m,
            surface_gravity_ms2,
            col: TerrainColliders::new(),
            diag_frame: 0,
        }
    }

    /// Surface gravity of the planet in m/s^2.
    #[allow(dead_code)]
    pub fn surface_gravity(&self) -> f32 {
        self.surface_gravity_ms2
    }

    /// Run one frame of terrain streaming and produce draw commands.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        camera_galactic_ly: WorldPos,
        planet_center_ly: WorldPos,
        mesh_store: &mut MeshStore,
        device: &wgpu::Device,
        physics: &mut PhysicsWorld,
        ship_down: [f32; 3],
        ship_physics_pos: [f32; 3],
    ) -> TerrainFrameResult {
        // DO NOT update planet_center_ly from orbital motion.
        // The planet orbits with TIME_SCALE=30 which can move it out of
        // the terrain activation zone within 1 second. Since the icosphere
        // is hidden, the terrain IS the planet — it stays at the activation
        // position. Galactic_position is also frozen in impulse mode, so
        // both reference points are stable.
        let _ = planet_center_ly; // unused — we keep the activation-time position

        let cam_rel_m = [
            (camera_galactic_ly.x - self.planet_center_ly.x) * LY_TO_M,
            (camera_galactic_ly.y - self.planet_center_ly.y) * LY_TO_M,
            (camera_galactic_ly.z - self.planet_center_ly.z) * LY_TO_M,
        ];

        let visible = select_visible_nodes(
            cam_rel_m,
            self.config.radius_m,
            self.max_lod,
            self.max_displacement_m,
        );

        let (new_chunks, removed_keys) =
            self.streaming.update(&visible, &self.config);

        // Upload newly generated chunks to GPU and cache for colliders.
        for chunk in &new_chunks {
            let is_new = !self.gpu_meshes.contains_key(&chunk.key);
            self.gpu_meshes.entry(chunk.key).or_insert_with(|| {
                let mesh_data = chunk_to_mesh_data(chunk);
                let handle = mesh_store.upload(device, &mesh_data);
                (handle, chunk.center_f64)
            });
            self.col.cache_chunk(chunk.key, chunk);
            if is_new && self.gpu_meshes.len() <= 3 {
                // Diagnostic: log first few chunk positions to verify radius
                let c = chunk.center_f64;
                let dist_from_center = (c[0]*c[0] + c[1]*c[1] + c[2]*c[2]).sqrt();
                log::info!("Terrain chunk LOD={} center dist={:.0}m (expected ~{:.0}m), pos=({:.0},{:.0},{:.0})",
                    chunk.key.lod, dist_from_center, self.config.radius_m,
                    c[0], c[1], c[2]);
            }
        }

        // Remove GPU meshes for chunks evicted from the LRU cache.
        // These are truly gone and won't be re-used without re-generation.
        for key in &removed_keys {
            self.gpu_meshes.remove(key);
        }
        self.col.remove_evicted(physics, &removed_keys);

        // --- Gravity computation ---
        let gravity = sa_terrain::gravity::compute_gravity(
            cam_rel_m,
            ship_down,
            self.config.radius_m,
            self.surface_gravity_ms2,
            9.81,
        );

        // --- Collider management ---
        // Pass the current visible set to prevent overlapping LOD colliders.
        let visible_keys: std::collections::HashSet<sa_terrain::ChunkKey> = visible.iter()
            .map(|n| sa_terrain::ChunkKey {
                face: n.face as u8,
                lod: n.lod,
                x: n.x,
                y: n.y,
            })
            .collect();
        self.col.update(
            physics,
            cam_rel_m,
            self.config.radius_m,
            self.max_displacement_m,
            &visible_keys,
            ship_physics_pos,
        );

        // --- Collision diagnostic (every 60 frames) ---
        self.diag_frame += 1;
        if self.diag_frame.is_multiple_of(60) {
            let cam_dist = (cam_rel_m[0] * cam_rel_m[0]
                + cam_rel_m[1] * cam_rel_m[1]
                + cam_rel_m[2] * cam_rel_m[2]).sqrt();
            let altitude_km = (cam_dist - self.config.radius_m) / 1000.0;

            // Sphere barrier diagnostic: log its world position + distance to ship
            if let Some(sh) = self.col.sphere_barrier
                && let Some(coll) = physics.collider_set.get(sh)
            {
                let world = coll.position().translation;
                let dx = world.x - ship_physics_pos[0];
                let dy = world.y - ship_physics_pos[1];
                let dz = world.z - ship_physics_pos[2];
                let dist_to_center = (dx * dx + dy * dy + dz * dz).sqrt();
                let barrier_gap = dist_to_center - self.config.radius_m as f32;
                log::info!(
                    "COLLISION_DIAG: altitude={:.1}km, ship_phys=({:.0},{:.0},{:.0}), \
                     barrier_center=({:.0},{:.0},{:.0}), dist_to_barrier_center={:.0}m, \
                     barrier_gap={:.0}m, heightfield_colliders={}, cam_rel_mag={:.0}m",
                    altitude_km,
                    ship_physics_pos[0], ship_physics_pos[1], ship_physics_pos[2],
                    world.x, world.y, world.z,
                    dist_to_center, barrier_gap,
                    self.col.colliders.len(),
                    cam_dist,
                );
            }
        }

        // Build draw commands using frozen planet position.
        let draw_commands = self.build_draw_commands(
            &visible,
            self.planet_center_ly,
            camera_galactic_ly,
        );

        // Don't hide the icosphere until terrain has enough chunks for visual
        // coverage. With fewer than 6 chunks (one per cube face), the terrain
        // sphere has visible gaps and the transition looks like the planet
        // disappearing and being replaced by floating panels.
        let hide_icosphere = self.gpu_meshes.len() >= 6;

        TerrainFrameResult {
            draw_commands,
            hidden_body_index: if hide_icosphere { Some(self.body_index) } else { None },
            gravity: Some(gravity),
        }
    }

    /// Remove all terrain colliders and the terrain rigid body from physics.
    pub fn cleanup(&mut self, physics: &mut PhysicsWorld) {
        self.col.cleanup(physics);
    }

    /// Terrain rigid body handle (for repositioning before physics step).
    pub fn terrain_body_handle(&self) -> Option<rapier3d::prelude::RigidBodyHandle> {
        self.col.terrain_body
    }

    /// Compute the correct terrain body position for a given ship rapier
    /// position and camera galactic position.
    ///
    /// The terrain body must be offset from the ship by the drift between
    /// `cam_rel_m` (current camera-to-planet displacement) and the collider
    /// anchor. Without this correction, terrain colliders move with the
    /// ship between anchor rebases, preventing collision.
    pub fn corrected_terrain_body_pos(
        &self,
        camera_galactic_ly: WorldPos,
        ship_physics_pos: [f32; 3],
    ) -> [f32; 3] {
        let cam_rel_m = [
            (camera_galactic_ly.x - self.planet_center_ly.x) * LY_TO_M,
            (camera_galactic_ly.y - self.planet_center_ly.y) * LY_TO_M,
            (camera_galactic_ly.z - self.planet_center_ly.z) * LY_TO_M,
        ];
        let anchor = self.col.anchor_f64;
        [
            ship_physics_pos[0] - (cam_rel_m[0] - anchor[0]) as f32,
            ship_physics_pos[1] - (cam_rel_m[1] - anchor[1]) as f32,
            ship_physics_pos[2] - (cam_rel_m[2] - anchor[2]) as f32,
        ]
    }

    /// Planet radius in meters.
    #[allow(dead_code)]
    pub fn planet_radius_m(&self) -> f64 {
        self.config.radius_m
    }

    /// Current cam_rel_m: camera displacement from planet center in meters.
    #[allow(dead_code)]
    pub fn cam_rel_m(&self, camera_galactic_ly: WorldPos) -> [f64; 3] {
        [
            (camera_galactic_ly.x - self.planet_center_ly.x) * LY_TO_M,
            (camera_galactic_ly.y - self.planet_center_ly.y) * LY_TO_M,
            (camera_galactic_ly.z - self.planet_center_ly.z) * LY_TO_M,
        ]
    }

    /// Returns true when the camera has moved far enough to deactivate terrain.
    pub fn should_deactivate(&self, camera_ly: WorldPos) -> bool {
        let dx = (camera_ly.x - self.planet_center_ly.x) * LY_TO_M;
        let dy = (camera_ly.y - self.planet_center_ly.y) * LY_TO_M;
        let dz = (camera_ly.z - self.planet_center_ly.z) * LY_TO_M;
        let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();
        let threshold = self.config.radius_m * DEACTIVATE_RADIUS_MULT;
        let should = dist_m > threshold;
        if !should && dist_m > threshold * 0.5 {
            log::debug!("Terrain deactivation check: dist={:.0}km, threshold={:.0}km, deactivate={}",
                dist_m / 1000.0, threshold / 1000.0, should);
        }
        should
    }

    /// Body index this terrain replaces.
    pub fn body_index(&self) -> usize {
        self.body_index
    }

    fn build_draw_commands(
        &self,
        visible: &[sa_terrain::quadtree::VisibleNode],
        planet_center_ly: WorldPos,
        camera_galactic_ly: WorldPos,
    ) -> Vec<DrawCommand> {
        // Compute planet-to-camera offset in f64 BEFORE scaling to meters.
        // This avoids catastrophic cancellation: (planet_m + center - camera_m)
        // would lose center precision at galactic scale (~20m jitter at 10,000 ly).
        // Instead: (planet_ly - camera_ly) * LY_TO_M + center_f64.
        let cam_offset_m = [
            (planet_center_ly.x - camera_galactic_ly.x) * LY_TO_M,
            (planet_center_ly.y - camera_galactic_ly.y) * LY_TO_M,
            (planet_center_ly.z - camera_galactic_ly.z) * LY_TO_M,
        ];

        let mut cmds = Vec::with_capacity(visible.len());
        for node in visible {
            let key = ChunkKey {
                face: node.face as u8,
                lod: node.lod,
                x: node.x,
                y: node.y,
            };
            if let Some(&(handle, center_f64)) = self.gpu_meshes.get(&key) {
                let ox = (cam_offset_m[0] + center_f64[0]) as f32;
                let oy = (cam_offset_m[1] + center_f64[1]) as f32;
                let oz = (cam_offset_m[2] + center_f64[2]) as f32;

                cmds.push(DrawCommand {
                    mesh: handle,
                    model_matrix: Mat4::from_translation(Vec3::new(ox, oy, oz)),
                    pre_rebased: true,
                });
            }
        }
        cmds
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a `ChunkData` into a `MeshData` suitable for GPU upload.
fn chunk_to_mesh_data(chunk: &sa_terrain::ChunkData) -> MeshData {
    let vertices = chunk
        .vertices
        .iter()
        .map(|tv| Vertex {
            position: tv.position,
            color: tv.color,
            normal: tv.normal,
        })
        .collect();

    MeshData {
        vertices,
        indices: chunk.indices.clone(),
    }
}

/// Returns true if the sub-type represents a landable (rocky) surface.
fn is_landable(sub_type: PlanetSubType) -> bool {
    matches!(
        sub_type,
        PlanetSubType::Molten
            | PlanetSubType::Barren
            | PlanetSubType::Desert
            | PlanetSubType::Temperate
            | PlanetSubType::Ocean
            | PlanetSubType::Frozen
    )
}

/// Find the nearest landable planet within activation range.
///
/// Returns `(body_index, planet_center_ly, TerrainConfig, surface_gravity_ms2)`
/// if a planet qualifies, or `None` if no planet is close enough.
pub fn find_terrain_planet(
    active_system: &ActiveSystem,
    camera_ly: WorldPos,
) -> Option<(usize, WorldPos, TerrainConfig, f32)> {
    let positions = active_system.compute_positions_ly_pub();

    let mut best: Option<(usize, f64, WorldPos)> = None;

    for (i, pos) in positions.iter().enumerate() {
        let radius_m = match active_system.body_radius_m(i) {
            Some(r) => r,
            None => continue,
        };

        let (_color_seed, sub_type, _disp_frac, _mass, _re) =
            match active_system.planet_data(i) {
                Some(data) => data,
                None => continue,
            };

        if !is_landable(sub_type) {
            continue;
        }

        let dx = (camera_ly.x - pos.x) * LY_TO_M;
        let dy = (camera_ly.y - pos.y) * LY_TO_M;
        let dz = (camera_ly.z - pos.z) * LY_TO_M;
        let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist_m > radius_m * ACTIVATE_RADIUS_MULT {
            continue;
        }

        let dominated = match &best {
            Some((_, best_dist, _)) => dist_m < *best_dist,
            None => true,
        };
        if dominated {
            best = Some((i, dist_m, *pos));
        }
    }

    let (body_idx, _dist, center_ly) = best?;

    let radius_m = active_system.body_radius_m(body_idx)?;
    let (color_seed, sub_type, disp_frac, mass_earth, radius_earth) =
        active_system.planet_data(body_idx)?;

    let config = TerrainConfig {
        radius_m,
        noise_seed: color_seed,
        sub_type,
        displacement_fraction: disp_frac,
    };

    let surface_grav =
        sa_terrain::gravity::surface_gravity(mass_earth, radius_earth);

    Some((body_idx, center_ly, config, surface_grav))
}
