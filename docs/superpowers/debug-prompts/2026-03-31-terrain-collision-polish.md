# SpaceAway — Terrain Collision & Rendering Polish

## Context

SpaceAway is a cooperative first-person space exploration game with a custom
Rust engine (wgpu + rapier3d). The terrain system was redesigned with CDLOD
cube-sphere, slab allocator, vertex morphing, and independent collision grid.
The approach system was unified into an ApproachManager state machine.

After extensive development, the macro systems work (warp → cruise → approach →
terrain activation → collision grid → landing state machine). But fine-grained
quality issues remain in collision accuracy and terrain rendering coverage.

## Current State (what works)

- Warp → cruise cascade on arrival at star system
- Cruise deceleration proportional to altitude (speed = altitude/8s)
- Ray-sphere flythrough prevention at any cruise speed
- Terrain activates at 5× planet radius, streams LOD 0-17 chunks
- Slab allocator (60MB, ~1023 slots) manages GPU terrain memory
- 49 HeightField colliders in 7×7 collision grid (fixed LOD)
- Ship stops near the surface (collision partially works)
- Atmosphere icosphere hidden when terrain active
- Planet icosphere at 0.985× radius (below all terrain valleys)

## Remaining Problems

### Problem 1: Ship stops slightly underground

**Symptom:** The ship lands but the camera sees through the terrain surface
to the planet interior. The collision surface appears to be a few hundred
meters below the visual terrain.

**What we know:**
- 49 heightfield colliders exist (confirmed from COLLISION_DIAG logs)
- The ship DOES stop (collision works, it's just offset)
- Skid raycasts eventually detect ground (clearance drops from 100m)
- Visual terrain is at `R + (h-0.5) × amplitude` (h in [0,1])
- Collision heights also use `R + (h-0.5) × amplitude` (same formula)
- HeightField positioning in `build_heightfield_from_grid` uses:
  - avg_r = average of all height samples
  - Chunk center at avg_r along surface normal
  - Offset by (min_h - avg_r) along normal to place Y=0 at min height
  - Heights normalized to [0,1] × height_range

**What needs investigation:**
- Is the heightfield offset math placing colliders at the EXACT visual surface?
- Are there floating-point precision issues in the f64→f32 conversion?
- Is the rapier HeightField Y-axis convention correctly accounted for?
- Does the anchor_f64 stay synchronized with the ship's rapier position?
- Add diagnostic: render collision heightfield positions as wireframe to
  visually compare with terrain surface

### Problem 2: Player falls through terrain surface

**Symptom:** After exiting the ship via airlock, the player character falls
continuously through the terrain without any collision response.

**What we know:**
- Player collision groups ARE correct: GROUP_3 (PLAYER) collides with
  GROUP_5 (TERRAIN) — confirmed in `sa_player/src/controller.rs:48-51`
- HeightField colliders have correct groups: GROUP_5 membership, GROUP_3
  filter — confirmed in `terrain_colliders.rs:390-395`
- The ship (with skid colliders) DOES stop — so the heightfields exist
- The player capsule is ~0.3m radius — much smaller than the ship's
  4m × 24m skid spread

**Theories to investigate:**
1. Gaps between adjacent heightfield chunks — the player capsule (0.3m)
   could fall through gaps that the ship skids (spread over 4-24m) span
2. The player is teleported to the airlock position which might be below
   the collision surface
3. The collision grid might not update/recenter when switching from ship
   to player tracking
4. The heightfield surface might be at a different height than where the
   player expects to stand

**What needs investigation:**
- Log the player's rapier position when they exit the airlock
- Log the nearest heightfield collider's position and height range
- Check if there are physical gaps between adjacent heightfield chunks
- Try making the player capsule larger to test the gap theory

### Problem 3: Visible frustum culling (terrain chunks popping at screen edges)

**Symptom:** When the user rotates the camera, terrain chunks visibly
appear/disappear at the screen edges. The culling boundary is clearly visible.

**What we know:**
- Frustum culling is implemented in `sa_terrain/quadtree.rs:124-129`
- The VP matrix is built in `render_frame.rs:179-191` by combining the
  camera VP with a planet-relative translation
- Node bounding spheres include displacement inflation
- The frustum is extracted using Gribb/Hartmann method from the VP matrix

**Theories to investigate:**
1. VP matrix f32 precision loss at planet-scale distances
2. Camera FOV mismatch (the projection matrix FOV doesn't match the
   frustum extraction)
3. Bounding sphere radius too small (not accounting for skirt vertices
   that extend beyond the node bounds)
4. This might actually be streaming latency (chunks not in slab yet when
   camera turns) rather than frustum culling

**Diagnostic approach:**
- Test with frustum culling DISABLED (pass `None` instead of
  `frustum.as_ref()`) — if chunks still pop, it's streaming, not culling
- If chunks stop popping, the frustum planes are wrong — compare the
  frustum planes with the actual camera FOV
- Log which nodes are culled each frame when the popping occurs

## Files to Read

Before making any changes, read ALL of these:

### Collision system:
- `crates/spaceaway/src/terrain_colliders.rs` — `build_heightfield_from_grid`
  function (the positioning math), anchor rebase, collision grid update
- `crates/sa_terrain/src/collision_grid.rs` — height generation,
  `generate_collision_heights`, `sphere_to_cube_face`
- `crates/spaceaway/src/landing.rs` — skid raycasts, state machine
- `crates/sa_player/src/controller.rs` — player capsule creation, collision
  groups
- `crates/spaceaway/src/walk_mode.rs` — player terrain walking, gravity

### Terrain rendering:
- `crates/sa_terrain/src/quadtree.rs` — frustum culling in
  `select_visible_nodes`, bounding sphere computation
- `crates/sa_terrain/src/frustum.rs` — frustum plane extraction,
  `contains_sphere`
- `crates/spaceaway/src/render_frame.rs:175-200` — VP matrix construction for
  terrain
- `crates/sa_terrain/src/chunk.rs` — visual terrain vertex positions, height
  displacement formula
- `crates/sa_render/src/slab_allocator.rs` — slab eviction
- `crates/spaceaway/src/terrain_integration.rs` — terrain manager update,
  streaming, draw command generation

### Reference:
- `docs/superpowers/specs/2026-03-30-terrain-redesign.md` — terrain system spec
- `docs/superpowers/specs/2026-03-30-approach-landing-design.md` — approach spec
- `crates/sa_terrain/src/config.rs` — terrain constants
- `crates/spaceaway/src/constants.rs` — gameplay constants
- `CLAUDE.md` — project conventions

## Approach

For each problem:

1. **Diagnose with certainty** — add logging/visualization to PROVE where the
   issue is. Don't guess.
2. **Confirm 100%** — state the exact line of code, the exact math that's
   wrong, and prove it with numbers.
3. **Fix with proof** — show that the fix is correct (unit test, math
   verification, or log comparison before/after).
4. **Don't break other things** — run the full test suite after each fix.
   The `full_descent_test` must pass.

## Diagnostic Tools to Add

### 1. Collision wireframe visualization (optional, complex)
Render the HeightField collider boundaries as wireframe to visually compare
with the terrain surface. This would definitively show if colliders are offset.

### 2. Collision position logging (simple, do this first)
In `build_heightfield_from_grid`, log the final collider position in rapier
space. In `landing.rs`, log the skid raycast origin and direction. Compare:
are the raycasts passing through the collider volume?

### 3. Frustum culling toggle
Add a runtime toggle (key press) that disables frustum culling. If the
popping stops, it's a frustum bug. If it continues, it's streaming latency.

### 4. Player exit position logging
When the player exits the airlock, log their rapier position and the nearest
heightfield collider's height at that position. Check if there's a gap.

## Output

Fix all three problems with confirmed proof for each. Every change must:
- Be confirmed with diagnostic evidence (logs or math)
- Pass `cargo test --workspace`
- Pass `cargo clippy --workspace -- -D warnings`
- Follow the 300-line file limit and existing code patterns
