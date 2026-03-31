# SpaceAway — Coordinate Synchronization Audit

## The Pattern

Every terrain/collision/approach bug in this codebase traces to the same root:
**fragmented state with implicit timing dependencies.** Multiple systems share
mutable state (anchor, rapier position, galactic position, planet position) but
initialize and update it at different times, in different files, with no
enforced ordering.

Examples of this pattern:
- Anchor initialized at (0,0,0), used by rapier sync for 6 minutes before
  collision sets it → ship at 8642 km from colliders
- System boundary at 100 AU, warp arrives at 630 AU → system loads then
  immediately unloads
- Thrust applied during cruise → rapier body drifts at 3000 m/s
- Atmosphere body not hidden → opaque shell blocks terrain
- VP matrix in f32 at planet scale → frustum culling pops chunks

## The Audit Technique

For every piece of shared state, answer these 5 questions:

1. **WHO writes it?** List every file:line that modifies this value.
2. **WHEN is it written?** What frame phase (approach update, helm mode,
   render frame, physics step)?
3. **WHO reads it?** List every file:line that reads this value.
4. **WHEN is it read?** What frame phase? Is it BEFORE or AFTER the write?
5. **What does each reader ASSUME?** Does it assume the value is fresh?
   Non-zero? In a specific coordinate system?

If any reader runs BEFORE the writer, or assumes something the writer
doesn't guarantee, that's a bug.

## Shared State to Audit

### 1. anchor_f64
- **What:** Physics anchor — maps planet-relative meters to rapier space
- **Location:** `TerrainColliders.anchor_f64` (terrain_colliders.rs)
- **Accessed via:** `terrain_mgr.anchor_f64()` (terrain_integration.rs)
- **Writers:**
  - `TerrainColliders::new()` → (0,0,0) (terrain_colliders.rs)
  - `terrain_mgr.set_anchor()` → cam_rel at terrain activation (render_frame.rs)
  - `update_collision_grid()` → cam_rel - ship_rapier on first call (terrain_colliders.rs)
  - `force_rebase()` → anchor += ship_rapier (terrain_colliders.rs)
  - `100m rebase` in update_collision_grid → anchor += ship_rapier (terrain_colliders.rs)
- **Readers:**
  - helm_mode.rs rapier body sync: `new_rapier = cam_rel - anchor`
  - helm_mode.rs galactic tracking: `galactic = planet + (anchor + rapier) / LY_TO_M`
  - build_heightfield_from_grid: `cx = chunk_world - anchor`
- **AUDIT:** Does every reader see a valid anchor? Is there a frame where
  anchor is (0,0,0) but a reader assumes it's cam_rel?

### 2. galactic_position
- **What:** Ship position in light-years (f64, galaxy-scale)
- **Location:** `App.galactic_position` (main.rs)
- **Writers:**
  - Cruise/warp delta in helm_mode.rs (lines ~450)
  - Impulse physics tracking in helm_mode.rs (lines ~320)
  - Teleport in input_handler.rs
  - Warp gravity well drop in helm_mode.rs
- **Readers:**
  - Approach manager (frame_update.rs)
  - Terrain cam_rel computation (terrain_integration.rs)
  - Solar system rendering (render_frame.rs)
  - Camera position (helm_mode.rs, walk_mode.rs)
  - Navigation lock update (frame_update.rs)
- **AUDIT:** Is galactic_position updated BEFORE or AFTER the readers use it
  each frame? Is there a frame where it's stale?

### 3. Ship rapier body position
- **What:** Ship rigid body translation in rapier space (f32, meters)
- **Location:** `physics.rigid_body_set[ship.body_handle].translation()`
- **Writers:**
  - Physics step integration (rapier internal)
  - Rapier body sync in helm_mode.rs: `set_translation(cam_rel - anchor)`
  - Rebase shift in terrain_colliders.rs: `translation += shift`
  - Velocity zero on disengage in helm_mode.rs
  - Teleport in input_handler.rs: `set_translation(zeros)`
  - apply_thrust (should only run in impulse, but verify)
- **Readers:**
  - Galactic position tracking in helm_mode.rs
  - Collision grid anchor init in terrain_colliders.rs
  - Landing skid raycasts in landing.rs
  - Camera position computation
- **AUDIT:** After a rebase, is the ship position consistent with anchor?
  After cruise→impulse transition, is velocity correct?

### 4. terrain_body_index
- **What:** Which solar system body has terrain active (for icosphere scaling)
- **Location:** `ActiveSystem.terrain_body_index` (solar_system.rs)
- **Writers:**
  - Set to Some(idx) in render_frame.rs when terrain activates
  - Set to None in render_frame.rs when terrain deactivates
- **Readers:**
  - Solar system rendering: scales icosphere, hides children (solar_system.rs)
- **AUDIT:** Is it set BEFORE the solar system renders? Is it cleared when
  terrain deactivates?

### 5. planet orbital position
- **What:** Where a planet is in its orbit (light-years)
- **Location:** Computed by `ActiveSystem.compute_positions_ly()`
- **Depends on:** `game_time_s` which advances by `dt` each frame
- **Frozen when:** `solar_dt = 0` (terrain active)
- **Writers:**
  - `system.update(dt)` advances `game_time_s`
  - `system.update(0.0)` when terrain active (no advance)
- **Readers:**
  - Approach manager: planet_pos for altitude computation
  - Terrain activation: planet_pos frozen as terrain center
  - Icosphere rendering: orbital position for model matrix
  - Lock icon: locked_target.galactic_pos (captured at lock time)
- **AUDIT:** When terrain activates, is the terrain center == icosphere
  position? Does the lock icon track the frozen position?

### 6. collision_active flag
- **What:** Whether the collision grid should generate heightfields
- **Location:** `ApproachState.collision_active` (approach.rs)
- **Writers:**
  - Approach manager: true when phase is LowerAtmosphere/Landing/Surface/Departing
- **Readers:**
  - render_frame.rs: passes to terrain_mgr.update()
  - terrain_integration.rs: gates update_collision_grid() call
- **AUDIT:** Is collision_active set BEFORE or AFTER the terrain update
  that uses it? Is there a frame gap?

## How to Run This Audit

### Method: Parallel Agent Sweep

Dispatch 3 agents in parallel, each auditing 2 state items:

**Agent 1:** Audit anchor_f64 and ship rapier body position
- Trace every write and read across all files
- Verify frame ordering (which runs first each frame)
- Check for any frame where a reader sees stale/zero data
- Report any inconsistencies with file:line references

**Agent 2:** Audit galactic_position and planet orbital position
- Trace every write and read
- Check if galactic_position is ever read before it's updated in a frame
- Check if planet position freezing works correctly at terrain activation
- Check if the lock icon position stays synced

**Agent 3:** Audit terrain_body_index and collision_active
- Trace the lifecycle of terrain activation/deactivation
- Check if icosphere scaling applies before the render pass
- Check if collision_active reaches the collision grid on the right frame
- Check if deactivation cleans up all state

### For Each Issue Found

1. State the exact read-before-write ordering problem
2. Show the frame execution order that causes it
3. Compute the numerical impact (how many meters/km of error)
4. Propose a fix that eliminates the timing dependency
5. Verify the fix doesn't introduce new ordering issues

### The Gold Standard Fix

For each shared state item, the ideal fix is one of:
- **Compute once, read many:** One system computes it at the start of the
  frame, all others read the cached value (like ApproachState)
- **Single owner:** Only one system writes, period. Others request changes
  via events/flags, the owner applies them at a defined time
- **Immutable after init:** Set once at creation, never modified. If the
  value needs to change, create a new instance

## Files to Read

The audit requires reading these files (in execution order per frame):

1. `crates/spaceaway/src/main.rs` — frame loop order
2. `crates/spaceaway/src/frame_update.rs` — approach state + physics update
3. `crates/spaceaway/src/helm_mode.rs` — rapier body sync, cruise movement, galactic tracking
4. `crates/spaceaway/src/walk_mode.rs` — player physics, galactic tracking
5. `crates/spaceaway/src/render_frame.rs` — terrain activation, solar system update, collision flag
6. `crates/spaceaway/src/terrain_integration.rs` — terrain manager, collision grid call
7. `crates/spaceaway/src/terrain_colliders.rs` — anchor, heightfield positioning, rebase
8. `crates/spaceaway/src/approach.rs` — phase computation, derived flags
9. `crates/spaceaway/src/solar_system.rs` — orbital positions, icosphere rendering
10. `crates/spaceaway/src/landing.rs` — skid raycasts, state machine

## Output

For each state item:
- Confirmed issues (with file:line and frame ordering proof)
- Fixes applied (with test verification)
- Remaining risks (timing windows that are acceptable vs dangerous)
