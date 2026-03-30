# Planet Terrain System Redesign

**Date:** 2026-03-30
**Status:** Approved
**Scope:** `sa_terrain`, `sa_render`, `spaceaway` terrain integration

## Overview

Redesign the planet terrain system to fix critical performance and visual bugs
while establishing an architecture that scales to future volumetric features
(caves, overhangs, ravines). The existing CDLOD cube-sphere quadtree approach
is architecturally sound — this redesign fixes implementation-level issues and
adds the missing pieces.

### Problems Solved

1. **800 draw calls** → <250 via slab allocator + shared index buffer + frustum culling
2. **Unbounded GPU memory** (~120MB+) → budget-driven slab allocator (50MB cap)
3. **LOD popping** → vertex morphing connected to shader (morph_target per vertex)
4. **Planet flickering** → depth-test coexistence replaces show/hide logic
5. **Collision-visual mismatch** → independent fixed-resolution collision grid
6. **Surface barrier hack** → removed, replaced by collision grid
7. **No frustum culling** → enabled (halves visible node count)

### Design Principles

- CPU mesh generation (supports both heightmap and future volumetric)
- Budget-driven GPU memory (configurable, bounded, growable)
- Visual and collision LOD fully independent
- Clean crate boundaries (sa_terrain has no render/physics dependencies)
- Every component independently testable

---

## 1. LOD Selection

### Quadtree Traversal

Keep the existing CDLOD cube-sphere quadtree with 6 face roots and analytic
Nowell 2005 cube-to-sphere projection. The traversal algorithm is correct.

**Range formula (corrected):**

```
range(lod) = chunk_face_size(lod) × K
chunk_face_size(lod) = (planet_radius × 2) / 2^lod
K = 2.5
```

K=2.5 means: subdivide when camera is within 2.5× the chunk's own size. Results:

| Planet | Radius | LOD Levels | Visible Nodes (culled) |
|--------|--------|------------|----------------------|
| Earth-like | 6,371 km | 18 | 150–250 |
| Small moon | 500 km | 14 | 100–180 |
| Gas giant | 70,000 km | 21 | 150–250 |

The formula scales naturally — larger planets get more LOD levels but the same
visible node count because coarse levels cover more area.

**Frustum culling:** Always enabled. Bounding sphere per node = chunk center +
radius inflated by max displacement at that LOD. Reject entire subtrees outside
the 6 frustum planes. Cuts visible nodes roughly in half.

**Node budget:** Hard cap at 400 (down from 800). With frustum culling + K=2.5,
typical counts are 150–250. The cap is a safety net, not the target.

**Morph factor (per node):**

```
morph_start = range × 0.5
morph_factor = clamp((distance - morph_start) / (range - morph_start), 0, 1)
```

Passed to GPU as a per-instance attribute on the terrain pipeline.

---

## 2. Vertex Format & Morphing

### TerrainVertex

```rust
struct TerrainVertex {
    position: [f32; 3],      // 12B — chunk-local position
    color: [f32; 3],         // 12B — biome color
    normal: [f32; 3],        // 12B — smooth vertex normal
    morph_target: [f32; 3],  // 12B — parent-LOD position (chunk-local)
}
// 48 bytes per vertex, ~1,217 vertices per chunk ≈ 58KB
```

### Morph Target Computation

During chunk generation, each vertex gets a `morph_target`:

- **Even-indexed grid vertices** (shared with parent LOD): `morph_target = position`
  (vertex exists at parent LOD, no morphing needed)
- **Odd-indexed vertices** (midpoints between parent vertices):
  `morph_target = average(neighbor_even_a.position, neighbor_even_b.position)`
  (position this vertex would have at the parent LOD level)

"Even" and "odd" are determined by grid coordinates within the 33×33 grid:
a vertex at `(gx, gy)` is odd if `gx % 2 == 1` OR `gy % 2 == 1`.

### Shader

The terrain vertex shader applies morphing before transform:

```wgsl
let morphed_pos = mix(vertex.position, vertex.morph_target, instance.morph_factor);
let world_pos = model * vec4<f32>(morphed_pos, 1.0);
```

At `morph_factor = 0`: full detail. At `morph_factor = 1`: odd vertices collapse
to parent positions, visually identical to the coarser LOD level. Transitions are
seamless — no popping, no cracks.

### Terrain Render Pipeline

Terrain uses its own render pipeline, separate from the shared geometry pipeline:

- **Same render pass**, same depth buffer, same uniform bind group
- **Different vertex layout**: `TerrainVertex` (48B) instead of `Vertex` (36B)
- **Different instance layout**: adds `morph_factor: f32` (padded to 16B)
- One extra `set_pipeline` call per frame (trivial cost)

This keeps the shared geometry pipeline clean — ships, planets, and other meshes
are unaffected by terrain-specific attributes.

**Terrain instance format:**

```rust
struct TerrainInstanceRaw {
    model: [[f32; 4]; 4],  // 64B — model matrix
    morph_factor: f32,      // 4B
    _pad: [f32; 3],         // 12B — alignment
}
// 80 bytes per instance
```

---

## 3. GPU Memory Architecture

### Slab Allocator

A budget-driven vertex buffer pool with two tiers:

| Tier | Slot Size | Budget | Slots | Purpose |
|------|-----------|--------|-------|---------|
| Heightmap | 58KB (fixed) | 30MB | ~517 | Standard 33×33 terrain chunks |
| Volumetric | Variable | 20MB | — | Future caves/overhangs (unused now) |

**Total GPU terrain budget: 50MB**, well under the 200MB target. Remaining budget
covers icosphere, ship meshes, and future features.

**Permanently reserved:** 30 slots for LOD 0–1 base chunks (6 LOD-0 + 24 LOD-1 =
~1.7MB). These are never evicted and serve as guaranteed fallback when finer
chunks haven't loaded yet. Leaves 487 slots for dynamic allocation.

### Shared Index Buffer

All heightmap chunks share identical topology (33×33 grid + skirt = ~2,816
triangles). One static index buffer (~34KB), created once, never changes.

Each chunk drawn with: `draw_indexed(shared_indices, base_vertex = slot × VERTS_PER_CHUNK, instance)`

### Slot Lifecycle

1. Streaming delivers `ChunkData` → claim free slot from free-list
2. Upload vertices to slot's region in the vertex buffer via `queue.write_buffer`
3. When chunk evicted → return slot to free-list, slot available for reuse

### Eviction Policy

When the slab is full and a new chunk needs a slot:

1. Never evict LOD 0–1 (permanently reserved)
2. Score each chunk: `score = distance_to_camera - (lod × distance_bias)`
   (fine LODs far away evicted first — they cover less area and regenerate fast)
3. Evict highest-score chunk, free its slot

### Future: Multi-Draw Indirect

With 150–250 visible chunks after culling, individual `draw_indexed` calls are
acceptable. As an optional future optimization, `multi_draw_indexed_indirect`
can batch all terrain draws into a single GPU call. The slab layout supports
this naturally (contiguous buffer, uniform slot size). Metal (primary target)
supports multi-draw.

### Slab Allocator Interface

```rust
// sa_render/slab_allocator.rs
struct TerrainSlab {
    vertex_buffer: wgpu::Buffer,
    free_list: Vec<u32>,
    slot_to_chunk: HashMap<u32, ChunkKey>,
    chunk_to_slot: HashMap<ChunkKey, u32>,
    slot_vertex_count: u32,      // 1,217 for heightmap tier
    slot_size_bytes: u32,        // 58,416 for heightmap tier
    total_slots: u32,            // derived from budget
    budget_bytes: u64,
}

impl TerrainSlab {
    fn new(device, budget_bytes, slot_size_bytes) -> Self
    fn allocate(&mut self, key: ChunkKey) -> Option<u32>
    fn free(&mut self, key: &ChunkKey)
    fn upload(&self, slot: u32, vertices: &[TerrainVertex], queue: &Queue)
    fn evict_farthest(&mut self, camera: [f64;3], protected: &HashSet<ChunkKey>) -> ChunkKey
    fn base_vertex(&self, slot: u32) -> u32
    fn contains(&self, key: &ChunkKey) -> bool
}
```

---

## 4. Icosphere-to-Terrain Handoff

### Depth-Test Coexistence

Both icosphere and terrain chunks render simultaneously. No show/hide logic,
no coverage thresholds, no flickering.

**How it works:**

- Icosphere renders at `radius × 0.999` (0.1% inset from true radius)
- Terrain chunks are displaced at or above true radius (displacement ∈ [0, amplitude])
- Terrain is always in front of the icosphere → wins depth test
- As chunks stream in, they progressively occlude the icosphere
- Icosphere visible through gaps until terrain fills in — acts as natural fallback

**Icosphere removal:** Only stop drawing the icosphere when ALL 6 LOD-0 base
chunks are confirmed GPU-ready (guaranteed full spherical coverage). This is
guaranteed after the synchronous base chunk seeding on terrain activation.

**Why 0.999×:** At displacement = 0, terrain vertices sit at exactly planet
radius — same surface as the icosphere. Different triangulations produce
different interpolated depth values → Z-fighting. The 0.1% inset eliminates
this. For Earth (6,371 km), the inset is ~6.4 km — imperceptible from any
viewing distance.

---

## 5. Collision System

### Independent Fixed-Resolution Grid

Collision is fully decoupled from visual LOD. The collision system maintains its
own chunks at a fixed resolution, centered on the player.

**Grid specification:**

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Grid size | 7×7 (49 chunks) | Full coverage + buffer zone for hysteresis |
| LOD level | `max(max_lod - 2, level_where_chunk ≤ 200m)` | ~3m cell resolution with 33×33 grid |
| Hysteresis | Re-center when player moves >1.5 chunk widths | Prevents per-frame collider churn |
| Collider type | rapier HeightField (33×33) | O(1) point queries, cheap construction |
| Collision group | TERRAIN, interacts with PLAYER \| SHIP_HULL \| SHIP_EXTERIOR | Existing groups |

**Why independent from visual LOD:**

- Visual LOD changes with camera distance/angle — collision must never change
  under a resting body
- Prevents energy injection (LOD transition changes surface height → rapier
  interprets as penetration → body bounces)
- Constant resolution regardless of where the camera is looking

### Collision Grid Interface

```rust
// sa_terrain/collision_grid.rs (pure math, no rapier dependency)
struct CollisionGrid {
    center_chunk: (u32, u32),
    face: u8,
    lod: u8,
    chunk_width_m: f64,
    active_chunks: HashMap<ChunkKey, Vec<f32>>,  // key → 33×33 heights
}

impl CollisionGrid {
    fn new(config: &TerrainConfig) -> Self
    fn update(&mut self, player_pos: [f64;3], config: &TerrainConfig) -> GridUpdate
    fn get_heights(&self, key: &ChunkKey) -> Option<&[f32]>
}

struct GridUpdate {
    added: Vec<(ChunkKey, Vec<f32>)>,
    removed: Vec<ChunkKey>,
}
```

The integration layer in `spaceaway/terrain_colliders.rs` converts `GridUpdate`
into rapier HeightField create/destroy calls. `sa_terrain` has no physics
dependency.

### Surface Barrier Removal

The current flat 10km × 100m × 10km cuboid barrier is **removed entirely**. It
doesn't match terrain shape and breaks rapier's CCD.

Replacement: the 7×7 collision grid provides continuous coverage. On first
activation, collision heights are generated synchronously (~0.1–0.2ms per chunk ×
49 = ~5–10ms one-time cost). A fallback altitude check teleports the player to
surface + 2m if they somehow fall through.

### Anchor Rebase

Keep existing 100m threshold. When triggered:

1. Shift all rigid bodies (ship, player) toward physics origin
2. Update `anchor_f64` to new galactic position
3. Reposition all HeightField collider positions
4. Sync rapier query pipeline

With 49 colliders (down from potentially hundreds), rebase is cheap.

### CPU Memory

Collision data is CPU-only (not in GPU slab). 49 chunks × 33×33 × 4 bytes =
~215KB. Negligible.

### Future: Volumetric Collision

When caves arrive, chunks flagged as volumetric produce `TriMesh` colliders
instead of `HeightField`:

```rust
match chunk_type {
    ChunkType::Heightmap  => build_heightfield(heights),   // O(1) queries
    ChunkType::Volumetric => build_trimesh(vertices, indices), // BVH queries
}
```

Only cave-containing chunks use TriMesh. The grid will have at most 1–3
volumetric chunks at any time.

---

## 6. Landing & Surface Flow

### Approach Phases

| Phase | Altitude | Systems Active |
|-------|----------|---------------|
| **Orbital** | > 2.0× radius | Icosphere only. No terrain. |
| **Terrain Activation** | 2.0× radius | TerrainManager spawns. LOD 0–1 base chunks generated synchronously. Streaming begins. Icosphere at 0.999× radius, terrain chunks progressively occlude it. |
| **Approach** | 2.0× → 0.2× radius | Quadtree subdivides as camera descends. Visual detail increases. |
| **Low Altitude** | < 0.2× radius | Collision grid activates (7×7 chunks). Gravity blending begins (smoothstep). HUD shows altitude, descent rate. |
| **Landing** | < 100m | Skid raycasts active (4 rays, TERRAIN collision group). Ship settles via state machine. |
| **Surface** | On ground | Ship locked. Player exits via airlock. Collision grid follows player. |
| **Departure** | Throttle up | Collision grid stays active until altitude > 500m. Terrain deactivates at 2.5× radius. |

### Landing State Machine

Keep existing states:

```
Flying  → Sliding:  min_clearance < 1.0m
Sliding → Landed:   lock_request + speed < 5 m/s
Landed  → Sliding:  lock_request (unlock)
Sliding → Flying:   all clearances > 10m + engine on + throttle > 0
```

**Changes from current:**

- Collision grid guaranteed present before `Flying → Sliding` (activates at 0.2×
  radius, well before 100m skid range)
- Skids raycast against HeightField colliders directly (no surface barrier)
- Impact categories unchanged: Clean <10, Minor 10–30, Major 30–80, Destroyed >80 m/s

### Terrain Deactivation

1. **Altitude > 500m:** Destroy collision grid
2. **Altitude > 2.5× radius:** Destroy terrain manager, free all slab slots
3. Icosphere was always rendering — visible immediately

The 2.0× activate / 2.5× deactivate hysteresis prevents oscillation.

### Player Surface Walking

When the player exits the ship via airlock:

- Collision grid re-centers on the player (not the ship)
- Grid follows player with 1.5-chunk-width hysteresis
- Player uses planet gravity as "down"
- Re-entering ship: grid re-centers on ship

---

## 7. Streaming

### Worker Architecture

Keep existing crossbeam MPMC with background threads.

**Changes:**

| Aspect | Current | New |
|--------|---------|-----|
| Request ordering | FIFO | Distance-priority (nearest first) |
| Worker count | 4 (hardcoded) | Configurable, default `num_cpus - 1` |
| Upload rate (burst) | 64/frame | 64/frame (unchanged) |
| Upload rate (steady) | 8/frame | 8/frame (unchanged) |
| CPU cache | LRU, 1000 entries | LRU, 1000 entries (unchanged) |

**Priority queue:** Replace the crossbeam channel with a priority-aware structure.
Workers pull the nearest-to-camera chunk first. This ensures the landing area
fills in before distant terrain.

**CPU cache independence:** The LRU cache manages CPU-side `ChunkData`. The GPU
slab manages GPU-side vertex data. They have independent lifetimes. Chunks are
deterministic — if the CPU cache evicts a chunk that the GPU still has, it can
be regenerated on demand.

### LOD Fallback

When the quadtree selects a node not yet in the GPU slab, walk up the parent
chain until a slab-resident ancestor is found. Always terminates at LOD 0–1
(permanently reserved, guaranteed present). Typically 1–2 levels.

Use a `rendered: HashSet<ChunkKey>` per frame to prevent drawing the same
ancestor twice when multiple children fall back to it.

---

## 8. Data Structures Summary

### Core Types (sa_terrain)

```rust
// Unchanged
struct ChunkKey { face: u8, lod: u8, x: u32, y: u32 }
struct TerrainConfig {
    radius_m: f64,
    noise_seed: u64,
    sub_type: PlanetSubType,
    displacement_fraction: f32,
    surface_gravity_ms2: f32,
}
struct VisibleNode { face, lod, x, y, center: [f64;3], morph_factor: f32 }

// Updated
struct TerrainVertex {
    position: [f32; 3],
    color: [f32; 3],
    normal: [f32; 3],
    morph_target: [f32; 3],  // NEW
}

struct ChunkData {
    key: ChunkKey,
    center_f64: [f64; 3],
    vertices: Vec<TerrainVertex>,
    indices: Vec<u32>,
    heights: Vec<f32>,        // 33×33 for collision
    min_height: f32,
    max_height: f32,
    chunk_type: ChunkType,    // NEW
}

enum ChunkType {
    Heightmap,
    // Volumetric,  // future — caves, overhangs
}
```

### New Types

```rust
// sa_render/slab_allocator.rs
struct TerrainSlab { ... }  // See Section 3

// sa_terrain/collision_grid.rs
struct CollisionGrid { ... }  // See Section 5
struct GridUpdate { added, removed }

// sa_render — terrain pipeline instance
struct TerrainInstanceRaw {
    model: [[f32; 4]; 4],
    morph_factor: f32,
    _pad: [f32; 3],
}

// sa_terrain/config.rs — centralized constants
const K_FACTOR: f64 = 2.5;
const MAX_VISIBLE_NODES: usize = 400;
const COLLISION_GRID_SIZE: usize = 7;
const COLLISION_LOD_OFFSET: u8 = 2;
const HEIGHTMAP_BUDGET_BYTES: u64 = 30_000_000;
const VOLUMETRIC_BUDGET_BYTES: u64 = 20_000_000;
const VERTS_PER_HEIGHTMAP_CHUNK: u32 = 1_217;
const BYTES_PER_TERRAIN_VERTEX: u32 = 48;
```

### Crate Interfaces

```
sa_terrain (pure math — no render, no physics)
    Produces: ChunkData, VisibleNode, GridUpdate, GravityState
    Consumes: TerrainConfig, camera position, frustum planes
    Internal: quadtree, chunk gen, streaming, collision_grid, heightmap, biome

spaceaway (integration layer)
    TerrainManager:
        ├─ Calls select_visible_nodes() → feeds slab allocator
        ├─ Calls CollisionGrid::update() → creates/destroys rapier colliders
        ├─ Calls compute_gravity() → applies to ship/player
        └─ Builds DrawCommands from slab slot assignments
    LandingSystem:
        └─ Reads skid raycasts, manages state machine

sa_render (GPU resources)
    ├─ TerrainSlab: budget-driven vertex buffer pool
    ├─ TerrainPipeline: terrain-specific render pipeline + shader
    ├─ Shared index buffer: static 33×33 grid + skirt indices
    └─ TerrainInstanceRaw: model matrix + morph_factor
```

---

## 9. Future Volumetric Extension

This section documents how caves, overhangs, and ravines plug into the
architecture. Nothing here is built now — it's the blueprint.

### Activation

The quadtree gains a data layer marking regions containing caves:

- A density function probed during traversal (cheap SDF sample at node center)
- Or deterministic flags derived from planet seed + coordinates
- A node flagged volumetric routes to a different generator

### Generation

```
Heightmap chunk:                    Volumetric chunk:
  33×33 UV grid                      Sample 3D SDF in chunk volume
  → cube_to_sphere()                 → Dual Contouring or Marching Cubes
  → noise displacement               → irregular mesh (variable vertex count)
  → regular mesh (1,217 verts)       → variable mesh (500–5,000 verts)
  → shared index buffer              → per-chunk index buffer
  → heightmap slab tier              → volumetric slab tier
```

Same `ChunkData` struct. `chunk_type: ChunkType::Volumetric` tells downstream
systems which path to take.

### Rendering

No pipeline changes needed. Both types produce `TerrainVertex` arrays in the
same vertex buffer. Heightmap chunks use shared indices with `base_vertex`.
Volumetric chunks upload their own indices to a separate index region.

**Morph transitions:** Volumetric chunks set `morph_target = position` (morph is
a no-op). Cave geometry is always viewed up close — minor pops at cave mouth LOD
boundaries are acceptable. If needed, alpha-fade at cave entrances handles it.

### Collision

Volumetric chunks produce `TriMesh` colliders instead of `HeightField`. More
expensive (BVH construction) but only applies to the 1–3 cave-containing chunks
in the collision grid at any time.

### Memory

The volumetric slab tier (20MB, currently unused) accommodates variable-size
meshes via a free-list allocator with coalescing. No code changes needed to
activate — just start generating `ChunkType::Volumetric` chunks.

### What to Build Now

These future-proofing elements have near-zero cost today:

- `ChunkType` enum on `ChunkData` (always `Heightmap`)
- Volumetric budget constant (allocated in config, tier unused)
- `chunk_type` match arm in collision consumer (always `Heightmap` branch)

---

## 10. Migration Path

### Keep As-Is

| Component | File | Reason |
|-----------|------|--------|
| Cube-sphere mapping | `sa_terrain/cube_sphere.rs` | Analytic projection correct |
| Heightmap noise | `sa_terrain/heightmap.rs` | fBm + domain warp pipeline solid |
| Biome colors | `sa_terrain/biome.rs` | Per-vertex color logic clean |
| Gravity blending | `sa_terrain/gravity.rs` | Smoothstep + antiparallel guard works |
| Frustum culling | `sa_terrain/frustum.rs` | Infrastructure correct, already wired |

### Modify

| Component | File | Change |
|-----------|------|--------|
| Quadtree | `sa_terrain/quadtree.rs` | Range formula: `face_size × 2.5`. MAX_VISIBLE_NODES → 400. |
| Chunk generation | `sa_terrain/chunk.rs` | Add morph_target computation. Add ChunkType. |
| Streaming | `sa_terrain/streaming.rs` | Priority queue. Configurable worker count. |
| Terrain integration | `spaceaway/terrain_integration.rs` | Replace HashMap with slab. Remove icosphere hide/show. Icosphere at 0.999×. |
| Terrain colliders | `spaceaway/terrain_colliders.rs` | CollisionGrid consumer. Remove surface barrier. |
| Pipeline | `sa_render/pipeline.rs` | Add TerrainPipeline alongside GeometryPipeline. |
| Shader | `sa_render/shaders/geometry.wgsl` | Terrain variant with morph_factor + morph_target. |
| Renderer | `sa_render/renderer.rs` | Terrain render pass using TerrainPipeline + slab. |

### Create

| Component | File | Purpose |
|-----------|------|---------|
| Slab allocator | `sa_render/slab_allocator.rs` | Budget-driven vertex buffer pool |
| Collision grid | `sa_terrain/collision_grid.rs` | Fixed-LOD 7×7 grid logic |
| Terrain constants | `sa_terrain/config.rs` | Centralized K_FACTOR, budgets, grid size |
| Terrain shader | `sa_render/shaders/terrain.wgsl` | Vertex morphing shader |

### Remove

| Component | Reason |
|-----------|--------|
| Surface barrier (10km cuboid) | Replaced by collision grid |
| Icosphere hide/show logic | Replaced by depth coexistence |
| `gpu_meshes: HashMap` | Replaced by slab allocator |
| GPU memory cap workarounds | Slab has built-in budget |

### Implementation Order

1. **Slab allocator + shared index buffer** — independent, testable
2. **TerrainVertex + morph_target** — chunk gen + vertex format + terrain shader
3. **Terrain render pipeline** — new pipeline, wire into renderer
4. **Quadtree range formula + node cap** — tune constants
5. **Wire terrain integration to slab** — replace HashMap
6. **Collision grid** — new sa_terrain module + wire into terrain_colliders
7. **Remove surface barrier + icosphere logic** — cleanup
8. **Priority streaming** — upgrade request channel

Steps 1–3 can be developed in parallel. Each step is independently testable.

---

## 11. Configuration Reference

All terrain constants centralized in `sa_terrain/config.rs`:

```rust
// LOD selection
pub const K_FACTOR: f64 = 2.5;
pub const MAX_VISIBLE_NODES: usize = 400;
pub const MIN_RANGE: f64 = 50.0;

// Mesh generation
pub const GRID_SIZE: usize = 33;
pub const CELLS: usize = 32;
pub const VERTS_PER_HEIGHTMAP_CHUNK: u32 = 1_217;
pub const BYTES_PER_TERRAIN_VERTEX: u32 = 48;

// GPU memory
pub const HEIGHTMAP_BUDGET_BYTES: u64 = 30_000_000;
pub const VOLUMETRIC_BUDGET_BYTES: u64 = 20_000_000;

// Collision
pub const COLLISION_GRID_SIZE: usize = 7;
pub const COLLISION_LOD_OFFSET: u8 = 2;
pub const COLLISION_MAX_CHUNK_WIDTH_M: f64 = 200.0;
pub const COLLISION_REBASE_THRESHOLD_M: f64 = 100.0;
pub const COLLISION_GRID_HYSTERESIS: f64 = 1.5;

// Streaming
pub const LRU_CAPACITY: usize = 1_000;
pub const MAX_UPLOADS_BURST: usize = 64;
pub const MAX_UPLOADS_STEADY: usize = 8;
pub const BURST_THRESHOLD: usize = 24;

// Activation
pub const TERRAIN_ACTIVATE_FACTOR: f64 = 2.0;
pub const TERRAIN_DEACTIVATE_FACTOR: f64 = 2.5;
pub const COLLISION_ACTIVATE_FACTOR: f64 = 0.2;
pub const COLLISION_DEACTIVATE_ALT_M: f64 = 500.0;
pub const ICOSPHERE_RADIUS_FACTOR: f64 = 0.999;

// Base chunks (never evicted)
pub const BASE_LOD_LEVELS: u8 = 2;  // LOD 0 + LOD 1
pub const BASE_CHUNK_COUNT: usize = 30;  // 6 + 24
```
