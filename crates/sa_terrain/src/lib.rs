//! CDLOD terrain system: cube-sphere quadtree with async chunk streaming.
//!
//! Pure terrain math — no rendering or physics dependencies.
//! Integration with GPU and collision happens in the spaceaway binary crate.

pub mod cube_sphere;
pub mod frustum;
pub mod quadtree;
pub mod heightmap;
pub mod biome;
pub mod chunk;
pub mod streaming;
pub mod gravity;

use sa_universe::PlanetSubType;

/// Configuration for a planet's terrain. Passed when terrain activates.
#[derive(Debug, Clone)]
pub struct TerrainConfig {
    /// Planet radius in meters.
    pub radius_m: f64,
    /// Noise seed (same as Planet::color_seed for visual consistency with icosphere).
    pub noise_seed: u64,
    /// Planet surface sub-type (determines biome colors and displacement amplitude).
    pub sub_type: PlanetSubType,
    /// Terrain height displacement as fraction of radius (0.01-0.04).
    pub displacement_fraction: f32,
}

/// Identifies a terrain chunk uniquely within a planet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    /// Cube face index (0-5: +X, -X, +Y, -Y, +Z, -Z).
    pub face: u8,
    /// LOD level (0 = coarsest full-face, max = finest ground-level).
    pub lod: u8,
    /// Grid X position within this face at this LOD level.
    pub x: u32,
    /// Grid Y position within this face at this LOD level.
    pub y: u32,
}

/// Vertex data for a terrain chunk (matches sa_render::Vertex layout).
#[derive(Debug, Clone, Copy)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
}

/// Generated chunk data ready for GPU upload.
#[derive(Debug, Clone)]
pub struct ChunkData {
    pub key: ChunkKey,
    /// Chunk center in planet-relative meters (f64 for precision).
    pub center_f64: [f64; 3],
    /// Mesh vertices (33x33 grid + skirt vertices).
    pub vertices: Vec<TerrainVertex>,
    /// Triangle indices.
    pub indices: Vec<u32>,
    /// Raw 33x33 height samples for future collision (Phase 2).
    pub heights: Vec<f32>,
    /// Min/max height for bounding sphere inflation during frustum culling.
    pub min_height: f32,
    pub max_height: f32,
}
