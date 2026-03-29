//! Terrain integration: activates CDLOD terrain when camera approaches a
//! landable planet, streams chunks, uploads meshes to GPU, and builds
//! pre-rebased DrawCommands.

use std::collections::HashMap;

use glam::{Mat4, Vec3};
use sa_core::Handle;
use sa_math::WorldPos;
use sa_render::{DrawCommand, MeshData, MeshMarker, MeshStore, Vertex};
use sa_terrain::quadtree::{max_lod_levels, select_visible_nodes};
use sa_terrain::streaming::ChunkStreaming;
use sa_terrain::{ChunkKey, TerrainConfig};
use sa_universe::PlanetSubType;

use crate::solar_system::ActiveSystem;

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
}

// ---------------------------------------------------------------------------
// TerrainManager
// ---------------------------------------------------------------------------

/// Owns the streaming pipeline and GPU mesh handles for one planet's terrain.
pub struct TerrainManager {
    streaming: ChunkStreaming,
    config: TerrainConfig,
    /// GPU handles keyed by chunk.
    gpu_meshes: HashMap<ChunkKey, Handle<MeshMarker>>,
    /// Planet center in light-years (updated each frame from orbital position).
    planet_center_ly: WorldPos,
    /// Body index in the ActiveSystem (for icosphere hiding).
    body_index: usize,
    /// Maximum LOD level for this planet.
    max_lod: u8,
    /// Maximum terrain displacement in meters.
    max_displacement_m: f64,
}

impl TerrainManager {
    /// Create a new terrain manager for a planet.
    pub fn new(
        config: TerrainConfig,
        planet_center_ly: WorldPos,
        body_index: usize,
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
        }
    }

    /// Run one frame of terrain streaming and produce draw commands.
    ///
    /// `camera_galactic_ly`: camera position in galactic light-years.
    /// Returns draw commands with `pre_rebased: true`.
    pub fn update(
        &mut self,
        camera_galactic_ly: WorldPos,
        planet_center_ly: WorldPos,
        mesh_store: &mut MeshStore,
        device: &wgpu::Device,
    ) -> TerrainFrameResult {
        // Update planet position (orbits move).
        self.planet_center_ly = planet_center_ly;

        // Camera position relative to planet center, in meters (f64).
        let cam_rel_m = [
            (camera_galactic_ly.x - planet_center_ly.x) * LY_TO_M,
            (camera_galactic_ly.y - planet_center_ly.y) * LY_TO_M,
            (camera_galactic_ly.z - planet_center_ly.z) * LY_TO_M,
        ];

        // Select visible quadtree nodes.
        let visible = select_visible_nodes(
            cam_rel_m,
            self.config.radius_m,
            self.max_lod,
            self.max_displacement_m,
        );

        // Stream chunks (request new, receive completed).
        let (new_chunks, _removed_keys) =
            self.streaming.update(&visible, &self.config);

        // Upload newly generated chunks to GPU.
        for chunk in &new_chunks {
            let mesh_data = chunk_to_mesh_data(chunk);
            let handle = mesh_store.upload(device, &mesh_data);
            self.gpu_meshes.insert(chunk.key, handle);
        }

        // Note: we do NOT remove GPU meshes for _removed_keys because
        // MeshStore has no remove API. The handles remain valid but unused;
        // they will be cleaned up when the TerrainManager is dropped and
        // a new one created.

        // Build draw commands for every visible node that has a GPU mesh.
        let planet_m = [
            planet_center_ly.x * LY_TO_M,
            planet_center_ly.y * LY_TO_M,
            planet_center_ly.z * LY_TO_M,
        ];
        let camera_m = [
            camera_galactic_ly.x * LY_TO_M,
            camera_galactic_ly.y * LY_TO_M,
            camera_galactic_ly.z * LY_TO_M,
        ];

        let mut draw_commands = Vec::with_capacity(visible.len());
        for node in &visible {
            let key = ChunkKey {
                face: node.face as u8,
                lod: node.lod,
                x: node.x,
                y: node.y,
            };
            if let Some(&handle) = self.gpu_meshes.get(&key) {
                // Chunk center is planet-relative meters (from ChunkData or
                // recomputed from the node center on the sphere).
                let chunk_center_m = node.center;

                // Model offset: planet_center + chunk_center - camera, all in
                // f64 meters, then cast to f32 for the matrix.
                let ox = (planet_m[0] + chunk_center_m[0] - camera_m[0]) as f32;
                let oy = (planet_m[1] + chunk_center_m[1] - camera_m[1]) as f32;
                let oz = (planet_m[2] + chunk_center_m[2] - camera_m[2]) as f32;

                let model = Mat4::from_translation(Vec3::new(ox, oy, oz));
                draw_commands.push(DrawCommand {
                    mesh: handle,
                    model_matrix: model,
                    pre_rebased: true,
                });
            }
        }

        TerrainFrameResult {
            draw_commands,
            hidden_body_index: Some(self.body_index),
        }
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
/// Returns `(body_index, planet_center_ly, TerrainConfig)` if a planet
/// qualifies, or `None` if no planet is close enough.
pub fn find_terrain_planet(
    active_system: &ActiveSystem,
    camera_ly: WorldPos,
) -> Option<(usize, WorldPos, TerrainConfig)> {
    let positions = active_system.compute_positions_ly_pub();

    let mut best: Option<(usize, f64, WorldPos)> = None;

    for (i, pos) in positions.iter().enumerate() {
        let radius_m = match active_system.body_radius_m(i) {
            Some(r) => r,
            None => continue,
        };

        // Only consider bodies that have planet_data (actual planets).
        let (_color_seed, sub_type, _disp_frac, _mass, _re) =
            match active_system.planet_data(i) {
                Some(data) => data,
                None => continue,
            };

        if !is_landable(sub_type) {
            continue;
        }

        // Distance in meters.
        let dx = (camera_ly.x - pos.x) * LY_TO_M;
        let dy = (camera_ly.y - pos.y) * LY_TO_M;
        let dz = (camera_ly.z - pos.z) * LY_TO_M;
        let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist_m > radius_m * ACTIVATE_RADIUS_MULT {
            continue;
        }

        // Pick closest qualifying planet.
        let dominated = match &best {
            Some((_, best_dist, _)) => dist_m < *best_dist,
            None => true,
        };
        if dominated {
            best = Some((i, dist_m, *pos));
        }
    }

    let (body_idx, _dist, center_ly) = best?;

    // Reconstruct config from planet data.
    let radius_m = active_system.body_radius_m(body_idx)?;
    let (color_seed, sub_type, disp_frac, _mass, _re) =
        active_system.planet_data(body_idx)?;

    let config = TerrainConfig {
        radius_m,
        noise_seed: color_seed,
        sub_type,
        displacement_fraction: disp_frac,
    };

    Some((body_idx, center_ly, config))
}
