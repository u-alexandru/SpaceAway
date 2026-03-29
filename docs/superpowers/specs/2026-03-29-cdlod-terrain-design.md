# CDLOD Terrain & Planetary Landing Design

## Goal

Seamless space-to-surface planetary terrain with full player-controlled landing and takeoff. No loading screens, no autopilot, no cuts. The player flies their ship from orbit through the atmosphere to the ground, walks on the surface, re-enters the ship, and flies back to space — all in one continuous experience.

## Implementation Phases

This spec covers four phases, each independently testable and shippable:

- **Phase 1: Terrain Rendering** — Cube-sphere CDLOD quadtree, chunk streaming, LOD morphing, visual-only. Fly near a planet, see terrain replace the icosphere. No collision, no landing.
- **Phase 2: Terrain Collision + Gravity** — HeightField colliders, physics anchor, gravity transition. Ship cannot fly through the ground.
- **Phase 3: Landing** — Ground contact detection, landed state, takeoff. Complete landing/takeoff cycle.
- **Phase 4: Surface Walking** — PlanetSurface player mode, exit/enter ship on surface. Full space-to-surface-to-space loop.

## Scope

### In Scope (v1, all phases)

- Cube-sphere CDLOD quadtree terrain (6 faces, analytic mapping)
- Seamless space-to-surface terrain streaming at ship velocity
- Hybrid CPU/GPU generation (CPU thread pool + vertex shader detail, max 0.2m GPU displacement)
- Procedural heightmap (fastnoise-lite fBm, 5-6 octaves, domain warping)
- Biome vertex coloring by altitude and latitude (biome logic in sa_terrain, not sa_render)
- Planetary gravity transition (zero-G → surface gravity) with antiparallel slerp guard
- Ship landing physics (player-controlled descent, multi-point ground contact)
- Player walks on planet surface after exiting ship (TERRAIN collision group)
- Per-patch f64→f32 camera-relative rebasing (no planet size limit)
- Skirt geometry for LOD crack hiding
- Takeoff (reverse of landing — fly up, terrain unloads)
- Icosphere/terrain handoff with hysteresis (activate 2.0×, deactivate 2.5× radius)
- Terrain integration extracted to `terrain_integration.rs` (not inline in main.rs)

### Deferred (separate specs)

- Atmospheric entry VFX (burnout, heat glow, reentry effects)
- Cloud layers
- Surface sky dome (atmosphere color from ground, sun position, sunset)
- Crash/damage system
- Surface vehicles
- Water/ocean rendering
- Caves/overhangs (requires volumetric, not heightmap)
- Vegetation/props placement
- Altimeter HUD element
- Atmosphere audio (wind, reentry roar)

## Architecture

### New Crate: `sa_terrain`

Pure terrain math — no rendering, no physics dependencies. Follows the existing crate architecture pattern.

```
sa_terrain/
  src/
    lib.rs          — public API: TerrainConfig, ChunkData, ChunkKey
    cube_sphere.rs  — cube face → sphere mapping (analytic projection, Nowell 2005)
    quadtree.rs     — CDLOD quadtree: node selection, LOD ranges, frustum culling
    heightmap.rs    — noise sampling: fBm + domain warp → height + color
    biome.rs        — biome color determination by altitude/latitude/sub_type
    chunk.rs        — chunk mesh generation: vertices, indices, normals, skirts
    streaming.rs    — async chunk manager: crossbeam channels, priority queue, LRU cache
    gravity.rs      — planetary gravity: direction + magnitude from altitude
```

**Dependencies (downward only):**
- `sa_terrain` → `sa_math` (WorldPos, units), `sa_core` (Handle, events), `fastnoise-lite`, `crossbeam-channel`
- `sa_terrain` does NOT depend on `sa_render`, `sa_physics`, or `sa_player`
- Biome color logic lives in `sa_terrain::biome` (not in `sa_render::planet_mesh`)
- Integration with rendering, physics, and player happens in `spaceaway/src/terrain_integration.rs`

### Data Flow Per Frame

```
1. Camera f64 position → quadtree traversal → visible nodes + LOD levels
2. Diff with currently loaded → new chunks needed, old chunks to evict
3. New chunks → send via crossbeam channel (priority: nearest first)
4. Background: worker threads recv → sample noise → build vertices/indices/normals → build skirts
5. Main thread: try_recv() → upload completed meshes to GPU mesh_store
6. Main thread: create/remove rapier3d colliders for chunks near player (Phase 2+)
7. Render: each chunk = DrawCommand with pre_rebased=true
8. Evicted chunks → remove GPU mesh + collider → keep in LRU cache
```

## Cube-Sphere Geometry

### Analytic Cube-to-Sphere Mapping (Nowell 2005)

Six cube faces mapped to a sphere using the analytic mapping described by Philip Nowell (2005). This eliminates corner clustering from naive normalization:

```
x' = x * sqrt(1 - y²/2 - z²/2 + y²z²/3)
y' = y * sqrt(1 - x²/2 - z²/2 + x²z²/3)
z' = z * sqrt(1 - x²/2 - y²/2 + x²y²/3)
```

The 6 faces are +X, -X, +Y, -Y, +Z, -Z. Each face maps the unit square [-1, +1]² to one sixth of the sphere. Each face gets its own quadtree root.

References: Nowell (2005), Zucker & Higashi JCGT 2018 ("Cube-to-sphere Projections for Procedural Texturing and Beyond").

### Face Coordinate System

Each face has a local 2D coordinate system (u, v) ∈ [-1, +1]. A point (u, v) on face +Z maps to cube position (u, v, 1), which is then analytically projected and scaled by planet radius.

## CDLOD Quadtree

### LOD Levels

For an Earth-sized planet (radius 6,371 km), one cube face spans ~10,000 km on the sphere surface. To achieve 50m ground patches:

| LOD | Patch Size | Purpose |
|-----|-----------|---------|
| 0 | ~10,000 km | Full face from deep space |
| 1 | ~5,000 km | Approaching planet |
| 2 | ~2,500 km | |
| ... | ... | |
| 10 | ~10 km | Low orbit |
| 14 | ~600 m | Low altitude flight |
| 17 | ~50 m | Ground level, walking |

Total: 18 LOD levels (0 = coarsest, 17 = finest). LOD count adapts per planet: `ceil(log2(face_size / min_patch_size))`. 50m minimum patch size gives ~1.56m vertex spacing at the finest level — adequate for the low-poly flat-shaded aesthetic.

### LOD Range Selection

Each LOD level has a range distance. If the camera is farther than the range, that LOD is sufficient (don't subdivide further). Per the Strugar (2010) CDLOD paper:

```
lod_range[level] = min_range * 2^level

min_range = 50m (finest LOD drawn within 50m of camera)
```

This doubles per level: LOD 17 = 50m, LOD 16 = 100m, LOD 15 = 200m, ..., LOD 0 = 6,553 km.

### Node Selection Algorithm

```
fn select_nodes(node, camera_pos, frustum) → Vec<VisibleNode>:
    if node is outside frustum → return empty
    dist = distance(camera_pos, node.center_on_sphere)
    if dist > lod_range[node.level] → return [node]  // far enough, draw this
    if node.level == max_lod → return [node]          // finest level, can't subdivide
    else → recurse into 4 children, collect results
```

Frustum culling uses the node's bounding sphere. The sphere radius must be inflated by the maximum terrain displacement at that LOD level to prevent incorrectly culling terrain with large height features:

```
sphere_radius = half_diagonal_of_patch + max_displacement_at_lod
```

### Vertex Morphing

At LOD boundaries, vertices morph smoothly to prevent popping. The morph factor is computed per-vertex in the vertex shader, per the Strugar (2010) CDLOD paper:

```
morph_start = lod_range[level] * 0.5    // begin morphing at 50% of range
morph_end   = lod_range[level]          // fully morphed at range boundary

t = clamp((dist - morph_start) / (morph_end - morph_start), 0, 1)

// Odd-indexed vertices morph toward their parent LOD position (midpoint of even neighbors)
morphed_pos = mix(fine_pos, coarse_pos, t)
```

Every other vertex in the grid (odd-indexed — the ones that would be removed at the coarser LOD) morphs toward the midpoint of its even-indexed neighbors. This makes LOD transitions invisible.

## Chunk Generation

### Chunk Data Structure

```rust
struct ChunkKey {
    face: u8,           // 0-5
    lod: u8,            // 0-17
    x: u32, y: u32,     // grid position within face at this LOD level
}

struct ChunkData {
    key: ChunkKey,
    center_f64: [f64; 3],         // chunk center in planet-relative meters
    vertices: Vec<TerrainVertex>, // 33×33 = 1,089 + skirt vertices
    indices: Vec<u32>,            // ~2,048 triangles + skirt triangles
    heights: Vec<f32>,            // 33×33 raw heights for collision
    min_height: f32,              // for bounding sphere inflation during frustum culling
    max_height: f32,
}

struct TerrainVertex {
    position: [f32; 3],   // patch-local position (small numbers)
    color: [f32; 3],      // biome color
    normal: [f32; 3],     // surface normal
}
```

### Terrain Configuration

Each planet's terrain is configured by a `TerrainConfig` passed to the terrain system on activation:

```rust
struct TerrainConfig {
    planet_radius_m: f64,
    noise_seed: u64,              // from Planet::color_seed — same seed as icosphere
    sub_type: PlanetSubType,      // determines biome colors and displacement amplitude
    surface_gravity_ms2: f32,     // precomputed: 9.81 * mass_ratio / radius_ratio²
    displacement_amplitude: f32,  // 0.01–0.04 × planet_radius
}
```

### Grid Size: 33×33

Each chunk is a 33×33 vertex grid (power-of-2 + 1 for seamless borders between adjacent chunks). Neighboring chunks share edge vertex positions, guaranteeing watertight seams. This produces:
- 1,089 vertices per chunk
- 2,048 triangles (32×32 quads × 2 triangles)
- Plus ~128 skirt vertices/triangles along edges
- Memory per chunk: ~76 KB (vertices + indices + heights)

### Generation Pipeline (per chunk, background thread)

1. **UV grid:** Generate 33×33 (u, v) points spanning the chunk's region on the cube face
2. **Cube → sphere:** Apply analytic mapping to each point → unit sphere direction
3. **Scale:** Multiply by planet radius → world-space position on sphere surface
4. **Height sampling:** For each point, sample fastnoise-lite:
   - Noise input: 3D sphere position (ensures seamless across face boundaries)
   - fBm with 5 octaves, lacunarity 2.0, gain 0.5
   - Domain warping for natural terrain features (adds ~3 extra noise evals per sample)
   - Output: height value normalized to [0, 1]
5. **Displacement:** Move vertex along surface normal by `height × amplitude`
   - Amplitude varies by planet sub-type (0.01–0.04 × planet_radius)
6. **Vertex color:** Determined by height (altitude) + absolute latitude (biome):
   - Biome logic in `sa_terrain::biome` — colors derived from planet sub_type and noise_seed
   - Low altitude: green (temperate), yellow (desert), white (frozen)
   - High altitude: brown → grey → white (snow)
   - Poles: white/ice regardless of altitude
7. **Normals:** Compute per-face normals from triangle cross products, average at vertices
8. **Skirts:** Extend edge vertices downward by `2 × max_displacement_at_this_LOD`
   - Skirt normals point outward (same as adjacent face normal)
   - Skirt vertices use the same biome color as the edge vertex they extend
   - Skirts are visual-only — not included in collision heightfield
9. **Heights:** Store raw 33×33 height samples for rapier3d collision heightfield

**Performance estimate:** 1,089 samples × ~8 noise evaluations each (5 fBm octaves + 3 domain warp) = ~8,700 noise evaluations per chunk. fastnoise-lite at f64: ~25M evals/sec → **~0.35ms per chunk** on one core. Budget: 0.5ms per chunk to account for mesh building overhead.

## Async Streaming

### Thread Pool Architecture

Uses `crossbeam-channel` (MPMC — multiple producers, multiple consumers) instead of `std::sync::mpsc` because `mpsc::Receiver` is not `Sync` and cannot be shared across worker threads.

```
Main Thread                              Worker Pool (4 threads)
───────────                              ─────────────────────────
quadtree.select_nodes(camera)
  → needed: HashSet<ChunkKey>
  → loaded: HashSet<ChunkKey>
  → to_generate = needed - loaded
  → to_evict = loaded - needed

sort to_generate by distance (nearest first)

for key in to_generate:
  if lru_cache.contains(key):
    restore from cache (skip generation)
  else:
    request_tx.send(ChunkRequest)  ───►  workers share one request_rx (MPMC)
                                           let req = request_rx.recv()
                                           let chunk = generate_chunk(req)
                                           result_tx.send(chunk)  // MPMC sender

while let Ok(chunk) = result_rx.try_recv():  ◄───
  upload_to_gpu(chunk)
  if near_player: create_collider(chunk)  // Phase 2+

for key in to_evict:
  remove_gpu_mesh(key)
  remove_collider(key)
  lru_cache.insert(key, chunk)
```

### Priority Queue

Chunk requests are sorted by distance to camera. When the player is descending fast, this ensures the landing area gets highest LOD first. Requests for chunks behind the camera or at very high altitude have lowest priority.

### LRU Cache

Capacity: ~500 chunks (~38 MB total). When the cache is full, the least-recently-used chunk is evicted entirely. On cache hit, the chunk skips generation and goes straight to GPU upload.

Cache key: `ChunkKey(face, lod, x, y)`. Cache stores the full `ChunkData` including vertices and collision heights.

### Budget Per Frame

Target: upload at most 8 chunks per frame (~569 KB via `queue.write_buffer`, well under GPU transfer limits). At 60 FPS that's 480 chunks/second — more than enough for ship-speed terrain streaming. The thread pool generates faster than we upload.

## Coordinate System & Precision

### Per-Patch Camera-Relative Rebasing

No planet size limit. Each chunk's position is stored in f64 (planet-relative meters). At render time:

```
chunk_world_f64 = planet_center_f64 + chunk_center_f64
camera_world_f64 = known from galactic_position + local offset

render_offset_f32 = (chunk_world_f64 - camera_world_f64) as f32
```

This gives sub-millimeter f32 precision near the camera regardless of planet radius (at 100m offset, f32 ULP = 0.012mm). A 70,000 km gas giant works identically to a 3,000 km moon.

Chunks use patch-local vertex positions (small f32 values, e.g. 0–500m). The render_offset positions the patch in camera space.

### Physics Anchor Point

The physics world uses a local anchor point on the planet surface (Phase 2+):

```
anchor_f64 = planet_center_f64 + surface_normal_below_camera * planet_radius
```

All physics bodies (ship, player, terrain colliders) are positioned relative to this anchor in f32:

```
ship_physics_pos = (ship_f64 - anchor_f64) as f32
collider_pos = (chunk_f64 - anchor_f64) as f32
```

When the camera moves >100m from the anchor, the anchor rebases: new anchor computed, all physics positions shifted by the delta. This is a discrete per-frame operation — everything shifts together so there's no visual or physical discontinuity. Multiple rebases per frame are allowed if the ship is descending fast.

**Post-rebase sync (required):** After shifting all body positions, call `sync_collider_positions()` then `update_query_pipeline()` before the next physics step. All shifts must complete atomically before any `step()` call.

### Collision Radius

Terrain colliders exist only within ~500m of the player (enough for physics interactions). Beyond that, only visual meshes are loaded. This keeps rapier3d's broadphase fast regardless of how many visual chunks are loaded.

Colliders use rapier3d's `HeightField` shape built from the chunk's 33×33 height samples. HeightField is O(1) for point-in-cell queries (direct grid index lookup). Each HeightField collider is oriented to the local tangent plane of the sphere surface — for 50m chunks at ground level the curvature is negligible (<0.0002° across the chunk), so a flat HeightField is accurate.

## Icosphere / Terrain Handoff

### Activation with Hysteresis

To prevent rapid toggling when flying near the boundary:
- **Activate terrain** when camera distance < 2.0 × planet radius
- **Deactivate terrain** when camera distance > 2.5 × planet radius

### Icosphere Suppression

When terrain activates for a planet, that planet's icosphere DrawCommand (and its atmosphere shell) must be suppressed to prevent Z-fighting and double rendering. The `ActiveSystem` exposes a method to skip DrawCommands for a specific body index.

The handoff is **atomic within a single frame**:
- Same code block that sets `active_terrain = Some(...)` also sets `hidden_body_index = Some(planet_idx)`
- Same code block that sets `active_terrain = None` also clears `hidden_body_index = None`
- No frame gap where neither terrain nor icosphere is visible

### Multi-Body Priority

Only one terrain can be active at a time. When multiple landable bodies are within activation range (e.g., two moons), the **nearest** body wins:

```rust
let terrain_planet = active_system.landable_bodies()
    .filter(|b| camera_distance_to(b) < b.radius_m * 2.0)
    .min_by(|a, b| camera_distance_to(a).partial_cmp(&camera_distance_to(b)).unwrap());
```

If terrain is active for body A and body B becomes closer, terrain switches: deactivate A's terrain, activate B's terrain in the same frame.

## Gravity Transition

### Altitude-Based Blending

```
atmosphere_top = planet_radius * 1.2   // gameplay parameter, not physical atmosphere model
surface = planet_radius

altitude = distance_to_planet_center - planet_radius

if altitude > atmosphere_top:
    // Space mode: ship-local gravity (existing behavior)
    gravity_dir = ship_down
    gravity_mag = 9.81 (ship artificial gravity)

else if altitude > 0:
    // Transition zone: blend from ship gravity to planet gravity
    t = 1.0 - (altitude / (atmosphere_top - surface))  // 0→1 as descending
    planet_down = normalize(planet_center - ship_position)

    // Guard for antiparallel case (ship approaching planet inverted):
    // if ship_down and planet_down are nearly opposite (dot < -0.99),
    // use ship's right vector as intermediate rotation axis
    gravity_dir = slerp(ship_down, planet_down, t)
    gravity_mag = lerp(9.81, surface_gravity, t)

else:
    // On surface
    gravity_dir = normalize(planet_center - player_position)
    gravity_mag = surface_gravity
```

The 1.2× radius factor is a gameplay tuning parameter — for an Earth-sized planet this gives a ~1,274 km transition zone, providing several minutes of gradual gravity change during descent.

### Surface Gravity Per Planet

Computed from planet data using Newton's law in Earth-relative units:

```
g = 9.81 * (mass / mass_earth) / (radius / radius_earth)²
```

Where `mass` and `radius` are the planet's values expressed as ratios to Earth. Stored as `surface_gravity_ms2: f32` in `LoadedBody`.

Examples:
- Earth (1.0 M⊕, 1.0 R⊕) = 9.81 m/s²
- Super-Earth (5.0 M⊕, 1.58 R⊕) = 19.6 m/s²
- Small moon (0.05 M⊕, 0.44 R⊕) = 2.5 m/s²

**Moon mass derivation:** The `Moon` struct in `sa_universe` lacks a mass field. Mass is derived from radius assuming rocky density (~3.3 g/cm³): `mass_earth = (radius_km / 6371)³ * (3.3 / 5.51)` where 5.51 g/cm³ is Earth's mean density.

### Gravity Inside Landed Ship

When the ship is landed on a slope, the interior floors are tilted relative to planet gravity. Design decision: **ship-local gravity applies inside the ship hull** (the player walks normally on tilted floors). Planet gravity applies outside. The transition occurs at the ship exit point — a small blend zone (~2m) smoothly rotates gravity from ship-local to planet-radial as the player walks through the airlock.

## Ship Landing

### Descent

Player flies the ship toward the planet using normal helm controls (WASD + throttle). As altitude decreases:
- Planetary gravity increases (see transition above)
- Terrain LOD increases (more detail appears)
- Ship must counter gravity with thrust (pitch + throttle management)

The player controls everything — speed, angle of approach, when to brake. Landing is a skill.

### Ground Contact Detection

Each frame while near surface (altitude < 100m), cast **4 rays from the ship's bottom geometry** (representing landing skid positions) along both `-gravity_dir` and ship-local down:

```
for each landing_point in ship_bottom_points:  // 4 points (fore, aft, port, starboard)
    world_pos = ship_transform * landing_point
    ray_dir = -gravity_dir
    if raycast(world_pos, ray_dir) hits terrain collider:
        record hit_distance

min_clearance = minimum of all 4 hit distances

if min_clearance < landing_height AND vertical_speed < max_landing_speed:
    → LANDED
elif min_clearance < landing_height AND vertical_speed >= max_landing_speed:
    → hard landing (for v1: zero velocity; crash damage deferred)
```

`landing_height`: ~2m (ship bottom to ground clearance)
`max_landing_speed`: 5 m/s (gentle touchdown threshold)

Multi-point raycasting prevents false readings when the ship is tilted during approach — a single center ray along gravity could miss terrain directly below on angled approaches.

### Landed State

When landed:
- Ship body velocity zeroed, gravity_scale set to 0, linvel/angvel zeroed each frame (avoids body type switch edge cases)
- Ship position locked at landing point
- Engine/throttle state preserved (player can re-throttle to take off)
- Player can exit ship (F key) and walk on surface

### Surface Walking

After exiting the ship on a planet surface, the player controller switches to `PlanetSurface` mode.

**Collision group setup:** A new `TERRAIN` collision group (e.g., `Group::GROUP_5`). The player's collision filter must include **both** `SHIP_INTERIOR | TERRAIN` when on a planet surface, allowing the player to walk on terrain and re-enter the ship.

**Transition sequence (ship exit):**
1. Terrain colliders already loaded around landing site (from descent)
2. Add `TERRAIN` to player's collision filter
3. Teleport player to ship exterior exit position
4. Switch player controller to `PlanetSurface` mode
5. `char_controller.up` = away from planet center
6. Ship interior colliders remain — player can walk back in

**PlanetSurface mode specifics:**
- Gravity direction: toward planet center (recalculated per frame from player f64 position)
- Ground: terrain HeightField colliders (same move_shape sweep as ship interior)
- Camera "up": away from planet center
- Ship interior colliders still exist — player can walk back in and re-enter

### Takeoff

Player re-enters ship → sits at helm → throttles up:
- Ship gravity_scale restored, dynamic forces resume
- Thrust overcomes gravity → ship lifts off
- Terrain streams lower LOD as altitude increases
- At atmosphere_top: gravity crossfades back to ship-local
- Terrain system deactivates when outside sphere of influence (2.5× radius hysteresis)

## Integration Points

### New Module: `spaceaway/src/terrain_integration.rs`

All terrain integration logic extracted from main.rs into a dedicated module (main.rs is already 2,592 lines — near 9× the 300-line convention). This module owns:

- `TerrainManager` state (wraps `sa_terrain` streaming + GPU handles + collider handles)
- Activation/deactivation with hysteresis
- Icosphere suppression coordination
- Gravity blending computation
- Landing state machine
- Collider lifecycle management
- Physics anchor rebasing

Exposes a single `fn update(...) → TerrainFrameResult` that main.rs calls each frame.

### Changes to Existing Code

**`spaceaway/src/main.rs`:**
- New field: `terrain: Option<terrain_integration::TerrainManager>`
- Calls `terrain_integration::update()` each frame
- Applies returned gravity vector and landing state

**`spaceaway/src/solar_system.rs`:**
- Add `surface_gravity_ms2: f32` to `LoadedBody`
- Add `sub_type: Option<PlanetSubType>`, `noise_seed: u64`, `planet_type: Option<PlanetType>` to `LoadedBody`
- Add `hidden_body_index: Option<usize>` — skip DrawCommand for terrain-replaced planet
- Add `is_landable(&self) -> bool` method: true for Rocky planets and moons
- `update()` skips DrawCommand generation for `hidden_body_index` and its child bodies (atmosphere, rings)

**`sa_universe/src/system.rs`:**
- Add `mass_earth: f32` to `Moon` struct (derived from radius at rocky density)

**`sa_player/src/controller.rs`:**
- New enum variant: `PlayerFrame::PlanetSurface`
- Gravity direction sourced from terrain system instead of ship-down
- Camera up vector = away from planet center
- Collision filter includes `TERRAIN` group when in PlanetSurface mode

**`sa_ship/src/ship.rs`:**
- Add `landed: bool` flag
- When landed: zero velocity each frame, gravity_scale 0
- Takeoff: throttle > 0 while landed → restore gravity_scale, `landed = false`

### No Changes Required

- `sa_render` — terrain chunks are regular DrawCommands with `pre_rebased=true`, same vertex format, same geometry pipeline
- Star streaming, navigation, drive system — untouched
- Helm controls — same WASD steering
- Visor HUD — same (altimeter is deferred)
- Audio system — untouched (atmosphere sounds deferred)

## Vertex Shader Enhancement

The CPU generates a 33×33 base mesh. The vertex shader adds 1-2 extra octaves of noise for visual crispness at zero CPU cost. **Maximum GPU displacement: 0.2m** to keep the visual/collision mismatch imperceptible.

```wgsl
// In terrain vertex shader (uses existing geometry pipeline):
let base_pos = vertex.position;           // from CPU-generated mesh (patch-local)
let sphere_dir = normalize(instance_offset + base_pos);  // reconstruct sphere direction
let detail = simplex_noise_3d(sphere_dir * high_freq) * 0.2;  // max 0.2m
let final_pos = base_pos + vertex.normal * detail;
```

The existing geometry shader's `@builtin(front_facing)` two-sided lighting applies to terrain automatically. Terrain uses the same pipeline — no new render pipeline needed.

The WGSL simplex noise is a small (~50 line) standalone function. Only 1-2 octaves needed.

## Performance Targets

| Metric | Target |
|--------|--------|
| Chunk generation | < 0.5ms per chunk (CPU thread, f64 + domain warp) |
| Chunks uploaded per frame | ≤ 8 (~569 KB) |
| Visible chunks at ground level | ~200-400 |
| Visible chunks from orbit | ~50-100 |
| Collision chunks near player | ~20-30 (Phase 2+) |
| Total triangle count (terrain) | 400k-800k |
| LRU cache capacity | ~500 chunks (~38 MB) |
| Thread pool size | 4 workers (crossbeam MPMC) |
| Frame budget (terrain system) | < 2ms main thread |

## Testing Strategy

### Unit Tests (sa_terrain)
- Analytic mapping correctness: cube corners map to sphere, all 6 faces tile seamlessly
- Quadtree selection: correct node count at various distances, finest LOD near camera
- Noise determinism: same seed + same position = same height (f64 precision)
- Chunk vertex counts: 33×33 = 1,089 + skirt count
- Biome colors: correct by altitude/latitude/sub_type
- Gravity: correct direction and magnitude at various altitudes
- Bounding sphere inflation: includes max displacement

### Integration Tests (spaceaway)
- Terrain activates at 2.0× radius, deactivates at 2.5× (hysteresis)
- Icosphere suppressed when terrain active, restored when deactivated (same frame)
- Gravity blending produces correct vectors through transition zone
- Landing detection triggers at threshold with multi-point raycasting (Phase 3)
- Player collision filter includes TERRAIN when on surface (Phase 4)

### Visual Verification
- Fly from space to surface and back — no LOD popping, no cracks between chunks
- Check for precision artifacts at large planet radii (test with 30,000+ km body)
- Verify terrain matches icosphere appearance at activation distance
- Confirm skirts hide LOD boundary gaps
