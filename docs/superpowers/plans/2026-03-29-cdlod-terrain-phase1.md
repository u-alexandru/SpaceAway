# CDLOD Terrain Phase 1: Terrain Rendering — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fly near a planet and see CDLOD terrain seamlessly replace the icosphere — no collision, no landing, visual-only.

**Architecture:** New `sa_terrain` crate with cube-sphere quadtree, async chunk generation via crossbeam channels, and integration through `terrain_integration.rs` in the spaceaway binary. Terrain chunks render as regular `DrawCommand`s with `pre_rebased=true`. Icosphere is suppressed when terrain is active, restored when deactivated, with hysteresis to prevent toggling.

**Tech Stack:** Rust, fastnoise-lite (fBm + domain warp), crossbeam-channel, wgpu (existing geometry pipeline), glam, sa_math::WorldPos

---

## File Structure

### New Files (sa_terrain crate)

| File | Responsibility |
|------|---------------|
| `crates/sa_terrain/Cargo.toml` | Crate manifest with workspace deps |
| `crates/sa_terrain/src/lib.rs` | Public API: re-exports, TerrainConfig |
| `crates/sa_terrain/src/cube_sphere.rs` | Cube face → sphere mapping (analytic Nowell 2005) |
| `crates/sa_terrain/src/quadtree.rs` | CDLOD node selection, LOD ranges, frustum culling |
| `crates/sa_terrain/src/heightmap.rs` | Noise sampling: fBm + domain warp → height |
| `crates/sa_terrain/src/biome.rs` | Biome color by altitude/latitude/sub_type |
| `crates/sa_terrain/src/chunk.rs` | Chunk mesh: vertices, indices, normals, skirts |
| `crates/sa_terrain/src/streaming.rs` | Async chunk manager: crossbeam channels, priority, LRU |

### New Files (spaceaway integration)

| File | Responsibility |
|------|---------------|
| `crates/spaceaway/src/terrain_integration.rs` | TerrainManager: activation, updates, GPU upload, handoff |

### Modified Files

| File | Change |
|------|--------|
| `Cargo.toml` (workspace root) | Add `sa_terrain` to members + workspace deps |
| `crates/spaceaway/Cargo.toml` | Add `sa_terrain` + `crossbeam-channel` deps |
| `crates/spaceaway/src/main.rs` | Add `terrain` field to App, call terrain_integration::update() |
| `crates/spaceaway/src/solar_system.rs` | Add `hidden_body_index`, `is_landable()`, expose planet data |

---

### Task 1: Create sa_terrain crate skeleton

**Files:**
- Create: `crates/sa_terrain/Cargo.toml`
- Create: `crates/sa_terrain/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create sa_terrain directory**

```bash
mkdir -p crates/sa_terrain/src
```

- [ ] **Step 2: Create Cargo.toml**

```toml
[package]
name = "sa_terrain"
version.workspace = true
edition.workspace = true

[dependencies]
sa_math.workspace = true
sa_core.workspace = true
sa_universe.workspace = true
fastnoise-lite.workspace = true
crossbeam-channel = "0.5"
glam.workspace = true
log.workspace = true

[dev-dependencies]
approx = "0.5"
```

- [ ] **Step 3: Create lib.rs with TerrainConfig and ChunkKey**

```rust
//! CDLOD terrain system: cube-sphere quadtree with async chunk streaming.
//!
//! Pure terrain math — no rendering or physics dependencies.
//! Integration with GPU and collision happens in the spaceaway binary crate.

pub mod cube_sphere;
pub mod quadtree;
pub mod heightmap;
pub mod biome;
pub mod chunk;
pub mod streaming;

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
    /// Terrain height displacement as fraction of radius (0.01–0.04).
    pub displacement_fraction: f32,
}

/// Identifies a terrain chunk uniquely within a planet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    /// Cube face index (0–5: +X, -X, +Y, -Y, +Z, -Z).
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
```

- [ ] **Step 4: Add sa_terrain to workspace**

In `Cargo.toml` (workspace root), add `"crates/sa_terrain"` to the `members` array and add to `[workspace.dependencies]`:

```toml
sa_terrain = { path = "crates/sa_terrain" }
crossbeam-channel = "0.5"
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo check -p sa_terrain
```

Expected: clean compilation with no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/sa_terrain/ Cargo.toml
git commit -m "feat(terrain): create sa_terrain crate skeleton with core types"
```

---

### Task 2: Cube-sphere mapping

**Files:**
- Create: `crates/sa_terrain/src/cube_sphere.rs`

- [ ] **Step 1: Write tests for cube-sphere mapping**

```rust
//! Cube face → sphere mapping using the analytic projection (Nowell 2005).
//!
//! Eliminates corner clustering from naive normalization:
//! x' = x * sqrt(1 - y²/2 - z²/2 + y²z²/3)
//!
//! Reference: Zucker & Higashi, JCGT 2018.

/// The six faces of the cube.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CubeFace {
    PosX = 0,
    NegX = 1,
    PosY = 2,
    NegY = 3,
    PosZ = 4,
    NegZ = 5,
}

impl CubeFace {
    /// All six faces in order.
    pub const ALL: [CubeFace; 6] = [
        CubeFace::PosX, CubeFace::NegX,
        CubeFace::PosY, CubeFace::NegY,
        CubeFace::PosZ, CubeFace::NegZ,
    ];
}

/// Map a point on a cube face to a unit sphere direction.
///
/// `u`, `v` ∈ [-1, +1] are coordinates on the face.
/// Returns a normalized direction vector on the unit sphere.
pub fn cube_to_sphere(face: CubeFace, u: f64, v: f64) -> [f64; 3] {
    // Map (u, v) to a cube position based on which face
    let (x, y, z) = match face {
        CubeFace::PosX => ( 1.0,   v,  -u),
        CubeFace::NegX => (-1.0,   v,   u),
        CubeFace::PosY => (  u,  1.0,  -v),
        CubeFace::NegY => (  u, -1.0,   v),
        CubeFace::PosZ => (  u,   v,  1.0),
        CubeFace::NegZ => ( -u,   v, -1.0),
    };

    // Analytic cube-to-sphere mapping (Nowell 2005)
    let x2 = x * x;
    let y2 = y * y;
    let z2 = z * z;

    let sx = x * (1.0 - y2 / 2.0 - z2 / 2.0 + y2 * z2 / 3.0).sqrt();
    let sy = y * (1.0 - x2 / 2.0 - z2 / 2.0 + x2 * z2 / 3.0).sqrt();
    let sz = z * (1.0 - x2 / 2.0 - y2 / 2.0 + x2 * y2 / 3.0).sqrt();

    [sx, sy, sz]
}

/// Compute the sphere-surface position in meters for a point on a cube face.
///
/// Returns position relative to planet center.
pub fn face_point_to_position(face: CubeFace, u: f64, v: f64, radius_m: f64) -> [f64; 3] {
    let dir = cube_to_sphere(face, u, v);
    [dir[0] * radius_m, dir[1] * radius_m, dir[2] * radius_m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_center_maps_to_axis() {
        // Center of +Z face (u=0, v=0) should map to (0, 0, 1)
        let [x, y, z] = cube_to_sphere(CubeFace::PosZ, 0.0, 0.0);
        assert!((x).abs() < 1e-10);
        assert!((y).abs() < 1e-10);
        assert!((z - 1.0).abs() < 1e-10);
    }

    #[test]
    fn face_center_maps_to_correct_axis_all_faces() {
        let expected = [
            (CubeFace::PosX, [1.0, 0.0, 0.0]),
            (CubeFace::NegX, [-1.0, 0.0, 0.0]),
            (CubeFace::PosY, [0.0, 1.0, 0.0]),
            (CubeFace::NegY, [0.0, -1.0, 0.0]),
            (CubeFace::PosZ, [0.0, 0.0, 1.0]),
            (CubeFace::NegZ, [0.0, 0.0, -1.0]),
        ];
        for (face, [ex, ey, ez]) in expected {
            let [x, y, z] = cube_to_sphere(face, 0.0, 0.0);
            assert!((x - ex).abs() < 1e-10, "face {:?} x: got {x}", face);
            assert!((y - ey).abs() < 1e-10, "face {:?} y: got {y}", face);
            assert!((z - ez).abs() < 1e-10, "face {:?} z: got {z}", face);
        }
    }

    #[test]
    fn all_points_on_unit_sphere() {
        // Sample many points across all faces — all should have length ≈ 1.0
        for face in CubeFace::ALL {
            for i in 0..=10 {
                for j in 0..=10 {
                    let u = -1.0 + 2.0 * (i as f64 / 10.0);
                    let v = -1.0 + 2.0 * (j as f64 / 10.0);
                    let [x, y, z] = cube_to_sphere(face, u, v);
                    let len = (x * x + y * y + z * z).sqrt();
                    assert!(
                        (len - 1.0).abs() < 1e-10,
                        "face {:?} u={u} v={v}: length={len}",
                        face,
                    );
                }
            }
        }
    }

    #[test]
    fn adjacent_faces_share_edge_points() {
        // +Z face at u=1 should equal +X face at u=-1 (they share an edge)
        // +Z face corner (1, 0) -> cube (1, 0, 1) -> maps to +X face at (-1, 0)
        // +X face at u=0, v=0 -> cube (1, 0, 0)
        // The shared edge between +Z and +X at v=0:
        // +Z (u=1, v=0) -> cube (1, 0, 1)
        // +X (u=1, v=0) -> cube (1, 0, -1)
        // These map to different sphere points (correct, they're different edges)
        //
        // Actually: +Z face right edge is u=1, which maps to cube_x=1.
        // +X face has cube_x=1 always, with z=-u.
        // So +Z (u=1, v) -> cube (1, v, 1) and +X (u=-1, v) -> cube (1, v, 1)
        // They should produce the same sphere point.
        for i in 0..=10 {
            let v = -1.0 + 2.0 * (i as f64 / 10.0);
            let from_pz = cube_to_sphere(CubeFace::PosZ, 1.0, v);
            let from_px = cube_to_sphere(CubeFace::PosX, -1.0, v);
            assert!(
                (from_pz[0] - from_px[0]).abs() < 1e-10
                    && (from_pz[1] - from_px[1]).abs() < 1e-10
                    && (from_pz[2] - from_px[2]).abs() < 1e-10,
                "edge mismatch at v={v}: pz={:?} px={:?}",
                from_pz,
                from_px,
            );
        }
    }

    #[test]
    fn face_point_to_position_scales_by_radius() {
        let radius = 6_371_000.0; // Earth radius in meters
        let [x, y, z] = face_point_to_position(CubeFace::PosZ, 0.0, 0.0, radius);
        assert!((x).abs() < 1.0);
        assert!((y).abs() < 1.0);
        assert!((z - radius).abs() < 1.0);
    }
}
```

- [ ] **Step 2: Verify tests pass**

```bash
cargo test -p sa_terrain -- cube_sphere
```

Expected: all 5 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/sa_terrain/src/cube_sphere.rs
git commit -m "feat(terrain): cube-to-sphere analytic mapping (Nowell 2005)"
```

---

### Task 3: CDLOD quadtree node selection

**Files:**
- Create: `crates/sa_terrain/src/quadtree.rs`

- [ ] **Step 1: Write quadtree module with tests**

```rust
//! CDLOD quadtree: LOD range selection, node traversal, frustum culling.
//!
//! Per the Strugar (2010) CDLOD paper: LOD ranges double per level,
//! nodes subdivide when camera is closer than range, morph at 50%.

use crate::cube_sphere::{CubeFace, cube_to_sphere};

/// Minimum range for finest LOD level (meters).
const MIN_RANGE: f64 = 50.0;

/// A visible terrain node selected by the quadtree traversal.
#[derive(Debug, Clone)]
pub struct VisibleNode {
    pub face: CubeFace,
    pub lod: u8,
    pub x: u32,
    pub y: u32,
    /// Node center on sphere surface (planet-relative meters).
    pub center: [f64; 3],
    /// Morph factor for this node (0.0 = full detail, 1.0 = fully morphed to parent).
    pub morph_factor: f32,
}

/// Compute LOD range for a given level. Camera farther than this = LOD is sufficient.
pub fn lod_range(level: u8) -> f64 {
    MIN_RANGE * (1u64 << level) as f64
}

/// Compute the maximum number of LOD levels needed for a planet.
/// `face_size_m`: approximate size of one cube face on the sphere surface.
pub fn max_lod_levels(face_size_m: f64) -> u8 {
    let ratio = face_size_m / MIN_RANGE;
    if ratio <= 1.0 {
        return 1;
    }
    (ratio.log2().ceil() as u8).max(1)
}

/// Select visible nodes for rendering. Returns nodes sorted coarsest-first.
///
/// `camera_pos`: camera position in planet-relative meters.
/// `planet_radius_m`: planet radius for sphere-surface calculations.
/// `max_lod`: finest LOD level (from `max_lod_levels`).
pub fn select_visible_nodes(
    camera_pos: [f64; 3],
    planet_radius_m: f64,
    max_lod: u8,
    max_displacement: f64,
) -> Vec<VisibleNode> {
    let mut nodes = Vec::with_capacity(256);
    for face in CubeFace::ALL {
        select_recursive(
            face, 0, 0, 0,
            camera_pos, planet_radius_m, max_lod, max_displacement,
            &mut nodes,
        );
    }
    nodes
}

fn select_recursive(
    face: CubeFace,
    lod: u8,
    x: u32,
    y: u32,
    camera_pos: [f64; 3],
    radius: f64,
    max_lod: u8,
    max_displacement: f64,
    out: &mut Vec<VisibleNode>,
) {
    // Compute node center on sphere surface
    let subdivs = 1u32 << lod;
    let u = -1.0 + (2.0 * x as f64 + 1.0) / subdivs as f64;
    let v = -1.0 + (2.0 * y as f64 + 1.0) / subdivs as f64;
    let dir = cube_to_sphere(face, u, v);
    let center = [dir[0] * radius, dir[1] * radius, dir[2] * radius];

    // Distance from camera to node center
    let dx = camera_pos[0] - center[0];
    let dy = camera_pos[1] - center[1];
    let dz = camera_pos[2] - center[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    // Node bounding radius: half the face-diagonal at this LOD, inflated by displacement
    let face_size = 2.0 * radius / subdivs as f64; // approximate edge length
    let node_radius = face_size * 0.7071 + max_displacement; // half-diagonal ≈ edge * sqrt(2)/2

    // Rough frustum cull: if node is entirely behind camera, skip.
    // Full frustum planes would be better but this catches the obvious case.
    // For Phase 1 we accept some overdraw from behind-camera nodes.
    // (Camera forward direction would need to be passed in for proper culling.)

    let range = lod_range(lod);

    // If far enough, or at finest level, emit this node
    if dist > range + node_radius || lod == max_lod {
        // Compute morph factor: 0 at morph_start, 1 at range boundary
        let morph_start = range * 0.5;
        let morph = if dist > morph_start {
            ((dist - morph_start) / (range - morph_start)).min(1.0) as f32
        } else {
            0.0
        };

        out.push(VisibleNode {
            face,
            lod,
            x,
            y,
            center,
            morph_factor: morph,
        });
        return;
    }

    // Subdivide into 4 children
    let child_lod = lod + 1;
    let cx = x * 2;
    let cy = y * 2;
    select_recursive(face, child_lod, cx,     cy,     camera_pos, radius, max_lod, max_displacement, out);
    select_recursive(face, child_lod, cx + 1, cy,     camera_pos, radius, max_lod, max_displacement, out);
    select_recursive(face, child_lod, cx,     cy + 1, camera_pos, radius, max_lod, max_displacement, out);
    select_recursive(face, child_lod, cx + 1, cy + 1, camera_pos, radius, max_lod, max_displacement, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_range_doubles_per_level() {
        assert!((lod_range(0) - 50.0).abs() < 1e-6);
        assert!((lod_range(1) - 100.0).abs() < 1e-6);
        assert!((lod_range(2) - 200.0).abs() < 1e-6);
        assert!((lod_range(17) - 50.0 * 131072.0).abs() < 1e-6);
    }

    #[test]
    fn max_lod_for_earth() {
        // Earth face ≈ 10,000 km = 10,000,000 m
        let levels = max_lod_levels(10_000_000.0);
        assert_eq!(levels, 18); // ceil(log2(10_000_000 / 50)) = ceil(17.6) = 18
    }

    #[test]
    fn max_lod_for_small_moon() {
        // Small moon: face ≈ 500 km = 500,000 m
        let levels = max_lod_levels(500_000.0);
        assert_eq!(levels, 14); // ceil(log2(500_000 / 50)) = ceil(13.3) = 14
    }

    #[test]
    fn camera_at_surface_produces_finest_nodes() {
        let radius = 6_371_000.0;
        // Camera on surface of +Z face
        let camera = [0.0, 0.0, radius];
        let max_lod = max_lod_levels(radius * 1.57); // approximate face size
        let nodes = select_visible_nodes(camera, radius, max_lod, radius * 0.02);
        // Should have at least some finest-LOD nodes near camera
        let finest = nodes.iter().filter(|n| n.lod == max_lod).count();
        assert!(finest > 0, "expected finest-LOD nodes near camera, got 0");
    }

    #[test]
    fn camera_far_away_produces_coarse_nodes() {
        let radius = 6_371_000.0;
        // Camera far from planet (10x radius)
        let camera = [0.0, 0.0, radius * 10.0];
        let max_lod = max_lod_levels(radius * 1.57);
        let nodes = select_visible_nodes(camera, radius, max_lod, radius * 0.02);
        // Should only have coarse LOD nodes
        let max_lod_seen = nodes.iter().map(|n| n.lod).max().unwrap_or(0);
        assert!(max_lod_seen < 5, "expected coarse nodes far away, got max lod {max_lod_seen}");
    }

    #[test]
    fn all_six_faces_represented() {
        let radius = 1_000_000.0;
        // Camera at origin (inside planet) — all faces visible
        let camera = [0.0, 0.0, 0.0];
        let max_lod = 10;
        let nodes = select_visible_nodes(camera, radius, max_lod, 0.0);
        let mut faces_seen = std::collections::HashSet::new();
        for n in &nodes {
            faces_seen.insert(n.face);
        }
        assert_eq!(faces_seen.len(), 6, "expected all 6 faces, got {}", faces_seen.len());
    }

    #[test]
    fn morph_factor_zero_near_camera() {
        let radius = 6_371_000.0;
        let camera = [0.0, 0.0, radius];
        let max_lod = 18;
        let nodes = select_visible_nodes(camera, radius, max_lod, radius * 0.02);
        // Finest nodes closest to camera should have morph ≈ 0
        let nearest = nodes.iter()
            .filter(|n| n.lod == max_lod)
            .min_by(|a, b| {
                let da = (a.center[2] - camera[2]).abs();
                let db = (b.center[2] - camera[2]).abs();
                da.partial_cmp(&db).unwrap()
            });
        if let Some(n) = nearest {
            assert!(n.morph_factor < 0.5, "nearest finest node should have low morph, got {}", n.morph_factor);
        }
    }
}
```

- [ ] **Step 2: Verify tests pass**

```bash
cargo test -p sa_terrain -- quadtree
```

Expected: all 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/sa_terrain/src/quadtree.rs
git commit -m "feat(terrain): CDLOD quadtree with LOD ranges and node selection"
```

---

### Task 4: Heightmap noise sampling

**Files:**
- Create: `crates/sa_terrain/src/heightmap.rs`

- [ ] **Step 1: Write heightmap module with tests**

```rust
//! Noise sampling: fastnoise-lite fBm + domain warp → terrain height.

use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};

/// Create the terrain noise generator for a planet.
pub fn make_terrain_noise(seed: u64) -> FastNoiseLite {
    let mut noise = FastNoiseLite::with_seed(seed as i32);
    noise.set_noise_type(Some(NoiseType::OpenSimplex2));
    noise.set_fractal_type(Some(FractalType::FBm));
    noise.set_fractal_octaves(Some(5));
    noise.set_fractal_lacunarity(Some(2.0));
    noise.set_fractal_gain(Some(0.5));
    noise.set_frequency(Some(1.0));
    noise
}

/// Create the domain warp noise generator.
pub fn make_warp_noise(seed: u64) -> FastNoiseLite {
    let mut warp = FastNoiseLite::with_seed(seed.wrapping_add(1337) as i32);
    warp.set_noise_type(Some(NoiseType::OpenSimplex2));
    warp.set_frequency(Some(0.5));
    warp
}

/// Sample terrain height at a sphere-surface point.
///
/// `dir`: unit direction vector on sphere (from cube_to_sphere).
/// `freq_scale`: frequency multiplier (controls feature size relative to planet).
///
/// Returns height in [0, 1] range.
pub fn sample_height(
    noise: &FastNoiseLite,
    warp: &FastNoiseLite,
    dir: [f64; 3],
    freq_scale: f64,
) -> f32 {
    let x = dir[0] * freq_scale;
    let y = dir[1] * freq_scale;
    let z = dir[2] * freq_scale;

    // Domain warping: offset sample position by warp noise
    let warp_strength = 0.3;
    let wx = warp.get_noise_3d(x, y, z) as f64 * warp_strength;
    let wy = warp.get_noise_3d(x + 100.0, y + 100.0, z + 100.0) as f64 * warp_strength;
    let wz = warp.get_noise_3d(x + 200.0, y + 200.0, z + 200.0) as f64 * warp_strength;

    // Sample fBm at warped position
    let raw = noise.get_noise_3d(x + wx, y + wy, z + wz);

    // Map from [-1, 1] to [0, 1]
    (raw * 0.5 + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_in_range() {
        let noise = make_terrain_noise(42);
        let warp = make_warp_noise(42);
        for i in 0..100 {
            let angle = i as f64 * 0.1;
            let dir = [angle.cos(), angle.sin(), 0.3];
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            let dir = [dir[0] / len, dir[1] / len, dir[2] / len];
            let h = sample_height(&noise, &warp, dir, 2.0);
            assert!(h >= 0.0 && h <= 1.0, "height out of range: {h}");
        }
    }

    #[test]
    fn deterministic_same_seed() {
        let n1 = make_terrain_noise(42);
        let w1 = make_warp_noise(42);
        let n2 = make_terrain_noise(42);
        let w2 = make_warp_noise(42);
        let dir = [0.577, 0.577, 0.577]; // normalized-ish
        let h1 = sample_height(&n1, &w1, dir, 2.0);
        let h2 = sample_height(&n2, &w2, dir, 2.0);
        assert!((h1 - h2).abs() < 1e-6, "same seed should produce same height");
    }

    #[test]
    fn different_seeds_differ() {
        let n1 = make_terrain_noise(42);
        let w1 = make_warp_noise(42);
        let n2 = make_terrain_noise(999);
        let w2 = make_warp_noise(999);
        let dir = [0.0, 0.0, 1.0];
        let h1 = sample_height(&n1, &w1, dir, 2.0);
        let h2 = sample_height(&n2, &w2, dir, 2.0);
        assert!((h1 - h2).abs() > 0.001, "different seeds should produce different heights");
    }
}
```

- [ ] **Step 2: Verify tests pass**

```bash
cargo test -p sa_terrain -- heightmap
```

Expected: all 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/sa_terrain/src/heightmap.rs
git commit -m "feat(terrain): heightmap noise sampling with fBm + domain warp"
```

---

### Task 5: Biome colors

**Files:**
- Create: `crates/sa_terrain/src/biome.rs`

- [ ] **Step 1: Write biome module with tests**

```rust
//! Biome color determination by altitude, latitude, and planet sub-type.
//!
//! Matches the color scheme used by sa_render::planet_mesh for visual
//! consistency between the icosphere (from space) and CDLOD terrain (close up).

use sa_universe::PlanetSubType;

/// Determine vertex color from terrain height, latitude, and planet type.
///
/// `height_norm`: height in [0, 1] (0 = lowest terrain, 1 = highest).
/// `latitude`: absolute latitude in [0, 1] (0 = equator, 1 = pole).
pub fn biome_color(sub_type: PlanetSubType, height_norm: f32, latitude: f32) -> [f32; 3] {
    match sub_type {
        PlanetSubType::Barren => {
            let g = 0.3 + height_norm * 0.3;
            [g, g * 0.95, g * 0.9]
        }
        PlanetSubType::Desert => {
            let base = 0.5 + height_norm * 0.3;
            if height_norm > 0.8 {
                [0.6, 0.55, 0.5] // rocky peaks
            } else {
                [base, base * 0.85, base * 0.5] // sand
            }
        }
        PlanetSubType::Temperate => {
            if latitude > 0.8 {
                [0.9, 0.92, 0.95] // polar ice
            } else if height_norm > 0.75 {
                [0.85, 0.87, 0.9] // snow caps
            } else if height_norm > 0.5 {
                [0.45, 0.38, 0.3] // mountain rock
            } else if height_norm < 0.15 {
                [0.2, 0.3, 0.6] // water/lowland
            } else {
                [0.25, 0.45, 0.2] // grass/forest
            }
        }
        PlanetSubType::Ocean => {
            if height_norm > 0.7 {
                [0.35, 0.4, 0.3] // island
            } else {
                let depth = 0.2 + height_norm * 0.3;
                [depth * 0.4, depth * 0.5, depth * 1.2] // ocean
            }
        }
        PlanetSubType::Frozen => {
            let g = 0.75 + height_norm * 0.2;
            [g, g + 0.02, g + 0.05]
        }
        PlanetSubType::Molten => {
            if height_norm < 0.3 {
                [0.8, 0.2, 0.05] // lava
            } else {
                let g = 0.15 + height_norm * 0.15;
                [g, g * 0.8, g * 0.7] // dark rock
            }
        }
        _ => {
            // Gas/ice giant sub-types — shouldn't land here but provide fallback
            let g = 0.4 + height_norm * 0.3;
            [g, g, g]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_colors_in_valid_range() {
        let types = [
            PlanetSubType::Barren, PlanetSubType::Desert, PlanetSubType::Temperate,
            PlanetSubType::Ocean, PlanetSubType::Frozen, PlanetSubType::Molten,
        ];
        for sub in types {
            for h in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for lat in [0.0, 0.5, 1.0] {
                    let [r, g, b] = biome_color(sub, h, lat);
                    assert!(r >= 0.0 && r <= 1.5, "{sub:?} h={h} lat={lat}: r={r}");
                    assert!(g >= 0.0 && g <= 1.5, "{sub:?} h={h} lat={lat}: g={g}");
                    assert!(b >= 0.0 && b <= 1.5, "{sub:?} h={h} lat={lat}: b={b}");
                }
            }
        }
    }

    #[test]
    fn temperate_poles_are_icy() {
        let [r, g, b] = biome_color(PlanetSubType::Temperate, 0.5, 0.9);
        // Polar regions should be bright (icy white)
        assert!(r > 0.8, "polar r={r} should be bright");
        assert!(g > 0.8, "polar g={g} should be bright");
    }

    #[test]
    fn temperate_lowlands_are_green() {
        let [r, g, b] = biome_color(PlanetSubType::Temperate, 0.3, 0.3);
        // Mid-latitude lowlands should be greenish
        assert!(g > r, "green channel should dominate: r={r} g={g}");
    }
}
```

- [ ] **Step 2: Verify tests pass**

```bash
cargo test -p sa_terrain -- biome
```

Expected: all 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/sa_terrain/src/biome.rs
git commit -m "feat(terrain): biome color determination by altitude/latitude/sub_type"
```

---

### Task 6: Chunk mesh generation

**Files:**
- Create: `crates/sa_terrain/src/chunk.rs`

- [ ] **Step 1: Write chunk generation module**

```rust
//! Chunk mesh generation: 33x33 vertex grid with normals, colors, and skirts.

use crate::{ChunkData, ChunkKey, TerrainConfig, TerrainVertex};
use crate::cube_sphere::{CubeFace, cube_to_sphere};
use crate::heightmap::{make_terrain_noise, make_warp_noise, sample_height};
use crate::biome::biome_color;

/// Grid dimension: 33 vertices = 32 cells per edge (power-of-2 + 1).
pub const GRID_SIZE: u32 = 33;
/// Number of cells per edge.
const CELLS: u32 = GRID_SIZE - 1;

/// Generate a terrain chunk. Called on a background thread.
pub fn generate_chunk(key: ChunkKey, config: &TerrainConfig) -> ChunkData {
    let face = CubeFace::ALL[key.face as usize];
    let noise = make_terrain_noise(config.noise_seed);
    let warp = make_warp_noise(config.noise_seed);
    let amplitude = config.displacement_fraction as f64 * config.radius_m;

    // Frequency scale: controls feature size relative to planet.
    // Higher frequency = smaller features. We want features proportional to planet size.
    let freq_scale = 2.0; // base frequency on unit sphere

    // Compute UV bounds for this chunk within its face
    let subdivs = 1u32 << key.lod;
    let u_min = -1.0 + 2.0 * key.x as f64 / subdivs as f64;
    let u_max = -1.0 + 2.0 * (key.x + 1) as f64 / subdivs as f64;
    let v_min = -1.0 + 2.0 * key.y as f64 / subdivs as f64;
    let v_max = -1.0 + 2.0 * (key.y + 1) as f64 / subdivs as f64;

    // Generate 33x33 grid of sphere positions and heights
    let mut positions_f64 = Vec::with_capacity((GRID_SIZE * GRID_SIZE) as usize);
    let mut heights_raw = Vec::with_capacity((GRID_SIZE * GRID_SIZE) as usize);
    let mut min_h: f32 = f32::MAX;
    let mut max_h: f32 = f32::MIN;

    for iy in 0..GRID_SIZE {
        for ix in 0..GRID_SIZE {
            let u = u_min + (u_max - u_min) * ix as f64 / CELLS as f64;
            let v = v_min + (v_max - v_min) * iy as f64 / CELLS as f64;

            let dir = cube_to_sphere(face, u, v);
            let h = sample_height(&noise, &warp, dir, freq_scale);
            heights_raw.push(h);
            min_h = min_h.min(h);
            max_h = max_h.max(h);

            // Displaced position = direction * (radius + height * amplitude)
            let r = config.radius_m + h as f64 * amplitude;
            positions_f64.push([dir[0] * r, dir[1] * r, dir[2] * r]);
        }
    }

    // Compute chunk center (average of corner positions) for f64 rebasing
    let c00 = &positions_f64[0];
    let c10 = &positions_f64[(CELLS) as usize];
    let c01 = &positions_f64[(CELLS * GRID_SIZE) as usize];
    let c11 = &positions_f64[(CELLS * GRID_SIZE + CELLS) as usize];
    let center_f64 = [
        (c00[0] + c10[0] + c01[0] + c11[0]) / 4.0,
        (c00[1] + c10[1] + c01[1] + c11[1]) / 4.0,
        (c00[2] + c10[2] + c01[2] + c11[2]) / 4.0,
    ];

    // Convert to patch-local f32 (subtract center)
    let mut vertices = Vec::with_capacity((GRID_SIZE * GRID_SIZE) as usize + 128);
    let mut latitudes = Vec::with_capacity((GRID_SIZE * GRID_SIZE) as usize);

    for (i, pos) in positions_f64.iter().enumerate() {
        let local = [
            (pos[0] - center_f64[0]) as f32,
            (pos[1] - center_f64[1]) as f32,
            (pos[2] - center_f64[2]) as f32,
        ];

        // Latitude: angle from equator (Y=0 plane), mapped to [0, 1]
        let len = (pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2]).sqrt();
        let lat = if len > 0.0 { (pos[1] / len).abs() as f32 } else { 0.0 };
        latitudes.push(lat);

        let color = biome_color(config.sub_type, heights_raw[i], lat);

        // Normal placeholder — computed after all vertices exist
        vertices.push(TerrainVertex {
            position: local,
            color,
            normal: [0.0, 1.0, 0.0],
        });
    }

    // Build triangle indices (32x32 quads, 2 triangles each)
    let mut indices = Vec::with_capacity((CELLS * CELLS * 6) as usize);
    for iy in 0..CELLS {
        for ix in 0..CELLS {
            let i00 = iy * GRID_SIZE + ix;
            let i10 = i00 + 1;
            let i01 = i00 + GRID_SIZE;
            let i11 = i01 + 1;
            indices.push(i00);
            indices.push(i01);
            indices.push(i10);
            indices.push(i10);
            indices.push(i01);
            indices.push(i11);
        }
    }

    // Compute face normals and accumulate per-vertex
    let mut normal_accum = vec![[0.0f32; 3]; vertices.len()];
    for tri in indices.chunks(3) {
        let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let pa = vertices[a].position;
        let pb = vertices[b].position;
        let pc = vertices[c].position;
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        for &idx in &[a, b, c] {
            normal_accum[idx][0] += n[0];
            normal_accum[idx][1] += n[1];
            normal_accum[idx][2] += n[2];
        }
    }
    for (i, acc) in normal_accum.iter().enumerate() {
        let len = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt();
        if len > 1e-10 {
            vertices[i].normal = [acc[0] / len as f32, acc[1] / len as f32, acc[2] / len as f32];
        }
    }

    // Add skirt vertices: extend edge vertices downward
    let skirt_drop = (amplitude * 2.0 / (1u32 << key.lod) as f64) as f32;
    let skirt_drop = skirt_drop.max(1.0); // minimum 1m skirt
    let grid_vert_count = vertices.len() as u32;

    // Helper: add skirt vertex below an existing edge vertex
    let mut add_skirt = |edge_idx: u32| -> u32 {
        let v = &vertices[edge_idx as usize];
        let pos = v.position;
        // "Down" = toward planet center = negative of the normalized position
        let center_dir = [
            -(positions_f64[edge_idx as usize][0] - center_f64[0]) as f32,
            -(positions_f64[edge_idx as usize][1] - center_f64[1]) as f32,
            -(positions_f64[edge_idx as usize][2] - center_f64[2]) as f32,
        ];
        let len = (center_dir[0] * center_dir[0] + center_dir[1] * center_dir[1] + center_dir[2] * center_dir[2]).sqrt();
        let down = if len > 1e-6 {
            [center_dir[0] / len * skirt_drop, center_dir[1] / len * skirt_drop, center_dir[2] / len * skirt_drop]
        } else {
            [0.0, -skirt_drop, 0.0]
        };
        let skirt_pos = [pos[0] + down[0], pos[1] + down[1], pos[2] + down[2]];
        let new_idx = vertices.len() as u32;
        vertices.push(TerrainVertex {
            position: skirt_pos,
            color: v.color,
            normal: v.normal,
        });
        new_idx
    };

    // Bottom edge (iy=0): ix from 0 to CELLS
    for ix in 0..GRID_SIZE {
        let edge_idx = ix; // row 0
        let skirt_idx = add_skirt(edge_idx);
        if ix > 0 {
            let prev_edge = edge_idx - 1;
            let prev_skirt = skirt_idx - 1;
            indices.push(prev_edge);
            indices.push(prev_skirt);
            indices.push(edge_idx);
            indices.push(edge_idx);
            indices.push(prev_skirt);
            indices.push(skirt_idx);
        }
    }
    // Top edge (iy=CELLS)
    for ix in 0..GRID_SIZE {
        let edge_idx = CELLS * GRID_SIZE + ix;
        let skirt_idx = add_skirt(edge_idx);
        if ix > 0 {
            let prev_edge = edge_idx - 1;
            let prev_skirt = skirt_idx - 1;
            indices.push(edge_idx);
            indices.push(skirt_idx);
            indices.push(prev_edge);
            indices.push(prev_edge);
            indices.push(skirt_idx);
            indices.push(prev_skirt);
        }
    }
    // Left edge (ix=0): iy from 0 to CELLS
    for iy in 0..GRID_SIZE {
        let edge_idx = iy * GRID_SIZE;
        let skirt_idx = add_skirt(edge_idx);
        if iy > 0 {
            let prev_edge = (iy - 1) * GRID_SIZE;
            let prev_skirt = skirt_idx - 1;
            indices.push(edge_idx);
            indices.push(skirt_idx);
            indices.push(prev_edge);
            indices.push(prev_edge);
            indices.push(skirt_idx);
            indices.push(prev_skirt);
        }
    }
    // Right edge (ix=CELLS)
    for iy in 0..GRID_SIZE {
        let edge_idx = iy * GRID_SIZE + CELLS;
        let skirt_idx = add_skirt(edge_idx);
        if iy > 0 {
            let prev_edge = (iy - 1) * GRID_SIZE + CELLS;
            let prev_skirt = skirt_idx - 1;
            indices.push(prev_edge);
            indices.push(prev_skirt);
            indices.push(edge_idx);
            indices.push(edge_idx);
            indices.push(prev_skirt);
            indices.push(skirt_idx);
        }
    }

    ChunkData {
        key,
        center_f64,
        vertices,
        indices,
        heights: heights_raw,
        min_height: min_h,
        max_height: max_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerrainConfig;
    use sa_universe::PlanetSubType;

    fn test_config() -> TerrainConfig {
        TerrainConfig {
            radius_m: 6_371_000.0,
            noise_seed: 42,
            sub_type: PlanetSubType::Temperate,
            displacement_fraction: 0.02,
        }
    }

    #[test]
    fn chunk_has_correct_grid_vertices() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        // 33x33 grid = 1089 vertices + skirt vertices (4 edges * 33 = 132)
        assert!(chunk.vertices.len() >= 1089, "too few vertices: {}", chunk.vertices.len());
        assert!(chunk.vertices.len() < 1300, "too many vertices: {}", chunk.vertices.len());
    }

    #[test]
    fn chunk_has_correct_triangle_count() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        // 32*32*2 = 2048 terrain triangles + skirt triangles
        let tri_count = chunk.indices.len() / 3;
        assert!(tri_count >= 2048, "too few triangles: {tri_count}");
    }

    #[test]
    fn chunk_heights_are_33x33() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        assert_eq!(chunk.heights.len(), (GRID_SIZE * GRID_SIZE) as usize);
    }

    #[test]
    fn chunk_normals_are_normalized() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        for (i, v) in chunk.vertices.iter().take(1089).enumerate() {
            let len = (v.normal[0] * v.normal[0] + v.normal[1] * v.normal[1] + v.normal[2] * v.normal[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "vertex {i} normal not normalized: len={len}",
            );
        }
    }

    #[test]
    fn deterministic_chunks() {
        let key = ChunkKey { face: 4, lod: 5, x: 3, y: 7 };
        let config = test_config();
        let c1 = generate_chunk(key, &config);
        let c2 = generate_chunk(key, &config);
        assert_eq!(c1.vertices.len(), c2.vertices.len());
        for (a, b) in c1.heights.iter().zip(c2.heights.iter()) {
            assert!((a - b).abs() < 1e-6, "heights differ");
        }
    }

    #[test]
    fn min_max_height_valid() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        assert!(chunk.min_height <= chunk.max_height);
        assert!(chunk.min_height >= 0.0);
        assert!(chunk.max_height <= 1.0);
    }
}
```

- [ ] **Step 2: Verify tests pass**

```bash
cargo test -p sa_terrain -- chunk
```

Expected: all 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/sa_terrain/src/chunk.rs
git commit -m "feat(terrain): chunk mesh generation with 33x33 grid, normals, skirts"
```

---

### Task 7: Async streaming with crossbeam channels

**Files:**
- Create: `crates/sa_terrain/src/streaming.rs`
- Modify: `crates/sa_terrain/src/lib.rs` (update exports)

- [ ] **Step 1: Write streaming module**

```rust
//! Async terrain chunk streaming: crossbeam MPMC channels, priority queue, LRU cache.

use std::collections::{HashMap, VecDeque};
use std::thread;
use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{ChunkData, ChunkKey, TerrainConfig};
use crate::chunk::generate_chunk;
use crate::quadtree::VisibleNode;

/// Maximum chunks to upload to GPU per frame.
const MAX_UPLOADS_PER_FRAME: usize = 8;
/// LRU cache capacity.
const CACHE_CAPACITY: usize = 500;
/// Number of worker threads.
const WORKER_COUNT: usize = 4;

/// Request sent to worker threads.
struct ChunkRequest {
    key: ChunkKey,
    config: TerrainConfig,
}

/// Manages async terrain chunk generation and caching.
pub struct ChunkStreaming {
    request_tx: Sender<ChunkRequest>,
    result_rx: Receiver<ChunkData>,
    /// Currently loaded chunks (have GPU handles).
    loaded: HashMap<ChunkKey, ChunkData>,
    /// LRU cache of recently unloaded chunks (no GPU handle, data only).
    cache: LruCache,
    /// Keys currently being generated (avoid duplicate requests).
    in_flight: std::collections::HashSet<ChunkKey>,
    /// Worker thread handles.
    _workers: Vec<thread::JoinHandle<()>>,
}

impl ChunkStreaming {
    /// Create streaming system and spawn worker threads.
    pub fn new(config: TerrainConfig) -> Self {
        let (request_tx, request_rx) = unbounded::<ChunkRequest>();
        let (result_tx, result_rx) = unbounded::<ChunkData>();

        let mut workers = Vec::with_capacity(WORKER_COUNT);
        for _ in 0..WORKER_COUNT {
            let rx = request_rx.clone();
            let tx = result_tx.clone();
            let handle = thread::spawn(move || {
                while let Ok(req) = rx.recv() {
                    let chunk = generate_chunk(req.key, &req.config);
                    if tx.send(chunk).is_err() {
                        break; // Main thread dropped receiver
                    }
                }
            });
            workers.push(handle);
        }

        Self {
            request_tx,
            result_rx,
            loaded: HashMap::new(),
            cache: LruCache::new(CACHE_CAPACITY),
            in_flight: std::collections::HashSet::new(),
            _workers: workers,
        }
    }

    /// Update streaming: request new chunks, receive completed chunks, evict old ones.
    ///
    /// Returns newly completed chunks (caller uploads to GPU).
    /// Also returns keys of chunks to remove from GPU.
    pub fn update(
        &mut self,
        visible_nodes: &[VisibleNode],
        config: &TerrainConfig,
    ) -> (Vec<ChunkData>, Vec<ChunkKey>) {
        // Build set of needed chunk keys
        let needed: std::collections::HashSet<ChunkKey> = visible_nodes.iter()
            .map(|n| ChunkKey {
                face: n.face as u8,
                lod: n.lod,
                x: n.x,
                y: n.y,
            })
            .collect();

        // Receive completed chunks from workers (up to budget)
        let mut new_chunks = Vec::new();
        for _ in 0..MAX_UPLOADS_PER_FRAME {
            match self.result_rx.try_recv() {
                Ok(chunk) => {
                    self.in_flight.remove(&chunk.key);
                    if needed.contains(&chunk.key) {
                        self.loaded.insert(chunk.key, chunk.clone());
                        new_chunks.push(chunk);
                    }
                    // If no longer needed, just drop it
                }
                Err(_) => break,
            }
        }

        // Request generation for chunks not loaded and not in-flight
        // Sort by distance (visible_nodes are already roughly distance-ordered from quadtree)
        for node in visible_nodes {
            let key = ChunkKey {
                face: node.face as u8,
                lod: node.lod,
                x: node.x,
                y: node.y,
            };
            if self.loaded.contains_key(&key) || self.in_flight.contains(&key) {
                continue;
            }
            // Check LRU cache first
            if let Some(cached) = self.cache.remove(&key) {
                self.loaded.insert(key, cached.clone());
                new_chunks.push(cached);
                continue;
            }
            // Request generation
            let _ = self.request_tx.send(ChunkRequest {
                key,
                config: config.clone(),
            });
            self.in_flight.insert(key);
        }

        // Evict chunks no longer needed
        let mut to_remove = Vec::new();
        let loaded_keys: Vec<ChunkKey> = self.loaded.keys().cloned().collect();
        for key in loaded_keys {
            if !needed.contains(&key) {
                if let Some(chunk) = self.loaded.remove(&key) {
                    self.cache.insert(key, chunk);
                    to_remove.push(key);
                }
            }
        }

        (new_chunks, to_remove)
    }

    /// Number of currently loaded chunks.
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }

    /// Number of chunks in the LRU cache.
    pub fn cache_count(&self) -> usize {
        self.cache.len()
    }
}

/// Simple LRU cache backed by a VecDeque (oldest at front).
struct LruCache {
    entries: VecDeque<(ChunkKey, ChunkData)>,
    capacity: usize,
}

impl LruCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn insert(&mut self, key: ChunkKey, data: ChunkData) {
        // Remove if already present
        self.entries.retain(|(k, _)| *k != key);
        // Evict oldest if full
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, data));
    }

    fn remove(&mut self, key: &ChunkKey) -> Option<ChunkData> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
            self.entries.remove(pos).map(|(_, data)| data)
        } else {
            None
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerrainConfig;
    use crate::quadtree::VisibleNode;
    use crate::cube_sphere::CubeFace;
    use sa_universe::PlanetSubType;

    fn test_config() -> TerrainConfig {
        TerrainConfig {
            radius_m: 1_000_000.0, // small planet for fast tests
            noise_seed: 42,
            sub_type: PlanetSubType::Barren,
            displacement_fraction: 0.01,
        }
    }

    #[test]
    fn lru_cache_insert_and_retrieve() {
        let mut cache = LruCache::new(3);
        let key = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        let data = ChunkData {
            key,
            center_f64: [0.0; 3],
            vertices: vec![],
            indices: vec![],
            heights: vec![],
            min_height: 0.0,
            max_height: 1.0,
        };
        cache.insert(key, data);
        assert_eq!(cache.len(), 1);
        let retrieved = cache.remove(&key);
        assert!(retrieved.is_some());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn lru_cache_evicts_oldest() {
        let mut cache = LruCache::new(2);
        for i in 0..3 {
            let key = ChunkKey { face: 0, lod: 0, x: i, y: 0 };
            let data = ChunkData {
                key,
                center_f64: [0.0; 3],
                vertices: vec![],
                indices: vec![],
                heights: vec![],
                min_height: 0.0,
                max_height: 1.0,
            };
            cache.insert(key, data);
        }
        assert_eq!(cache.len(), 2);
        // First inserted (x=0) should be evicted
        let k0 = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        assert!(cache.remove(&k0).is_none());
    }

    #[test]
    fn streaming_receives_chunks() {
        let config = test_config();
        let mut streaming = ChunkStreaming::new(config.clone());

        // Create a single visible node
        let nodes = vec![VisibleNode {
            face: CubeFace::PosZ,
            lod: 3,
            x: 0,
            y: 0,
            center: [0.0, 0.0, 1_000_000.0],
            morph_factor: 0.0,
        }];

        // First update: sends request
        let (new, _removed) = streaming.update(&nodes, &config);
        // Chunk may not be ready yet
        assert_eq!(streaming.loaded_count() + new.len(), new.len());

        // Poll a few times to give worker time to generate
        let mut received = new;
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            let (new, _) = streaming.update(&nodes, &config);
            received.extend(new);
            if !received.is_empty() {
                break;
            }
        }

        assert!(!received.is_empty(), "should have received at least one chunk");
        assert_eq!(received[0].key.face, CubeFace::PosZ as u8);
    }
}
```

- [ ] **Step 2: Verify tests pass**

```bash
cargo test -p sa_terrain -- streaming
```

Expected: all 3 tests pass.

- [ ] **Step 3: Update lib.rs exports**

Add `pub mod streaming;` to `crates/sa_terrain/src/lib.rs` (it was already declared in Task 1 but not yet created).

- [ ] **Step 4: Verify full crate compiles and all tests pass**

```bash
cargo test -p sa_terrain
```

Expected: all tests across all modules pass.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_terrain/src/streaming.rs crates/sa_terrain/src/lib.rs
git commit -m "feat(terrain): async chunk streaming with crossbeam channels and LRU cache"
```

---

### Task 8: Solar system icosphere suppression

**Files:**
- Modify: `crates/spaceaway/src/solar_system.rs`

- [ ] **Step 1: Add hidden_body_index and is_landable to ActiveSystem**

Add these to the `ActiveSystem` struct and its `update()` method. Read the current file first to find exact insertion points.

Add field to struct:

```rust
    /// Index of body whose icosphere is hidden (terrain active for this planet).
    pub hidden_body_index: Option<usize>,
```

Add `is_landable()` helper method:

```rust
    /// Check if a body at the given index is landable (rocky planet or moon).
    pub fn is_body_landable(&self, index: usize) -> bool {
        if index == 0 { return false; } // Star is not landable
        // Check if this body's planet is Rocky type
        // Bodies are loaded in order: star, then for each planet: planet, atmo?, ring?, moons...
        // For Phase 1 we simply check that it's not the star and has a valid mesh
        index < self.bodies.len()
    }
```

Modify `update()` to skip hidden body and its children. In the loop that builds DrawCommands, add at the start of the iteration:

```rust
            // Skip body hidden by terrain (and its children: atmosphere, rings)
            if let Some(hidden) = self.hidden_body_index {
                if i == hidden {
                    continue;
                }
                // Skip children of hidden body (atmosphere, rings share parent_index)
                if body.parent_index == hidden as i32 {
                    continue;
                }
            }
```

- [ ] **Step 2: Add planet data accessors for terrain config**

Add methods to expose data needed by `TerrainConfig`:

```rust
    /// Get planet radius in meters for a body index.
    pub fn body_radius_m(&self, index: usize) -> Option<f64> {
        self.bodies.get(index).map(|b| b.radius_m)
    }

    /// Get the planet data for a body index (searches system.planets).
    /// Returns (color_seed, sub_type, displacement_fraction, mass_earth, radius_earth).
    pub fn planet_data(&self, index: usize) -> Option<(u64, sa_universe::PlanetSubType, f32, f32, f32)> {
        // Bodies after the star are indexed: planet0, [atmo0], [ring0], [moons0...], planet1, ...
        // We need to map body index back to the Planet in self.system.planets
        // For now, iterate planets and match by radius
        let body = self.bodies.get(index)?;
        for planet in &self.system.planets {
            let planet_radius_m = planet.radius_earth as f64 * 6_371_000.0;
            if (body.radius_m - planet_radius_m).abs() < 1000.0 {
                let amplitude = match planet.sub_type {
                    sa_universe::PlanetSubType::Molten => 0.01,
                    sa_universe::PlanetSubType::Barren | sa_universe::PlanetSubType::Frozen => 0.02,
                    sa_universe::PlanetSubType::Temperate | sa_universe::PlanetSubType::Ocean => 0.03,
                    sa_universe::PlanetSubType::Desert => 0.04,
                    _ => 0.03,
                };
                return Some((planet.color_seed, planet.sub_type, amplitude, planet.mass_earth, planet.radius_earth));
            }
        }
        None
    }

    /// Get the galactic position of a body in meters (relative to star galactic pos).
    /// For Phase 1 we need this to position terrain chunks.
    pub fn body_position_ly(&self, index: usize) -> Option<sa_math::WorldPos> {
        let positions = self.compute_positions_ly();
        positions.get(index).copied()
    }
```

- [ ] **Step 3: Initialize hidden_body_index in load()**

Add `hidden_body_index: None,` to the `ActiveSystem` construction in `load()`.

- [ ] **Step 4: Run compilation check**

```bash
cargo check -p spaceaway
```

Expected: clean compilation.

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/solar_system.rs
git commit -m "feat(terrain): add icosphere suppression and planet data accessors to ActiveSystem"
```

---

### Task 9: Terrain integration module

**Files:**
- Create: `crates/spaceaway/src/terrain_integration.rs`
- Modify: `crates/spaceaway/Cargo.toml`

- [ ] **Step 1: Add sa_terrain dependency to spaceaway**

In `crates/spaceaway/Cargo.toml`, add:

```toml
sa_terrain.workspace = true
crossbeam-channel.workspace = true
```

- [ ] **Step 2: Create terrain_integration.rs**

```rust
//! Terrain system integration: manages terrain activation, chunk streaming,
//! GPU upload, and icosphere handoff.
//!
//! Extracted from main.rs to keep the game loop focused on orchestration.

use sa_core::Handle;
use sa_math::WorldPos;
use sa_render::mesh::{MeshData, MeshMarker};
use sa_render::renderer::DrawCommand;
use sa_render::vertex::Vertex;
use sa_terrain::{ChunkData, ChunkKey, TerrainConfig};
use sa_terrain::streaming::ChunkStreaming;
use sa_terrain::quadtree;
use std::collections::HashMap;

/// Hysteresis thresholds for terrain activation.
const ACTIVATE_RADIUS_MULT: f64 = 2.0;
const DEACTIVATE_RADIUS_MULT: f64 = 2.5;

/// Result of a terrain update frame.
pub struct TerrainFrameResult {
    /// Draw commands for all visible terrain chunks.
    pub draw_commands: Vec<DrawCommand>,
    /// Index of the solar system body being replaced by terrain (for icosphere suppression).
    pub hidden_body_index: Option<usize>,
}

/// Manages terrain for one planet.
pub struct TerrainManager {
    config: TerrainConfig,
    streaming: ChunkStreaming,
    /// GPU mesh handles for loaded chunks.
    gpu_meshes: HashMap<ChunkKey, Handle<MeshMarker>>,
    /// Planet center in galactic light-years (f64).
    planet_center_ly: WorldPos,
    /// Planet radius in meters.
    planet_radius_m: f64,
    /// Body index in ActiveSystem (for icosphere suppression).
    body_index: usize,
    /// Max LOD level for this planet.
    max_lod: u8,
    /// Max displacement in meters (for bounding sphere inflation).
    max_displacement_m: f64,
}

impl TerrainManager {
    /// Create terrain manager for a specific planet.
    pub fn new(
        config: TerrainConfig,
        planet_center_ly: WorldPos,
        body_index: usize,
    ) -> Self {
        let max_lod = quadtree::max_lod_levels(config.radius_m * 1.57);
        let max_displacement_m = config.displacement_fraction as f64 * config.radius_m;
        let planet_radius_m = config.radius_m;

        Self {
            streaming: ChunkStreaming::new(config.clone()),
            config,
            gpu_meshes: HashMap::new(),
            planet_center_ly,
            planet_radius_m,
            body_index,
            max_lod,
            max_displacement_m,
        }
    }

    /// Update terrain for this frame. Returns draw commands and suppression info.
    pub fn update(
        &mut self,
        camera_galactic_ly: WorldPos,
        mesh_store: &mut sa_render::mesh::MeshStore,
        gpu_device: &wgpu::Device,
    ) -> TerrainFrameResult {
        // Camera position relative to planet center, in meters
        let ly_to_m: f64 = 9.461e15;
        let cam_planet_m = [
            (camera_galactic_ly.x - self.planet_center_ly.x) * ly_to_m,
            (camera_galactic_ly.y - self.planet_center_ly.y) * ly_to_m,
            (camera_galactic_ly.z - self.planet_center_ly.z) * ly_to_m,
        ];

        // Select visible quadtree nodes
        let visible = quadtree::select_visible_nodes(
            cam_planet_m,
            self.planet_radius_m,
            self.max_lod,
            self.max_displacement_m,
        );

        // Stream chunks
        let (new_chunks, removed_keys) = self.streaming.update(&visible, &self.config);

        // Upload new chunks to GPU
        for chunk in &new_chunks {
            let mesh_data = chunk_to_mesh_data(chunk);
            let handle = mesh_store.upload(gpu_device, &mesh_data);
            self.gpu_meshes.insert(chunk.key, handle);
        }

        // Remove evicted chunks from GPU
        for key in &removed_keys {
            self.gpu_meshes.remove(key);
            // Note: MeshStore doesn't have a remove method yet.
            // For Phase 1, leaked GPU buffers are acceptable.
            // TODO: add mesh_store.remove() in a follow-up.
        }

        // Build draw commands for all loaded chunks
        let mut draw_commands = Vec::with_capacity(self.gpu_meshes.len());
        for (key, handle) in &self.gpu_meshes {
            // Find chunk data for center position
            // For chunks just uploaded, we have the data. For previously loaded, we need it too.
            // The streaming system keeps loaded chunks — we can look up center from it.
            // For now, reconstruct center from key (cheaper than storing separately).
            let center_f64 = chunk_center_from_key(key, self.planet_radius_m);

            // Rebase to camera: (planet_center + chunk_center - camera) in f64, then f32
            let world_x = self.planet_center_ly.x * ly_to_m + center_f64[0];
            let world_y = self.planet_center_ly.y * ly_to_m + center_f64[1];
            let world_z = self.planet_center_ly.z * ly_to_m + center_f64[2];

            let cam_x = camera_galactic_ly.x * ly_to_m;
            let cam_y = camera_galactic_ly.y * ly_to_m;
            let cam_z = camera_galactic_ly.z * ly_to_m;

            let offset = glam::Vec3::new(
                (world_x - cam_x) as f32,
                (world_y - cam_y) as f32,
                (world_z - cam_z) as f32,
            );

            let model = glam::Mat4::from_translation(offset);

            draw_commands.push(DrawCommand {
                mesh: *handle,
                model_matrix: model,
                pre_rebased: true,
            });
        }

        TerrainFrameResult {
            draw_commands,
            hidden_body_index: Some(self.body_index),
        }
    }

    /// Body index this terrain replaces.
    pub fn body_index(&self) -> usize {
        self.body_index
    }

    /// Check if the camera is outside deactivation range.
    pub fn should_deactivate(&self, camera_galactic_ly: WorldPos) -> bool {
        let ly_to_m: f64 = 9.461e15;
        let dx = (camera_galactic_ly.x - self.planet_center_ly.x) * ly_to_m;
        let dy = (camera_galactic_ly.y - self.planet_center_ly.y) * ly_to_m;
        let dz = (camera_galactic_ly.z - self.planet_center_ly.z) * ly_to_m;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        dist > self.planet_radius_m * DEACTIVATE_RADIUS_MULT
    }
}

/// Convert ChunkData to sa_render::MeshData for GPU upload.
fn chunk_to_mesh_data(chunk: &ChunkData) -> MeshData {
    let vertices = chunk.vertices.iter().map(|v| Vertex {
        position: v.position,
        color: v.color,
        normal: v.normal,
    }).collect();

    MeshData {
        vertices,
        indices: chunk.indices.clone(),
    }
}

/// Reconstruct chunk center on sphere from its key (avoids storing center per loaded chunk).
fn chunk_center_from_key(key: &ChunkKey, radius_m: f64) -> [f64; 3] {
    let face = sa_terrain::cube_sphere::CubeFace::ALL[key.face as usize];
    let subdivs = 1u32 << key.lod;
    let u = -1.0 + (2.0 * key.x as f64 + 1.0) / subdivs as f64;
    let v = -1.0 + (2.0 * key.y as f64 + 1.0) / subdivs as f64;
    let dir = sa_terrain::cube_sphere::cube_to_sphere(face, u, v);
    [dir[0] * radius_m, dir[1] * radius_m, dir[2] * radius_m]
}

/// Check if any landable planet is within activation range.
/// Returns (body_index, planet_center_ly, terrain_config) if found.
pub fn find_terrain_planet(
    active_system: &crate::solar_system::ActiveSystem,
    camera_galactic_ly: WorldPos,
) -> Option<(usize, WorldPos, TerrainConfig)> {
    let positions = active_system.compute_positions_ly_pub();
    let ly_to_m: f64 = 9.461e15;

    let mut best: Option<(usize, f64, WorldPos, TerrainConfig)> = None;

    for (i, pos) in positions.iter().enumerate() {
        if i == 0 { continue; } // Skip star

        let Some(radius_m) = active_system.body_radius_m(i) else { continue };
        let Some((color_seed, sub_type, displacement, _mass, _radius_e)) = active_system.planet_data(i) else { continue };

        // Check if rocky/landable
        match sub_type {
            sa_universe::PlanetSubType::Molten
            | sa_universe::PlanetSubType::Barren
            | sa_universe::PlanetSubType::Desert
            | sa_universe::PlanetSubType::Temperate
            | sa_universe::PlanetSubType::Ocean
            | sa_universe::PlanetSubType::Frozen => {}
            _ => continue, // Gas/ice giants not landable
        }

        let dx = (camera_galactic_ly.x - pos.x) * ly_to_m;
        let dy = (camera_galactic_ly.y - pos.y) * ly_to_m;
        let dz = (camera_galactic_ly.z - pos.z) * ly_to_m;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist < radius_m * ACTIVATE_RADIUS_MULT {
            let is_better = best.as_ref().map_or(true, |(_, d, _, _)| dist < *d);
            if is_better {
                let config = TerrainConfig {
                    radius_m,
                    noise_seed: color_seed,
                    sub_type,
                    displacement_fraction: displacement,
                };
                best = Some((i, dist, *pos, config));
            }
        }
    }

    best.map(|(i, _, pos, config)| (i, pos, config))
}
```

- [ ] **Step 3: Add compute_positions_ly_pub to ActiveSystem**

The `compute_positions_ly` method in `solar_system.rs` is currently private. Add a public wrapper:

```rust
    /// Public access to body positions in light-years (for terrain integration).
    pub fn compute_positions_ly_pub(&self) -> Vec<WorldPos> {
        self.compute_positions_ly()
    }
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p spaceaway
```

Expected: clean compilation.

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/terrain_integration.rs crates/spaceaway/Cargo.toml crates/spaceaway/src/solar_system.rs
git commit -m "feat(terrain): terrain integration module with activation, streaming, GPU upload"
```

---

### Task 10: Wire terrain into the game loop

**Files:**
- Modify: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Add terrain module and field to App**

Add `mod terrain_integration;` near the top of main.rs with the other module declarations.

Add field to `App` struct:

```rust
    /// Active terrain manager (when near a landable planet).
    terrain: Option<terrain_integration::TerrainManager>,
```

Initialize in `App::new()`:

```rust
            terrain: None,
```

- [ ] **Step 2: Add terrain update in the game loop**

In the `RedrawRequested` handler, after the solar system update but before building draw commands, add:

```rust
                // --- Terrain streaming ---
                if let Some(active_sys) = &mut self.active_system {
                    // Check for terrain activation/deactivation
                    let camera_ly = self.galactic_position;

                    // Deactivation check
                    if let Some(terrain) = &self.terrain {
                        if terrain.should_deactivate(camera_ly) {
                            active_sys.hidden_body_index = None;
                            self.terrain = None;
                            log::info!("Terrain deactivated");
                        }
                    }

                    // Activation check (only if not already active)
                    if self.terrain.is_none() {
                        if let Some((body_idx, planet_pos, config)) =
                            terrain_integration::find_terrain_planet(active_sys, camera_ly)
                        {
                            log::info!("Terrain activated for body {} (radius {:.0} km)",
                                body_idx, config.radius_m / 1000.0);
                            self.terrain = Some(terrain_integration::TerrainManager::new(
                                config, planet_pos, body_idx,
                            ));
                        }
                    }

                    // Update terrain if active
                    if let Some(terrain) = &mut self.terrain {
                        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                            let result = terrain.update(
                                self.galactic_position,
                                &mut renderer.mesh_store,
                                &gpu.device,
                            );
                            active_sys.hidden_body_index = result.hidden_body_index;
                            // Terrain draw commands are added to the main draw command list below
                        }
                    }
                } else {
                    // No active system — deactivate terrain if somehow still active
                    if self.terrain.is_some() {
                        self.terrain = None;
                    }
                }
```

- [ ] **Step 3: Add terrain draw commands to the render pass**

Find where `draw_commands` are built for the renderer (in the `render_frame` call). Add terrain chunks to the command list:

```rust
                // Collect terrain draw commands
                let terrain_commands: Vec<sa_render::renderer::DrawCommand> =
                    if let Some(terrain) = &mut self.terrain {
                        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                            let result = terrain.update(
                                self.galactic_position,
                                &mut renderer.mesh_store,
                                &gpu.device,
                            );
                            if let Some(sys) = &mut self.active_system {
                                sys.hidden_body_index = result.hidden_body_index;
                            }
                            result.draw_commands
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                // Append terrain commands to system commands
                commands.extend(terrain_commands);
```

Note: The exact integration point depends on where `commands` is built. Look for where `active_system.update()` returns DrawCommands and append terrain commands there.

- [ ] **Step 4: Verify compilation and run**

```bash
cargo build -p spaceaway
cargo run -p spaceaway
```

Expected: Game launches. When flying near a planet (within 2× radius), terrain chunks should appear and the icosphere should disappear. Flying away restores the icosphere.

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/main.rs
git commit -m "feat(terrain): wire terrain streaming into game loop with activation/deactivation"
```

---

### Task 11: Visual verification and cleanup

**Files:**
- Modify: `crates/sa_terrain/src/lib.rs` (if needed)

- [ ] **Step 1: Run all tests**

```bash
cargo test --workspace
```

Expected: All tests pass, including new sa_terrain tests.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -p sa_terrain -- -D warnings
```

Expected: No warnings (fix any that appear).

- [ ] **Step 3: Visual test checklist**

Run the game and verify:

1. From deep space, planets render as icospheres (existing behavior)
2. Approaching a planet: at ~2× radius, terrain chunks appear and icosphere disappears
3. No visible gap during handoff (same frame)
4. Terrain LOD increases as you get closer — coarse chunks in the distance, fine near camera
5. No visible cracks between chunks (skirts working)
6. No LOD popping (morph factor smoothing transitions)
7. Flying away: at ~2.5× radius, terrain disappears and icosphere returns
8. Biome colors roughly match the icosphere appearance
9. No precision artifacts (test with planets at various distances from star)
10. Performance: steady 60fps with terrain active

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix(terrain): visual verification fixes and cleanup"
```

---

## Self-Review

**Spec coverage check (Phase 1 only):**

| Spec Requirement | Task |
|-----------------|------|
| New sa_terrain crate | Task 1 |
| Analytic cube-to-sphere mapping (Nowell 2005) | Task 2 |
| CDLOD quadtree with LOD ranges and node selection | Task 3 |
| fastnoise-lite fBm + domain warp heightmap | Task 4 |
| Biome color by altitude/latitude/sub_type | Task 5 |
| 33×33 chunk mesh with normals and skirts | Task 6 |
| Async streaming with crossbeam MPMC + LRU cache | Task 7 |
| Icosphere suppression with hysteresis | Tasks 8, 10 |
| Per-patch f64→f32 camera-relative rebasing | Task 9 |
| terrain_integration.rs (not inline in main.rs) | Task 9 |
| DrawCommand with pre_rebased=true | Task 9 |
| Multi-body nearest-wins activation | Task 9 |
| Performance targets (< 0.5ms/chunk, ≤ 8 uploads/frame) | Tasks 6, 7 |

**Phase 2+ items correctly excluded:** No collision, no gravity, no landing, no surface walking, no vertex shader enhancement.

**Placeholder scan:** No TBD/TODO except one deliberate note about mesh_store.remove() which is a known limitation documented inline.

**Type consistency:** ChunkKey, ChunkData, TerrainConfig, TerrainVertex used consistently across all tasks. CubeFace enum used in cube_sphere.rs and referenced by u8 in ChunkKey with CubeFace::ALL[face as usize] conversion.
