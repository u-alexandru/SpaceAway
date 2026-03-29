# Landing System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the flight-to-ground-to-flight landing cycle so the player can descend, land on terrain, lock the ship, unlock, and take off.

**Architecture:** Landing skid colliders (4 solid spheres on ship bottom) provide physical ground contact via rapier. A 4-point raycast system detects altitude for the landing state machine (FLYING/SLIDING/LANDED). A cockpit lock button freezes/unfreezes the ship. Impact events and audio cues give feedback.

**Tech Stack:** rapier3d (physics), sa_audio (sound), sa_core::EventBus (events), sa_ship (ship/helm/interactables)

**Spec:** `docs/completed/specs/2026-03-29-landing-system-design.md`

---

### Task 1: Increase Ship Thrust

**Files:**
- Modify: `crates/sa_ship/src/ship.rs`
- Test: `crates/sa_ship/src/ship.rs` (inline tests)

- [ ] **Step 1: Update DEFAULT_MAX_THRUST constant**

In `crates/sa_ship/src/ship.rs`, change line 26:

```rust
// Before:
const DEFAULT_MAX_THRUST: f32 = 500_000.0;
// After:
const DEFAULT_MAX_THRUST: f32 = 750_000.0;
```

This gives 15 m/s² max acceleration (50,000 kg ship), enough to hover on < 1.2g planets.

- [ ] **Step 2: Run tests**

```bash
cargo test -p sa_ship
```

Expected: all 48 tests pass. The `thrust_applies_force_when_engine_on` test uses `max_thrust` from the struct which defaults to this constant.

- [ ] **Step 3: Commit**

```bash
git add crates/sa_ship/src/ship.rs
git commit -m "feat(ship): increase max thrust to 750kN for planetary landing"
```

---

### Task 2: Add SHIP_EXTERIOR Collision Group and Landing Skid Colliders

**Files:**
- Modify: `crates/spaceaway/src/ship_colliders.rs` (add group constant)
- Modify: `crates/sa_ship/src/ship.rs` (add skid colliders to ship body)
- Test: `crates/sa_ship/src/ship.rs` (inline tests)

- [ ] **Step 1: Add SHIP_EXTERIOR collision group constant**

In `crates/spaceaway/src/ship_colliders.rs`, after the existing TERRAIN constant, add:

```rust
/// Landing skid colliders — interact with TERRAIN only.
pub const SHIP_EXTERIOR: Group = Group::GROUP_6;
```

- [ ] **Step 2: Add landing skid positions and collider creation to Ship**

In `crates/sa_ship/src/ship.rs`, add skid constants after the existing constants:

```rust
/// Landing skid positions in ship-local space (bottom of hull).
/// 4 points: fore, aft, port, starboard.
const SKID_POSITIONS: [[f32; 3]; 4] = [
    [0.0, -1.5, -12.0], // fore
    [0.0, -1.5, 12.0],  // aft
    [-2.0, -1.5, 0.0],  // port
    [2.0, -1.5, 0.0],   // starboard
];
const SKID_RADIUS: f32 = 0.3;
const SKID_FRICTION: f32 = 0.6;
```

Add a public method to `Ship` that creates skid colliders. This takes the exterior collision group as a parameter to avoid a dependency from `sa_ship` on `spaceaway`:

```rust
/// Create 4 landing skid colliders on the ship body.
/// Returns collider handles for later removal/query.
/// `exterior_group`: the collision group for ship exterior (SHIP_EXTERIOR).
/// `terrain_group`: the group to collide with (TERRAIN).
pub fn add_landing_skids(
    &self,
    physics: &mut PhysicsWorld,
    exterior_group: rapier3d::prelude::Group,
    terrain_group: rapier3d::prelude::Group,
) -> [ColliderHandle; 4] {
    let groups = rapier3d::prelude::InteractionGroups::new(
        exterior_group,
        terrain_group,
    );
    let mut handles = [rapier3d::prelude::ColliderHandle::invalid(); 4];
    for (i, pos) in SKID_POSITIONS.iter().enumerate() {
        let collider = rapier3d::prelude::ColliderBuilder::ball(SKID_RADIUS)
            .friction(SKID_FRICTION)
            .restitution(0.0)
            .collision_groups(groups)
            .translation(rapier3d::prelude::vector![pos[0], pos[1], pos[2]])
            .build();
        handles[i] = physics.add_collider(collider, self.body_handle);
    }
    handles
}

/// Landing skid positions in ship-local space (for raycasting).
pub fn skid_positions() -> &'static [[f32; 3]; 4] {
    &SKID_POSITIONS
}
```

- [ ] **Step 3: Write test for skid colliders**

Add to the `#[cfg(test)]` module in `crates/sa_ship/src/ship.rs`:

```rust
#[test]
fn landing_skids_created() {
    let mut physics = PhysicsWorld::new();
    let ship = Ship::new(&mut physics, 0.0, 5.0, 0.0);
    let skids = ship.add_landing_skids(
        &mut physics,
        rapier3d::prelude::Group::GROUP_6,
        rapier3d::prelude::Group::GROUP_5,
    );
    for handle in &skids {
        assert!(physics.collider_set.get(*handle).is_some(), "skid collider should exist");
    }
    // Verify skids are NOT sensors
    for handle in &skids {
        let coll = physics.collider_set.get(*handle).unwrap();
        assert!(!coll.is_sensor(), "skid colliders must be solid, not sensors");
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p sa_ship
```

Expected: all tests pass including new `landing_skids_created`.

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/ship_colliders.rs crates/sa_ship/src/ship.rs
git commit -m "feat(ship): add SHIP_EXTERIOR collision group and 4 landing skid colliders"
```

---

### Task 3: Add HeightField Friction and Update Sphere Barrier Filter

**Files:**
- Modify: `crates/spaceaway/src/terrain_colliders.rs`
- Test: manual (physics behavior, not unit-testable)

- [ ] **Step 1: Add friction to HeightField colliders**

In `crates/spaceaway/src/terrain_colliders.rs`, in the `build_heightfield()` function, change the collider builder (around line 342):

```rust
// Before:
let collider = ColliderBuilder::heightfield(heights, scale)
    .collision_groups(groups)
    .position(position)
    .build();

// After:
let collider = ColliderBuilder::heightfield(heights, scale)
    .collision_groups(groups)
    .friction(0.8)
    .position(position)
    .build();
```

- [ ] **Step 2: Update sphere barrier collision filter to include SHIP_EXTERIOR**

In the `update()` method, where the sphere barrier is created (around line 126), update the collision groups:

```rust
// Before:
let collider = ColliderBuilder::ball(radius_m as f32)
    .collision_groups(InteractionGroups::new(
        ship_colliders::TERRAIN,
        ship_colliders::PLAYER.union(ship_colliders::SHIP_HULL),
    ))

// After:
let collider = ColliderBuilder::ball(radius_m as f32)
    .collision_groups(InteractionGroups::new(
        ship_colliders::TERRAIN,
        ship_colliders::PLAYER
            .union(ship_colliders::SHIP_HULL)
            .union(ship_colliders::SHIP_EXTERIOR),
    ))
```

- [ ] **Step 3: Update HeightField collision filter to include SHIP_EXTERIOR**

In `build_heightfield()`, update the groups (around line 337):

```rust
// Before:
let groups = InteractionGroups::new(
    ship_colliders::TERRAIN,
    ship_colliders::PLAYER.union(ship_colliders::SHIP_HULL),
);

// After:
let groups = InteractionGroups::new(
    ship_colliders::TERRAIN,
    ship_colliders::PLAYER
        .union(ship_colliders::SHIP_HULL)
        .union(ship_colliders::SHIP_EXTERIOR),
);
```

- [ ] **Step 4: Build and run tests**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Expected: all pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/terrain_colliders.rs
git commit -m "feat(terrain): add friction to HeightField colliders, update collision filters for SHIP_EXTERIOR"
```

---

### Task 4: Create Landing State Machine Module

**Files:**
- Create: `crates/spaceaway/src/landing.rs`
- Modify: `crates/spaceaway/src/main.rs` (add `mod landing;`)

- [ ] **Step 1: Create the landing module with types and state machine**

Create `crates/spaceaway/src/landing.rs`:

```rust
//! Landing state machine: FLYING → SLIDING → LANDED.
//!
//! Manages ground contact detection via 4-point raycasting,
//! state transitions, and landing impact events.

use sa_physics::PhysicsWorld;

/// Landing skid positions in ship-local space (matches Ship::SKID_POSITIONS).
const SKID_POSITIONS: [[f32; 3]; 4] = [
    [0.0, -1.5, -12.0],
    [0.0, -1.5, 12.0],
    [-2.0, -1.5, 0.0],
    [2.0, -1.5, 0.0],
];

/// Raycast activation altitude (meters above terrain).
const RAYCAST_ALTITUDE_M: f32 = 100.0;

/// Maximum speed for landing lock (m/s).
const LOCK_MAX_SPEED: f32 = 5.0;

/// Altitude threshold to transition from SLIDING back to FLYING (meters).
const FLYING_ALTITUDE_M: f32 = 10.0;

// ---------------------------------------------------------------------------
// Impact categories
// ---------------------------------------------------------------------------

/// Impact severity based on vertical speed at ground contact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImpactCategory {
    /// < 10 m/s — no damage.
    Clean,
    /// 10–30 m/s — minor damage, camera shake.
    Minor,
    /// 30–80 m/s — major damage, systems failure.
    Major,
    /// > 80 m/s — ship destroyed.
    Destroyed,
}

impl ImpactCategory {
    pub fn from_speed(speed: f32) -> Self {
        if speed < 10.0 {
            Self::Clean
        } else if speed < 30.0 {
            Self::Minor
        } else if speed < 80.0 {
            Self::Major
        } else {
            Self::Destroyed
        }
    }
}

/// Emitted on FLYING → SLIDING transition via EventBus.
#[derive(Debug, Clone)]
pub struct LandingImpactEvent {
    pub impact_speed_ms: f32,
    pub per_skid_speeds: [f32; 4],
    pub planet_gravity: f32,
    pub category: ImpactCategory,
}

// ---------------------------------------------------------------------------
// Landing state
// ---------------------------------------------------------------------------

/// Ship landing state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LandingState {
    Flying,
    Sliding,
    Landed,
}

/// Per-frame output from the landing system.
pub struct LandingUpdate {
    /// Current state after this frame's transitions.
    pub state: LandingState,
    /// Minimum clearance from 4-point raycast (meters). None if raycasts inactive.
    pub min_clearance: Option<f32>,
    /// Impact event if FLYING → SLIDING transition occurred this frame.
    pub impact: Option<LandingImpactEvent>,
}

/// Landing system state.
pub struct LandingSystem {
    state: LandingState,
    /// Whether the player has requested lock/unlock this frame.
    lock_requested: bool,
}

impl LandingSystem {
    pub fn new() -> Self {
        Self {
            state: LandingState::Flying,
            lock_requested: false,
        }
    }

    /// Current landing state.
    pub fn state(&self) -> LandingState {
        self.state
    }

    /// Call when the player clicks the landing gear lock button.
    pub fn request_lock_toggle(&mut self) {
        self.lock_requested = true;
    }

    /// Run one frame of landing state logic.
    ///
    /// `ship_speed`: linear velocity magnitude (m/s).
    /// `vertical_speed`: velocity component along gravity direction (m/s, positive = downward).
    /// `ship_transform`: ship body isometry for transforming skid positions.
    /// `gravity_dir`: normalized gravity direction (points toward planet center).
    /// `planet_gravity`: surface gravity in m/s².
    /// `physics`: for raycasting against terrain colliders.
    /// `terrain_active`: whether terrain system is active.
    /// `engine_on`: whether ship engine is on.
    /// `throttle`: current throttle (0.0–1.0).
    pub fn update(
        &mut self,
        ship_speed: f32,
        vertical_speed: f32,
        ship_transform: &nalgebra::Isometry3<f32>,
        gravity_dir: [f32; 3],
        planet_gravity: f32,
        physics: &PhysicsWorld,
        terrain_active: bool,
        engine_on: bool,
        throttle: f32,
    ) -> LandingUpdate {
        let lock_req = std::mem::take(&mut self.lock_requested);

        // If terrain is not active, force FLYING
        if !terrain_active {
            self.state = LandingState::Flying;
            return LandingUpdate {
                state: self.state,
                min_clearance: None,
                impact: None,
            };
        }

        // 4-point raycast for altitude
        let min_clearance = self.raycast_clearance(ship_transform, gravity_dir, physics);

        let mut impact = None;

        match self.state {
            LandingState::Flying => {
                // Transition to SLIDING when any skid contacts terrain
                // (clearance < skid radius means contact)
                if let Some(clearance) = min_clearance {
                    if clearance < 1.0 {
                        self.state = LandingState::Sliding;

                        // Compute per-skid vertical speeds for impact event
                        let per_skid = [vertical_speed; 4]; // simplified: same for all
                        let category = ImpactCategory::from_speed(vertical_speed);

                        impact = Some(LandingImpactEvent {
                            impact_speed_ms: vertical_speed,
                            per_skid_speeds: per_skid,
                            planet_gravity,
                            category,
                        });

                        log::info!(
                            "LANDING: FLYING → SLIDING at {:.1} m/s ({:?})",
                            vertical_speed, category,
                        );
                    }
                }
            }
            LandingState::Sliding => {
                // Lock: player requested + speed low enough
                if lock_req && ship_speed < LOCK_MAX_SPEED {
                    self.state = LandingState::Landed;
                    log::info!("LANDING: SLIDING → LANDED (speed {:.1} m/s)", ship_speed);
                }
                // Back to FLYING if altitude > threshold and thrust active
                else if let Some(clearance) = min_clearance {
                    if clearance > FLYING_ALTITUDE_M && engine_on && throttle > 0.0 {
                        self.state = LandingState::Flying;
                        log::info!("LANDING: SLIDING → FLYING (altitude {:.1}m)", clearance);
                    }
                } else {
                    // No raycast hits = high altitude
                    if engine_on && throttle > 0.0 {
                        self.state = LandingState::Flying;
                        log::info!("LANDING: SLIDING → FLYING (above raycast range)");
                    }
                }
            }
            LandingState::Landed => {
                // Unlock: player requested
                if lock_req {
                    self.state = LandingState::Sliding;
                    log::info!("LANDING: LANDED → SLIDING (unlocked)");
                }
            }
        }

        LandingUpdate {
            state: self.state,
            min_clearance,
            impact,
        }
    }

    /// Cast 4 rays from skid positions along -gravity_dir.
    /// Returns minimum clearance in meters, or None if no hits.
    fn raycast_clearance(
        &self,
        ship_transform: &nalgebra::Isometry3<f32>,
        gravity_dir: [f32; 3],
        physics: &PhysicsWorld,
    ) -> Option<f32> {
        let ray_dir = nalgebra::Vector3::new(-gravity_dir[0], -gravity_dir[1], -gravity_dir[2]);
        if ray_dir.magnitude_squared() < 0.5 {
            return None;
        }
        let ray_dir = ray_dir.normalize();

        let mut min_dist: Option<f32> = None;

        for local_pos in &SKID_POSITIONS {
            let world_pos = ship_transform
                * nalgebra::Point3::new(local_pos[0], local_pos[1], local_pos[2]);

            let ray = rapier3d::prelude::Ray::new(
                nalgebra::Point3::new(world_pos.x, world_pos.y, world_pos.z),
                ray_dir,
            );

            // Cast against all colliders, max distance = RAYCAST_ALTITUDE_M
            if let Some((_handle, toi)) = physics.query_pipeline.cast_ray(
                &physics.rigid_body_set,
                &physics.collider_set,
                &ray,
                RAYCAST_ALTITUDE_M,
                true, // solid
                rapier3d::prelude::QueryFilter::default()
                    .groups(rapier3d::prelude::InteractionGroups::new(
                        rapier3d::prelude::Group::ALL,
                        crate::ship_colliders::TERRAIN,
                    )),
            ) {
                min_dist = Some(match min_dist {
                    Some(d) => d.min(toi),
                    None => toi,
                });
            }
        }

        min_dist
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_category_thresholds() {
        assert_eq!(ImpactCategory::from_speed(0.0), ImpactCategory::Clean);
        assert_eq!(ImpactCategory::from_speed(9.9), ImpactCategory::Clean);
        assert_eq!(ImpactCategory::from_speed(10.0), ImpactCategory::Minor);
        assert_eq!(ImpactCategory::from_speed(29.9), ImpactCategory::Minor);
        assert_eq!(ImpactCategory::from_speed(30.0), ImpactCategory::Major);
        assert_eq!(ImpactCategory::from_speed(79.9), ImpactCategory::Major);
        assert_eq!(ImpactCategory::from_speed(80.0), ImpactCategory::Destroyed);
        assert_eq!(ImpactCategory::from_speed(200.0), ImpactCategory::Destroyed);
    }

    #[test]
    fn initial_state_is_flying() {
        let sys = LandingSystem::new();
        assert_eq!(sys.state(), LandingState::Flying);
    }

    #[test]
    fn lock_rejected_above_speed() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Sliding;
        sys.request_lock_toggle();

        let identity = nalgebra::Isometry3::identity();
        let physics = PhysicsWorld::new();
        let result = sys.update(
            10.0, // speed above LOCK_MAX_SPEED
            0.0,
            &identity,
            [0.0, -1.0, 0.0],
            9.81,
            &physics,
            true,
            false,
            0.0,
        );
        assert_eq!(result.state, LandingState::Sliding, "lock should be rejected above 5 m/s");
    }

    #[test]
    fn lock_accepted_below_speed() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Sliding;
        sys.request_lock_toggle();

        let identity = nalgebra::Isometry3::identity();
        let physics = PhysicsWorld::new();
        let result = sys.update(
            3.0, // speed below LOCK_MAX_SPEED
            0.0,
            &identity,
            [0.0, -1.0, 0.0],
            9.81,
            &physics,
            true,
            false,
            0.0,
        );
        assert_eq!(result.state, LandingState::Landed, "lock should be accepted below 5 m/s");
    }

    #[test]
    fn unlock_from_landed() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Landed;
        sys.request_lock_toggle();

        let identity = nalgebra::Isometry3::identity();
        let physics = PhysicsWorld::new();
        let result = sys.update(
            0.0, 0.0, &identity, [0.0, -1.0, 0.0], 9.81, &physics, true, false, 0.0,
        );
        assert_eq!(result.state, LandingState::Sliding);
    }

    #[test]
    fn terrain_inactive_forces_flying() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Landed;

        let identity = nalgebra::Isometry3::identity();
        let physics = PhysicsWorld::new();
        let result = sys.update(
            0.0, 0.0, &identity, [0.0, -1.0, 0.0], 9.81, &physics,
            false, // terrain NOT active
            false, 0.0,
        );
        assert_eq!(result.state, LandingState::Flying);
    }
}
```

- [ ] **Step 2: Add module declaration to main.rs**

Add near the top of `crates/spaceaway/src/main.rs` with other `mod` declarations:

```rust
mod landing;
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p spaceaway -- landing
cargo clippy --workspace -- -D warnings
```

Expected: 5 landing tests pass, no clippy warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/spaceaway/src/landing.rs crates/spaceaway/src/main.rs
git commit -m "feat(landing): add landing state machine with impact categories and 4-point raycasting"
```

---

### Task 5: Add Landing Gear Lock Button to Cockpit

**Files:**
- Modify: `crates/sa_ship/src/station.rs` (add 6th interactable)
- Modify: `crates/sa_ship/src/interactable.rs` (no changes needed — ToggleButton already exists)

- [ ] **Step 1: Add landing lock button to cockpit layout**

In `crates/sa_ship/src/station.rs`, in the `cockpit_layout()` function, add a 6th interactable after the speed display:

```rust
// 6. Landing gear lock button (on console, right of engine button)
interactables.push(Placement {
    position: glam::Vec3::new(0.3, -0.15, 1.8),
    kind: InteractableKind::Button {
        pressed: false,
        mode: ButtonMode::Toggle,
    },
});
```

Update the function's doc comment or label constants if any exist.

- [ ] **Step 2: Add a constant for the lock button index**

In `crates/sa_ship/src/station.rs`, add a public constant:

```rust
/// Index of the landing gear lock button in the cockpit layout.
pub const LANDING_LOCK_BUTTON: usize = 5;
```

- [ ] **Step 3: Update cockpit test if it checks interactable count**

In `crates/sa_ship/src/station.rs` tests, update any count assertions:

```rust
// Before:
assert_eq!(layout.interactables.len(), 5);
// After:
assert_eq!(layout.interactables.len(), 6);
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p sa_ship
```

Expected: all pass with updated count.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_ship/src/station.rs
git commit -m "feat(cockpit): add landing gear lock button to cockpit layout"
```

---

### Task 6: Integrate Landing System into Main Loop

**Files:**
- Modify: `crates/spaceaway/src/main.rs`

This task connects the landing module to the game loop. It's the largest integration step.

- [ ] **Step 1: Add landing system field to App struct**

In the `App` struct definition in `main.rs`, add:

```rust
/// Landing state machine.
landing: landing::LandingSystem,
/// Landing skid collider handles.
landing_skids: Option<[rapier3d::prelude::ColliderHandle; 4]>,
```

Initialize in `App::new()`:

```rust
landing: landing::LandingSystem::new(),
landing_skids: None,
```

- [ ] **Step 2: Create skid colliders when ship spawns**

After the ship is created (where `self.ship = Some(ship)` is set), add:

```rust
if let Some(ship) = &self.ship {
    self.landing_skids = Some(ship.add_landing_skids(
        &mut self.physics,
        ship_colliders::SHIP_EXTERIOR,
        ship_colliders::TERRAIN,
    ));
}
```

- [ ] **Step 3: Wire landing lock button interaction**

In the interaction handling section (where button presses are processed), detect the landing lock button press. When the player clicks interactable at index `sa_ship::station::LANDING_LOCK_BUTTON` and it's a toggle button:

```rust
if interactable_index == sa_ship::station::LANDING_LOCK_BUTTON {
    self.landing.request_lock_toggle();
}
```

- [ ] **Step 4: Call landing system update each frame in helm mode**

In the helm physics section (after physics.step() and before camera update), add:

```rust
// Landing system update
if let Some(ship) = &self.ship
    && let Some(body) = self.physics.get_body(ship.body_handle)
{
    let ship_speed = ship.speed(&self.physics);
    let gravity_dir = self.terrain_gravity.as_ref()
        .map(|g| g.direction)
        .unwrap_or([0.0, -1.0, 0.0]);
    let grav_vec = nalgebra::Vector3::new(gravity_dir[0], gravity_dir[1], gravity_dir[2]);
    let vertical_speed = body.linvel().dot(&grav_vec).max(0.0);
    let planet_gravity = self.terrain_gravity.as_ref()
        .map(|g| g.magnitude)
        .unwrap_or(0.0);
    let ship_isometry = nalgebra::Isometry3::from_parts(
        nalgebra::Translation3::from(*body.translation()),
        *body.rotation(),
    );

    let landing_result = self.landing.update(
        ship_speed,
        vertical_speed,
        &ship_isometry,
        gravity_dir,
        planet_gravity,
        &self.physics,
        self.terrain.is_some(),
        ship.engine_on,
        ship.throttle,
    );

    // Apply LANDED state: zero velocity, lock position
    if landing_result.state == landing::LandingState::Landed {
        if let Some(body) = self.physics.get_body_mut(ship.body_handle) {
            body.set_linvel(nalgebra::Vector3::zeros(), true);
            body.set_angvel(nalgebra::Vector3::zeros(), true);
            body.set_gravity_scale(0.0, true);
        }
    } else if landing_result.state == landing::LandingState::Sliding
           || landing_result.state == landing::LandingState::Flying
    {
        // Restore gravity scale when not landed (terrain gravity handles pull)
        if let Some(body) = self.physics.get_body_mut(ship.body_handle) {
            if body.gravity_scale() == 0.0 && self.terrain.is_some() {
                // Don't change — terrain gravity is applied manually
            }
        }
    }

    // Handle impact events
    if let Some(impact) = &landing_result.impact {
        // Camera shake based on impact category
        match impact.category {
            landing::ImpactCategory::Clean => {}
            landing::ImpactCategory::Minor => {
                log::info!("IMPACT: Minor ({:.1} m/s) — camera shake small", impact.impact_speed_ms);
                // TODO: camera shake small
            }
            landing::ImpactCategory::Major => {
                log::info!("IMPACT: Major ({:.1} m/s) — camera shake large", impact.impact_speed_ms);
                // TODO: camera shake large
            }
            landing::ImpactCategory::Destroyed => {
                log::info!("IMPACT: DESTROYED ({:.1} m/s)", impact.impact_speed_ms);
                // TODO: ship destruction
            }
        }

        // Emit event for future damage system
        self.events.emit(landing_result.impact.clone().unwrap());
    }
}
```

- [ ] **Step 5: Build and test**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Expected: compiles clean, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/spaceaway/src/main.rs
git commit -m "feat(landing): integrate landing state machine into main game loop"
```

---

### Task 7: Add Altitude Display to Helm Screen

**Files:**
- Modify: `crates/spaceaway/src/ui/helm_screen.rs` (add altitude field to HelmData)
- Modify: `crates/spaceaway/src/main.rs` (pass altitude from landing system to HelmData)

- [ ] **Step 1: Add altitude field to HelmData**

In `crates/spaceaway/src/ui/helm_screen.rs`, add to the `HelmData` struct:

```rust
/// Altitude above terrain in meters (None if not near terrain).
pub altitude_m: Option<f32>,
```

- [ ] **Step 2: Render altitude on helm screen**

In the helm screen draw function, add below the speed display:

```rust
if let Some(alt) = data.altitude_m {
    let alt_text = if alt < 1000.0 {
        format!("ALT {:.0}m", alt)
    } else {
        format!("ALT {:.1}km", alt / 1000.0)
    };
    // Render alt_text in the helm UI (same style as speed readout)
}
```

- [ ] **Step 3: Pass altitude from landing result to HelmData**

In `main.rs` where `HelmData` is constructed, add:

```rust
altitude_m: if self.terrain.is_some() {
    // Use min_clearance from most recent landing update
    self.last_clearance
} else {
    None
},
```

Add `last_clearance: Option<f32>` to App struct. Set it from `landing_result.min_clearance` in the landing update section.

- [ ] **Step 4: Build and test**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/ui/helm_screen.rs crates/spaceaway/src/main.rs
git commit -m "feat(ui): display terrain altitude on helm screen"
```

---

### Task 8: Add Landing Audio Cues

**Files:**
- Modify: `crates/sa_audio/src/lib.rs` (add SfxId variants if needed)
- Modify: `crates/spaceaway/src/main.rs` (play sounds on landing events)

- [ ] **Step 1: Add landing-related SfxId variants**

Check if `SfxId` exists in `sa_audio`. If it has a general-purpose `SfxId` enum, add variants. If sounds are played by path, use the path-based API. Add appropriate variants or path constants:

```rust
// In the audio trigger section of main.rs or a constants block:
const SFX_ALTITUDE_BEEP: &str = "sounds/sci-fi/beep_short.wav";
const SFX_TOUCHDOWN_SOFT: &str = "sounds/sci-fi/thud_metal_soft.wav";
const SFX_TOUCHDOWN_HARD: &str = "sounds/sci-fi/crash_metal.wav";
const SFX_EXPLOSION: &str = "sounds/sci-fi/explosion_large.wav";
const SFX_BUTTON_CLICK: &str = "sounds/sci-fi/button_click.wav";
```

Note: exact paths depend on available WAV files in `assets/sounds/`. Select the closest match from the 3100 available files.

- [ ] **Step 2: Add altitude beep logic**

In the helm mode section, after the landing system update, add altitude-based beeping:

```rust
// Altitude warning beep
if let Some(clearance) = landing_result.min_clearance {
    let beep_interval = if clearance < 5.0 {
        0.0 // continuous
    } else if clearance < 10.0 {
        0.125 // 8/sec
    } else if clearance < 20.0 {
        0.25 // 4/sec
    } else if clearance < 50.0 {
        0.5 // 2/sec
    } else {
        1.0 // 1/sec
    };
    // Track beep timer in App struct; play beep when timer expires
    self.altitude_beep_timer -= dt;
    if self.altitude_beep_timer <= 0.0 && clearance < RAYCAST_ALTITUDE_M {
        self.audio.play_sfx_by_path(SFX_ALTITUDE_BEEP, None);
        self.altitude_beep_timer = beep_interval;
    }
} else {
    self.altitude_beep_timer = 0.0; // reset when above raycast range
}
```

Add `altitude_beep_timer: f32` to the App struct, initialized to `0.0`.

- [ ] **Step 3: Add impact sounds**

In the impact event handler (from Task 6), add sound playback:

```rust
match impact.category {
    landing::ImpactCategory::Clean => {
        self.audio.play_sfx_by_path(SFX_TOUCHDOWN_SOFT, None);
    }
    landing::ImpactCategory::Minor => {
        self.audio.play_sfx_by_path(SFX_TOUCHDOWN_HARD, None);
    }
    landing::ImpactCategory::Major => {
        self.audio.play_sfx_by_path(SFX_TOUCHDOWN_HARD, None);
        self.audio.play_alarm(sa_audio::AlarmId::Hull);
    }
    landing::ImpactCategory::Destroyed => {
        self.audio.play_sfx_by_path(SFX_EXPLOSION, None);
    }
}
```

- [ ] **Step 4: Build and test**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Expected: compiles, tests pass. Audio tested manually in-game.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_audio/src/lib.rs crates/spaceaway/src/main.rs
git commit -m "feat(audio): add altitude beep, touchdown, and impact sounds for landing"
```

---

### Task 9: Integration Test — Full Landing Cycle

This is a manual play-test checklist, not automated tests.

- [ ] **Step 1: Build the game**

```bash
cargo build -p spaceaway
```

- [ ] **Step 2: Test descent**

1. Run with `RUST_LOG=info cargo run -p spaceaway`
2. Press 8 to enter a system, Tab to lock planet, engage cruise (2)
3. Approach planet — cruise should auto-disengage at 2.0x radius
4. Switch to impulse, engage engine, throttle up
5. Fly toward the planet surface — verify terrain LODs increase

- [ ] **Step 3: Test ground contact**

1. Approach surface at moderate speed (< 30 m/s)
2. Verify altitude beep starts at ~100m and increases in frequency
3. Verify ship contacts terrain and decelerates (friction from skid colliders)
4. Verify "LANDING: FLYING → SLIDING" in console log

- [ ] **Step 4: Test landing lock**

1. Wait until ship speed drops below 5 m/s
2. Click the landing lock button on the cockpit console
3. Verify "LANDING: SLIDING → LANDED" in console log
4. Verify ship is frozen (no movement, no drift)

- [ ] **Step 5: Test takeoff**

1. Throttle up while landed
2. Click unlock button
3. Verify "LANDING: LANDED → SLIDING" in console log
4. Verify ship lifts off with thrust > gravity
5. Verify "LANDING: SLIDING → FLYING" when altitude > 10m

- [ ] **Step 6: Test hard landing**

1. Approach surface at > 30 m/s
2. Verify camera shake and crash audio
3. Verify "IMPACT: Major" in console log

- [ ] **Step 7: Commit final state**

```bash
git add -A
git commit -m "feat(landing): Phase 3 landing system complete — descent, contact, lock/unlock, takeoff"
```
