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
const ACTIVATE_RADIUS_MULT: f64 = 2.0;

/// Terrain deactivates when camera exceeds this multiple of the planet radius.
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
        }
    }

    /// Surface gravity of the planet in m/s^2.
    pub fn surface_gravity(&self) -> f32 {
        self.surface_gravity_ms2
    }

    /// Run one frame of terrain streaming and produce draw commands.
    pub fn update(
        &mut self,
        camera_galactic_ly: WorldPos,
        planet_center_ly: WorldPos,
        mesh_store: &mut MeshStore,
        device: &wgpu::Device,
        physics: &mut PhysicsWorld,
        ship_down: [f32; 3],
    ) -> TerrainFrameResult {
        self.planet_center_ly = planet_center_ly;

        let cam_rel_m = [
            (camera_galactic_ly.x - planet_center_ly.x) * LY_TO_M,
            (camera_galactic_ly.y - planet_center_ly.y) * LY_TO_M,
            (camera_galactic_ly.z - planet_center_ly.z) * LY_TO_M,
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
            if !self.gpu_meshes.contains_key(&chunk.key) {
                let mesh_data = chunk_to_mesh_data(chunk);
                let handle = mesh_store.upload(device, &mesh_data);
                self.gpu_meshes.insert(chunk.key, (handle, chunk.center_f64));
            }
            self.col.cache_chunk(chunk.key, chunk);
        }

        // Only remove GPU meshes for chunks that are truly evicted from the
        // streaming cache (not just off-screen). Off-screen chunks may come
        // back when the player turns around.
        // removed_keys from streaming = chunks no longer in the visible set.
        // We keep GPU meshes alive as long as the chunk exists in the LRU cache.
        // Only remove when gpu_meshes has keys that aren't in visible AND aren't
        // expected to return soon. For now, let GPU meshes accumulate up to a
        // budget and evict the oldest.
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
        );

        // Build draw commands.
        let draw_commands = self.build_draw_commands(
            &visible,
            planet_center_ly,
            camera_galactic_ly,
        );

        TerrainFrameResult {
            draw_commands,
            hidden_body_index: Some(self.body_index),
            gravity: Some(gravity),
        }
    }

    /// Remove all terrain colliders and the terrain rigid body from physics.
    pub fn cleanup(&mut self, physics: &mut PhysicsWorld) {
        self.col.cleanup(physics);
    }

    /// Returns true when the camera has moved far enough to deactivate terrain.
    pub fn should_deactivate(&self, camera_ly: WorldPos) -> bool {
        let dx = (camera_ly.x - self.planet_center_ly.x) * LY_TO_M;
        let dy = (camera_ly.y - self.planet_center_ly.y) * LY_TO_M;
        let dz = (camera_ly.z - self.planet_center_ly.z) * LY_TO_M;
        let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();
        dist_m > self.config.radius_m * DEACTIVATE_RADIUS_MULT
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
