//! Terrain integration: activates CDLOD terrain when camera approaches a
//! landable planet, streams chunks, uploads meshes to GPU, computes gravity,
//! and manages HeightField colliders for physics.

use glam::{Mat4, Vec3};
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use sa_render::{GpuTerrainVertex, TerrainDrawCommand, TerrainSlab};
use sa_terrain::quadtree::{max_lod_levels, select_visible_nodes};
use sa_terrain::streaming::ChunkStreaming;
use sa_terrain::{ChunkKey, TerrainConfig};
use sa_universe::PlanetSubType;

use crate::solar_system::ActiveSystem;
use spaceaway::terrain_colliders::TerrainColliders;

/// Light-years to meters conversion factor.
const LY_TO_M: f64 = 9.461e15;

/// Terrain activates when camera is within this multiple of the planet radius.
/// At 2.0x radius the initial LOD is coarse (LOD 0-2 panels), but streaming
/// fills finer chunks within seconds as the player approaches. This wide zone
/// ensures terrain activates before the player reaches the icosphere surface,
/// even at cruise speeds (1c ~ 4,800 km/frame).
const ACTIVATE_RADIUS_MULT: f64 = 2.0;

/// Terrain deactivates with hysteresis to prevent toggling. The 0.5x gap
/// between activation (2.0x) and deactivation (2.5x) is wide enough that
/// orbital drift and cruise overshoot don't cause rapid toggling.
const DEACTIVATE_RADIUS_MULT: f64 = 2.5;

// ---------------------------------------------------------------------------
// TerrainFrameResult
// ---------------------------------------------------------------------------

/// Per-frame output from the terrain manager.
pub struct TerrainFrameResult {
    /// Draw commands for all visible terrain chunks (slab-based).
    pub terrain_draws: Vec<TerrainDrawCommand>,
    /// Blended gravity state (ship <-> planet transition).
    pub gravity: Option<sa_terrain::gravity::GravityState>,
}

// ---------------------------------------------------------------------------
// TerrainManager
// ---------------------------------------------------------------------------

/// Owns the streaming pipeline for one planet's terrain.
/// GPU meshes live in the shared TerrainSlab on the Renderer.
pub struct TerrainManager {
    streaming: ChunkStreaming,
    config: TerrainConfig,
    /// Planet center in light-years (updated each frame from orbital position).
    planet_center_ly: WorldPos,
    /// Body index in the ActiveSystem.
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

        let streaming = ChunkStreaming::new(config.clone(), 4);

        Self {
            streaming,
            config,
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

    /// Synchronously generate and upload LOD 0+1 chunks to the slab.
    /// Called once at terrain activation. These chunks serve as permanent
    /// fallback ancestors for the LOD fallback system.
    pub fn seed_base_chunks(
        &mut self,
        terrain_slab: &mut TerrainSlab,
        queue: &wgpu::Queue,
    ) {
        use sa_terrain::chunk::generate_chunk;

        for face_idx in 0..6u8 {
            let key = ChunkKey { face: face_idx, lod: 0, x: 0, y: 0 };
            if terrain_slab.contains(&key) {
                continue;
            }
            let chunk = generate_chunk(key, &self.config);
            upload_chunk_to_slab(terrain_slab, queue, &chunk);
        }
        // Also seed LOD 1 (4 chunks per face = 24 total) for better coverage
        for face_idx in 0..6u8 {
            for x in 0..2u32 {
                for y in 0..2u32 {
                    let key = ChunkKey { face: face_idx, lod: 1, x, y };
                    if terrain_slab.contains(&key) {
                        continue;
                    }
                    let chunk = generate_chunk(key, &self.config);
                    upload_chunk_to_slab(terrain_slab, queue, &chunk);
                }
            }
        }
        log::info!(
            "Seeded base terrain chunks (6xLOD0 + 24xLOD1), slab: {}/{}",
            terrain_slab.occupied_slots(),
            terrain_slab.total_slots,
        );
    }

    /// Run one frame of terrain streaming and produce draw commands.
    #[allow(clippy::too_many_arguments)]
    #[profiling::function]
    pub fn update(
        &mut self,
        camera_galactic_ly: WorldPos,
        planet_center_ly: WorldPos,
        terrain_slab: &mut TerrainSlab,
        queue: &wgpu::Queue,
        physics: &mut PhysicsWorld,
        ship_down: [f32; 3],
        rebase_bodies: &spaceaway::terrain_colliders::RebaseBodies,
        vp_planet_relative: Option<[f64; 16]>,
    ) -> TerrainFrameResult {
        let _ = planet_center_ly; // unused -- we keep the activation-time position

        let cam_rel_m = [
            (camera_galactic_ly.x - self.planet_center_ly.x) * LY_TO_M,
            (camera_galactic_ly.y - self.planet_center_ly.y) * LY_TO_M,
            (camera_galactic_ly.z - self.planet_center_ly.z) * LY_TO_M,
        ];

        let frustum = vp_planet_relative.map(sa_terrain::frustum::Frustum::from_vp_matrix);
        let visible = select_visible_nodes(
            cam_rel_m,
            self.config.radius_m,
            self.max_lod,
            self.max_displacement_m,
            frustum.as_ref(),
        );

        let (new_chunks, _removed_keys) =
            self.streaming.update(&visible, &self.config, cam_rel_m);

        // Upload newly generated chunks to the slab.
        for chunk in &new_chunks {
            if !terrain_slab.contains(&chunk.key) {
                if terrain_slab.free_slots() == 0 {
                    // Evict farthest non-protected chunk
                    let mut protected = std::collections::HashSet::new();
                    for face in 0..6u8 {
                        protected.insert(ChunkKey { face, lod: 0, x: 0, y: 0 });
                        for x in 0..2u32 {
                            for y in 0..2u32 {
                                protected.insert(ChunkKey { face, lod: 1, x, y });
                            }
                        }
                    }
                    terrain_slab.evict_farthest(cam_rel_m, &protected);
                }
                upload_chunk_to_slab(terrain_slab, queue, chunk);
            }
        }

        // --- Gravity computation ---
        let gravity = sa_terrain::gravity::compute_gravity(
            cam_rel_m,
            ship_down,
            self.config.radius_m,
            self.surface_gravity_ms2,
            9.81,
        );

        // --- Collision grid: independent of visual LOD ---
        let cam_dist = (cam_rel_m[0] * cam_rel_m[0]
            + cam_rel_m[1] * cam_rel_m[1]
            + cam_rel_m[2] * cam_rel_m[2])
            .sqrt();
        let altitude = cam_dist - self.config.radius_m;
        if altitude
            < self.config.radius_m * sa_terrain::config::COLLISION_ACTIVATE_FACTOR
        {
            self.col
                .update_collision_grid(cam_rel_m, &self.config, physics, rebase_bodies);
        }

        // --- Collision diagnostic (every 60 frames) ---
        self.diag_frame += 1;
        if self.diag_frame.is_multiple_of(60) {
            let altitude_km = altitude / 1000.0;
            log::info!(
                "COLLISION_DIAG: alt={:.1}km, hf_colliders={}",
                altitude_km,
                self.col.colliders.len(),
            );
        }

        // Build draw commands using frozen planet position.
        let terrain_draws = self.build_draw_commands(
            &visible,
            self.planet_center_ly,
            camera_galactic_ly,
            terrain_slab,
        );

        // Diagnostic: log terrain state every 60 frames
        if self.diag_frame.is_multiple_of(60) {
            let altitude_km = altitude / 1000.0;
            log::info!(
                "TERRAIN_DIAG: alt={:.1}km, visible={}, slab={}/{}, draws={}, \
                 lod_range={}-{}",
                altitude_km,
                visible.len(),
                terrain_slab.occupied_slots(),
                terrain_slab.total_slots,
                terrain_draws.len(),
                visible.iter().map(|n| n.lod).min().unwrap_or(0),
                visible.iter().map(|n| n.lod).max().unwrap_or(0),
            );
        }

        // The icosphere renders at 0.999× radius so terrain chunks
        // (at or above true radius) always win the depth test. No need
        // to hide the icosphere — depth testing handles occlusion.
        TerrainFrameResult {
            terrain_draws,
            gravity: Some(gravity),
        }
    }

    /// Remove all terrain colliders and the terrain rigid body from physics.
    pub fn cleanup(&mut self, physics: &mut PhysicsWorld) {
        self.col.cleanup(physics);
    }

    /// Terrain rigid body handle.
    #[allow(dead_code)]
    pub fn terrain_body_handle(&self) -> Option<rapier3d::prelude::RigidBodyHandle> {
        self.col.terrain_body
    }

    /// The frozen planet center in light-years (set at activation time).
    pub fn frozen_planet_center_ly(&self) -> WorldPos {
        self.planet_center_ly
    }

    /// Current physics anchor in planet-relative meters (f64).
    pub fn anchor_f64(&self) -> [f64; 3] {
        self.col.anchor_f64
    }

    /// Set the physics anchor directly (for teleport).
    pub fn set_anchor(&mut self, new_anchor: [f64; 3]) {
        self.col.anchor_f64 = new_anchor;
    }

    /// Clear stale GPU meshes, flush the streaming cache, and force burst
    /// uploads after a teleport.
    pub fn flush_for_teleport(&mut self, terrain_slab: &mut TerrainSlab) {
        terrain_slab.clear();
        self.streaming.flush();
        self.streaming.burst_frames_remaining = 120;
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
            log::debug!(
                "Terrain deactivation check: dist={:.0}km, threshold={:.0}km",
                dist_m / 1000.0, threshold / 1000.0,
            );
        }
        should
    }

    /// Body index this terrain replaces.
    pub fn body_index(&self) -> usize {
        self.body_index
    }

    #[profiling::function]
    fn build_draw_commands(
        &self,
        visible: &[sa_terrain::quadtree::VisibleNode],
        planet_center_ly: WorldPos,
        camera_galactic_ly: WorldPos,
        terrain_slab: &TerrainSlab,
    ) -> Vec<TerrainDrawCommand> {
        let cam_offset_m = [
            (planet_center_ly.x - camera_galactic_ly.x) * LY_TO_M,
            (planet_center_ly.y - camera_galactic_ly.y) * LY_TO_M,
            (planet_center_ly.z - camera_galactic_ly.z) * LY_TO_M,
        ];

        let mut rendered = std::collections::HashSet::new();
        let mut cmds = Vec::with_capacity(visible.len());

        for node in visible {
            let mut key = ChunkKey {
                face: node.face as u8,
                lod: node.lod,
                x: node.x,
                y: node.y,
            };

            loop {
                if let Some(slot) = terrain_slab.get_slot(&key) {
                    if rendered.insert(key) {
                        let center = terrain_slab
                            .get_center(&key)
                            .unwrap_or([0.0; 3]);
                        let ox = (cam_offset_m[0] + center[0]) as f32;
                        let oy = (cam_offset_m[1] + center[1]) as f32;
                        let oz = (cam_offset_m[2] + center[2]) as f32;

                        cmds.push(TerrainDrawCommand {
                            slab_slot: slot,
                            model_matrix: Mat4::from_translation(
                                Vec3::new(ox, oy, oz),
                            ),
                            morph_factor: node.morph_factor,
                        });
                    }
                    break;
                }
                if key.lod == 0 {
                    break;
                }
                key.x /= 2;
                key.y /= 2;
                key.lod -= 1;
            }
        }
        cmds
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Upload a terrain chunk's vertices to the slab allocator.
fn upload_chunk_to_slab(
    terrain_slab: &mut TerrainSlab,
    queue: &wgpu::Queue,
    chunk: &sa_terrain::ChunkData,
) {
    if let Some(slot) = terrain_slab.allocate(chunk.key) {
        let gpu_verts: Vec<GpuTerrainVertex> = chunk
            .vertices
            .iter()
            .map(|v| GpuTerrainVertex {
                position: v.position,
                color: v.color,
                normal: v.normal,
                morph_target: v.morph_target,
            })
            .collect();
        terrain_slab.upload(slot, bytemuck::cast_slice(&gpu_verts), queue);
        terrain_slab.set_center(chunk.key, chunk.center_f64);
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
