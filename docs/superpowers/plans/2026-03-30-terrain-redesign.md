# Terrain System Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the planet terrain system to fix draw call count, GPU memory management, LOD popping, collision mismatch, and planet flickering — while preparing for future volumetric terrain.

**Architecture:** CPU-generated meshes uploaded to a budget-driven slab allocator with shared index buffer. Separate terrain render pipeline with vertex morphing. Independent fixed-resolution collision grid. Depth-test icosphere coexistence.

**Tech Stack:** Rust, wgpu, rapier3d, crossbeam, fastnoise-lite

**Spec:** `docs/superpowers/specs/2026-03-30-terrain-redesign.md`

---

## File Map

### New Files
- `crates/sa_terrain/src/config.rs` — centralized terrain constants
- `crates/sa_terrain/src/collision_grid.rs` — fixed-LOD 7×7 collision grid (pure math)
- `crates/sa_render/src/terrain_vertex.rs` — TerrainVertex with morph_target
- `crates/sa_render/src/slab_allocator.rs` — budget-driven vertex buffer pool
- `crates/sa_render/src/terrain_pipeline.rs` — terrain-specific render pipeline
- `crates/sa_render/src/shaders/terrain.wgsl` — terrain shader with vertex morphing

### Modified Files
- `crates/sa_terrain/src/lib.rs` — add config + collision_grid modules, update TerrainVertex, add ChunkType
- `crates/sa_terrain/src/chunk.rs` — compute morph_target per vertex
- `crates/sa_terrain/src/quadtree.rs` — new range formula, reduce MAX_VISIBLE_NODES
- `crates/sa_terrain/src/streaming.rs` — priority queue, configurable workers
- `crates/sa_render/src/lib.rs` — export new terrain types
- `crates/sa_render/src/renderer.rs` — add terrain render pass with slab
- `crates/spaceaway/src/terrain_integration.rs` — replace HashMap with slab, remove icosphere hide/show
- `crates/spaceaway/src/terrain_colliders.rs` — use CollisionGrid, remove surface barrier

---

## Task 1: Centralize Terrain Constants

**Files:**
- Create: `crates/sa_terrain/src/config.rs`
- Modify: `crates/sa_terrain/src/lib.rs:6-13`
- Modify: `crates/sa_terrain/src/quadtree.rs:10-15`
- Modify: `crates/sa_terrain/src/streaming.rs:17-21`
- Modify: `crates/sa_terrain/src/chunk.rs:11-13`

- [ ] **Step 1: Create config.rs with all constants**

Create `crates/sa_terrain/src/config.rs`:

```rust
//! Centralized terrain system constants.
//!
//! All tuning parameters live here so they can be adjusted in one place.
//! See docs/superpowers/specs/2026-03-30-terrain-redesign.md for rationale.

// -- LOD selection --

/// LOD range multiplier: subdivide when camera is within K × chunk_face_size.
pub const K_FACTOR: f64 = 2.5;

/// Hard cap on visible nodes per frame. Safety net, not the target.
/// With frustum culling + K=2.5, typical counts are 150–250.
pub const MAX_VISIBLE_NODES: usize = 400;

/// Minimum range floor for finest LOD level (meters).
pub const MIN_RANGE: f64 = 50.0;

// -- Mesh generation --

/// Vertices along one edge of the chunk grid (32 cells + 1).
pub const GRID_SIZE: u32 = 33;

/// Cells along one edge.
pub const CELLS: u32 = 32;

/// Total grid vertices per chunk (33 × 33).
pub const GRID_VERTEX_COUNT: u32 = GRID_SIZE * GRID_SIZE;

/// Skirt vertices per chunk (4 edges × 33).
pub const SKIRT_VERTEX_COUNT: u32 = 4 * GRID_SIZE;

/// Total vertices per heightmap chunk (grid + skirt).
pub const VERTS_PER_HEIGHTMAP_CHUNK: u32 = GRID_VERTEX_COUNT + SKIRT_VERTEX_COUNT;

// -- GPU memory --

/// Budget for heightmap terrain slab (bytes). ~517 slots at 58KB each.
pub const HEIGHTMAP_BUDGET_BYTES: u64 = 30_000_000;

/// Budget for future volumetric terrain slab (bytes). Unused until caves.
pub const VOLUMETRIC_BUDGET_BYTES: u64 = 20_000_000;

// -- Collision --

/// Collision grid size (NxN chunks centered on player).
pub const COLLISION_GRID_SIZE: usize = 7;

/// Collision LOD offset from max_lod (coarser by this many levels).
pub const COLLISION_LOD_OFFSET: u8 = 2;

/// Maximum collision chunk width in meters. Floor for collision LOD selection.
pub const COLLISION_MAX_CHUNK_WIDTH_M: f64 = 200.0;

/// Physics anchor rebase threshold in meters.
pub const COLLISION_REBASE_THRESHOLD_M: f64 = 100.0;

/// Collision grid re-centers when player moves this many chunk-widths.
pub const COLLISION_GRID_HYSTERESIS: f64 = 1.5;

// -- Streaming --

/// Maximum chunks in the LRU cache.
pub const LRU_CAPACITY: usize = 1000;

/// Chunks uploaded per frame in burst mode (activation).
pub const MAX_UPLOADS_BURST: usize = 64;

/// Chunks uploaded per frame in steady state.
pub const MAX_UPLOADS_STEADY: usize = 8;

/// Cache size threshold to exit burst mode.
pub const BURST_THRESHOLD: usize = 24;

// -- Activation --

/// Terrain activates when camera is within this factor × planet radius.
pub const TERRAIN_ACTIVATE_FACTOR: f64 = 2.0;

/// Terrain deactivates when camera exceeds this factor × planet radius.
pub const TERRAIN_DEACTIVATE_FACTOR: f64 = 2.5;

/// Collision grid activates at this altitude factor × radius.
pub const COLLISION_ACTIVATE_FACTOR: f64 = 0.2;

/// Collision grid destroyed above this altitude (meters).
pub const COLLISION_DEACTIVATE_ALT_M: f64 = 500.0;

/// Icosphere rendered at this factor × radius (inset to prevent Z-fighting).
pub const ICOSPHERE_RADIUS_FACTOR: f64 = 0.999;

// -- Base chunks (never evicted) --

/// Number of LOD levels that are permanently loaded (LOD 0 + LOD 1).
pub const BASE_LOD_LEVELS: u8 = 2;

/// Total base chunks: 6 (LOD 0) + 24 (LOD 1) = 30.
pub const BASE_CHUNK_COUNT: usize = 30;
```

- [ ] **Step 2: Add config module to lib.rs**

In `crates/sa_terrain/src/lib.rs`, add `pub mod config;` to the module list (after line 5, before `pub mod cube_sphere`).

- [ ] **Step 3: Update quadtree.rs to use config constants**

In `crates/sa_terrain/src/quadtree.rs`:
- Remove `const MIN_RANGE: f64 = 50.0;` (line 10)
- Remove `const MAX_VISIBLE_NODES: usize = 800;` (line 15)
- Add `use crate::config::{MIN_RANGE, MAX_VISIBLE_NODES, K_FACTOR};` at the top
- Change the range formula at line 153 from:
  ```rust
  let range = (face_size * 2.0).max(MIN_RANGE);
  ```
  to:
  ```rust
  let range = (face_size * K_FACTOR).max(MIN_RANGE);
  ```

- [ ] **Step 4: Update chunk.rs to use config constants**

In `crates/sa_terrain/src/chunk.rs`:
- Remove `pub const GRID_SIZE: u32 = 33;` (line 11) and `pub const CELLS: u32 = 32;` (line 13)
- Add `use crate::config::{GRID_SIZE, CELLS};`
- Keep `pub use crate::config::{GRID_SIZE, CELLS};` for backward compatibility (other crates import from chunk)

- [ ] **Step 5: Update streaming.rs to use config constants**

In `crates/sa_terrain/src/streaming.rs`:
- Remove `const WORKER_COUNT: usize = 4;` (line 17) and `const LRU_CAPACITY: usize = 1000;` (line 21)
- Add `use crate::config::LRU_CAPACITY;`
- Make worker count configurable: change `ChunkStreaming::new` to accept `worker_count: usize` parameter, defaulting to `num_cpus::get().saturating_sub(1).max(2)` at the call site

- [ ] **Step 6: Run tests**

Run: `cargo test -p sa_terrain`
Expected: All existing tests pass (constants have same values, just moved)

- [ ] **Step 7: Run clippy and commit**

```bash
cargo clippy -p sa_terrain -- -D warnings
git add crates/sa_terrain/src/config.rs crates/sa_terrain/src/lib.rs crates/sa_terrain/src/quadtree.rs crates/sa_terrain/src/chunk.rs crates/sa_terrain/src/streaming.rs
git commit -m "refactor(terrain): centralize constants in config.rs"
```

---

## Task 2: TerrainVertex with Morph Target

**Files:**
- Create: `crates/sa_render/src/terrain_vertex.rs`
- Modify: `crates/sa_terrain/src/lib.rs:44-49` — update TerrainVertex, add ChunkType
- Modify: `crates/sa_terrain/src/chunk.rs:88-318` — compute morph_target per vertex
- Modify: `crates/sa_render/src/lib.rs` — export TerrainVertex

- [ ] **Step 1: Update TerrainVertex in sa_terrain/lib.rs**

In `crates/sa_terrain/src/lib.rs`, replace the TerrainVertex struct (lines 44-49):

```rust
/// Vertex data for a terrain chunk. Includes morph target for CDLOD
/// vertex morphing (odd vertices blend toward parent-LOD positions).
#[derive(Debug, Clone, Copy)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
    /// Position at parent LOD level. For even-indexed vertices (shared with
    /// parent), this equals `position`. For odd vertices (midpoints), this is
    /// the average of the two neighboring even vertices.
    pub morph_target: [f32; 3],
}
```

Also add ChunkType enum after TerrainVertex:

```rust
/// Whether a chunk is heightmap-based or volumetric (future caves).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkType {
    Heightmap,
    // Volumetric, // future — caves, overhangs
}
```

And add `chunk_type: ChunkType` field to ChunkData.

- [ ] **Step 2: Create GPU-side TerrainVertex in sa_render**

Create `crates/sa_render/src/terrain_vertex.rs`:

```rust
//! GPU vertex format for terrain chunks, with morph target for CDLOD morphing.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
    pub morph_target: [f32; 3],
}

impl TerrainVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        static ATTRIBUTES: &[wgpu::VertexAttribute] = &[
            // position
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            // color
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            // normal
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x3,
            },
            // morph_target
            wgpu::VertexAttribute {
                offset: 36,
                shader_location: 7,
                format: wgpu::VertexFormat::Float32x3,
            },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRIBUTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_vertex_size_is_48_bytes() {
        assert_eq!(std::mem::size_of::<TerrainVertex>(), 48);
    }

    #[test]
    fn terrain_vertex_layout_has_four_attributes() {
        assert_eq!(TerrainVertex::layout().attributes.len(), 4);
    }

    #[test]
    fn morph_target_at_location_7() {
        let attrs = TerrainVertex::layout().attributes;
        assert_eq!(attrs[3].shader_location, 7);
        assert_eq!(attrs[3].offset, 36);
    }
}
```

- [ ] **Step 3: Add terrain_vertex module to sa_render/lib.rs**

Add `pub mod terrain_vertex;` and `pub use terrain_vertex::TerrainVertex as GpuTerrainVertex;` to `crates/sa_render/src/lib.rs`.

- [ ] **Step 4: Compute morph_target in chunk generation**

In `crates/sa_terrain/src/chunk.rs`, modify `generate_chunk` to compute morph targets after the grid vertices are built (after line 239, before skirt generation).

The logic: for each vertex at grid position (gx, gy), determine if it's "odd" (exists only at this LOD, not at parent):

```rust
// -----------------------------------------------------------------------
// Morph targets: parent-LOD positions for CDLOD vertex morphing.
// Even vertices (both gx and gy are even) exist at the parent LOD —
// their morph_target equals their position. Odd vertices (gx or gy is
// odd) are midpoints that don't exist at the parent LOD — their
// morph_target is the average of the two neighboring even vertices.
// -----------------------------------------------------------------------
let mut morph_targets = vec![[0.0f32; 3]; n * n];
for gy in 0..n {
    for gx in 0..n {
        let idx = gy * n + gx;
        let even_x = gx % 2 == 0;
        let even_y = gy % 2 == 0;

        if even_x && even_y {
            // Shared with parent LOD — morph target is self
            morph_targets[idx] = local_pos[idx];
        } else if !even_x && even_y {
            // Midpoint along X — average of left and right even neighbors
            let left = gy * n + (gx - 1);
            let right = gy * n + (gx + 1).min(n - 1);
            morph_targets[idx] = avg_pos(local_pos[left], local_pos[right]);
        } else if even_x && !even_y {
            // Midpoint along Y — average of top and bottom even neighbors
            let top = (gy - 1) * n + gx;
            let bottom = ((gy + 1).min(n - 1)) * n + gx;
            morph_targets[idx] = avg_pos(local_pos[top], local_pos[bottom]);
        } else {
            // Diagonal midpoint — average of 4 surrounding even neighbors
            let tl = (gy - 1) * n + (gx - 1);
            let tr = (gy - 1) * n + (gx + 1).min(n - 1);
            let bl = ((gy + 1).min(n - 1)) * n + (gx - 1);
            let br = ((gy + 1).min(n - 1)) * n + (gx + 1).min(n - 1);
            morph_targets[idx] = [
                (local_pos[tl][0] + local_pos[tr][0] + local_pos[bl][0] + local_pos[br][0]) * 0.25,
                (local_pos[tl][1] + local_pos[tr][1] + local_pos[bl][1] + local_pos[br][1]) * 0.25,
                (local_pos[tl][2] + local_pos[tr][2] + local_pos[bl][2] + local_pos[br][2]) * 0.25,
            ];
        }
    }
}
```

Add the helper function:
```rust
#[inline]
fn avg_pos(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}
```

Update the vertex push to include morph_target:
```rust
vertices.push(TerrainVertex {
    position: local_pos[i],
    color: colors[i],
    normal: normals[i],
    morph_target: morph_targets[i],
});
```

For skirt vertices, set `morph_target: skirt_pos` (morph target = self, skirts don't morph).

Update ChunkData construction to include `chunk_type: ChunkType::Heightmap`.

- [ ] **Step 5: Write morph target tests**

Add to the tests in `crates/sa_terrain/src/chunk.rs`:

```rust
#[test]
fn even_vertices_morph_to_self() {
    let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
    let chunk = generate_chunk(key, &test_config());
    let n = GRID_SIZE as usize;
    // Check a few even-indexed vertices
    for gy in (0..n).step_by(2) {
        for gx in (0..n).step_by(4) {
            let idx = gy * n + gx;
            let v = &chunk.vertices[idx];
            assert_eq!(v.position, v.morph_target,
                "even vertex ({gx},{gy}) morph_target should equal position");
        }
    }
}

#[test]
fn odd_vertices_morph_to_neighbor_average() {
    let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
    let chunk = generate_chunk(key, &test_config());
    let n = GRID_SIZE as usize;
    // Odd-X vertex at (1, 0): should be average of (0,0) and (2,0)
    let left = &chunk.vertices[0];
    let right = &chunk.vertices[2];
    let mid = &chunk.vertices[1];
    let expected = [
        (left.position[0] + right.position[0]) * 0.5,
        (left.position[1] + right.position[1]) * 0.5,
        (left.position[2] + right.position[2]) * 0.5,
    ];
    for i in 0..3 {
        assert!((mid.morph_target[i] - expected[i]).abs() < 1e-4,
            "odd vertex morph_target[{i}] = {}, expected {}",
            mid.morph_target[i], expected[i]);
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p sa_terrain && cargo test -p sa_render`
Expected: All tests pass including new morph target tests.

- [ ] **Step 7: Commit**

```bash
cargo clippy -p sa_terrain -p sa_render -- -D warnings
git add crates/sa_terrain/src/lib.rs crates/sa_terrain/src/chunk.rs crates/sa_render/src/terrain_vertex.rs crates/sa_render/src/lib.rs
git commit -m "feat(terrain): add morph_target to TerrainVertex for CDLOD morphing"
```

---

## Task 3: Slab Allocator

**Files:**
- Create: `crates/sa_render/src/slab_allocator.rs`
- Modify: `crates/sa_render/src/lib.rs` — export TerrainSlab

- [ ] **Step 1: Write slab allocator tests**

Create `crates/sa_render/src/slab_allocator.rs` starting with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_slab_has_correct_slot_count() {
        let slab = TerrainSlab::new_cpu(30_000_000, 58_416);
        // 30MB / 58KB ≈ 513 slots
        assert!(slab.total_slots >= 500);
        assert!(slab.total_slots <= 520);
    }

    #[test]
    fn allocate_returns_slot_and_tracks_key() {
        let mut slab = TerrainSlab::new_cpu(1_000_000, 1000);
        let key = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        let slot = slab.allocate(key);
        assert!(slot.is_some());
        assert!(slab.contains(&key));
    }

    #[test]
    fn free_returns_slot_to_pool() {
        let mut slab = TerrainSlab::new_cpu(1_000_000, 1000);
        let key = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        slab.allocate(key);
        assert!(slab.contains(&key));
        slab.free(&key);
        assert!(!slab.contains(&key));
    }

    #[test]
    fn allocate_fails_when_full() {
        let mut slab = TerrainSlab::new_cpu(3000, 1000); // 3 slots
        let k1 = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        let k2 = ChunkKey { face: 1, lod: 0, x: 0, y: 0 };
        let k3 = ChunkKey { face: 2, lod: 0, x: 0, y: 0 };
        let k4 = ChunkKey { face: 3, lod: 0, x: 0, y: 0 };
        assert!(slab.allocate(k1).is_some());
        assert!(slab.allocate(k2).is_some());
        assert!(slab.allocate(k3).is_some());
        assert!(slab.allocate(k4).is_none()); // full
    }

    #[test]
    fn base_vertex_offset_is_correct() {
        let slab = TerrainSlab::new_cpu(1_000_000, 1000);
        // slot_vertex_count = 1000 / bytes_per_vertex ... but in CPU mode
        // we use a simpler model. Let's test with real numbers.
        assert_eq!(slab.base_vertex(0), 0);
        assert_eq!(slab.base_vertex(1), slab.slot_vertex_count);
        assert_eq!(slab.base_vertex(2), slab.slot_vertex_count * 2);
    }

    #[test]
    fn evict_farthest_removes_correct_chunk() {
        let mut slab = TerrainSlab::new_cpu(3000, 1000); // 3 slots
        let k1 = ChunkKey { face: 0, lod: 5, x: 0, y: 0 };
        let k2 = ChunkKey { face: 1, lod: 5, x: 0, y: 0 };
        let k3 = ChunkKey { face: 2, lod: 5, x: 0, y: 0 };
        slab.allocate(k1);
        slab.allocate(k2);
        slab.allocate(k3);

        // Set centers for distance calculation
        slab.set_center(k1, [100.0, 0.0, 0.0]);
        slab.set_center(k2, [1000.0, 0.0, 0.0]); // farthest
        slab.set_center(k3, [200.0, 0.0, 0.0]);

        let protected = std::collections::HashSet::new();
        let evicted = slab.evict_farthest([0.0, 0.0, 0.0], &protected);
        assert_eq!(evicted, Some(k2));
        assert!(!slab.contains(&k2));
    }

    #[test]
    fn evict_skips_protected_chunks() {
        let mut slab = TerrainSlab::new_cpu(3000, 1000);
        let k1 = ChunkKey { face: 0, lod: 0, x: 0, y: 0 }; // LOD 0 = protected
        let k2 = ChunkKey { face: 1, lod: 5, x: 0, y: 0 };
        let k3 = ChunkKey { face: 2, lod: 5, x: 0, y: 0 };
        slab.allocate(k1);
        slab.allocate(k2);
        slab.allocate(k3);
        slab.set_center(k1, [5000.0, 0.0, 0.0]); // farthest but protected
        slab.set_center(k2, [1000.0, 0.0, 0.0]);
        slab.set_center(k3, [200.0, 0.0, 0.0]);

        let mut protected = std::collections::HashSet::new();
        protected.insert(k1);
        let evicted = slab.evict_farthest([0.0, 0.0, 0.0], &protected);
        assert_eq!(evicted, Some(k2)); // k2 farthest non-protected
        assert!(slab.contains(&k1)); // still there
    }
}
```

- [ ] **Step 2: Implement TerrainSlab**

Write the implementation above the tests in `crates/sa_render/src/slab_allocator.rs`:

```rust
//! Budget-driven vertex buffer pool for terrain chunks.
//!
//! All heightmap chunks share identical topology (33×33 grid + skirts),
//! so every slot has the same vertex count. Slots are managed via a free-list.

use std::collections::{HashMap, HashSet};
use sa_terrain::ChunkKey;

/// Budget-driven slab allocator for terrain vertex data.
///
/// Manages a single large wgpu::Buffer divided into fixed-size slots.
/// Each slot holds one terrain chunk's vertices.
pub struct TerrainSlab {
    /// GPU vertex buffer (None in CPU-only test mode).
    vertex_buffer: Option<wgpu::Buffer>,
    /// Available slot indices.
    free_list: Vec<u32>,
    /// Slot → chunk key mapping.
    slot_to_chunk: HashMap<u32, ChunkKey>,
    /// Chunk key → slot index mapping.
    chunk_to_slot: HashMap<ChunkKey, u32>,
    /// Chunk key → f64 center (for eviction distance calculation).
    chunk_centers: HashMap<ChunkKey, [f64; 3]>,
    /// Vertices per slot.
    pub slot_vertex_count: u32,
    /// Bytes per slot.
    pub slot_size_bytes: u32,
    /// Total number of slots.
    pub total_slots: u32,
    /// Total budget in bytes.
    budget_bytes: u64,
}

impl TerrainSlab {
    /// Create a slab with GPU buffer.
    pub fn new(device: &wgpu::Device, budget_bytes: u64, slot_size_bytes: u32) -> Self {
        let total_slots = (budget_bytes / slot_size_bytes as u64) as u32;
        let buffer_size = total_slots as u64 * slot_size_bytes as u64;

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Slab Vertex Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let free_list = (0..total_slots).rev().collect();

        Self {
            vertex_buffer: Some(vertex_buffer),
            free_list,
            slot_to_chunk: HashMap::with_capacity(total_slots as usize),
            chunk_to_slot: HashMap::with_capacity(total_slots as usize),
            chunk_centers: HashMap::with_capacity(total_slots as usize),
            slot_vertex_count: slot_size_bytes / 48, // 48 bytes per TerrainVertex
            slot_size_bytes,
            total_slots,
            budget_bytes,
        }
    }

    /// Create a CPU-only slab for testing (no GPU buffer).
    #[cfg(test)]
    pub fn new_cpu(budget_bytes: u64, slot_size_bytes: u32) -> Self {
        let total_slots = (budget_bytes / slot_size_bytes as u64) as u32;
        let free_list = (0..total_slots).rev().collect();
        Self {
            vertex_buffer: None,
            free_list,
            slot_to_chunk: HashMap::new(),
            chunk_to_slot: HashMap::new(),
            chunk_centers: HashMap::new(),
            slot_vertex_count: slot_size_bytes / 48,
            slot_size_bytes,
            total_slots,
            budget_bytes,
        }
    }

    /// Allocate a slot for a chunk. Returns the slot index, or None if full.
    pub fn allocate(&mut self, key: ChunkKey) -> Option<u32> {
        if self.chunk_to_slot.contains_key(&key) {
            return self.chunk_to_slot.get(&key).copied();
        }
        let slot = self.free_list.pop()?;
        self.slot_to_chunk.insert(slot, key);
        self.chunk_to_slot.insert(key, slot);
        Some(slot)
    }

    /// Free a slot, returning it to the pool.
    pub fn free(&mut self, key: &ChunkKey) {
        if let Some(slot) = self.chunk_to_slot.remove(key) {
            self.slot_to_chunk.remove(&slot);
            self.chunk_centers.remove(key);
            self.free_list.push(slot);
        }
    }

    /// Upload vertex data to a slot's region in the GPU buffer.
    pub fn upload(&self, slot: u32, data: &[u8], queue: &wgpu::Queue) {
        if let Some(ref buf) = self.vertex_buffer {
            let offset = slot as u64 * self.slot_size_bytes as u64;
            queue.write_buffer(buf, offset, data);
        }
    }

    /// Get the GPU vertex buffer.
    pub fn vertex_buffer(&self) -> Option<&wgpu::Buffer> {
        self.vertex_buffer.as_ref()
    }

    /// Set the f64 center position for distance-based eviction.
    pub fn set_center(&mut self, key: ChunkKey, center: [f64; 3]) {
        self.chunk_centers.insert(key, center);
    }

    /// Evict the farthest non-protected chunk. Returns the evicted key.
    pub fn evict_farthest(
        &mut self,
        camera: [f64; 3],
        protected: &HashSet<ChunkKey>,
    ) -> Option<ChunkKey> {
        let mut worst_key: Option<ChunkKey> = None;
        let mut worst_score: f64 = f64::NEG_INFINITY;

        for (&key, &center) in &self.chunk_centers {
            if protected.contains(&key) {
                continue;
            }
            let dx = camera[0] - center[0];
            let dy = camera[1] - center[1];
            let dz = camera[2] - center[2];
            let dist_sq = dx * dx + dy * dy + dz * dz;
            // Fine LODs (high lod number) are cheaper to lose
            let score = dist_sq + (key.lod as f64 * 1000.0);
            if score > worst_score {
                worst_score = score;
                worst_key = Some(key);
            }
        }

        if let Some(key) = worst_key {
            self.free(&key);
        }
        worst_key
    }

    /// Compute the base_vertex offset for draw_indexed.
    pub fn base_vertex(&self, slot: u32) -> u32 {
        slot * self.slot_vertex_count
    }

    /// Check if a chunk is in the slab.
    pub fn contains(&self, key: &ChunkKey) -> bool {
        self.chunk_to_slot.contains_key(key)
    }

    /// Get the slot index for a chunk.
    pub fn get_slot(&self, key: &ChunkKey) -> Option<u32> {
        self.chunk_to_slot.get(key).copied()
    }

    /// Number of free slots remaining.
    pub fn free_slots(&self) -> u32 {
        self.free_list.len() as u32
    }

    /// Number of occupied slots.
    pub fn occupied_slots(&self) -> u32 {
        self.total_slots - self.free_slots()
    }
}
```

- [ ] **Step 3: Add slab_allocator to sa_render/lib.rs**

Add `pub mod slab_allocator;` and `pub use slab_allocator::TerrainSlab;` to `crates/sa_render/src/lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p sa_render`
Expected: All slab allocator tests pass.

- [ ] **Step 5: Commit**

```bash
cargo clippy -p sa_render -- -D warnings
git add crates/sa_render/src/slab_allocator.rs crates/sa_render/src/lib.rs
git commit -m "feat(render): add TerrainSlab budget-driven vertex buffer pool"
```

---

## Task 4: Terrain Shader & Pipeline

**Files:**
- Create: `crates/sa_render/src/shaders/terrain.wgsl`
- Create: `crates/sa_render/src/terrain_pipeline.rs`
- Modify: `crates/sa_render/src/lib.rs` — export TerrainPipeline

- [ ] **Step 1: Create terrain shader**

Create `crates/sa_render/src/shaders/terrain.wgsl`:

```wgsl
// Terrain shader with CDLOD vertex morphing.
// Shares uniforms with the geometry pipeline but has a different vertex
// layout: adds morph_target (location 7) and morph_factor (location 8).

struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    _pad: f32,
    light_color: vec3<f32>,
    _pad2: f32,
    ambient: vec3<f32>,
    _pad3: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct Instance {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
    @location(8) morph_factor: f32,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
    @location(7) morph_target: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: Instance) -> VertexOutput {
    // CDLOD vertex morphing: blend position toward parent-LOD position.
    // At morph_factor=0: full detail. At morph_factor=1: matches parent LOD.
    let morphed_pos = mix(vertex.position, vertex.morph_target, instance.morph_factor);

    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );
    let world_pos = model * vec4<f32>(morphed_pos, 1.0);

    // Cofactor matrix for correct normal transform under non-uniform scale
    let col0 = model[0].xyz;
    let col1 = model[1].xyz;
    let col2 = model[2].xyz;
    let cofactor0 = cross(col1, col2);
    let cofactor1 = cross(col2, col0);
    let cofactor2 = cross(col0, col1);
    let world_normal = normalize(
        cofactor0 * vertex.normal.x + cofactor1 * vertex.normal.y + cofactor2 * vertex.normal.z
    );

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * world_pos;
    out.color = vertex.color;
    out.world_normal = world_normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) front_facing: bool) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let adjusted_n = select(-n, n, front_facing);
    let l = normalize(-uniforms.light_dir);
    let ndotl = max(dot(adjusted_n, l), 0.0);
    let diffuse = uniforms.light_color * ndotl;
    let color = in.color * (uniforms.ambient + diffuse);
    return vec4<f32>(color, 1.0);
}
```

- [ ] **Step 2: Create TerrainPipeline**

Create `crates/sa_render/src/terrain_pipeline.rs`:

```rust
//! Terrain-specific render pipeline with vertex morphing support.
//!
//! Uses the same uniform bind group as GeometryPipeline but a different
//! vertex layout (TerrainVertex with morph_target) and instance layout
//! (TerrainInstanceRaw with morph_factor).

use crate::terrain_vertex::TerrainVertex;

/// Per-instance data for terrain chunks: model matrix + morph factor.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainInstanceRaw {
    pub model: [[f32; 4]; 4],
    pub morph_factor: f32,
    pub _pad: [f32; 3],
}

impl TerrainInstanceRaw {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        static ATTRIBUTES: &[wgpu::VertexAttribute] = &[
            // model matrix columns
            wgpu::VertexAttribute { offset: 0,  shader_location: 3, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 16, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 32, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 48, shader_location: 6, format: wgpu::VertexFormat::Float32x4 },
            // morph_factor
            wgpu::VertexAttribute { offset: 64, shader_location: 8, format: wgpu::VertexFormat::Float32 },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainInstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRIBUTES,
        }
    }
}

/// Terrain render pipeline. Same render pass and uniforms as GeometryPipeline
/// but with terrain-specific vertex and instance formats.
pub struct TerrainPipeline {
    pub pipeline: wgpu::RenderPipeline,
}

impl TerrainPipeline {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        uniform_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/terrain.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain Pipeline Layout"),
            bind_group_layouts: &[uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[TerrainVertex::layout(), TerrainInstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None, // terrain visible from both sides
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::GreaterEqual, // reversed-Z
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_instance_size_is_80_bytes() {
        assert_eq!(std::mem::size_of::<TerrainInstanceRaw>(), 80);
    }

    #[test]
    fn terrain_instance_layout_has_five_attributes() {
        assert_eq!(TerrainInstanceRaw::layout().attributes.len(), 5);
    }

    #[test]
    fn morph_factor_at_location_8() {
        let attrs = TerrainInstanceRaw::layout().attributes;
        assert_eq!(attrs[4].shader_location, 8);
        assert_eq!(attrs[4].offset, 64);
    }
}
```

- [ ] **Step 3: Add terrain_pipeline to sa_render/lib.rs**

Add `pub mod terrain_pipeline;` and export `TerrainPipeline` and `TerrainInstanceRaw`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p sa_render`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
cargo clippy -p sa_render -- -D warnings
git add crates/sa_render/src/shaders/terrain.wgsl crates/sa_render/src/terrain_pipeline.rs crates/sa_render/src/lib.rs
git commit -m "feat(render): add terrain pipeline with CDLOD vertex morphing shader"
```

---

## Task 5: Collision Grid

**Files:**
- Create: `crates/sa_terrain/src/collision_grid.rs`
- Modify: `crates/sa_terrain/src/lib.rs` — add collision_grid module

- [ ] **Step 1: Write collision grid tests**

Create `crates/sa_terrain/src/collision_grid.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
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
    fn collision_lod_produces_reasonable_chunk_width() {
        let config = test_config();
        let max_lod = crate::quadtree::max_lod_levels(config.radius_m * 1.57);
        let col_lod = collision_lod(max_lod, config.radius_m);
        let chunk_width = face_size_at_lod(config.radius_m, col_lod);
        assert!(chunk_width <= 200.0, "chunk width {chunk_width}m exceeds 200m");
        assert!(chunk_width >= 10.0, "chunk width {chunk_width}m too small");
    }

    #[test]
    fn new_grid_starts_empty() {
        let config = test_config();
        let grid = CollisionGrid::new(&config);
        assert!(grid.active_chunks.is_empty());
    }

    #[test]
    fn first_update_generates_chunks() {
        let config = test_config();
        let mut grid = CollisionGrid::new(&config);
        let player_pos = [0.0, 0.0, config.radius_m]; // on surface at +Z
        let update = grid.update(player_pos, &config);
        assert!(!update.added.is_empty());
        let expected = COLLISION_GRID_SIZE * COLLISION_GRID_SIZE;
        assert_eq!(update.added.len(), expected,
            "expected {expected} chunks, got {}", update.added.len());
    }

    #[test]
    fn small_movement_does_not_trigger_update() {
        let config = test_config();
        let mut grid = CollisionGrid::new(&config);
        let pos = [0.0, 0.0, config.radius_m];
        grid.update(pos, &config);

        // Move 1 meter — within hysteresis
        let pos2 = [1.0, 0.0, config.radius_m];
        let update2 = grid.update(pos2, &config);
        assert!(update2.added.is_empty());
        assert!(update2.removed.is_empty());
    }

    #[test]
    fn heights_are_33x33() {
        let config = test_config();
        let mut grid = CollisionGrid::new(&config);
        let pos = [0.0, 0.0, config.radius_m];
        let update = grid.update(pos, &config);
        for (key, heights) in &update.added {
            assert_eq!(heights.len(), (GRID_SIZE * GRID_SIZE) as usize,
                "chunk {:?} heights len {}", key, heights.len());
        }
    }
}
```

- [ ] **Step 2: Implement CollisionGrid**

Write the implementation above the tests:

```rust
//! Fixed-resolution collision grid, independent of visual LOD.
//!
//! Maintains a 7×7 grid of height chunks centered on the player.
//! Pure math — no physics dependency. The integration layer converts
//! GridUpdate into rapier HeightField create/destroy calls.

use crate::{ChunkKey, TerrainConfig};
use crate::config::{COLLISION_GRID_SIZE, COLLISION_LOD_OFFSET, COLLISION_MAX_CHUNK_WIDTH_M,
                     COLLISION_GRID_HYSTERESIS, GRID_SIZE};
use crate::cube_sphere::{CubeFace, cube_to_sphere};
use crate::heightmap::{make_terrain_noise, make_warp_noise, sample_height};
use std::collections::HashMap;

/// Compute the collision LOD level for a planet.
pub fn collision_lod(max_lod: u8, radius_m: f64) -> u8 {
    let target_lod = max_lod.saturating_sub(COLLISION_LOD_OFFSET);
    // Ensure chunk width doesn't exceed COLLISION_MAX_CHUNK_WIDTH_M
    let mut lod = target_lod;
    while lod < max_lod && face_size_at_lod(radius_m, lod) > COLLISION_MAX_CHUNK_WIDTH_M {
        lod += 1;
    }
    lod
}

/// Compute the face size at a given LOD level (meters).
pub fn face_size_at_lod(radius_m: f64, lod: u8) -> f64 {
    (2.0 * radius_m) / (1u64 << lod) as f64
}

/// Chunks to add and remove from the physics world.
pub struct GridUpdate {
    /// New chunks with their 33×33 height data.
    pub added: Vec<(ChunkKey, Vec<f32>)>,
    /// Chunks to destroy colliders for.
    pub removed: Vec<ChunkKey>,
}

/// Fixed-resolution collision grid centered on the player.
pub struct CollisionGrid {
    /// Current grid center in chunk coordinates.
    center_chunk: Option<(u32, u32)>,
    /// Cube face the grid is on.
    face: u8,
    /// Fixed collision LOD level.
    lod: u8,
    /// World-space width of one collision chunk (meters).
    chunk_width_m: f64,
    /// Active chunks with their height data.
    pub active_chunks: HashMap<ChunkKey, Vec<f32>>,
    /// Planet radius for coordinate calculations.
    radius_m: f64,
}

impl CollisionGrid {
    /// Create an inactive collision grid.
    pub fn new(config: &TerrainConfig) -> Self {
        let max_lod = crate::quadtree::max_lod_levels(config.radius_m * 1.57);
        let lod = collision_lod(max_lod, config.radius_m);
        let chunk_width_m = face_size_at_lod(config.radius_m, lod);

        Self {
            center_chunk: None,
            face: 0,
            lod,
            chunk_width_m,
            active_chunks: HashMap::new(),
            radius_m: config.radius_m,
        }
    }

    /// Update the collision grid based on player position.
    /// Returns chunks to add/remove from physics.
    pub fn update(&mut self, player_pos: [f64; 3], config: &TerrainConfig) -> GridUpdate {
        // Determine which cube face and chunk the player is over
        let len = (player_pos[0] * player_pos[0]
            + player_pos[1] * player_pos[1]
            + player_pos[2] * player_pos[2]).sqrt();
        if len < 1.0 {
            return GridUpdate { added: vec![], removed: vec![] };
        }
        let dir = [player_pos[0] / len, player_pos[1] / len, player_pos[2] / len];

        let (face, u, v) = sphere_to_cube_face(dir);
        let tiles = 1u32 << self.lod;
        let chunk_x = ((u + 1.0) * 0.5 * tiles as f64).floor() as u32;
        let chunk_y = ((v + 1.0) * 0.5 * tiles as f64).floor() as u32;
        let chunk_x = chunk_x.min(tiles - 1);
        let chunk_y = chunk_y.min(tiles - 1);

        // Check hysteresis — don't re-center for small movements
        if let Some((cx, cy)) = self.center_chunk {
            if self.face == face as u8 {
                let dx = (chunk_x as f64 - cx as f64).abs();
                let dy = (chunk_y as f64 - cy as f64).abs();
                if dx < COLLISION_GRID_HYSTERESIS && dy < COLLISION_GRID_HYSTERESIS {
                    return GridUpdate { added: vec![], removed: vec![] };
                }
            }
        }

        // Compute new grid
        let half = (COLLISION_GRID_SIZE / 2) as i64;
        let mut new_keys = HashMap::new();

        let noise = make_terrain_noise(config.noise_seed);
        let warp = make_warp_noise(config.noise_seed);
        let freq_scale = 2.0;

        for dy in -half..=half {
            for dx in -half..=half {
                let cx = chunk_x as i64 + dx;
                let cy = chunk_y as i64 + dy;
                if cx < 0 || cy < 0 || cx >= tiles as i64 || cy >= tiles as i64 {
                    continue;
                }
                let key = ChunkKey {
                    face: face as u8,
                    lod: self.lod,
                    x: cx as u32,
                    y: cy as u32,
                };

                // Reuse existing height data if available
                if let Some(heights) = self.active_chunks.get(&key) {
                    new_keys.insert(key, heights.clone());
                } else {
                    // Generate heights (no mesh, just 33×33 noise samples)
                    let heights = generate_collision_heights(
                        key, config, &noise, &warp, freq_scale,
                    );
                    new_keys.insert(key, heights);
                }
            }
        }

        // Compute diff
        let mut added = Vec::new();
        let mut removed = Vec::new();

        for (key, heights) in &new_keys {
            if !self.active_chunks.contains_key(key) {
                added.push((*key, heights.clone()));
            }
        }
        for key in self.active_chunks.keys() {
            if !new_keys.contains_key(key) {
                removed.push(*key);
            }
        }

        self.active_chunks = new_keys;
        self.center_chunk = Some((chunk_x, chunk_y));
        self.face = face as u8;

        GridUpdate { added, removed }
    }

    /// Get heights for a specific chunk.
    pub fn get_heights(&self, key: &ChunkKey) -> Option<&[f32]> {
        self.active_chunks.get(key).map(|v| v.as_slice())
    }

    /// Clear all chunks (called on terrain deactivation).
    pub fn clear(&mut self) {
        self.active_chunks.clear();
        self.center_chunk = None;
    }
}

/// Generate 33×33 height samples for collision (no mesh vertices needed).
fn generate_collision_heights(
    key: ChunkKey,
    config: &TerrainConfig,
    noise: &fastnoise_lite::FastNoiseLite,
    warp: &fastnoise_lite::FastNoiseLite,
    freq_scale: f64,
) -> Vec<f32> {
    let face = CubeFace::ALL[key.face as usize];
    let tiles = 1u32 << key.lod;
    let tile_size = 2.0 / tiles as f64;
    let u_start = -1.0 + key.x as f64 * tile_size;
    let v_start = -1.0 + key.y as f64 * tile_size;

    let n = GRID_SIZE as usize;
    let mut heights = Vec::with_capacity(n * n);

    for row in 0..n {
        for col in 0..n {
            let u = u_start + col as f64 / (n - 1) as f64 * tile_size;
            let v = v_start + row as f64 / (n - 1) as f64 * tile_size;
            let dir = cube_to_sphere(face, u, v);
            let h = sample_height(noise, warp, dir, freq_scale);
            heights.push(h);
        }
    }

    heights
}

/// Map a unit sphere direction to the nearest cube face and UV coordinates.
fn sphere_to_cube_face(dir: [f64; 3]) -> (CubeFace, f64, f64) {
    let abs = [dir[0].abs(), dir[1].abs(), dir[2].abs()];
    let (face, u, v) = if abs[0] >= abs[1] && abs[0] >= abs[2] {
        if dir[0] > 0.0 {
            (CubeFace::PosX, -dir[2] / abs[0], dir[1] / abs[0])
        } else {
            (CubeFace::NegX, dir[2] / abs[0], dir[1] / abs[0])
        }
    } else if abs[1] >= abs[0] && abs[1] >= abs[2] {
        if dir[1] > 0.0 {
            (CubeFace::PosY, dir[0] / abs[1], -dir[2] / abs[1])
        } else {
            (CubeFace::NegY, dir[0] / abs[1], dir[2] / abs[1])
        }
    } else if dir[2] > 0.0 {
        (CubeFace::PosZ, dir[0] / abs[2], dir[1] / abs[2])
    } else {
        (CubeFace::NegZ, -dir[0] / abs[2], dir[1] / abs[2])
    };
    (face, u.clamp(-1.0, 1.0), v.clamp(-1.0, 1.0))
}
```

- [ ] **Step 3: Add collision_grid to lib.rs**

Add `pub mod collision_grid;` to `crates/sa_terrain/src/lib.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p sa_terrain`
Expected: All collision grid tests pass.

- [ ] **Step 5: Commit**

```bash
cargo clippy -p sa_terrain -- -D warnings
git add crates/sa_terrain/src/collision_grid.rs crates/sa_terrain/src/lib.rs
git commit -m "feat(terrain): add independent fixed-resolution collision grid"
```

---

## Task 6: Wire Terrain Rendering to Slab + Pipeline

This is the main integration task. Replace the HashMap-based GPU mesh management with the slab allocator and terrain pipeline.

**Files:**
- Modify: `crates/sa_render/src/renderer.rs` — add terrain pipeline, shared index buffer, terrain render pass
- Modify: `crates/spaceaway/src/terrain_integration.rs` — replace HashMap with slab, new draw command flow
- Modify: `crates/sa_render/src/lib.rs` — update exports

- [ ] **Step 1: Add terrain infrastructure to Renderer**

In `crates/sa_render/src/renderer.rs`:

Add fields to the `Renderer` struct:
```rust
pub terrain_pipeline: TerrainPipeline,
pub terrain_slab: TerrainSlab,
/// Shared index buffer for all heightmap terrain chunks.
terrain_index_buffer: wgpu::Buffer,
/// Number of indices in the shared terrain index buffer.
terrain_index_count: u32,
/// Persistent instance buffer for terrain chunks.
terrain_instance_buffer: wgpu::Buffer,
/// Current capacity of terrain instance buffer.
terrain_instance_capacity: u64,
```

In `Renderer::new()`, initialize these after the geometry pipeline:
```rust
let terrain_pipeline = TerrainPipeline::new(
    &gpu.device,
    gpu.config.format,
    &geometry_pipeline.bind_group_layout,
);

let terrain_indices = sa_terrain::chunk::shared_indices();
let terrain_index_buffer = gpu.device.create_buffer_init(
    &wgpu::util::BufferInitDescriptor {
        label: Some("Terrain Shared Index Buffer"),
        contents: bytemuck::cast_slice(terrain_indices),
        usage: wgpu::BufferUsages::INDEX,
    },
);
let terrain_index_count = terrain_indices.len() as u32;

let slot_size = sa_terrain::config::VERTS_PER_HEIGHTMAP_CHUNK as u32 * 48;
let terrain_slab = TerrainSlab::new(
    &gpu.device,
    sa_terrain::config::HEIGHTMAP_BUDGET_BYTES,
    slot_size,
);

let terrain_instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("Terrain Instance Buffer"),
    size: 512 * std::mem::size_of::<TerrainInstanceRaw>() as u64,
    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
});
```

- [ ] **Step 2: Add terrain draw command type**

Add a new struct for terrain-specific draw commands in `crates/sa_render/src/renderer.rs`:

```rust
/// A draw command for a terrain chunk rendered via the slab allocator.
pub struct TerrainDrawCommand {
    /// Slot in the terrain slab.
    pub slab_slot: u32,
    /// Model matrix (already camera-relative f32).
    pub model_matrix: Mat4,
    /// CDLOD morph factor (0 = full detail, 1 = parent LOD).
    pub morph_factor: f32,
}
```

- [ ] **Step 3: Add terrain render pass to render_frame**

In `render_frame()`, after the geometry pass draws (after the `if !batches.is_empty()` block, around line 456), add the terrain pass:

```rust
// Terrain pass — uses slab allocator + shared index buffer
if !terrain_draws.is_empty() {
    pass.set_pipeline(&self.terrain_pipeline.pipeline);
    pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

    if let Some(slab_buf) = self.terrain_slab.vertex_buffer() {
        pass.set_vertex_buffer(0, slab_buf.slice(..));
    }
    pass.set_index_buffer(
        self.terrain_index_buffer.slice(..),
        wgpu::IndexFormat::Uint32,
    );

    let instance_stride = std::mem::size_of::<TerrainInstanceRaw>() as u64;
    for (i, cmd) in terrain_draws.iter().enumerate() {
        let offset = i as u64 * instance_stride;
        let size = instance_stride;
        pass.set_vertex_buffer(
            1,
            self.terrain_instance_buffer.slice(offset..offset + size),
        );
        pass.draw_indexed(
            0..self.terrain_index_count,
            self.terrain_slab.base_vertex(cmd.slab_slot) as i32,
            0..1,
        );
    }
}
```

Add `terrain_draws: &[TerrainDrawCommand]` parameter to `render_frame()`. Write terrain instances to `terrain_instance_buffer` before the render pass (same pattern as geometry instances).

- [ ] **Step 4: Update terrain_integration.rs to use slab**

This is the largest change. In `crates/spaceaway/src/terrain_integration.rs`:

1. Remove `gpu_meshes: HashMap<ChunkKey, (Handle<MeshMarker>, [f64; 3])>` from TerrainManager
2. Replace with: references to the renderer's `terrain_slab`
3. When new chunks arrive from streaming:
   - Convert `sa_terrain::TerrainVertex` to `sa_render::GpuTerrainVertex` (Pod/Zeroable)
   - Allocate a slab slot
   - Upload vertex data to the slot
   - Track `center_f64` via `slab.set_center()`
4. Change `build_draw_commands` to produce `Vec<TerrainDrawCommand>` instead of `Vec<DrawCommand>`:
   - Walk visible nodes
   - For each, check if exact key is in slab. If not, walk up parent chain.
   - Create `TerrainDrawCommand { slab_slot, model_matrix, morph_factor }`
5. Remove icosphere hide/show logic — icosphere always renders

The model matrix computation stays the same: use chunk `center_f64` rebased to camera-relative f32.

- [ ] **Step 5: Update render_frame signature and wiring**

In the game binary's main loop (wherever `render_frame` is called), pass the new `terrain_draws` parameter.

- [ ] **Step 6: Build and test**

Run: `cargo build -p spaceaway`
Run: `cargo run -p spaceaway` — visually verify terrain renders correctly with the new pipeline.

- [ ] **Step 7: Commit**

```bash
cargo clippy --workspace -- -D warnings
git add crates/sa_render/src/renderer.rs crates/sa_render/src/lib.rs crates/spaceaway/src/terrain_integration.rs
git commit -m "feat(terrain): wire slab allocator + terrain pipeline for rendering"
```

---

## Task 7: Wire Collision Grid + Remove Surface Barrier

**Files:**
- Modify: `crates/spaceaway/src/terrain_colliders.rs` — use CollisionGrid, remove barrier
- Modify: `crates/spaceaway/src/terrain_integration.rs` — wire collision grid updates

- [ ] **Step 1: Replace ad-hoc collider creation with CollisionGrid**

In `crates/spaceaway/src/terrain_colliders.rs`:

1. Add a `CollisionGrid` field to `TerrainColliders`
2. In the `update()` method:
   - Call `collision_grid.update(player_pos, config)` to get `GridUpdate`
   - For each `added` chunk: call `build_heightfield()` to create rapier HeightField collider
   - For each `removed` chunk: destroy the rapier collider
3. Remove the `surface_barrier` field and all barrier-related code (creation, repositioning)
4. Keep the anchor rebase logic but simplify — only rebase the collision grid's colliders

- [ ] **Step 2: Wire collision grid in terrain_integration.rs**

In `TerrainManager::update()`:
- Create `CollisionGrid` when terrain activates (alongside streaming)
- Call `collision_grid.update()` each frame when altitude < `COLLISION_ACTIVATE_FACTOR × radius`
- Destroy collision grid when altitude > `COLLISION_DEACTIVATE_ALT_M`

- [ ] **Step 3: Test collision**

Run: `cargo run -p spaceaway`
- Approach a planet, land on it
- Verify ship stops at the visual surface (not floating above or falling through)
- Walk on the surface after exiting via airlock
- Take off and verify clean departure

- [ ] **Step 4: Commit**

```bash
cargo clippy --workspace -- -D warnings
git add crates/spaceaway/src/terrain_colliders.rs crates/spaceaway/src/terrain_integration.rs
git commit -m "feat(terrain): independent collision grid, remove surface barrier"
```

---

## Task 8: Icosphere Depth Coexistence

**Files:**
- Modify: `crates/spaceaway/src/terrain_integration.rs` — remove hide/show, set 0.999× radius
- Modify: icosphere rendering code (wherever icosphere scale is set)

- [ ] **Step 1: Remove icosphere hide/show logic**

In `crates/spaceaway/src/terrain_integration.rs`:
- Remove `icosphere_committed` field from TerrainManager
- Remove the logic that hides the icosphere based on GPU chunk count
- The icosphere always renders at `ICOSPHERE_RADIUS_FACTOR × planet_radius` (0.999×)

- [ ] **Step 2: Set icosphere scale to 0.999×**

Find where the icosphere mesh scale is computed (in `terrain_integration.rs` or the solar system manager). Multiply the icosphere radius by `sa_terrain::config::ICOSPHERE_RADIUS_FACTOR`.

- [ ] **Step 3: Test visually**

Run: `cargo run -p spaceaway`
- Approach a planet from space
- Verify smooth transition: terrain chunks appear and occlude the icosphere
- No flickering, no popping
- Planet never "disappears" during approach

- [ ] **Step 4: Commit**

```bash
git add crates/spaceaway/src/terrain_integration.rs
git commit -m "fix(terrain): depth-test icosphere coexistence, remove hide/show logic"
```

---

## Task 9: Priority Streaming

**Files:**
- Modify: `crates/sa_terrain/src/streaming.rs` — priority queue for requests

- [ ] **Step 1: Replace FIFO channel with priority ordering**

In `crates/sa_terrain/src/streaming.rs`:

The simplest approach: collect all needed-but-not-cached keys, sort by distance to camera, and send in that order. The workers still use a crossbeam channel (FIFO), but we feed it in priority order.

In `ChunkStreaming::update()`, change the request dispatch loop (lines 254-259):

```rust
// Sort needed keys by distance to camera (nearest first)
let mut requests: Vec<(ChunkKey, f64)> = needed
    .iter()
    .filter(|key| !self.cache.contains(key) && !self.in_flight.contains(key))
    .map(|key| {
        // Approximate distance using visible node centers
        let node = visible_nodes.iter().find(|n| {
            n.face as u8 == key.face && n.lod == key.lod && n.x == key.x && n.y == key.y
        });
        let dist = node.map_or(f64::MAX, |n| {
            let dx = camera_pos[0] - n.center[0];
            let dy = camera_pos[1] - n.center[1];
            let dz = camera_pos[2] - n.center[2];
            dx * dx + dy * dy + dz * dz
        });
        (*key, dist)
    })
    .collect();
requests.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

for (key, _dist) in requests {
    self.in_flight.insert(key);
    let _ = self.request_tx.send(key);
}
```

Add `camera_pos: [f64; 3]` parameter to `update()`.

- [ ] **Step 2: Make worker count configurable**

Change `ChunkStreaming::new()` to accept `worker_count: usize`:

```rust
pub fn new(config: TerrainConfig, worker_count: usize) -> Self {
    // ... spawn worker_count workers instead of WORKER_COUNT
}
```

At the call site in `terrain_integration.rs`, pass `num_cpus::get().saturating_sub(1).max(2)`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p sa_terrain`
Expected: Streaming test still passes.

- [ ] **Step 4: Commit**

```bash
cargo clippy -p sa_terrain -- -D warnings
git add crates/sa_terrain/src/streaming.rs crates/spaceaway/src/terrain_integration.rs
git commit -m "feat(terrain): distance-priority streaming, configurable worker count"
```

---

## Task 10: Final Integration Test & Cleanup

- [ ] **Step 1: Run full test suite**

```bash
cargo test --workspace
```

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 3: Visual testing**

Run `cargo run -p spaceaway` and test:
1. **Approach from space**: Planet visible, terrain fades in smoothly over icosphere
2. **No popping**: LOD transitions are smooth (morph factor working)
3. **No gaps**: Skirts cover LOD boundaries
4. **Performance**: Check FPS — should be 60+ on approach and surface
5. **Landing**: Ship stops at visual surface height
6. **Walking**: Exit ship via airlock, walk on terrain surface
7. **Departure**: Take off, terrain deactivates smoothly at 2.5× radius
8. **Teleport**: Use key 8 to jump near planet, terrain loads quickly (priority streaming)

- [ ] **Step 4: Remove dead code**

- Delete any remaining references to the old `gpu_meshes: HashMap`
- Delete old surface barrier code
- Delete old icosphere hide/show logic
- Remove any unused imports

- [ ] **Step 5: Final commit**

```bash
cargo clippy --workspace -- -D warnings
git add -A
git commit -m "fix(terrain): cleanup dead code from terrain redesign"
```
