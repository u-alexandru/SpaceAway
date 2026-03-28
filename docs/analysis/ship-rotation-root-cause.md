# Ship Rotation Root Cause Analysis

## Executive Summary

There are **two independent root causes** creating unwanted ship rotation and drift when the player walks inside. Both stem from the same architectural decision: the player rigid body is independent from the ship rigid body, and their interaction is mediated entirely through Rapier3d contact forces.

**Root Cause 1 (PRIMARY):** `set_linvel` sets world-space velocity, not ship-relative velocity. When the ship is moving, every frame the player's velocity is overwritten to a value that diverges from the ship's velocity, generating massive corrective contact forces.

**Root Cause 2 (SECONDARY):** The 785N gravity counterforce is applied at the ship's center of mass, but the player's weight acts at an off-center contact point, producing a net torque.

---

## System Architecture

```
Physics World (gravity = 0, -9.81, 0)

Ship rigid body (dynamic)
  - gravity_scale: 0.0  (immune to gravity)
  - mass: 50,000 kg (from hull sensor collider)
  - linear_damping: 0.0
  - angular_damping: 0.5
  - Hull sensor collider (cuboid 5x3x30, mass provider only, no contacts)
  - Interior colliders (floors, walls, ceilings, bulkheads)
    - Children of ship body -> move with ship
    - Collision group: SHIP_INTERIOR, filter: PLAYER
    - Floor friction: 0.8, wall friction: 0.5

Player rigid body (dynamic, SEPARATE body)
  - gravity_scale: 1.0 (default -- affected by world gravity)
  - mass: 80 kg
  - linear_damping: 0.0
  - lock_rotations: true
  - Capsule collider, friction: 1.0
  - Collision group: PLAYER (implicit default -- collides with everything)
```

Key: The player is NOT a child of the ship body. The player is a fully independent dynamic rigid body that happens to be in contact with colliders that ARE children of the ship body.

---

## Root Cause 1: set_linvel Uses World-Space Velocity

### The Code Path (frame by frame)

**controller.rs lines 88-94:**
```rust
if move_dir.length_squared() > 0.0 {
    let target_vel = move_dir * MOVE_SPEED;  // MOVE_SPEED = 5.0 m/s
    let new_vel = nalgebra::Vector3::new(target_vel.x, current_vel.y, target_vel.z);
    body.set_linvel(new_vel, true);
}
```

`move_dir` is computed from `self.yaw` (lines 62-63), which is in world space. `target_vel` is therefore in world space. The call `set_linvel(new_vel, true)` sets the player's **absolute world velocity**.

### Mathematical Proof

**Setup:** Ship traveling at velocity V_ship = (0, 0, -20) m/s (forward at 20 m/s). Player presses W to walk forward (same direction as ship nose = -Z). Player yaw = 0, so forward_dir = (0, 0, -1).

**Frame N, before player.update():**
- Player velocity (inherited from friction last frame): approximately (0, y, -20)
- Ship velocity: (0, 0, -20)
- Relative velocity between player and ship floor: approximately 0

**Frame N, during player.update():**
```
move_dir = (0, 0, -1)
target_vel = (0, 0, -5)          // 5 m/s in world -Z
new_vel = (0, current_y, -5)     // SET via set_linvel
```

**Frame N, after set_linvel:**
- Player velocity: (0, y, -5)
- Ship velocity: (0, 0, -20)
- **Relative velocity of player vs ship floor: (0, y, +15)**

The player is now moving at 15 m/s in the +Z direction relative to the ship floor. This is as if the player suddenly started sprinting backward through the ship at 15 m/s.

### Contact Force Calculation

Rapier's velocity solver must correct this 15 m/s relative velocity within a single timestep (dt = 1/60 s). The contact force required:

```
Delta_v = 15 m/s (relative velocity to correct via friction)
dt = 1/60 s

Required impulse on player: J = m_player * Delta_v = 80 * 15 = 1200 Ns
Required force (if corrected in one step): F = J / dt = 1200 / (1/60) = 72,000 N
```

By Newton's third law, this 72,000 N friction force acts equally and oppositely on the ship (via the floor collider which is a child of the ship body).

For the ship:
```
a_ship = F / m_ship = 72,000 / 50,000 = 1.44 m/s^2
```

This is a significant spurious acceleration, but the real damage is the **torque**.

### Torque from Off-Center Contact

The ship hull sensor is a cuboid centered at the ship body origin: half-extents (2.5, 1.5, 15.0), so the center of mass is at local position (0, 0, 0). The ship sections span Z = 0 to Z = 29. This means the center of mass is at Z = 0 in local coordinates, while the ship interior spans from Z = 0 to Z = 29.

The player spawns at (0, 0, 2.5) in world space, which is at Z = 2.5 in ship-local space. The center of mass of the hull cuboid is at Z = 0 (since it spans from -15 to +15 in local Z). But wait -- the collider is placed at the ship body's origin, and the ship body is at (0, 0, 0). The hull cuboid has half-extent Z = 15, so it spans Z = -15 to Z = +15. But the interior colliders are placed from Z = 0 to Z = 29 in local coordinates.

**This means the center of mass (at the hull cuboid center Z=0) does NOT align with the interior geometry center (at Z=14.5).** The player walking at Z = 2.5 is at an offset of r_z = 2.5 from the CoM. But if they walk to, say, the engineering room (Z ~ 22), the offset is r_z = 22.

The friction force acts at the contact point, not at the CoM. The torque about the Y-axis (yaw):

```
For a player at Z = 14.5 (middle of the ship interior):
  r = 14.5 m (offset from CoM along Z)
  F_x = friction force in X (from set_linvel correction)
  tau_y = r * F_x

If the player walks sideways (pressing A/D) at ship speed 20 m/s:
  F_x = 72,000 N (as calculated above, correcting the full 20 m/s discrepancy in X)
  tau_y = 14.5 * 72,000 = 1,044,000 Nm
```

For the ship moment of inertia about Y (yaw axis), treating it as a uniform box (5 x 3 x 30 m, 50,000 kg):
```
I_y = (1/12) * M * (L_x^2 + L_z^2)
    = (1/12) * 50,000 * (5^2 + 30^2)
    = (1/12) * 50,000 * (25 + 900)
    = (1/12) * 50,000 * 925
    = 3,854,167 kg*m^2
```

Angular acceleration:
```
alpha = tau / I = 1,044,000 / 3,854,167 = 0.271 rad/s^2
```

After just 1 second: omega = 0.271 rad/s = 15.5 deg/s. **This is extremely visible rotation.**

### Why It Gets Worse at Higher Ship Speeds

The discrepancy between `set_linvel` target and actual ship velocity scales linearly with ship speed:

| Ship Speed | Velocity Error | Contact Force | Torque (at r=14.5m) | Angular Accel |
|-----------|---------------|--------------|---------------------|---------------|
| 0 m/s     | 0 m/s         | 0 N          | 0 Nm                | 0 rad/s^2     |
| 5 m/s     | 5 m/s         | 24,000 N     | 348,000 Nm          | 0.090 rad/s^2 |
| 20 m/s    | 20 m/s        | 72,000 N (*)  | 1,044,000 Nm        | 0.271 rad/s^2 |
| 100 m/s   | 100 m/s       | 360,000 N (*) | 5,220,000 Nm       | 1.354 rad/s^2 |

(*) Actual forces may be capped by friction cone: F_friction <= mu * F_normal. But the normal force is also enormous because the velocity solver must prevent interpenetration, and Rapier uses impulse-based resolution that can produce very high forces in a single frame.

### What About When the Ship Is Stationary?

Even at ship velocity = 0, walking creates torque. The player sets their velocity to (5, y, 0) via set_linvel, then friction between the player capsule (friction=1.0) and the floor (friction=0.8) transmits a reaction force. Combined friction coefficient in Rapier = sqrt(1.0 * 0.8) = 0.894 (geometric mean by default).

The force is much smaller here because Rapier only needs to accelerate the player from 0 to 5 m/s (which set_linvel does instantaneously), so the contact force is just the friction needed to maintain traction. However, the velocity override each frame prevents the natural friction coupling, so there IS still a parasitic force.

---

## Root Cause 2: Gravity Counterforce Applied at Wrong Point

### The Code Path

**main.rs lines 789-795:**
```rust
ship.reset_forces(&mut self.physics);
ship.apply_thrust(&mut self.physics);
// Counteract player weight on ship
if let Some(body) = self.physics.get_body_mut(ship.body_handle) {
    body.add_force(nalgebra::Vector3::new(0.0, 785.0, 0.0), true);
}
```

`body.add_force()` in Rapier applies the force at the body's center of mass. It produces zero torque by definition (force through CoM).

But the player's weight (80 kg * 9.81 = 784.8 N downward) acts on the ship via a **contact point** on the floor, which is wherever the player is standing. This contact force produces torque because it is off-center.

### Free Body Diagram of the Ship

Forces on the ship from the player interaction:

1. **Contact normal force (floor pushing up on player, player pushing down on floor):**
   - Location: player's contact point on the floor, position P = (p_x, floor_y, p_z) in ship-local coords
   - Magnitude: ~785 N downward on ship (Newton's 3rd law reaction to floor supporting player)
   - This force creates torque: tau = r x F where r = P - CoM

2. **Counterforce (code-applied):**
   - Location: center of mass (by definition of add_force)
   - Magnitude: 785 N upward
   - Torque: zero (force through CoM)

3. **Net effect:**
   ```
   F_net = 785 up + 785 down = 0 N  (translational: correct!)
   tau_net = r x (-785 j) + 0        (rotational: NOT zero!)
   ```

### Torque Calculation

Player at position (p_x, floor_y, p_z), ship CoM at origin (0, 0, 0):

```
r = (p_x, floor_y, p_z)
F_contact = (0, -785, 0)  [player weight pushing ship floor down]

tau = r x F = (floor_y * 0 - p_z * (-785),
               p_z * 0 - p_x * 0,
               p_x * (-785) - floor_y * 0)
    = (785 * p_z, 0, -785 * p_x)
```

For the floor at y = -1.0, player at (0, -1.0, 14.5) (center of ship interior):

```
tau_x (pitch) = 785 * 14.5 = 11,382 Nm
tau_z (roll)  = -785 * 0   = 0 Nm  (player centered in X)
```

If the player walks to X = 1.0 (off-center):
```
tau_z (roll)  = -785 * 1.0 = -785 Nm
```

Ship pitch moment of inertia:
```
I_x = (1/12) * 50,000 * (3^2 + 30^2) = (1/12) * 50,000 * 909 = 3,787,500 kg*m^2
```

Angular acceleration from gravity torque alone:
```
alpha_pitch = 11,382 / 3,787,500 = 0.003 rad/s^2
```

After 60 seconds: omega = 0.18 rad/s = 10.3 deg/s. This is slow-building but **clearly noticeable over time**, especially because it is constant (gravity never stops).

---

## Root Cause Interaction: Combined Effect

Both root causes compound:

1. **Root Cause 1** produces enormous, velocity-dependent forces every frame the player walks while the ship is moving. These forces spike instantly and create large torques.

2. **Root Cause 2** produces a constant, moderate torque whenever the player is standing off-center from the ship's CoM. This accumulates over time even when the ship is stationary and the player is idle.

Together, they explain the observed behavior: the ship drifts and rotates gradually when the player stands still (RC2), and rotates violently when the player walks while the ship is in motion (RC1).

---

## Angular Damping Analysis

The ship has `angular_damping(0.5)`. In Rapier, angular damping applies a multiplicative velocity decay each step:

```
omega_new = omega_old * (1 - damping * dt)
         = omega_old * (1 - 0.5 * (1/60))
         = omega_old * 0.99167
```

This is a 0.83% reduction per frame. At equilibrium (where damping torque equals driving torque):

```
For Root Cause 1 at 20 m/s ship speed:
  Driving torque = 1,044,000 Nm (intermittent, while walking)
  alpha = 0.271 rad/s^2

  Damping brings omega from 0.271 to 0.271 * 0.99167 = 0.2687 after one frame
  Equilibrium omega where alpha * dt = omega * damping * dt:
    0.271 = omega * 0.5
    omega_eq = 0.542 rad/s = 31 deg/s

For Root Cause 2:
  Driving torque = 11,382 Nm (constant)
  alpha = 0.003 rad/s^2
  omega_eq = 0.003 / 0.5 = 0.006 rad/s = 0.34 deg/s
```

The damping is far too weak to counteract Root Cause 1. It provides marginal help against Root Cause 2 but the equilibrium rotation rate is still visible.

---

## Additional Concern: CoM vs Interior Geometry Mismatch

The hull sensor cuboid has half-extents (2.5, 1.5, 15.0), placing the CoM at the cuboid center. But the interior colliders span Z = 0 to Z = 29 in ship-local space. Unless the ship body is positioned such that the hull cuboid center aligns with the interior midpoint, the CoM is offset from the walkable area.

If the ship body origin is at world (0, 0, 0) and the hull cuboid has no translation offset, the CoM is at (0, 0, 0) in ship-local space, while the interior midpoint is at Z = 14.5. Every position the player can occupy is 0 to 29 meters from the CoM in Z, guaranteeing large torque arms.

---

## Confirmation/Denial of Initial Hypotheses

### Hypothesis 1: "set_linvel sets WORLD velocity, not ship-relative velocity"
**CONFIRMED.** The code at controller.rs:91-94 computes `target_vel = move_dir * MOVE_SPEED` where `move_dir` is derived from `self.yaw` (a world-space angle). No ship velocity is added. The resulting set_linvel call puts the player at an absolute world velocity of at most 5 m/s, regardless of ship velocity.

### Hypothesis 2: "Torque = r x F where r is offset distance, F = 785N"
**PARTIALLY CONFIRMED, BUT UNDERSTATED.** The 785N figure is the gravitational component only (Root Cause 2). The dominant force is from Root Cause 1: the velocity correction force, which can reach tens of thousands of Newtons at typical ship speeds.

### Hypothesis 3: "785N counterforce applied at CoM, weight acts at contact point"
**CONFIRMED.** `body.add_force()` at main.rs:794 applies force at the CoM. The player's weight transfers through the contact point. The torque from this mismatch is real, calculated above.

### Hypothesis 4: "angular_damping(0.5) is too weak"
**CONFIRMED.** Equilibrium angular velocity for RC1 is 0.54 rad/s (31 deg/s). Damping value of 0.5 is negligibly small against the driving torques.

### Hypothesis 5: "Fundamental issue is world-space vs ship-relative velocity"
**CONFIRMED as the PRIMARY root cause.** This is quantitatively the dominant effect by 2-3 orders of magnitude over the gravity counterforce issue when the ship is in motion.

---

## Solution Concept (No Code)

The root cause is that the player body's velocity is set in world space, ignoring the ship's velocity. The fix must ensure that when the player walks, their velocity is **ship-relative**.

**Approach: Add ship velocity to walk velocity.**

When computing the player's target velocity in `controller.rs`, the walk speed should be added to the ship's current velocity, not set as an absolute world value:

```
target_world_vel = ship_velocity + (move_dir * MOVE_SPEED)
```

This way, when the ship moves at (0, 0, -20) and the player walks at (5, 0, 0) relative to the ship, the world velocity becomes (5, y, -20) -- matching the ship's motion plus the walk offset. No velocity discrepancy means no corrective contact force, no parasitic torque.

For Root Cause 2, the gravity counterforce must be applied **at the player's contact point** rather than at the ship's center of mass. Rapier provides `add_force_at_point()` which applies a force at an arbitrary world-space point, automatically computing the correct torque. The counterforce should be 785N upward, applied at the player's current (x, y, z) position projected onto the ship floor.

**When no keys are pressed:** The existing design (lines 96-97: "don't touch velocity, let friction grip") is correct for the idle case. Friction naturally keeps the player moving with the ship. The only remaining issue when idle is Root Cause 2 (gravity counterforce at wrong point).
