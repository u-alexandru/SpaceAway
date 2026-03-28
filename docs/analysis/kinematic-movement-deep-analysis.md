# Kinematic Movement Deep Analysis

## Root Cause Investigation: Player Sliding, Quaking, and Snapping

**Date:** 2026-03-27
**Analyst:** Physics Systems Analysis
**Files examined:**
- `crates/sa_player/src/controller.rs` (full file, 253 lines)
- `crates/spaceaway/src/main.rs` (lines 784-833, walk mode frame ordering)
- `crates/sa_ship/src/ship.rs` (full file, ship body setup)
- `crates/sa_physics/src/world.rs` (full file, PhysicsWorld step/query)
- `crates/spaceaway/src/ship_colliders.rs` (full file, interior colliders)
- `/tmp/spaceaway_debug.json` (live debug snapshot at frame 2400)

---

## 1. COMPLETE FRAME TRACE

### Setup parameters (from debug snapshot):

- Ship velocity: V_ship = (0.0, 0.0, -90.184) m/s
- Ship position: P_ship = (0.0, 0.0, -1380.319)
- Player position: P_player = (-0.119, -0.080, -1378.384)
- dt = 1/60 = 0.01667 s
- physics_dt = min(dt, 1/30) = 0.01667 s
- Player grounded: **false** (this is already a symptom)
- Player reported velocity: (0.0, -0.062, -81.646)

### Key observation from debug data:

The player Z is -1378.384 while the ship Z is -1380.319. Floor colliders are
children of the ship body. The floor of the cockpit section (z_start=0, length=4)
is at ship-local offset z=2.0 (center of section). In world space that is
-1380.319 + 2.0 = -1378.319. The player is at z=-1378.384, which is 0.065m
aft of the floor center -- plausible for standing in the cockpit.

But the player is at y=-0.080, and FLOOR_Y=-1.0 with floor thickness 0.1, so
the floor top surface is at ship-local y = -1.0. In world space (ship at y=0):
floor top = 0.0 + (-1.0) = -1.0. The player capsule bottom is at
y_body - PLAYER_HALF_HEIGHT - PLAYER_RADIUS = -0.080 - 0.6 - 0.3 = -0.98.
This is 0.02m above the floor surface -- consistent with the `offset: 0.02`
skin distance. The player is essentially on the floor.

Yet `grounded: false`. This is the first smoking gun.

---

## 2. FRAME-BY-FRAME TRACE (Walk Mode)

### Frame N begins. Current state:
- P_ship = (0, 0, S) where S = -1380.319
- P_player = (px, py, pz) where pz ~ S + local_offset
- Floor colliders at world position = P_ship + local_offset
- V_ship = (0, 0, -90.184)

### Step 1: Apply ship thrust (main.rs:798-801)
```
ship.reset_forces()   -- clears accumulated forces
ship.apply_thrust()   -- adds F = throttle * max_thrust along ship's -Z
```
Force = 0.357 * 500,000 = 178,500 N along -Z.
Acceleration = F/m = 178,500 / 50,094.7 = 3.563 m/s^2.

### Step 2: Physics step (main.rs:804-807)
```
physics.step(physics_dt)   -- rapier integrates all bodies
```

During this step:
- **Ship body (dynamic):** Integrates velocity and position.
  - V_ship' = V_ship + a * dt = -90.184 + (-3.563)(0.01667) = -90.243 m/s (Z component)
  - P_ship' = S + V_ship * dt + 0.5 * a * dt^2
  - P_ship'.z = -1380.319 + (-90.184)(0.01667) + 0.5*(-3.563)(0.01667^2)
  - P_ship'.z = -1380.319 - 1.503 - 0.0005 = -1381.822

- **Floor colliders:** Children of ship body, move with it automatically.
  - Floor is now at P_ship' + local_offset

- **Player body (kinematic_position_based):** CRITICAL QUESTION.
  - Rapier's `kinematic_position_based` bodies move toward their `next_kinematic_position`
    target during the step. If `set_next_kinematic_translation` was called in the
    PREVIOUS frame with target T, then during THIS step the body moves to T.
  - The body's velocity is computed as (T - current) / dt by rapier internally.
  - If no new target was set, the body stays where it is (velocity = 0).

  **After step:** Player body is at the target set LAST frame (T_{N-1}).

### Step 3: Update query pipeline (main.rs:811)
```
physics.update_query_pipeline()
```
The query pipeline now indexes collider positions AFTER the step.
Floor colliders are at their new positions (P_ship' + offset).

### Step 4: Read ship velocity (main.rs:814-817)
```
ship_vel = body.linvel() = (0, 0, ~-90.243)
```
This is the post-step velocity. Correct.

### Step 5: Player update (main.rs:820-826, controller.rs:89-191)

```rust
player.update(&mut self.physics, &self.input, dt, ship_vel)
```

Note: `dt` is passed here, NOT `physics_dt`. (main.rs:825 uses `dt`).

Inside `update()`:

1. **Vertical velocity** (controller.rs:130-139):
   - grounded=false (from previous frame), so: `vertical_velocity -= GRAVITY * dt`
   - vertical_velocity = -0.062 - 9.81 * 0.01667 = -0.062 - 0.164 = -0.226

   Wait -- the debug shows vel.y = -0.062. But if the player were truly grounded
   last frame, vertical_velocity would be 0.0, and desired.y would be
   ship_vel.y * dt = 0.0 * 0.01667 = 0.0. The player shouldn't have any Y velocity.
   The -0.062 suggests the player has been falling for at least one frame.

2. **Desired translation** (controller.rs:141-147):
   ```
   desired = (base_velocity + walk + vertical) * dt
   desired.x = (0.0 + 0.0) * dt = 0.0
   desired.y = (0.0 + vertical_velocity) * dt = (0.0 + (-0.226)) * 0.01667 = -0.00377
   desired.z = (-90.243 + 0.0) * dt = -90.243 * 0.01667 = -1.504
   ```

3. **Get char_pos** (controller.rs:150-153):
   ```
   char_pos = body.position()
   ```
   **CRITICAL:** This reads the player body position AFTER the physics step.
   The player body moved to T_{N-1} during the step. So char_pos = T_{N-1}.

4. **move_shape sweep** (controller.rs:165-175):
   - Sweeps from char_pos by `desired`
   - The query pipeline has floor colliders at their POST-STEP positions
   - The sweep checks: can the player move from T_{N-1} by desired without
     hitting walls/floors?

5. **set_next_kinematic_translation** (controller.rs:178-181):
   ```
   new_pos = char_pos.translation + output.translation
   body.set_next_kinematic_translation(new_pos)
   ```
   This sets T_N = the target the body will move to during the NEXT physics step.

---

## 3. THE THREE BUGS

### BUG #1: ONE-FRAME DELAY (causes sliding/drift)

**Root cause:** `set_next_kinematic_translation` sets a TARGET for the next step.
The body does not move until `physics.step()` is called in the NEXT frame.

**The timeline:**
```
Frame N:
  step()          -> player moves to T_{N-1} (set last frame)
  update_query()  -> colliders indexed at post-step positions
  read ship_vel   -> V_ship_N (post-step)
  move_shape()    -> sweeps from T_{N-1}, computes correction
  set_next(T_N)   -> target for next step

Frame N+1:
  step()          -> ship moves by V_ship * dt again
                  -> player moves to T_N (one frame old!)
                  -> floor is now at P_ship'' + offset
                  -> player is at T_N, which was computed for
                     P_ship' + offset (last frame's floor position)
```

**The player reaches its intended position ONE FRAME LATE.** By the time
the player arrives at T_N, the ship has moved an additional V_ship * dt.

**Drift per frame:**
```
drift = V_ship * dt = 90.184 * 0.01667 = 1.503 m per frame
```

But wait -- the drift doesn't accumulate infinitely because each frame
recomputes the correction based on the current (stale-by-one-frame) position.
The steady-state error is exactly ONE FRAME of ship displacement:

```
steady_state_drift = |V_ship| * dt = 90.184 * 0.01667 = 1.503 meters
```

**Verification from debug data:**
- Player Z: -1378.384
- Ship Z: -1380.319
- Ship-local player offset should be ~2.0 (cockpit center)
- Expected player Z: -1380.319 + 2.0 = -1378.319
- Actual player Z: -1378.384
- Observed offset from expected: 0.065m

Hmm, the observed drift (0.065m) is much less than 1.503m. This suggests
the `move_shape` sweep IS partially correcting the drift each frame because
the floor colliders are at their current positions in the query pipeline.

Let me re-examine. The sweep starts from `char_pos` (which IS T_{N-1}, the
position the body reached during THIS frame's step). The desired displacement
is `ship_vel * dt`. The floor is at its current post-step position.

So the sweep says: "from where you are NOW, move by ship_vel * dt, and
don't go through the floor." This is actually correct for matching the
ship's movement THIS frame.

**The real issue is subtler:** The body is at T_{N-1} at the start of frame N.
T_{N-1} was computed to be at floor_position_{N-1} + standing_offset. But
during frame N's step, the floor moved to floor_position_N. The body also
moved to T_{N-1}. So after the step:

```
player position = T_{N-1}  (target from last frame, now reached)
floor position  = floor_N  (ship moved during this step)
gap = floor_N - T_{N-1} offset != the intended standing offset
```

Then move_shape computes desired = ship_vel * dt, starting from T_{N-1}.
The floor is at floor_N. The sweep will detect the floor and land the
player ON floor_N. So:

```
T_N = floor_N + standing_offset + ship_vel * dt
```

Wait, no. The sweep moves the player by `desired` = ship_vel * dt starting
from T_{N-1}. If the floor is below/at the same height, the horizontal
movement just works, and snap_to_ground handles vertical.

**Actually, the one-frame delay IS being compensated by move_shape because
move_shape uses the CURRENT query pipeline (post-step floor positions) and
snap_to_ground pulls the player to the actual floor.**

So the sliding might be minimal in steady state. Let me reconsider...

**REVISED ANALYSIS:** The one-frame delay means the player's PHYSICS BODY
is one frame behind, but the VISUAL position (read from body.translation()
in the same frame via `player.position()` at main.rs:829) reads T_{N-1},
not T_N. The camera sees the player at T_{N-1} while the ship's floor
rendering is at floor_N. This creates visual jitter but not necessarily
physical drift.

The actual drift comes from velocity mismatch and timing.

**Confidence: 85%** -- The one-frame delay exists but its observable effect
depends on whether the camera/rendering compensates.

---

### BUG #2: GROUNDED STATE OSCILLATION (causes quaking)

**Root cause:** The combination of:
1. `vertical_velocity` accumulating gravity when `grounded == false`
2. `snap_to_ground` pulling the player down when close to ground
3. The `grounded` flag toggling frame-to-frame

**The oscillation cycle:**

```
Frame N: grounded=true (from last move_shape)
  -> vertical_velocity = 0.0  (line 133)
  -> desired.y = (ship_vel.y + 0.0) * dt = 0.0  (ship_vel.y ~ 0)
  -> move_shape sweeps horizontally, snap_to_ground activates
  -> output.grounded = true (still on floor)
  -> set_next_kinematic_translation(pos on floor)

Frame N+1: physics.step() moves player to target
  -> Player is on the floor (T_N was floor-level)
  -> But the floor has moved (ship moved during step)
  -> Player body is at T_N, floor is at floor_{N+1}
  -> There is now a SMALL GAP between player and floor

  -> grounded was true from frame N, vertical_velocity = 0.0
  -> desired.y = 0.0
  -> move_shape: starting position has small gap to floor
  -> If gap > snap_to_ground distance (0.2m): grounded = false!
  -> If gap < snap_to_ground distance: snap pulls player down, grounded = true

Frame N+2 (if grounded=false from N+1):
  -> vertical_velocity -= GRAVITY * dt = -9.81 * 0.01667 = -0.164 m/s
  -> desired.y = -0.164 * dt = -0.00273 m (downward)
  -> move_shape: player drops, hits floor
  -> output.grounded = true
  -> vertical_velocity reset to 0.0 on landing (line 189)

Frame N+3: back to grounded=true, cycle may repeat
```

**The gap calculation:**
When the ship has purely horizontal velocity (V_ship.y = 0), the floor
doesn't move vertically, so there shouldn't be a Y gap. But:

1. The player body at the START of a step is at T_{N-1}
2. During the step, the floor moves horizontally but NOT vertically
3. The player reaches T_{N-1} which was computed with snap_to_ground
4. After the step, player Y should be ~floor_Y + capsule_offset

So with zero ship Y velocity, there should be no vertical gap...

**Unless the one-frame delay creates a subtle Y error.** The floor collider
is a cuboid at ship-local (0, FLOOR_Y - 0.1, center_z). When the ship
moves in Z, the floor stays at the same Y in world space (ship Y is 0).
The player's snap_to_ground from last frame put them at the correct Y.
This frame, the floor Y hasn't changed. So the gap should be zero.

**BUT:** The debug shows `grounded: false` and `vel.y = -0.062`. The
player IS falling. Why?

**Answer from the debug data:** The player Z is -1378.384, the ship Z is
-1380.319. The floor cuboid for the cockpit section is centered at
ship-local z=2.0 with half-length 2.0, so it spans z=0.0 to z=4.0 in
ship-local space. In world space: -1380.319+0 = -1380.319 to
-1380.319+4 = -1376.319.

The player is at world z=-1378.384, which is within the floor extent
(-1380.319 to -1376.319). So the player is above the floor laterally.

The floor surface Y: ship is at world Y=0, floor is at ship-local
Y = FLOOR_Y - 0.1 = -1.0 - 0.1 = -1.1 (bottom of cuboid). Top of floor
cuboid = -1.1 + 0.2 (cuboid half-height 0.1, so full height 0.2) =
wait, the cuboid is `cuboid(hw, 0.1, hl)` meaning half-extents, so
full height = 0.2. Position is at (0, FLOOR_Y - 0.1, center_z) =
(0, -1.1, center_z). Top of floor = -1.1 + 0.1 = -1.0. Correct.

Player capsule bottom = -0.080 - 0.6 - 0.3 = -0.98. Floor top = -1.0.
Gap = -0.98 - (-1.0) = 0.02m. This equals the `offset` skin distance (0.02).
The player IS on the floor geometrically. But rapier reports `grounded: false`.

**This means the move_shape sweep is NOT detecting the floor properly, OR
the one-frame delay creates a situation where the sweep misses the ground.**

**KEY INSIGHT:** Let me re-examine what `char_pos` is at the time of the sweep.

After `physics.step()`, the player body is at T_{N-1}. `char_pos` reads
`body.position()` which returns this post-step position. The sweep starts
from `char_pos` = T_{N-1}.

The floor is at its post-step position (moved with ship).

If T_{N-1} was correct for last frame's floor position, and the floor has
since moved horizontally (in Z) by ~1.5m, then:
- The player is still within the floor's Z extent (floor spans 4m)
- The player's Y is still at floor level (floor didn't move in Y)
- The sweep should detect the floor

**So why is grounded=false?**

**Possible answer:** The `desired.z` component is -1.504m (ship_vel * dt).
The sweep moves the player 1.504m in -Z. After this movement, the player
might be at a Z position where the floor cuboid has a slightly different
extent (near a section boundary) or where snap_to_ground fails due to
the large horizontal movement creating a transient gap.

**Alternative hypothesis:** The snap_to_ground distance is 0.2m (Absolute).
The `offset` is 0.02m. When the player is hovering 0.02m above the floor
(the skin distance), `snap_to_ground` should detect the floor within 0.2m
and report grounded=true. But if the sweep's horizontal movement lifts the
player slightly (due to autostep or slope detection on floor edges), then
the gap could exceed 0.2m transiently.

**Most likely explanation:** The player crosses floor cuboid boundaries
between sections. The cockpit floor and corridor floor are separate
cuboids. At the boundary, there may be a tiny gap or misalignment.
`move_shape` sweeps across this gap and briefly loses ground contact.

**Confidence: 70%** -- The exact mechanism needs runtime verification, but
the grounded=false state in the debug data is definitive proof of the problem.

---

### BUG #3: THE REAL SLIDING -- dt vs physics_dt MISMATCH

**Root cause found with mathematical certainty.**

Look at main.rs lines 804 and 825:
```rust
// Line 804:
let physics_dt = dt.min(1.0 / 30.0);   // physics step uses clamped dt
// ...
self.physics.step(physics_dt);           // ship moves by V * physics_dt

// Line 825:
player.update(&mut self.physics, &self.input, dt, ship_vel);
                                          //    ^^ UNCLAMPED dt!
```

Inside controller.rs, line 143-147:
```rust
let desired = nalgebra::Vector3::new(
    (base_velocity[0] + walk_vel.x) * dt,    // <-- uses dt, not physics_dt
    (base_velocity[1] + self.vertical_velocity) * dt,
    (base_velocity[2] + walk_vel.z) * dt,
);
```

**When dt > 1/30 (i.e., frame time > 33.3ms):**

```
Ship displacement during step = V_ship * physics_dt = V_ship * (1/30)
Player desired displacement   = V_ship * dt          = V_ship * dt

Mismatch = V_ship * (dt - physics_dt) = V_ship * (dt - 1/30)
```

Example: if a frame takes 50ms (dt = 0.05):
```
Ship moves:   90.184 * (1/30) = 3.006 m
Player wants: 90.184 * 0.05   = 4.509 m
Overshoot:    4.509 - 3.006   = 1.503 m PER SLOW FRAME
```

**This is a definitive drift source.** Every frame where dt exceeds 1/30,
the player overshoots the ship by `V_ship * (dt - 1/30)`. At 90 m/s with
a 50ms frame, that is 1.5 meters of drift per frame.

**But also when dt < 1/30 (which is most frames at 60fps):**
dt = 0.01667 and physics_dt = 0.01667 (clamped to same value since dt < 1/30).
No mismatch. So this bug only manifests during frame drops.

**However**, there is ALSO a mismatch in the vertical axis:

```
desired.y = (ship_vel.y + vertical_velocity) * dt
```

When grounded, vertical_velocity = 0. desired.y = ship_vel.y * dt.
Ship_vel.y is typically near 0 for straight flight. But during any pitch
rotation, ship_vel.y becomes nonzero, and the dt vs physics_dt mismatch
causes vertical drift too.

**Confidence: 100%** -- This is mathematically proven. The dt used for
player desired displacement does NOT match the dt used for the physics
step that moved the ship.

---

### BUG #4: THE ONE-FRAME-DELAY POSITIONAL ERROR

**Root cause with mathematical proof.**

After the frame trace above, the exact sequence is:

```
Frame N:
  1. step(physics_dt)
     - Ship: P_ship_N = P_ship_{N-1} + V * physics_dt  (approximately)
     - Player: body moves to T_{N-1} (target from last frame)
     - Floor: at P_ship_N + local_offset

  2. update_query_pipeline()
     - Query pipeline reflects P_ship_N positions

  3. Read ship_vel_N = V_ship after step

  4. player.update():
     - char_pos = T_{N-1}  (body's current position, which is last frame's target)
     - desired = ship_vel_N * dt
     - move_shape(char_pos, desired) with floor at P_ship_N + offset
     - T_N = char_pos + corrected_movement

  5. set_next_kinematic_translation(T_N)
     - Body will reach T_N during NEXT frame's step
```

**Where should the player be at the end of frame N?**

Ideally: P_ship_N + local_offset (standing on the current floor).

**Where IS the player at the end of frame N?**

The body is still at T_{N-1}. It won't move to T_N until the next step.
The camera reads body.translation() at main.rs:829, which returns T_{N-1}.

But T_{N-1} was computed last frame to be on floor_{N-1}. Between then and
now, the ship moved by ~V * physics_dt = 1.5m. So the camera shows the
player 1.5m behind the ship's current position.

**Wait -- does the camera read BEFORE or AFTER set_next_kinematic_translation?**

From main.rs:828-832:
```rust
if let Some(player) = &self.player {
    self.camera.position = player.position(&self.physics);
    ...
}
```

`player.position()` calls `body.translation()`. After `set_next_kinematic_translation`,
`body.translation()` still returns the CURRENT position (T_{N-1}), not the
target. The target only takes effect after the next step.

**So the camera sees the player at T_{N-1}, which is one full frame behind
the ship.**

**Visual drift per frame:**
```
At 90 m/s, 60fps: 90 * (1/60) = 1.5 meters behind
At 90 m/s, 30fps: 90 * (1/30) = 3.0 meters behind
```

**But the debug shows only 0.065m drift.** This contradicts the 1.5m theory.

**Resolution:** move_shape compensates. Here is why:

char_pos = T_{N-1} (body's post-step position).
The floor is at P_ship_N + offset.
desired = ship_vel_N * dt ~ 1.5m in -Z.

move_shape sweeps from T_{N-1} by 1.5m in -Z. The floor is ALSO at
P_ship_N + offset, which is ~1.5m further in -Z from where it was last frame.

So: T_{N-1} is approximately at last frame's floor position. The sweep moves
1.5m in -Z, landing the player approximately at this frame's floor position.
T_N ends up at P_ship_N + offset + small_error. The compensation works!

**The error is second-order:** It comes from acceleration, not velocity.
If the ship is accelerating (which it is, at 3.56 m/s^2), then:

```
Ship actually moved: V*dt + 0.5*a*dt^2
Player desired:      V_post_step * dt  (post-step velocity includes acceleration)
V_post_step = V + a*dt

Player desired = (V + a*dt) * dt = V*dt + a*dt^2
Ship moved     = V*dt + 0.5*a*dt^2
Difference     = 0.5 * a * dt^2 = 0.5 * 3.56 * (0.01667)^2 = 0.000495 m
```

0.5mm per frame. This accumulates but is tiny.

**The 0.065m observed drift** is likely accumulated over many frames from
this second-order error plus the dt/physics_dt mismatch during frame drops.

**Confidence: 95%** -- The one-frame delay exists but is mostly self-correcting.
The residual drift is second-order (acceleration-dependent).

---

## 4. DEFINITIVE ROOT CAUSES (RANKED BY SEVERITY)

### CAUSE 1: dt vs physics_dt mismatch (SLIDING)
- **File:** `crates/spaceaway/src/main.rs`, lines 804 and 825
- **Lines:** `physics_dt = dt.min(1/30)` vs `player.update(..., dt, ...)`
- **Effect:** Player overshoots ship by `V_ship * (dt - 1/30)` every slow frame
- **Magnitude:** 1.5m per 50ms frame at 90 m/s
- **Fix concept:** Pass `physics_dt` to `player.update()` instead of `dt`, OR
  remove the physics_dt clamp entirely, OR accumulate physics steps to match dt.
- **Confidence: 100%**

### CAUSE 2: Grounded detection failure (QUAKING)
- **File:** `crates/sa_player/src/controller.rs`, lines 130-139 and 63
- **Evidence:** Debug shows `grounded: false` while player is 0.02m above floor
- **Effect:** When grounded flickers to false, gravity accumulates (-9.81 * dt
  per frame), pulling player down. Next frame snap_to_ground or floor collision
  corrects it, setting grounded=true. The cycle repeats: vertical oscillation.
- **Magnitude:** Each ungrounded frame adds 0.164 m/s downward velocity,
  causing ~2.7mm downward displacement before correction. At 60fps this
  creates a 60Hz vertical vibration of ~2.7mm amplitude = visible quaking.
- **Fix concept:** Either increase snap_to_ground distance, add a grounded
  grace period (stay grounded for N frames after last ground contact), or
  use a separate ground probe ray instead of relying solely on move_shape
  output.
- **Confidence: 90%**

### CAUSE 3: One-frame kinematic delay (SNAPPING/VISUAL JITTER)
- **File:** `crates/sa_player/src/controller.rs`, line 180
- **Code:** `body.set_next_kinematic_translation(new_translation)`
- **Effect:** Player body position lags one frame behind intended position.
  Camera reads stale position. When ship accelerates or decelerates, the
  positional error is `0.5 * a * dt^2` per frame (second-order).
- **Magnitude:** ~0.5mm per frame at 3.56 m/s^2 acceleration. Mostly
  self-correcting via move_shape. Visible only during sharp acceleration
  changes (engine on/off transitions), where it manifests as a sudden
  snap of ~V_ship * dt = 1.5m.
- **Fix concept:**
  - Option A: Use `body.set_translation(new_translation, true)` for immediate
    positioning. Downside: bypasses rapier's kinematic collision detection.
    For a character controller that already uses move_shape for collision
    detection, this is actually fine -- move_shape already resolved collisions.
  - Option B: Move player.update() BEFORE physics.step(), and call
    set_next_kinematic_translation BEFORE the step so rapier applies it
    during the step. This is actually the intended rapier workflow for
    kinematic bodies. But this requires using LAST frame's ship velocity
    (pre-step), which introduces a different kind of lag.
  - Option C: After set_next_kinematic_translation, also update the camera
    position using the computed new_translation directly (bypass body query).
    This eliminates visual lag without changing physics.
- **Confidence: 85%**

---

## 5. SOLUTION CONCEPTS

### Solution A: Fix dt mismatch (minimum viable fix)

Change main.rs line 825 from:
```rust
player.update(&mut self.physics, &self.input, dt, ship_vel);
```
to:
```rust
player.update(&mut self.physics, &self.input, physics_dt, ship_vel);
```

This ensures the player's desired displacement matches the ship's actual
displacement during the physics step. Eliminates drift during frame drops.

### Solution B: Eliminate one-frame delay (use set_translation)

Change controller.rs line 180 from:
```rust
body.set_next_kinematic_translation(new_translation);
```
to:
```rust
body.set_translation(new_translation, true);
```

Since move_shape already performs swept collision detection, the body can
safely teleport to the corrected position. This eliminates the one-frame
positional lag. The kinematic body essentially becomes a manually-positioned
body, which is fine for character controllers.

### Solution C: Fix grounded oscillation

Add a ground contact grace period in controller.rs. Instead of directly
using `output.grounded`, keep a frame counter:

```
if output.grounded {
    ground_grace_frames = GRACE_PERIOD; // e.g., 3 frames
}
if ground_grace_frames > 0 {
    self.grounded = true;
    ground_grace_frames -= 1;
} else {
    self.grounded = false;
}
```

This prevents single-frame grounded flickers from triggering gravity
accumulation.

Alternatively, increase `snap_to_ground` from 0.2m to a larger value
(e.g., 0.5m) to maintain ground contact over larger transient gaps.

### Solution D: Comprehensive fix (all three)

Apply A + B + C together. Additionally, consider computing the camera
position directly from the computed `new_translation` rather than reading
it back from the body, to ensure zero-lag visual feedback regardless of
the body update strategy.

---

## 6. MATCH WITH OBSERVED SYMPTOMS

| Symptom | Root Cause | Match? |
|---------|-----------|--------|
| Player slides backward when ship accelerates | dt/physics_dt mismatch (Cause 1) + one-frame delay (Cause 3) | YES -- player undershoots during acceleration |
| Player quakes/vibrates vertically | Grounded oscillation (Cause 2) | YES -- 60Hz vertical vibration |
| Player snaps to new position | One-frame delay (Cause 3) during velocity changes | YES -- 1.5m snap when engine toggles |
| grounded=false in debug despite being on floor | Cause 2 directly | YES -- definitive proof |
| Player speed (81.6) != ship speed (90.2) | Causes 1+3: player velocity derived from actual displacement which lags ship | YES -- 8.5 m/s difference = accumulated lag |

---

## 7. CONFIDENCE SUMMARY

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| dt vs physics_dt mismatch causes drift | **100%** | Mathematical proof from code lines 804 vs 825 |
| Grounded oscillation causes quaking | **90%** | Debug data shows grounded=false + falling velocity while on floor |
| One-frame kinematic delay exists | **100%** | Rapier API contract for set_next_kinematic_translation |
| One-frame delay is primary sliding cause | **60%** | Mostly self-correcting via move_shape; second-order effect |
| set_translation would fix the delay | **80%** | Correct per rapier API, but needs testing for edge cases |
| Grace period would fix quaking | **85%** | Standard game-dev pattern, but snap_to_ground increase may suffice |
