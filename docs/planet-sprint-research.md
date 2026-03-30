# Planet Sprint — Research & Analysis Document

## Sprint Scope

Complete rework of the planet approach, terrain LOD, collision, landing, and takeoff systems. Every aspect from "dot in the sky" to "walking on the surface" must work seamlessly.

---

## Part 1: Current System Audit (SpaceAway)

### Architecture Overview

The terrain system uses CDLOD (Continuous Distance-Dependent LOD) with a cube-sphere quadtree, async chunk streaming via 4 worker threads, and rapier3d HeightField colliders. The design doc (`docs/superpowers/specs/2026-03-29-cdlod-terrain-design.md`) is thorough and well-architected. The implementation has critical bugs.

### Critical Bugs Found

#### Bug 1: Skirt Drop = 254km (CRITICAL)
**File:** `sa_terrain/src/chunk.rs` line 286
**Code:** `let drop = ((config.radius_m * skirt_drop_fraction) as f32).max(skirt_drop_min);`
**Problem:** For Earth-sized planet (radius 6,371km), displacement_fraction=0.02:
- `skirt_drop_fraction = 0.02 * 2.0 = 0.04`
- `drop = 6,371,000 * 0.04 = 254,840m = 254km`
- Comment says "capped at 500m" but NO CAP EXISTS in code
**Impact:** Every chunk has 254km-long skirt triangles hanging from its edges. These massive distorted triangles are what the user sees as "hexagonal shapes" from distance. They also confuse the LOD system and create impossible collision geometry.
**Fix:** Cap the drop: `let drop = ((config.radius_m * skirt_drop_fraction) as f32).min(500.0).max(skirt_drop_min);`

#### Bug 2: Orbital Drift (FIXED in this session)
**File:** `render_frame.rs` line 469
**Problem:** Solar system TIME_SCALE=30 keeps planets orbiting while terrain freezes the planet position. Icosphere separates from terrain chunks.
**Fix applied:** Pass `dt=0` to solar system when terrain is active.

#### Bug 3: Icosphere Hidden Too Early
**File:** `terrain_integration.rs` line 269
**Problem:** Original threshold was 6 absolute chunks. With 100+ visible nodes at activation distance, 6 chunks = 6% coverage. Planet dissolves into floating panels.
**Fix applied:** Require max(6, visible_count/2) before hiding. Still needs validation.

#### Bug 4: LRU Cache O(n) Promotion
**File:** `streaming.rs` lines 78-82
**Problem:** `retain()` walks entire VecDeque on every cache hit. With 1,000 entries, this is measurably slow.
**Fix needed:** Replace with doubly-linked list or indexed structure.

#### Bug 5: Surface Barrier Recomputed Every Frame
**File:** `terrain_colliders.rs` lines 118-148
**Problem:** The 10km×100m×10km collision barrier is repositioned every frame, breaking rapier's continuous collision detection. Ship can tunnel through at high speed.

#### Bug 6: No Frustum Culling Active
**File:** `terrain_integration.rs` line 143
**Problem:** Frustum culling infrastructure exists but is passed as `None`. Back-hemisphere chunks are traversed and streamed unnecessarily, wasting half the node budget.

### Data Flow: Space to Surface

```
1. Solar System → icosphere visible, planet orbiting at 30× speed
2. Camera < 2.0× radius → terrain activates, planet position frozen
3. Quadtree traversal → 100-800 visible nodes based on distance
4. Worker threads generate chunks → LRU cache → GPU upload
5. When enough chunks ready → icosphere hides, terrain takes over
6. Colliders created for nearby chunks (LOD ≥ 8)
7. Gravity blends from ship-local to planet-radial
8. Landing raycasts detect ground within 100m
9. FLYING → SLIDING → LANDED state machine
```

### Where It Breaks

| Step | Problem | Severity |
|------|---------|----------|
| 2 | Activation at 2× radius is far — coarse LOD chunks look terrible | Medium |
| 3 | 254km skirts distort all chunk geometry | CRITICAL |
| 4 | Streaming is FIFO, not priority-ordered by distance | Medium |
| 5 | Icosphere hides before terrain has adequate coverage | High |
| 6 | Collider height scaling uses average-subtracted heights — correct but fragile | Low |
| 7 | Gravity works correctly | OK |
| 8 | 100m raycast limit misses tall terrain features | Medium |

---

## Part 2: Industry Research

*[Sections below will be filled by research agents]*

### LOD Techniques Comparison

| Technique | Used By | Pros | Cons |
|-----------|---------|------|------|
| **CDLOD** (Strugar 2010) | SpaceAway, Outerra, most indie | Smooth morphing, GPU-friendly, no stitching needed | Heightmap-only, cube corners distort |
| **Clipmaps** (Losasso/Hoppe 2005) | Some AAA | Constant memory, simple streaming | Wastes detail behind camera, hard on sphere |
| **Chunked LOD** | KSP, many indie | Easy to implement | Visible popping without morphing |
| **ROAM** (1997) | Legacy only | Optimal triangle count | Obsolete — CPU-heavy, hostile to GPU batching |
| **Voxel SDF** | No Man's Sky | Caves, overhangs, destructible | Far more complex, CPU-intensive meshing |

**Verdict:** CDLOD on cube-sphere is the correct choice for SpaceAway. The implementation just needs its bugs fixed.

### Key CDLOD Principles (from Strugar 2010)

1. **Vertex morphing eliminates cracks AND popping.** Odd-indexed vertices morph toward their even neighbors based on distance. When morph=1.0, the vertex matches the coarser LOD exactly. No skirts needed for LOD seams (skirts only needed for chunk edge gaps during streaming).
2. **Single fixed-resolution grid mesh** reused for every node. Vertex shader transforms it to each node's area. SpaceAway already does this (33×33 grid).
3. **LOD ranges double per level.** Camera farther than range = don't subdivide. SpaceAway implements this correctly.
4. **Screen-space error metric** is the gold standard for subdivision. Current SpaceAway uses distance-only.
5. **Morph zone starts at 50% of LOD range.** SpaceAway computes morph_factor but never passes it to the shader.

### Space-to-Surface Transition (Research Consensus)

The seamless transition from orbit to ground uses ONE continuous system:
- Same quadtree spans from 10,000km orbital view down to 1m ground detail
- No scene switch, no loading screen — just deeper subdivision
- Vertex morphing prevents popping at every LOD level
- Double-precision positions (f64) converted to camera-relative f32 for rendering
- Normal maps at each LOD level so lighting detail persists when geometry is coarse

### Seam Handling (Research Consensus)

Four approaches, ranked by quality:
1. **CDLOD morphing** — eliminates mismatch entirely (best, SpaceAway should use this)
2. **Degenerate triangles** — move cracking vertices to match coarser neighbor
3. **Skirts** — vertical walls under edges (SpaceAway currently uses this, simplest but lowest quality)
4. **Triangle fan stitching** — connect fine edge to coarse edge with fan

**Key constraint:** Adjacent patches must be within ±1 LOD level. Enforced during quadtree traversal.

### What Games Use

| Game | Technique | Key Lesson |
|------|-----------|------------|
| **No Man's Sky** | Voxel SDF + Dual Contouring | Overlapping chunks for seams, CPU mesh generation |
| **Star Citizen** | Quadtree cube-sphere + procedural rules | Artist-driven biomes, "Planet Tech v5" |
| **Outerra** | CDLOD-derived quadtree on WGS84 ellipsoid | Real Earth scale, GPU morphing |
| **KSP** | Chunked LOD on scaled sphere | Simple but effective for indie |
| **SpaceEngine** | Multi-resolution quadtree | Billions of planets, heavy LOD optimization |

### Sources

- [Strugar CDLOD Paper](https://aggrobird.com/files/cdlod_latest.pdf)
- [GPU Gems 2 Ch.2 — Geometry Clipmaps](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
- [No Man's Sky GDC Talk](https://www.gdcvault.com/play/1024265/Continuous-World-Generation-in-No)
- [Planetary terrain rendering (dexyfex)](https://dexyfex.com/2015/11/30/planetary-terrain-rendering/)
- [Planet-LOD adaptive subdivision](https://github.com/sp4cerat/Planet-LOD)
- [CDLOD Rust demo](https://github.com/tschie/terrain-cdlod)
- [Cube-sphere projections (Acko)](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)

### Reference Implementations

| Project | Lang/GPU | Technique | Key Lesson for SpaceAway |
|---------|----------|-----------|--------------------------|
| **[Terra](https://github.com/fintelia/terra)** | Rust/wgpu | Sphere-mapped CDLOD, compute shader detail | Closest architecture match — study this |
| **[Proland](https://github.com/LarsFlaeten/Proland_dev)** | C++/OpenGL | Quadtree cube-sphere, GPU tile cache, Bruneton atmosphere | Producer pipeline with GPU caching |
| **[CDLOD Reference](https://github.com/fstrugar/CDLOD)** | C++/DX11 | Original Strugar implementation | Canonical morphing implementation |
| **[Planet-LOD](https://github.com/sp4cerat/Planet-LOD)** | C++ | Adaptive subdivision on GPU | GPU-driven LOD selection |
| **[Kosmos](https://github.com/kaylendog/kosmos)** | Rust/Bevy | Compute shader noise graph | Elegant noise pipeline |
| **[bevy_terrain](https://github.com/kurtkuehnert/terrain_renderer)** | Rust/Bevy | UDLOD — fully GPU-based culling + morphing | GPU-centric approach |
| **[Bruneton Atmosphere](https://github.com/ebruneton/precomputed_atmospheric_scattering)** | C++/GLSL | Precomputed 4D scattering LUTs | Standard atmosphere technique, needs WGSL port |

### Collision & Landing Approaches

#### Collision on Spherical Terrain (Research Consensus)

1. **Radial distance check**: `distance_from_center - (radius + height)`. If negative, object is below terrain.
2. **Local tangent-plane collision**: Near surface, project a patch into local tangent plane and use standard flat heightfield collision. Avoids full spherical math for narrow-phase.
3. **Rapier3d heightfield colliders**: Feed 33×33 height grids as `ColliderBuilder::heightfield(heights, scale)`. Fast and memory-efficient. SpaceAway already does this.
4. **Separate collision LOD from visual LOD**: Collision should be fixed moderate resolution (33×33 or 65×65) that does NOT change with camera distance. Visual LOD changes freely.
5. **Never change collision LOD under resting objects**: LOD transitions inject energy (objects bounce). Keep collision fixed or only transition when no bodies rest on that chunk.
6. **Async with fallback**: Generate collision on background thread. Until ready, use analytical sphere at `planet_radius + average_height` as fallback. Prevents falling through.

#### Landing Mechanics (Best Practices from KSP/Elite/Star Citizen)

| Game | Key Design | Lesson |
|------|-----------|--------|
| **KSP** | Full Newtonian, no autopilot, spring-damper landing legs, 12 m/s impact tolerance | Landing is a SKILL — satisfaction from mastery. Separate collision mesh from visual mesh. |
| **Elite Dangerous** | Three approach phases: supercruise → glide (2500 m/s, pitch-constrained) → normal flight | Distinct phases create drama and pacing. Terrain scanner below 75m aids precision. |
| **Star Citizen** | Seamless, 64-bit coords, physics grids per ship, VTOL manual landing | Local reference frames for walking in ships. Cinematic scale revelation. |

#### Recommended Landing Phases for SpaceAway

```
1. WARP APPROACH (100,000c–5,000,000c)
   - Planet is a navigation target, auto-deceleration near star
   - Gravity well auto-drops from warp (already implemented)

2. CRUISE APPROACH (1c–500c)
   - Planet visually grows, orbital features visible
   - Auto-disengage at atmosphere boundary (already implemented)

3. ATMOSPHERIC ENTRY / GLIDE
   - NEW: Unpowered/low-power descent phase
   - Pitch-constrained, dramatic reentry effects
   - Terrain LOD starts loading, icosphere transitions to terrain

4. POWERED DESCENT (impulse only)
   - Full manual control, ship counters gravity with thrust
   - Terrain scanner HUD shows altitude + surface topology
   - Landing site selection — flat terrain indicators

5. TOUCHDOWN
   - 4-point raycast from landing skids (already implemented)
   - Spring-damper contact, impact speed → damage category
   - FLYING → SLIDING → LANDED state machine (already implemented)

6. SURFACE OPS
   - Ship locked on ground, player exits via airlock
   - Planet gravity full strength, walk on terrain
   - Resource gathering, exploration

7. TAKEOFF
   - Reverse of landing: unlock → throttle up → lift off
   - Terrain unloads as altitude increases
   - Resume cruise/warp when clear of atmosphere
```

#### Origin Rebasing / Large World Precision

All surveyed engines use the same pattern SpaceAway already implements:
- **f64 simulation** (WorldPos) for all game logic, physics, orbits
- **Camera-relative f32** for GPU rendering: `(object_f64 - camera_f64) as f32`
- **Origin shifting** when camera moves beyond threshold (SpaceAway: 100m)

| Distance from origin | f32 precision |
|---------------------|---------------|
| 1 km | ~0.06 mm |
| 10 km | ~1 mm |
| 100 km | ~8 mm |
| 1,000 km | ~60 mm |
| 6,371 km (Earth radius) | ~0.38 m |

#### Gravity Transition (Research Consensus)

- **Dominant-body inverse-square with SOI falloff** — compute gravity from nearest body, smooth blend at boundaries
- SpaceAway's `gravity.rs` already implements this correctly with smoothstep blending
- Key: gravity direction = toward planet center, magnitude = `surface_g × (radius/distance)²`

#### Terrain Streaming ↔ Physics Sync

1. Physics and graphics use different LODs — physics stays at fixed moderate resolution
2. Only load collision within ~500m of player (SpaceAway already does this)
3. Background thread collision generation with low-res sphere fallback
4. Never remove collision under resting bodies
5. CPU-side heightmap (SpaceAway's approach) avoids GPU readback latency

---

## Part 3: Proposed Fixes (Priority Order)

### P0: Fix Skirt Drop Formula
- Cap at 500m as the comment says
- `drop = ((face_size * 0.1) as f32).min(500.0).max(skirt_drop_min)`

### P0: Validate Icosphere-Terrain Transition
- Current 50% threshold may be too high or too low
- Need to test at different planet sizes and approach speeds
- Consider: keep icosphere visible (with slight alpha/scale reduction) until 80%+ coverage

### P1: Enable Frustum Culling
- Fix the VP-to-planet-relative matrix math
- This halves the node count and streaming load

### P1: Priority-Ordered Chunk Streaming
- Sort requests by distance to camera before sending to workers
- Nearest chunks generate first, fill landing area before distant terrain

### P2: LOD Morphing in Vertex Shader
- morph_factor is already computed by the quadtree
- Need to pass it as an instance attribute and blend in the shader
- Eliminates visible LOD popping

### P2: Surface Barrier Caching
- Only recompute when camera moves >50m
- Cache position between rebases

---

## Part 4: Sprint Backlog (Prioritized)

### P0 — Game-Breaking Bugs (Fix First)

| # | Task | File | Issue |
|---|------|------|-------|
| 1 | **Fix skirt drop formula** | chunk.rs:286 | 254km drop instead of ~500m max. Causes massive distorted geometry. Add `.min(500.0)` cap. |
| 2 | **Fix orbital freeze** | render_frame.rs | Solar system keeps orbiting while terrain is frozen. Pass dt=0 when terrain active. **(DONE)** |
| 3 | **Fix icosphere hide threshold** | terrain_integration.rs:269 | 6-chunk threshold too low. Use max(6, visible/2). Validate at different planet sizes. **(PARTIALLY DONE)** |

### P1 — Core Experience (Planet Approach Must Work)

| # | Task | File | Issue |
|---|------|------|-------|
| 4 | **Implement LOD vertex morphing in shader** | geometry.wgsl + terrain_integration.rs | morph_factor already computed per node but never used. Pass as instance attribute, blend odd vertices toward even neighbors. This is the CDLOD signature feature — eliminates popping AND makes skirts less necessary. |
| 5 | **Enable frustum culling** | terrain_integration.rs + quadtree.rs | Infrastructure exists but passed as None. Need correct VP-to-planet-relative transform. Halves node count and streaming load. |
| 6 | **Priority-ordered chunk streaming** | streaming.rs | Sort requests by distance before sending to workers. Nearest chunks (landing area) generate first. Currently FIFO. |
| 7 | **Reduce terrain activation distance** | terrain_integration.rs | 2.0× radius is too far — LOD 0 chunks are enormous. Consider 1.5× with faster streaming burst. |

### P2 — Polish & Performance

| # | Task | File | Issue |
|---|------|------|-------|
| 8 | **Cache surface barrier position** | terrain_colliders.rs | Recomputed every frame, breaks CCD. Cache until camera moves >50m. |
| 9 | **Optimize LRU cache** | streaming.rs:78-82 | O(n) retain() on every cache hit. Replace VecDeque+HashMap with proper LRU (e.g., linked-hash-map). |
| 10 | **Increase landing raycast range** | landing.rs:25 | MAX_RAY_DIST=100m misses tall terrain. Increase to 500m or 1km. |
| 11 | **Add approach HUD elements** | ui/ | Altitude readout, terrain scanner below 100m, surface flatness indicator for landing site selection. |

### P3 — Future (Not This Sprint)

| # | Task | Notes |
|---|------|-------|
| 12 | Atmospheric entry VFX | Reentry heat, wind shake, engine sound changes |
| 13 | Atmospheric scattering shader | Bruneton precomputed scattering (WGSL port) |
| 14 | Surface walking (Phase 4) | Player exits ship, walks on terrain |
| 15 | Normal map detail layers | Fine surface detail without geometry cost |
| 16 | Biome variation by latitude/climate | Currently static per height band |

---

## Part 5: Key Architectural Decisions

### Decision 1: Keep CDLOD (Validated)
Research confirms CDLOD on cube-sphere is the dominant approach for heightmap-based planet rendering in indie games. Used by Outerra, Terra, SpaceEngine, and most indie implementations. The SpaceAway architecture is correct — the implementation just has bugs.

### Decision 2: Vertex Morphing > Skirts for LOD Seams
Skirts are a hack that creates visible artifacts (the 254km skirt bug is a symptom). CDLOD's vertex morphing inherently eliminates cracks by smoothly blending vertices between LOD levels. The morph_factor is already computed — it just needs to reach the shader.

### Decision 3: Separate Collision LOD from Visual LOD
Every surveyed engine maintains a separate, fixed-resolution collision mesh. Visual LOD changes freely; collision stays at moderate resolution (33×33 heightfield per chunk). Never change collision under resting bodies.

### Decision 4: Freeze Orbital Motion During Terrain
When terrain activates, the planet MUST stop moving. Both the terrain system and the solar system renderer must agree on the planet's position. Otherwise terrain chunks and the icosphere drift apart at TIME_SCALE=30.

### Decision 5: Phased Approach Pacing
Warp → Cruise → Glide → Powered Descent → Touchdown. Each phase reveals more detail and gives the player time to orient. The drive system tiers already map to this naturally.

---

## Sources

### Papers & Technical
- [Strugar CDLOD Paper (2010)](https://aggrobird.com/files/cdlod_latest.pdf)
- [GPU Gems 2 Ch.2 — Geometry Clipmaps](https://developer.nvidia.com/gpugems/gpugems2/part-i-geometric-complexity/chapter-2-terrain-rendering-using-gpu-based-geometry)
- [Bruneton Atmospheric Scattering](https://ebruneton.github.io/precomputed_atmospheric_scattering/)
- [Rapier3d Colliders Documentation](https://rapier.rs/docs/user_guides/rust/colliders/)

### Game References
- [No Man's Sky GDC Talk](https://www.gdcvault.com/play/1024265/Continuous-World-Generation-in-No)
- [Star Citizen Planet Tech v1](https://starcitizen.tools/Planet_Tech_v1)
- [Elite Dangerous Planetary Approach](https://elite-dangerous.fandom.com/wiki/Planetary_Approach_Suite)
- [KSP Developer Insights: Planet Tech](https://forum.kerbalspaceprogram.com/topic/205930-developer-insights-12-%E2%80%93-planet-tech)
- [SpaceEngine Quadtree Blog](https://spaceengine.org/news/blog171120/)
- [Outerra Engine](https://outerra.blogspot.com/2012/02/)

### Open Source Implementations
- [Terra (Rust/wgpu CDLOD)](https://github.com/fintelia/terra)
- [CDLOD Reference (Strugar)](https://github.com/fstrugar/CDLOD)
- [Planet-LOD](https://github.com/sp4cerat/Planet-LOD)
- [Proland](https://github.com/LarsFlaeten/Proland_dev)
- [Kosmos (Rust/Bevy)](https://github.com/kaylendog/kosmos)

### Techniques
- [Planetary Terrain Rendering (dexyfex)](https://dexyfex.com/2015/11/30/planetary-terrain-rendering/)
- [Origin Rebasing (Frozen Fractal)](https://frozenfractal.com/blog/2024/4/11/around-the-world-14-floating-the-origin/)
- [Godot Large World Coordinates](https://docs.godotengine.org/en/stable/tutorials/physics/large_world_coordinates.html)
- [Cube-Sphere Projections (Acko)](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
