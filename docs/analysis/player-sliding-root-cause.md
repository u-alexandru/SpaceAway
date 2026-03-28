# Root Cause Analysis: Player Slides Backward Inside Moving Ship

**Date:** 2026-03-27
**Status:** Analysis complete, no fix applied
**Severity:** Gameplay-breaking when ship is under thrust

---

## 1. Symptom

The player walks inside a ship that is moving in world space. When the player
releases all WASD keys, they slide toward the aft of the ship (backward
relative to the ship) instead of stopping in place. The slide is fast and
covers several meters before friction arrests it.

---

## 2. Root Cause: World-Frame Velocity Assignment

The bug is a **reference frame error** in `PlayerController::update()`.

### Code trace (controller.rs lines 88-94)

```rust
if move_dir.length_squared() > 0.0 {
    let target_vel = move_dir * MOVE_SPEED;          // WORLD-frame velocity
    let new_vel =
        nalgebra::Vector3::new(target_vel.x, current_vel.y, target_vel.z);
    body.set_linvel(new_vel, true);                   // set WORLD velocity
}
```

`move_dir` is a unit vector computed from the player's yaw in **world
coordinates**. `MOVE_SPEED` is 5.0 m/s. The resulting `target_vel` is
therefore a velocity in the **world frame**, not the ship frame.

### What happens while walking

Suppose the ship travels at V_ship = (0, 0, -20) m/s (forward thrust, -Z).

| Event | Player world velocity | Ship world velocity | Relative to ship |
|---|---|---|---|
| Standing still, matched | (0, y, -20) | (0, 0, -20) | 0 |
| Press W (walk forward) | set to **(0, y, -5)** | (0, 0, -20) | **+15 m/s aft** |
| Friction fights to close gap | accelerating toward -20 | (0, 0, -20) | slowly shrinking |

The code **overwrites** the player's world velocity to 5 m/s in the walk
direction every frame. It does not add the ship's velocity. The player is
therefore moving 15 m/s slower than the ship floor in world space, and
friction must do all the work to re-match them.

### What happens when the player stops walking

```rust
// When no keys pressed: don't touch velocity -- friction keeps
// the player gripped to the ship floor.
```

The current "fix" (lines 96-97) avoids zeroing velocity when idle. This is
better than the previous version (which called `set_linvel(0, y, 0)` on
release), but it only masks the deeper problem:

- While walking, the player was being held at ~5 m/s world (not -20).
- The moment WASD is released, the player retains whatever world velocity it
  had on the last walking frame: approximately -5 m/s on the Z axis.
- The ship floor is still at -20 m/s.
- Delta = 15 m/s. The player slides aft at 15 m/s relative to the deck.

The "don't set_linvel when idle" change **does not help** in any meaningful
way. The damage was done during walking: every walking frame forced the
player to a wrong world velocity. Releasing the keys just reveals the
accumulated error.

---

## 3. Numerical Proof

### Given values (from the codebase)

| Parameter | Value | Source |
|---|---|---|
| MOVE_SPEED | 5.0 m/s | controller.rs line 10 |
| Player mass | 80.0 kg | controller.rs line 39 |
| Player collider friction | 1.0 | controller.rs line 36 |
| Floor collider friction | 0.8 | ship_colliders.rs line 270 |
| Gravity | (0, -9.81, 0) | main.rs line 216, PhysicsWorld::new() |
| Mag-boot counterforce on ship | 785 N upward | main.rs line 794 |
| Ship mass | 50,000 kg | ship.rs line 26 |

### Effective friction coefficient

Rapier3d computes the combined friction coefficient between two colliders
using the **geometric mean** by default:

    mu_eff = sqrt(mu_player * mu_floor) = sqrt(1.0 * 0.8) = 0.894

### Normal force

The player is subject to world gravity: F_gravity = 80 * 9.81 = 784.8 N
downward. The floor is a child collider of the ship body, so the contact
produces a normal force N = 784.8 N (approximately; the 785 N counterforce
on the ship keeps the ship from being pushed down).

### Maximum friction force

    F_friction = mu_eff * N = 0.894 * 784.8 = 701.6 N

### Friction acceleration on player

    a_friction = F_friction / m_player = 701.6 / 80 = 8.77 m/s^2

### Velocity gap to close

When the player releases WASD while the ship moves at 20 m/s:

    delta_v = V_ship - V_player = 20 - 5 = 15 m/s

### Time to match ship velocity via friction alone

    t = delta_v / a_friction = 15 / 8.77 = 1.71 seconds

### Sliding distance (relative to ship floor)

    d = delta_v * t / 2 = 15 * 1.71 / 2 = 12.8 meters

The ship interior is only 29 meters long. A 12.8-meter slide will send the
player through multiple bulkheads (if they clip) or slam them into one.

### At lower ship speeds

| Ship speed (m/s) | Velocity gap | Slide time (s) | Slide distance (m) |
|---|---|---|---|
| 5 | 0 | 0 | 0 |
| 10 | 5 | 0.57 | 1.4 |
| 20 | 15 | 1.71 | 12.8 |
| 50 | 45 | 5.13 | 115.4 |
| 100 | 95 | 10.83 | 514.6 |

The bug is proportional to ship speed. At 5 m/s (matching MOVE_SPEED),
there is no visible slide. At any speed above that, sliding begins.

---

## 4. Friction Coefficient Audit

| Surface | Friction | File | Line |
|---|---|---|---|
| Player capsule | 1.0 | controller.rs | 36 |
| Floor cuboid | 0.8 | ship_colliders.rs | 270 |
| Hex face walls | 0.5 | ship_colliders.rs | 216 |
| Ceiling cuboid | 0.3 | ship_colliders.rs | 286 |
| Bulkhead panels | 0.5 | ship_colliders.rs | 318, 340, 358 |
| Endcap walls | 0.5 | ship_colliders.rs | 384 |

The floor friction of 0.8 combined with the player's 1.0 yields an
effective 0.894. This is reasonable for "grippy boots on metal deck" and
would be perfectly adequate **if the velocity were set correctly**. The
friction values are not the problem.

---

## 5. Secondary Bug: Helm Exit Velocity Reset

In `main.rs` line 756, when the player stands up from the helm:

```rust
body.set_linvel(nalgebra::Vector3::zeros(), true);
```

This sets the player's world velocity to zero. If the ship is moving at
(0, 0, -20), the player instantly has a 20 m/s velocity difference from the
ship floor. They will slide the full length of the ship and hit the aft
wall. This is the same class of bug (world-frame velocity instead of
ship-frame).

---

## 6. Does the "Don't Set Linvel When Idle" Fix Help?

**No, it makes the problem slightly less catastrophic but does not solve it.**

Previously the code set `linvel(0, y, 0)` when idle, which meant:
- Walking at ship speed 20: player at -5 world
- Release keys: player snapped to 0 world
- Gap: 20 m/s (worse than the walking gap of 15)

Now with the idle-passthrough:
- Walking at ship speed 20: player at -5 world
- Release keys: player stays at -5 world
- Gap: still 15 m/s

The improvement is from 20 m/s gap to 15 m/s gap. Still catastrophic.

---

## 7. Solution Concept

### Core fix: Ship-relative velocity

The player's target velocity must be computed in the **ship's reference
frame**, then transformed to world coordinates:

```rust
// Pseudocode
let ship_vel = ship_body.linvel();
let ship_rot = ship_body.rotation();

if move_dir.length_squared() > 0.0 {
    // Walk direction in ship-local frame (ship_rot * local_dir)
    let walk_world = ship_rot * (move_dir_local * MOVE_SPEED);
    // Target = ship velocity + walk offset
    let target = ship_vel + walk_world;
    body.set_linvel(Vector3::new(target.x, current_vel.y, target.z), true);
} else {
    // Idle: match ship horizontal velocity exactly
    body.set_linvel(Vector3::new(ship_vel.x, current_vel.y, ship_vel.z), true);
}
```

When walking: `V_player = V_ship + V_walk` (correct world velocity).
When stopped: `V_player = V_ship` (zero relative motion, no sliding).

### Why explicit velocity matching beats friction

Relying on friction to keep the player matched to the ship is fragile:
- It takes time (1.7s at 20 m/s ship speed).
- It fails entirely above ~1g of ship acceleration.
- It produces visible jitter as friction fights the velocity override.

Explicitly setting the player's velocity to `V_ship + V_walk` is
instantaneous, exact, and works at any ship speed.

### The idle case is critical

The current code's comment says "let friction handle it." This is wrong.
When idle, the player must be **actively matched** to ship velocity, not
left to drift. The friction-only approach fails because the velocity was
wrong during walking, and friction cannot correct a 15 m/s gap fast enough.

---

## 8. Rotation and Acceleration Considerations

### Ship rotation

When the ship rotates, the player's "forward" direction (from yaw) is in
world space. On a rotating ship, the walk direction should be relative to
the ship's orientation:

```
walk_world = ship_rotation * local_walk_direction
```

Currently `forward_dir` is computed from `self.yaw` in world space
(controller.rs line 62). For a slowly rotating ship this is acceptable, but
for aggressive maneuvering the player's walk direction will diverge from the
ship's frame. This should be addressed in a future pass.

### Ship acceleration (non-inertial frame effects)

When the ship accelerates, the player experiences a pseudo-force pushing
them aft (like being pushed back in an accelerating car). With the
ship-relative velocity fix:

- **Constant velocity:** Player matches perfectly, no sliding.
- **Acceleration < 1g:** Friction (8.77 m/s^2) can keep the player matched.
  Small lag, quickly corrected.
- **Acceleration > 1g:** Player slides aft. This is physically correct
  behavior ("mag-boots can't grip hard enough"). No fix needed; this is a
  feature.

The counterforce of 785 N on the ship (main.rs line 794) handles the
**static** weight of the player but does NOT handle the dynamic reaction to
ship acceleration. This is a separate, smaller issue.

---

## 9. Summary of Findings

| # | Finding | Severity | Location |
|---|---|---|---|
| 1 | `set_linvel` uses world-frame velocity, not ship-relative | **Critical** | controller.rs:91-94 |
| 2 | Idle path relies on friction instead of velocity matching | **Critical** | controller.rs:96-97 |
| 3 | Helm exit sets linvel to world zero, not ship velocity | **High** | main.rs:756 |
| 4 | Walk direction uses world yaw, not ship-relative yaw | Low | controller.rs:62 |
| 5 | Friction coefficients are reasonable (0.894 effective) | OK | controller.rs:36, ship_colliders.rs:270 |
| 6 | Counterforce handles static weight only, not acceleration | Low | main.rs:794 |

---

## 10. Standards and Documentation to Prevent This Class of Bug

### Proposed standard: "Ship-Frame Velocity Rule"

> Any system that sets or modifies the player's linear velocity while the
> player is inside a ship MUST add the ship's current world velocity as a
> base. World-frame velocity values must NEVER be assigned directly to an
> entity that is semantically "riding" another moving body.

### Checklist for velocity-setting code

1. Is the entity inside/on a moving parent body?
2. If yes, have you added the parent's world velocity?
3. If the entity stops moving, does it match the parent's velocity (not zero)?
4. If the entity is teleported, is its velocity set to the parent's velocity?

### Where to document

- Add a `docs/physics-standards.md` covering reference frame rules.
- Add a comment block at the top of `controller.rs` stating the invariant.
- Add a unit test: spawn player on moving ship floor, release keys, assert
  player velocity matches ship velocity within 0.1 m/s after 2 physics
  frames.
