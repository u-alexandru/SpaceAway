//! Centralized terrain system constants.
//!
//! All tuning parameters live here so they can be adjusted in one place.
//! See docs/superpowers/specs/2026-03-30-terrain-redesign.md for rationale.

// -- LOD selection --
pub const K_FACTOR: f64 = 2.5;
pub const MAX_VISIBLE_NODES: usize = 400;
pub const MIN_RANGE: f64 = 50.0;

// -- Mesh generation --
pub const GRID_SIZE: u32 = 33;
pub const CELLS: u32 = 32;
pub const GRID_VERTEX_COUNT: u32 = GRID_SIZE * GRID_SIZE;
pub const SKIRT_VERTEX_COUNT: u32 = 4 * GRID_SIZE;
pub const VERTS_PER_HEIGHTMAP_CHUNK: u32 = GRID_VERTEX_COUNT + SKIRT_VERTEX_COUNT;

// -- GPU memory --
/// 60MB ≈ 1034 slots at 58KB each. 30MB (511 slots) was too small —
/// at close approach the quadtree selects ~367 visible nodes, causing
/// streaming churn and visible gaps when only 177 fit.
pub const HEIGHTMAP_BUDGET_BYTES: u64 = 60_000_000;
pub const VOLUMETRIC_BUDGET_BYTES: u64 = 20_000_000;

// -- Collision --
pub const COLLISION_GRID_SIZE: usize = 7;
pub const COLLISION_LOD_OFFSET: u8 = 2;
pub const COLLISION_MAX_CHUNK_WIDTH_M: f64 = 200.0;
pub const COLLISION_REBASE_THRESHOLD_M: f64 = 100.0;
pub const COLLISION_GRID_HYSTERESIS: f64 = 1.5;

// -- Streaming --
pub const LRU_CAPACITY: usize = 1000;
pub const MAX_UPLOADS_BURST: usize = 64;
pub const MAX_UPLOADS_STEADY: usize = 8;
pub const BURST_THRESHOLD: usize = 24;

// -- Activation --
pub const TERRAIN_ACTIVATE_FACTOR: f64 = 2.0;
pub const TERRAIN_DEACTIVATE_FACTOR: f64 = 2.5;
pub const COLLISION_ACTIVATE_FACTOR: f64 = 0.2;
pub const COLLISION_DEACTIVATE_ALT_M: f64 = 500.0;
pub const ICOSPHERE_RADIUS_FACTOR: f64 = 0.999;

// -- Base chunks (never evicted) --
pub const BASE_LOD_LEVELS: u8 = 2;
pub const BASE_CHUNK_COUNT: usize = 30;
