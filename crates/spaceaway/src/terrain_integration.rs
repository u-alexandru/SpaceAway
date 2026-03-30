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
use spaceaway::terrain_colliders::TerrainColliders;

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
    /// Once true, the icosphere stays hidden until terrain deactivates.
    /// Prevents flickering when LOD changes increase visible node count
    /// faster than streaming can fill them.
    icosphere_committed: bool,
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
            gpu_meshes: HashMap::new(),
            planet_center_ly,
            body_index,
            max_lod,
            max_displacement_m,
            surface_gravity_ms2,
            col: TerrainColliders::new(),
            icosphere_committed: false,
            diag_frame: 0,
        }
    }

    /// Surface gravity of the planet in m/s^2.
    #[allow(dead_code)]
    pub fn surface_gravity(&self) -> f32 {
        self.surface_gravity_ms2
    }

    /// Synchronously generate and upload LOD 0 chunks for all 6 cube faces.
    /// Called once at terrain activation. These chunks serve as permanent
    /// fallback ancestors for the LOD fallback system — when fine LOD nodes
    /// haven't streamed yet, the renderer walks up to these coarse chunks
    /// to ensure the planet is always fully covered.
    ///
    /// Without this, the quadtree at activation distance subdivides LOD 0
    /// into LOD 1+, so LOD 0 is never in the visible list, never requested
    /// from streaming, and the LOD fallback has no ultimate ancestor.
    pub fn seed_base_chunks(
        &mut self,
        mesh_store: &mut MeshStore,
        device: &wgpu::Device,
    ) {
        use sa_terrain::chunk::generate_chunk;

        for face_idx in 0..6u8 {
            let key = ChunkKey { face: face_idx, lod: 0, x: 0, y: 0 };
            if self.gpu_meshes.contains_key(&key) {
                continue; // already exists
            }
            let chunk = generate_chunk(key, &self.config);
            let mesh_data = chunk_to_mesh_data(&chunk);
            let handle = mesh_store.upload(device, &mesh_data);
            self.gpu_meshes.insert(key, (handle, chunk.center_f64));
        }
        // Also seed LOD 1 (4 chunks per face = 24 total) for better coverage
        for face_idx in 0..6u8 {
            for x in 0..2u32 {
                for y in 0..2u32 {
                    let key = ChunkKey { face: face_idx, lod: 1, x, y };
                    if self.gpu_meshes.contains_key(&key) {
                        continue;
                    }
                    let chunk = generate_chunk(key, &self.config);
                    let mesh_data = chunk_to_mesh_data(&chunk);
                    let handle = mesh_store.upload(device, &mesh_data);
                    self.gpu_meshes.insert(key, (handle, chunk.center_f64));
                }
            }
        }
        log::info!(
            "Seeded {} base terrain chunks (6×LOD0 + 24×LOD1)",
            self.gpu_meshes.len(),
        );
    }

    /// Run one frame of terrain streaming and produce draw commands.
    ///
    /// `vp_matrix`: optional view-projection matrix (column-major f64) for
    /// frustum culling. When provided, terrain chunks outside the camera's
    /// view frustum are skipped during quadtree traversal.
    #[allow(clippy::too_many_arguments)]
    #[profiling::function]
    pub fn update(
        &mut self,
        camera_galactic_ly: WorldPos,
        planet_center_ly: WorldPos,
        mesh_store: &mut MeshStore,
        device: &wgpu::Device,
        physics: &mut PhysicsWorld,
        ship_down: [f32; 3],
        rebase_bodies: &spaceaway::terrain_colliders::RebaseBodies,
        vp_planet_relative: Option<[f64; 16]>,
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

        let frustum = vp_planet_relative.map(sa_terrain::frustum::Frustum::from_vp_matrix);
        let visible = select_visible_nodes(
            cam_rel_m,
            self.config.radius_m,
            self.max_lod,
            self.max_displacement_m,
            frustum.as_ref(),
        );

        let (new_chunks, removed_keys) =
            self.streaming.update(&visible, &self.config, cam_rel_m);

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

        // LRU eviction: remove collision data but KEEP GPU mesh handles.
        // The LRU cache manages CPU memory (full ChunkData: vertices, heights).
        // GPU mesh handles in gpu_meshes are tiny (32 bytes each) and are
        // needed by the LOD fallback: when the quadtree selects fine nodes
        // that haven't streamed yet, the ancestor walk needs coarser chunks
        // to still be in gpu_meshes. Without this, approaching a planet
        // causes it to disappear — the coarsest chunks (uploaded first) get
        // evicted first, and the LOD fallback has no ancestor to render.
        //
        self.col.remove_evicted(physics, &removed_keys);

        // Cap gpu_meshes to prevent unbounded GPU memory growth (~70KB per mesh).
        // Evict farthest chunks first, NEVER evict LOD 0-1 (permanent fallback).
        // Also free the GPU buffer via mesh_store.remove().
        const GPU_MESH_CAP: usize = 600;
        if self.gpu_meshes.len() > GPU_MESH_CAP {
            // Sort by distance from camera (farthest first), then by LOD (finest first)
            let mut keys_with_dist: Vec<(ChunkKey, f64)> = self.gpu_meshes.keys()
                .filter(|k| k.lod > 1) // never evict LOD 0-1
                .map(|k| {
                    let dist = self.gpu_meshes.get(k)
                        .map(|(_, c)| {
                            let dx = c[0] - cam_rel_m[0];
                            let dy = c[1] - cam_rel_m[1];
                            let dz = c[2] - cam_rel_m[2];
                            dx * dx + dy * dy + dz * dz
                        })
                        .unwrap_or(f64::MAX);
                    (*k, dist)
                })
                .collect();
            keys_with_dist.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let excess = self.gpu_meshes.len() - GPU_MESH_CAP;
            for (key, _) in keys_with_dist.iter().take(excess) {
                if let Some((handle, _)) = self.gpu_meshes.remove(key) {
                    mesh_store.remove(handle);
                }
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
            rebase_bodies,
        );

        // --- Collision diagnostic (every 60 frames) ---
        self.diag_frame += 1;
        if self.diag_frame.is_multiple_of(60) {
            let cam_dist = (cam_rel_m[0] * cam_rel_m[0]
                + cam_rel_m[1] * cam_rel_m[1]
                + cam_rel_m[2] * cam_rel_m[2]).sqrt();
            let altitude_km = (cam_dist - self.config.radius_m) / 1000.0;

            // Surface barrier diagnostic: position + distance from ship.
            let ship_pos = rebase_bodies.ship
                .and_then(|h| physics.rigid_body_set.get(h))
                .map(|b| *b.translation())
                .unwrap_or(nalgebra::Vector3::zeros());
            let barrier_info = self.col.surface_barrier
                .and_then(|sh| physics.collider_set.get(sh))
                .map(|coll| {
                    let pos = coll.position().translation;
                    let dx = pos.x - ship_pos.x;
                    let dy = pos.y - ship_pos.y;
                    let dz = pos.z - ship_pos.z;
                    (pos.x, pos.y, pos.z, (dx*dx + dy*dy + dz*dz).sqrt())
                });
            if let Some((bx, by, bz, bdist)) = barrier_info {
                log::info!(
                    "COLLISION_DIAG: alt={:.1}km, ship=({:.0},{:.0},{:.0}), \
                     barrier=({:.0},{:.0},{:.0}), gap={:.0}m, hf_colliders={}",
                    altitude_km,
                    ship_pos.x, ship_pos.y, ship_pos.z,
                    bx, by, bz, bdist,
                    self.col.colliders.len(),
                );
            } else {
                log::info!(
                    "COLLISION_DIAG: alt={:.1}km, NO_BARRIER, hf_colliders={}",
                    altitude_km,
                    self.col.colliders.len(),
                );
            }
        }

        // Build draw commands using frozen planet position.
        let draw_commands = self.build_draw_commands(
            &visible,
            self.planet_center_ly,
            camera_galactic_ly,
        );

        // Diagnostic: log terrain state every 60 frames to debug disappearing planet
        if self.diag_frame.is_multiple_of(60) {
            let cam_dist = (cam_rel_m[0] * cam_rel_m[0]
                + cam_rel_m[1] * cam_rel_m[1]
                + cam_rel_m[2] * cam_rel_m[2]).sqrt();
            let altitude_km = (cam_dist - self.config.radius_m) / 1000.0;
            log::info!(
                "TERRAIN_DIAG: alt={:.1}km, visible={}, gpu_meshes={}, draw_cmds={}, \
                 committed={}, lod_range={}-{}",
                altitude_km,
                visible.len(),
                self.gpu_meshes.len(),
                draw_commands.len(),
                self.icosphere_committed,
                visible.iter().map(|n| n.lod).min().unwrap_or(0),
                visible.iter().map(|n| n.lod).max().unwrap_or(0),
            );
        }

        // Icosphere hiding with commit-and-hold:
        // Once the icosphere is hidden, it STAYS hidden until terrain deactivates.
        // This prevents flickering when the camera moves closer and the quadtree
        // produces more visible nodes (finer LODs) that haven't streamed yet —
        // the existing coarser chunks still cover the area visually.
        //
        // Initial hide requires 33% of visible nodes to be GPU-ready AND at
        // least 6 chunks (one per face). After that, the decision is locked.
        let hide_icosphere = if self.icosphere_committed {
            true
        } else {
            let visible_in_gpu = visible.iter().filter(|n| {
                let key = ChunkKey {
                    face: n.face as u8,
                    lod: n.lod,
                    x: n.x,
                    y: n.y,
                };
                self.gpu_meshes.contains_key(&key)
            }).count();
            let ready = visible_in_gpu >= 6 && visible_in_gpu * 3 >= visible.len();
            if ready {
                self.icosphere_committed = true;
                log::info!(
                    "Icosphere hidden: {}/{} visible chunks GPU-ready",
                    visible_in_gpu, visible.len(),
                );
            }
            ready
        };

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
    /// After calling this, the ship body should be at rapier origin.
    pub fn set_anchor(&mut self, new_anchor: [f64; 3]) {
        self.col.anchor_f64 = new_anchor;
    }

    /// Clear stale GPU meshes, flush the streaming cache, and force burst
    /// uploads after a teleport. Old chunks are at the previous position and
    /// must be fully discarded — including the LRU cache, otherwise cached
    /// chunks won't be re-requested and gpu_meshes stays empty.
    pub fn flush_for_teleport(&mut self) {
        self.gpu_meshes.clear();
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
            log::debug!("Terrain deactivation check: dist={:.0}km, threshold={:.0}km, deactivate={}",
                dist_m / 1000.0, threshold / 1000.0, should);
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

        // LOD fallback: when a fine chunk isn't GPU-ready, walk up the LOD
        // tree to find a coarser ancestor that IS ready. This ensures the
        // planet is always fully covered visually, even during LOD transitions
        // when the streaming system hasn't caught up with the new node set.
        //
        // Without this, approaching a planet causes it to disappear: the
        // quadtree selects finer nodes, but only coarser chunks exist in
        // gpu_meshes. None of the fine nodes match → zero draw commands.
        let mut rendered: std::collections::HashSet<ChunkKey> = std::collections::HashSet::new();
        let mut cmds = Vec::with_capacity(visible.len());

        for node in visible {
            // Try the exact chunk first
            let mut key = ChunkKey {
                face: node.face as u8,
                lod: node.lod,
                x: node.x,
                y: node.y,
            };

            // Walk up to coarser ancestors if the exact chunk isn't ready
            let mut found = false;
            loop {
                if let Some(&(handle, center_f64)) = self.gpu_meshes.get(&key) {
                    // Don't render the same ancestor twice (multiple fine
                    // nodes can share one coarse ancestor)
                    if rendered.insert(key) {
                        let ox = (cam_offset_m[0] + center_f64[0]) as f32;
                        let oy = (cam_offset_m[1] + center_f64[1]) as f32;
                        let oz = (cam_offset_m[2] + center_f64[2]) as f32;

                        cmds.push(DrawCommand {
                            mesh: handle,
                            model_matrix: Mat4::from_translation(Vec3::new(ox, oy, oz)),
                            pre_rebased: true,
                        });
                    }
                    found = true;
                    break;
                }
                // Move to parent: halve coordinates, decrease LOD
                if key.lod == 0 {
                    break; // no coarser ancestor exists
                }
                key.x /= 2;
                key.y /= 2;
                key.lod -= 1;
            }
            let _ = found;
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
