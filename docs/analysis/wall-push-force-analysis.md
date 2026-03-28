# Wall-Push Force Analysis: Player Moving the Ship

## Summary

The player pushing against an interior wall produces a noticeable ship acceleration
because `set_linvel()` is called every frame, forcing Rapier to generate large contact
impulses every frame to resolve the wall penetration. Secondary issues include
unaccounted collider mass, a torque-producing gravity counterforce, and low damping.

---

## 1. Every Force Applied to the Ship Body

| # | Source | Magnitude | Direction | Applied where | Code location |
|---|--------|-----------|-----------|---------------|---------------|
| 1 | **Contact impulse from `set_linvel` wall collision** | ~24,000 N per frame | Along walk direction | Contact point on interior wall | `controller.rs:99` -> Rapier solver |
| 2 | **Gravity counterforce** | 785 N | +Y (up) | Center of mass | `main.rs:803` |
| 3 | **Forward thrust** | 0 to 500,000 N | Ship local -Z | Center of mass | `ship.rs:109` via `main.rs:800` |
| 4 | **Player gravity contact force** | ~785 N | -Y (down) | Floor contact point (offset from CoM) | Rapier solver (player weight on floor) |
| 5 | **Player friction on floor** | Variable | Horizontal | Floor contact point | Rapier solver (friction from `set_linvel`) |
| 6 | **Linear damping** | 0.01 * mass * velocity | Opposing velocity | Implicit (Rapier integrator) | `ship.rs:39` |

Forces 1, 4, and 5 are **implicit** -- they arise from Rapier's constraint solver
reacting to the player body. They are not called explicitly but are real forces on the
ship rigid body.

---

## 2. Issue #1 (CRITICAL): set_linvel Creates 24 kN Contact Impulse Every Frame

### The mechanism

`controller.rs:89-99` -- Every frame, regardless of wall contact:

```rust
let walk_vel = move_dir * MOVE_SPEED;  // MOVE_SPEED = 5.0 m/s
let new_vel = nalgebra::Vector3::new(
    base_velocity[0] + walk_vel.x,
    current_vel.y,
    base_velocity[2] + walk_vel.z,
);
body.set_linvel(new_vel, true);
```

### The math

When the player walks into a wall:

- **Frame N**: Player velocity set to `ship_vel + (5, 0, 0)` (walking into +X wall)
- **Rapier solve**: Wall contact stops the player. Player velocity becomes ~`ship_vel + (0, 0, 0)`
- **Frame N+1**: `set_linvel` sets velocity back to `ship_vel + (5, 0, 0)` AGAIN
- **Rapier must resolve** the wall contact AGAIN, generating a contact impulse

The contact impulse per frame:

```
impulse = m * delta_v = 80 kg * 5.0 m/s = 400 N*s (per physics step)
```

At 60 Hz (dt = 1/60 s), the equivalent force:

```
F = impulse / dt = 400 / (1/60) = 24,000 N = 24 kN
```

### Effect on the ship

The ship's effective mass is at least 50,000 kg (see Issue #3 for why it may be more):

```
a = F / m = 24,000 / 50,000 = 0.48 m/s^2
```

This is **very noticeable**. At 0.48 m/s^2 the ship gains ~0.5 m/s every second the
player leans into a wall. Over 10 seconds that is 5 m/s of unwanted drift.

### Why this is wrong

Newton's third law is being violated by the game code. The player's `set_linvel` is an
**external velocity override** -- it injects momentum from outside the physics system.
Rapier sees the player suddenly having 5 m/s toward the wall and must stop it via a
contact impulse. That impulse pushes the wall (and therefore the ship) with equal and
opposite force. But the player never actually "pushed off" anything to gain that 5 m/s
-- it was set directly. So the ship receives a reaction force with no corresponding
action.

Every single frame the player holds a movement key into a wall, this 24 kN impulse is
applied to the ship.

### Proof from code

- `controller.rs:10`: `MOVE_SPEED = 5.0`
- `controller.rs:39`: `mass(80.0)` on the player collider
- `controller.rs:91`: `walk_vel = move_dir * MOVE_SPEED`
- `controller.rs:94-98`: velocity = base + walk, unconditionally
- `controller.rs:99`: `body.set_linvel(new_vel, true)` -- overrides every frame
- Interior colliders are children of ship body (`ship_colliders.rs:169`), so contact
  forces from the player transfer directly to the ship rigid body

---

## 3. Issue #2 (MODERATE): Gravity Counterforce Applied at Center of Mass Creates No Torque Cancellation

### The mechanism

`main.rs:803`:
```rust
body.add_force(nalgebra::Vector3::new(0.0, 785.0, 0.0), true);
```

This applies 785 N upward at the ship's **center of mass**.

The player's weight (785 N downward) is transmitted via the floor contact point, which
is at the player's position. The ship is 29 m long (Z = 0 to 29). If the center of mass
is near Z = 14.5 and the player stands at Z = 2.5 (cockpit spawn point), the offset is
~12 m.

### The torque

```
torque = F * r = 785 N * 12 m = 9,420 N*m
```

The counterforce at CoM produces zero torque. The contact force at Z=2.5 produces
9,420 N*m. The **net torque is 9,420 N*m**, which will rotate the ship.

However, `angular_damping = 5.0` (`ship.rs:40`) is quite high and should suppress this
rotation in practice. This is a secondary concern compared to Issue #1, but it is
physically incorrect and will produce slight wobble as the player walks fore/aft.

### Note on Y-component

The base_velocity passed to the player strips the Y component:

```rust
// main.rs:791
player.update(..., [ship_vel[0], 0.0, ship_vel[2]]);
```

This is correct -- the player's Y velocity is driven by gravity, not the ship's vertical
motion.

---

## 4. Issue #3 (MODERATE): Interior Colliders Add Uncontrolled Mass

### The mechanism

`ship_colliders.rs` creates ~70+ colliders (5 walls + floor + ceiling per section,
plus bulkheads and endcaps). None of them set `.mass()` or `.density()`:

```rust
// ship_colliders.rs:215-220 (wall collider example)
ColliderBuilder::convex_hull(&points).map(|b| {
    b.friction(0.5)
        .restitution(0.0)
        .collision_groups(interior_groups())
        .build()
})
```

Rapier's default collider density is **1.0 kg/m^3**. Each convex hull collider has
volume proportional to its dimensions. For a wall segment that is roughly 5m long x 3m
tall x 0.15m thick:

```
volume_per_wall ~= 5.0 * 3.0 * 0.15 = 2.25 m^3
mass_per_wall ~= 2.25 kg (at density 1.0)
```

With ~50 wall segments, ~10 floors, ~10 ceilings, ~20 bulkhead pieces:

```
estimated total collider mass ~= 50 * 2.25 + 10 * (2.0 * 0.2 * 1.5) + 10 * (2.0 * 0.2 * 1.5) + 20 * 1.0
                               ~= 112.5 + 6.0 + 6.0 + 20.0
                               ~= 145 kg (rough estimate)
```

This is small relative to 50,000 kg (~0.3%), so the mass contribution is negligible.
However, these colliders **shift the center of mass** away from the geometric center
of the hull sensor. Since all interior colliders are below Y=1.2 (ceiling) and above
Y=-1.0 (floor), they pull the CoM downward and toward whichever sections have more/larger
colliders. This exacerbates the torque issue from Issue #2.

The hull sensor uses `.mass(50_000.0)` (`ship.rs:58`), which sets the collider's mass
directly (overriding density-based calculation). The total body mass = hull sensor mass +
sum of all child collider masses = ~50,145 kg.

---

## 5. Issue #4 (MINOR): Linear Damping Is Negligible Against 24 kN

### The math

`ship.rs:39`: `linear_damping(0.01)`

Rapier applies damping as: `damping_force = -linear_damping * mass * velocity`

At 1 m/s ship velocity:
```
damping_force = 0.01 * 50,000 * 1.0 = 500 N
```

The player's wall-push force is 24,000 N. The damping provides only 2% of the
counteracting force. Damping alone cannot prevent the ship from accelerating.

Even at a more aggressive damping of 1.0:
```
damping_force = 1.0 * 50,000 * 1.0 = 50,000 N
```
This would balance at ~0.48 m/s equilibrium velocity -- still noticeable and physically
wrong (the ship would have extremely sluggish behavior under thrust too).

**Damping is not the solution.** The root cause (Issue #1) must be fixed.

---

## 6. Issue #5 (MINOR): Floor Friction Also Transfers Horizontal Force

### The mechanism

The player collider has `friction(1.0)` (`controller.rs:36`). Floor colliders have
`friction(0.8)` (`ship_colliders.rs:270`). The effective friction coefficient is
`min(1.0, 0.8) = 0.8` (or average, depending on Rapier's combine rule).

When `set_linvel` sets horizontal velocity and Rapier resolves it, the floor contact
also generates a friction impulse. If the player is walking and being stopped by a wall,
the friction force is:

```
F_friction = mu * F_normal = 0.8 * 785 N = 628 N
```

This is much smaller than the 24 kN wall contact force but still non-trivial. It acts
in the horizontal plane on the floor contact point.

---

## 7. Actual Per-Frame Acceleration Summary

Assuming the player walks into a side wall at 60 Hz:

| Force | Magnitude | Ship acceleration |
|-------|-----------|-------------------|
| Wall contact impulse (set_linvel) | 24,000 N | 0.480 m/s^2 |
| Floor friction | ~628 N | 0.013 m/s^2 |
| Gravity torque mismatch | 9,420 N*m torque | Rotation, not translation |
| Linear damping at 1 m/s | -500 N | -0.010 m/s^2 |
| **Net horizontal** | **~24,128 N** | **~0.483 m/s^2** |

The ship accelerates at approximately **0.48 m/s^2** when the player walks into a wall.
After 2 seconds of wall contact, the ship has gained ~1 m/s of velocity. This is
absolutely noticeable and clearly the bug the player is experiencing.

---

## 8. Root Cause

**The root cause is `set_linvel` on line `controller.rs:99`.**

This function bypasses Rapier's impulse-based physics. It directly sets the player's
velocity to a desired value every frame, regardless of contacts. When a wall prevents
the player from achieving that velocity, Rapier generates a massive contact impulse
(24 kN) to enforce the constraint. That impulse transfers to the ship body through the
child collider hierarchy.

The `set_linvel` pattern is fundamentally incompatible with having the player physically
interact with a movable parent body. It works fine for walking on immovable terrain but
creates phantom forces when the terrain can move.

---

## 9. Solution Concepts

### Solution A: Force-based player movement (recommended)

Replace `set_linvel` with `add_force` or `apply_impulse`. Calculate a force that
accelerates the player toward the desired velocity, clamped to a reasonable maximum:

```
desired_vel = base_velocity + walk_vel
current_vel = body.linvel()
delta_vel = desired_vel - current_vel
force = clamp(delta_vel * mass / dt, max_walk_force)
body.add_force(force)
```

Where `max_walk_force` is something physically reasonable like 200-400 N (a person
pushing with their legs). This way, when the player hits a wall, the force is capped
at a realistic value and the ship receives at most 400 N (producing 0.008 m/s^2 --
imperceptible on a 50,000 kg ship).

### Solution B: Velocity clamping with contact awareness

Before calling `set_linvel`, check if the player has active wall contacts in the
movement direction. If so, do not set velocity in that direction. This prevents the
repeated impulse cycle but requires querying Rapier's contact manifolds.

### Solution C: Kinematic player with manual collision

Make the player kinematic and handle collisions manually. This gives full control but
loses Rapier's built-in gravity, stacking, and contact response.

### Solution D: Apply equal-and-opposite force to cancel the leak

After the physics step, measure the impulse that Rapier applied to the ship from player
contacts and subtract it. This is a band-aid -- it hides the symptom without fixing the
energy injection.

### Recommendation

**Solution A** is the cleanest fix. It respects Newton's third law: the player can only
push the ship as hard as a human can push (a few hundred Newtons), and the ship's
response is physically correct. The walk feel can be tuned via the force magnitude and
a velocity-proportional drag term on the player.

For the gravity counterforce (Issue #2), use `body.add_force_at_point()` instead of
`body.add_force()`, applying the 785 N upward force at the player's current floor
contact position. This produces zero net torque.

For the collider mass (Issue #3), add `.density(0.0)` to all interior colliders so they
contribute zero mass and do not shift the center of mass.
