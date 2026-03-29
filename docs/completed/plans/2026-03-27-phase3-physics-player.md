# Phase 3: Physics & Player — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build sa_physics (Newtonian rigid body simulation via rapier3d) and sa_player (first-person character controller), producing a scene where the player walks on a floor with gravity, collides with walls/objects, and can fly a ship body with Newtonian thrust.

**Architecture:** sa_physics wraps rapier3d to manage rigid bodies and colliders, synced with the ECS via components. sa_player provides a first-person character controller (capsule collider + kinematic body) that reads input and moves through the physics world. The game binary integrates both with the renderer, replacing the free-fly camera with a physics-driven player.

**Tech Stack:** rapier3d (physics), sa_math (WorldPos, units), sa_ecs (GameWorld), sa_input (InputState), sa_render (Camera, Renderer)

---

## File Structure

```
crates/
├── sa_physics/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Re-exports
│       ├── world.rs            # PhysicsWorld: rapier pipeline wrapper
│       ├── bodies.rs           # RigidBody component, body creation helpers
│       ├── colliders.rs        # Collider component, shape creation helpers
│       ├── forces.rs           # Force/thrust application (Newtonian)
│       └── sync.rs             # Sync physics positions ↔ ECS components
├── sa_player/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Re-exports
│       ├── controller.rs       # First-person character controller
│       └── player_systems.rs   # ECS systems for player update
└── spaceaway/
    └── src/
        └── main.rs             # Updated with physics + player
```

---

### Task 1: sa_physics Crate — PhysicsWorld

**Files:**
- Create: `crates/sa_physics/Cargo.toml`
- Create: `crates/sa_physics/src/lib.rs`
- Create: `crates/sa_physics/src/world.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add sa_physics to workspace**

Add `"crates/sa_physics"` to workspace members in root `Cargo.toml`. Add under `[workspace.dependencies]`:
```toml
rapier3d = { version = "0.22", features = ["simd-stable"] }
sa_physics = { path = "crates/sa_physics" }
```

Create `crates/sa_physics/Cargo.toml`:
```toml
[package]
name = "sa_physics"
version.workspace = true
edition.workspace = true

[dependencies]
rapier3d.workspace = true
glam.workspace = true
log.workspace = true
sa_core.workspace = true
sa_math.workspace = true
```

- [ ] **Step 2: Write failing tests for PhysicsWorld**

```rust
// crates/sa_physics/src/world.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_world_has_gravity() {
        let world = PhysicsWorld::new();
        assert!(world.gravity().y < 0.0);
    }

    #[test]
    fn step_does_not_panic() {
        let mut world = PhysicsWorld::new();
        world.step(1.0 / 60.0);
    }

    #[test]
    fn zero_gravity_option() {
        let world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        assert_eq!(world.gravity().y, 0.0);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p sa_physics`
Expected: FAIL — PhysicsWorld not defined.

- [ ] **Step 4: Implement PhysicsWorld**

```rust
// crates/sa_physics/src/world.rs
use rapier3d::prelude::*;

/// Wraps the rapier3d physics pipeline.
pub struct PhysicsWorld {
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    gravity: nalgebra::Vector3<f32>,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self::with_gravity(0.0, -9.81, 0.0)
    }

    pub fn with_gravity(x: f32, y: f32, z: f32) -> Self {
        Self {
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            gravity: nalgebra::Vector3::new(x, y, z),
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
        }
    }

    pub fn gravity(&self) -> glam::Vec3 {
        glam::Vec3::new(self.gravity.x, self.gravity.y, self.gravity.z)
    }

    pub fn set_gravity(&mut self, x: f32, y: f32, z: f32) {
        self.gravity = nalgebra::Vector3::new(x, y, z);
    }

    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
    }

    pub fn add_rigid_body(&mut self, body: RigidBody) -> RigidBodyHandle {
        self.rigid_body_set.insert(body)
    }

    pub fn add_collider(
        &mut self,
        collider: Collider,
        parent: RigidBodyHandle,
    ) -> ColliderHandle {
        self.collider_set
            .insert_with_parent(collider, parent, &mut self.rigid_body_set)
    }

    pub fn add_collider_without_parent(&mut self, collider: Collider) -> ColliderHandle {
        self.collider_set.insert(collider)
    }

    pub fn get_body(&self, handle: RigidBodyHandle) -> Option<&RigidBody> {
        self.rigid_body_set.get(handle)
    }

    pub fn get_body_mut(&mut self, handle: RigidBodyHandle) -> Option<&mut RigidBody> {
        self.rigid_body_set.get_mut(handle)
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Create lib.rs**

```rust
// crates/sa_physics/src/lib.rs
pub mod world;

pub use world::PhysicsWorld;
// Re-export rapier types that game code needs
pub use rapier3d::prelude::{
    RigidBody, RigidBodyBuilder, RigidBodyHandle,
    Collider, ColliderBuilder, ColliderHandle,
};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p sa_physics`
Expected: All 3 tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/sa_physics/ Cargo.toml
git commit -m "feat(sa_physics): add PhysicsWorld wrapping rapier3d pipeline"
```

---

### Task 2: sa_physics — Body and Collider Helpers

**Files:**
- Create: `crates/sa_physics/src/bodies.rs`
- Create: `crates/sa_physics/src/colliders.rs`

- [ ] **Step 1: Write failing tests for body helpers**

```rust
// crates/sa_physics/src/bodies.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::PhysicsWorld;

    #[test]
    fn create_dynamic_body() {
        let mut world = PhysicsWorld::new();
        let handle = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        let body = world.get_body(handle).unwrap();
        assert!(body.is_dynamic());
    }

    #[test]
    fn create_static_body() {
        let mut world = PhysicsWorld::new();
        let handle = spawn_static_body(&mut world, 0.0, 0.0, 0.0);
        let body = world.get_body(handle).unwrap();
        assert!(body.is_fixed());
    }

    #[test]
    fn dynamic_body_falls_with_gravity() {
        let mut world = PhysicsWorld::new();
        let handle = spawn_dynamic_body(&mut world, 0.0, 10.0, 0.0, 1.0);
        let y_before = world.get_body(handle).unwrap().translation().y;
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let y_after = world.get_body(handle).unwrap().translation().y;
        assert!(y_after < y_before);
    }

    #[test]
    fn kinematic_body_does_not_fall() {
        let mut world = PhysicsWorld::new();
        let handle = spawn_kinematic_body(&mut world, 0.0, 5.0, 0.0);
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let y = world.get_body(handle).unwrap().translation().y;
        assert!((y - 5.0).abs() < 0.01);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_physics`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement body helpers**

```rust
// crates/sa_physics/src/bodies.rs
use crate::world::PhysicsWorld;
use rapier3d::prelude::*;

/// Spawn a dynamic rigid body (affected by forces and gravity).
pub fn spawn_dynamic_body(
    world: &mut PhysicsWorld,
    x: f32, y: f32, z: f32,
    mass: f32,
) -> RigidBodyHandle {
    let body = RigidBodyBuilder::dynamic()
        .translation(nalgebra::Vector3::new(x, y, z))
        .additional_mass(mass)
        .build();
    world.add_rigid_body(body)
}

/// Spawn a static (fixed) rigid body (never moves).
pub fn spawn_static_body(
    world: &mut PhysicsWorld,
    x: f32, y: f32, z: f32,
) -> RigidBodyHandle {
    let body = RigidBodyBuilder::fixed()
        .translation(nalgebra::Vector3::new(x, y, z))
        .build();
    world.add_rigid_body(body)
}

/// Spawn a kinematic body (moved by code, not affected by forces).
pub fn spawn_kinematic_body(
    world: &mut PhysicsWorld,
    x: f32, y: f32, z: f32,
) -> RigidBodyHandle {
    let body = RigidBodyBuilder::kinematic_position_based()
        .translation(nalgebra::Vector3::new(x, y, z))
        .build();
    world.add_rigid_body(body)
}

/// Get the position of a rigid body as (x, y, z).
pub fn body_position(world: &PhysicsWorld, handle: RigidBodyHandle) -> Option<(f32, f32, f32)> {
    world.get_body(handle).map(|b| {
        let t = b.translation();
        (t.x, t.y, t.z)
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_physics`
Expected: All tests PASS.

- [ ] **Step 5: Write failing tests for collider helpers**

```rust
// crates/sa_physics/src/colliders.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bodies::spawn_dynamic_body;
    use crate::PhysicsWorld;

    #[test]
    fn add_box_collider() {
        let mut world = PhysicsWorld::new();
        let body = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        let _col = attach_box_collider(&mut world, body, 1.0, 1.0, 1.0);
    }

    #[test]
    fn add_sphere_collider() {
        let mut world = PhysicsWorld::new();
        let body = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        let _col = attach_sphere_collider(&mut world, body, 0.5);
    }

    #[test]
    fn floor_stops_falling_body() {
        let mut world = PhysicsWorld::new();

        // Floor: static body with large box collider
        let floor = crate::bodies::spawn_static_body(&mut world, 0.0, 0.0, 0.0);
        attach_box_collider(&mut world, floor, 50.0, 0.1, 50.0);

        // Ball above the floor
        let ball = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        attach_sphere_collider(&mut world, ball, 0.5);

        // Simulate 3 seconds
        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }

        let y = world.get_body(ball).unwrap().translation().y;
        // Ball should have come to rest near floor level (above 0 due to radius)
        assert!(y > -0.1);
        assert!(y < 2.0);
    }

    #[test]
    fn add_ground_plane() {
        let mut world = PhysicsWorld::new();
        add_ground(&mut world, 0.0);
        // Just verify it doesn't panic
    }
}
```

- [ ] **Step 6: Implement collider helpers**

```rust
// crates/sa_physics/src/colliders.rs
use crate::world::PhysicsWorld;
use rapier3d::prelude::*;

/// Attach a box collider to a rigid body. Half-extents: hx, hy, hz.
pub fn attach_box_collider(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    hx: f32, hy: f32, hz: f32,
) -> ColliderHandle {
    let collider = ColliderBuilder::cuboid(hx, hy, hz)
        .restitution(0.2)
        .build();
    world.add_collider(collider, body)
}

/// Attach a sphere collider to a rigid body.
pub fn attach_sphere_collider(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    radius: f32,
) -> ColliderHandle {
    let collider = ColliderBuilder::ball(radius)
        .restitution(0.2)
        .build();
    world.add_collider(collider, body)
}

/// Attach a capsule collider to a rigid body (for characters).
/// Total height = height + 2*radius.
pub fn attach_capsule_collider(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    half_height: f32,
    radius: f32,
) -> ColliderHandle {
    let collider = ColliderBuilder::capsule_y(half_height, radius)
        .restitution(0.0)
        .friction(0.5)
        .build();
    world.add_collider(collider, body)
}

/// Add a large static ground plane at the given Y height.
pub fn add_ground(world: &mut PhysicsWorld, y: f32) {
    let body = crate::bodies::spawn_static_body(world, 0.0, y, 0.0);
    attach_box_collider(world, body, 500.0, 0.1, 500.0);
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p sa_physics`
Expected: All tests PASS.

- [ ] **Step 8: Update lib.rs**

```rust
// crates/sa_physics/src/lib.rs
pub mod world;
pub mod bodies;
pub mod colliders;

pub use world::PhysicsWorld;
pub use bodies::{spawn_dynamic_body, spawn_static_body, spawn_kinematic_body, body_position};
pub use colliders::{attach_box_collider, attach_sphere_collider, attach_capsule_collider, add_ground};
pub use rapier3d::prelude::{
    RigidBody, RigidBodyBuilder, RigidBodyHandle,
    Collider, ColliderBuilder, ColliderHandle,
};
```

- [ ] **Step 9: Run clippy and all tests**

Run: `cargo clippy -p sa_physics -- -D warnings && cargo test -p sa_physics`
Expected: Clean, all tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/sa_physics/src/
git commit -m "feat(sa_physics): add body and collider helpers with ground plane"
```

---

### Task 3: sa_physics — Newtonian Force Application

**Files:**
- Create: `crates/sa_physics/src/forces.rs`

- [ ] **Step 1: Write failing tests**

```rust
// crates/sa_physics/src/forces.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhysicsWorld, bodies::spawn_dynamic_body, colliders::attach_box_collider};

    #[test]
    fn apply_force_changes_velocity() {
        let mut world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0); // zero-g
        let handle = spawn_dynamic_body(&mut world, 0.0, 0.0, 0.0, 1.0);
        attach_box_collider(&mut world, handle, 0.5, 0.5, 0.5);

        apply_force(&mut world, handle, 10.0, 0.0, 0.0);
        world.step(1.0 / 60.0);

        let body = world.get_body(handle).unwrap();
        let vel = body.linvel();
        assert!(vel.x > 0.0); // should be moving in +X
    }

    #[test]
    fn no_drag_momentum_persists() {
        let mut world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let handle = spawn_dynamic_body(&mut world, 0.0, 0.0, 0.0, 1.0);
        attach_box_collider(&mut world, handle, 0.5, 0.5, 0.5);

        // Apply force for one frame then stop
        apply_force(&mut world, handle, 100.0, 0.0, 0.0);
        world.step(1.0 / 60.0);
        let vel_after_thrust = world.get_body(handle).unwrap().linvel().x;

        // Coast for 60 more frames with no force
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let vel_after_coast = world.get_body(handle).unwrap().linvel().x;

        // Velocity should be approximately the same (Newtonian: no drag)
        assert!((vel_after_coast - vel_after_thrust).abs() < 0.01);
    }

    #[test]
    fn apply_torque_changes_angular_velocity() {
        let mut world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let handle = spawn_dynamic_body(&mut world, 0.0, 0.0, 0.0, 1.0);
        attach_box_collider(&mut world, handle, 0.5, 0.5, 0.5);

        apply_torque(&mut world, handle, 0.0, 10.0, 0.0);
        world.step(1.0 / 60.0);

        let body = world.get_body(handle).unwrap();
        let angvel = body.angvel();
        assert!(angvel.y.abs() > 0.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_physics -- forces`
Expected: FAIL.

- [ ] **Step 3: Implement force application**

```rust
// crates/sa_physics/src/forces.rs
use crate::world::PhysicsWorld;
use rapier3d::prelude::*;

/// Apply a force to a rigid body (Newtonian: F=ma, force applied for one physics step).
pub fn apply_force(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    fx: f32, fy: f32, fz: f32,
) {
    if let Some(body) = world.get_body_mut(handle) {
        body.add_force(nalgebra::Vector3::new(fx, fy, fz), true);
    }
}

/// Apply an impulse (instantaneous velocity change).
pub fn apply_impulse(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    ix: f32, iy: f32, iz: f32,
) {
    if let Some(body) = world.get_body_mut(handle) {
        body.apply_impulse(nalgebra::Vector3::new(ix, iy, iz), true);
    }
}

/// Apply a torque to a rigid body (for rotation).
pub fn apply_torque(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    tx: f32, ty: f32, tz: f32,
) {
    if let Some(body) = world.get_body_mut(handle) {
        body.add_torque(nalgebra::Vector3::new(tx, ty, tz), true);
    }
}

/// Get the linear velocity of a body.
pub fn linear_velocity(world: &PhysicsWorld, handle: RigidBodyHandle) -> Option<(f32, f32, f32)> {
    world.get_body(handle).map(|b| {
        let v = b.linvel();
        (v.x, v.y, v.z)
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_physics -- forces`
Expected: All 3 tests PASS.

- [ ] **Step 5: Update lib.rs**

Add `pub mod forces;` and re-exports:
```rust
pub use forces::{apply_force, apply_impulse, apply_torque, linear_velocity};
```

- [ ] **Step 6: Commit**

```bash
git add crates/sa_physics/src/
git commit -m "feat(sa_physics): add Newtonian force, impulse, and torque application"
```

---

### Task 4: sa_player Crate — Character Controller

**Files:**
- Create: `crates/sa_player/Cargo.toml`
- Create: `crates/sa_player/src/lib.rs`
- Create: `crates/sa_player/src/controller.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add sa_player to workspace**

Add `"crates/sa_player"` to workspace members. Add under `[workspace.dependencies]`:
```toml
sa_player = { path = "crates/sa_player" }
```

Create `crates/sa_player/Cargo.toml`:
```toml
[package]
name = "sa_player"
version.workspace = true
edition.workspace = true

[dependencies]
glam.workspace = true
log.workspace = true
sa_core.workspace = true
sa_math.workspace = true
sa_physics.workspace = true
sa_input.workspace = true
rapier3d.workspace = true
```

- [ ] **Step 2: Write failing tests for PlayerController**

```rust
// crates/sa_player/src/controller.rs
#[cfg(test)]
mod tests {
    use super::*;
    use sa_physics::PhysicsWorld;

    #[test]
    fn spawn_creates_body() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        assert!(physics.get_body(player.body_handle).is_some());
    }

    #[test]
    fn player_does_not_tip_over() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        // Body should have rotation locked
        let body = physics.get_body(player.body_handle).unwrap();
        assert!(body.is_rotation_locked(0)); // X locked
        assert!(body.is_rotation_locked(1)); // Y locked
        assert!(body.is_rotation_locked(2)); // Z locked
    }

    #[test]
    fn initial_state() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        assert_eq!(player.yaw, 0.0);
        assert_eq!(player.pitch, 0.0);
        assert!(player.grounded);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p sa_player`
Expected: FAIL — PlayerController not defined.

- [ ] **Step 4: Implement PlayerController**

```rust
// crates/sa_player/src/controller.rs
use glam::Vec3;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use rapier3d::prelude::*;

const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HALF_HEIGHT: f32 = 0.6;
const MOVE_SPEED: f32 = 5.0;
const JUMP_IMPULSE: f32 = 5.0;
const MOUSE_SENSITIVITY: f32 = 0.003;

pub struct PlayerController {
    pub body_handle: RigidBodyHandle,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
}

impl PlayerController {
    /// Spawn a player at the given position. Creates a capsule rigid body
    /// with locked rotation (won't tip over).
    pub fn spawn(physics: &mut PhysicsWorld, x: f32, y: f32, z: f32) -> Self {
        let body = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector3::new(x, y, z))
            .lock_rotations()
            .linear_damping(0.5) // slight damping for character feel
            .build();
        let handle = physics.add_rigid_body(body);

        let collider = ColliderBuilder::capsule_y(PLAYER_HALF_HEIGHT, PLAYER_RADIUS)
            .friction(0.0)
            .restitution(0.0)
            .build();
        physics.add_collider(collider, handle);

        Self {
            body_handle: handle,
            yaw: 0.0,
            pitch: 0.0,
            grounded: true,
        }
    }

    /// Update player from input. Call before physics step.
    pub fn update(
        &mut self,
        physics: &mut PhysicsWorld,
        input: &sa_input::InputState,
        dt: f32,
    ) {
        use winit::keyboard::KeyCode;

        // Mouse look
        let (dx, dy) = input.mouse.delta();
        self.yaw += dx * MOUSE_SENSITIVITY;
        self.pitch -= dy * MOUSE_SENSITIVITY;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);

        // Movement direction in world space
        let forward = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos()).normalize();
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize();

        let mut move_dir = Vec3::ZERO;
        if input.keyboard.is_pressed(KeyCode::KeyW) { move_dir += forward; }
        if input.keyboard.is_pressed(KeyCode::KeyS) { move_dir -= forward; }
        if input.keyboard.is_pressed(KeyCode::KeyA) { move_dir -= right; }
        if input.keyboard.is_pressed(KeyCode::KeyD) { move_dir += right; }

        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        // Apply movement as velocity (horizontal only, preserve vertical velocity)
        if let Some(body) = physics.get_body_mut(self.body_handle) {
            let current_vel = body.linvel();
            let target_vel = move_dir * MOVE_SPEED;
            body.set_linvel(
                nalgebra::Vector3::new(target_vel.x, current_vel.y, target_vel.z),
                true,
            );

            // Ground check: simple Y velocity check (proper raycast later)
            self.grounded = current_vel.y.abs() < 0.1;

            // Jump
            if input.keyboard.is_pressed(KeyCode::Space) && self.grounded {
                body.apply_impulse(nalgebra::Vector3::new(0.0, JUMP_IMPULSE, 0.0), true);
            }
        }
    }

    /// Get camera-compatible forward and right vectors.
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        ).normalize()
    }

    /// Get the player's world position from the physics body.
    pub fn position(&self, physics: &PhysicsWorld) -> WorldPos {
        if let Some(body) = physics.get_body(self.body_handle) {
            let t = body.translation();
            // Offset Y up by capsule height so camera is at "eye level"
            WorldPos::new(t.x as f64, (t.y + PLAYER_HALF_HEIGHT + PLAYER_RADIUS) as f64, t.z as f64)
        } else {
            WorldPos::ORIGIN
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p sa_player`
Expected: All 3 tests PASS.

- [ ] **Step 6: Create lib.rs**

```rust
// crates/sa_player/src/lib.rs
pub mod controller;

pub use controller::PlayerController;
```

- [ ] **Step 7: Run clippy**

Run: `cargo clippy -p sa_player -- -D warnings`
Expected: Clean.

- [ ] **Step 8: Commit**

```bash
git add crates/sa_player/ Cargo.toml
git commit -m "feat(sa_player): add first-person character controller with physics"
```

---

### Task 5: Integration — Update Game Binary

**Files:**
- Modify: `crates/spaceaway/Cargo.toml`
- Rewrite: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Update spaceaway Cargo.toml**

Add to `[dependencies]`:
```toml
sa_physics.workspace = true
sa_player.workspace = true
rapier3d.workspace = true
```

- [ ] **Step 2: Rewrite main.rs**

Replace entirely. The new version:
- Creates a physics world with gravity
- Adds a ground plane and some box obstacles
- Spawns the player as a physics-driven character controller
- Replaces free-fly camera with physics player position/rotation
- Still renders the same geometry (cubes) and stars
- WASD moves the player, mouse looks, Space jumps, Escape releases cursor

```rust
// crates/spaceaway/src/main.rs
use glam::{Mat4, Vec3};
use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use sa_input::InputState;
use sa_physics::PhysicsWorld;
use sa_player::PlayerController;
use sa_render::{Camera, DrawCommand, GpuContext, MeshData, Renderer, Vertex};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{CursorGrabMode, Window, WindowId};

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    camera: Camera,
    input: InputState,
    world: GameWorld,
    events: EventBus,
    time: FrameTime,
    schedule: Schedule,
    last_frame: Instant,
    physics: PhysicsWorld,
    player: Option<PlayerController>,
    cube_mesh: Option<sa_core::Handle<sa_render::MeshMarker>>,
    cursor_grabbed: bool,
}

impl App {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new();
        sa_physics::add_ground(&mut physics, 0.0);

        // Add some box obstacles as static bodies
        for (x, z) in [(5.0_f32, -3.0_f32), (-4.0, -6.0), (2.0, -10.0)] {
            let body = sa_physics::spawn_static_body(&mut physics, x, 1.0, z);
            sa_physics::attach_box_collider(&mut physics, body, 1.0, 1.0, 1.0);
        }

        let player = PlayerController::spawn(&mut physics, 0.0, 2.0, 10.0);

        Self {
            window: None, gpu: None, renderer: None,
            camera: Camera::new(), input: InputState::new(),
            world: GameWorld::new(), events: EventBus::new(),
            time: FrameTime::new(), schedule: Schedule::new(),
            last_frame: Instant::now(),
            physics,
            player: Some(player),
            cube_mesh: None,
            cursor_grabbed: false,
        }
    }

    fn setup_scene(&mut self) {
        let renderer = self.renderer.as_mut().unwrap();
        let gpu = self.gpu.as_ref().unwrap();
        let handle = renderer.mesh_store.upload(&gpu.device, &make_cube());
        self.cube_mesh = Some(handle);
    }

    fn update(&mut self) {
        let dt = self.time.delta_seconds() as f32;

        // Update player controller
        if let Some(player) = &mut self.player {
            player.update(&mut self.physics, &self.input, dt);
        }

        // Step physics
        self.physics.step(dt.min(1.0 / 30.0)); // clamp physics dt

        // Sync camera to player
        if let Some(player) = &self.player {
            self.camera.position = player.position(&self.physics);
            self.camera.yaw = player.yaw;
            self.camera.pitch = player.pitch;
        }
    }

    fn grab_cursor(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
            self.cursor_grabbed = true;
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("SpaceAway")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            let gpu = GpuContext::new(window.clone());
            let renderer = Renderer::new(&gpu);
            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.window = Some(window);
            self.last_frame = Instant::now();
            self.setup_scene();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    self.input.keyboard.set_pressed(code, event.state.is_pressed());
                    if code == KeyCode::Escape && event.state.is_pressed() {
                        if let Some(window) = &self.window {
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                            window.set_cursor_visible(true);
                            self.cursor_grabbed = false;
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, .. } => {
                if state.is_pressed() && !self.cursor_grabbed {
                    self.grab_cursor();
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size.width, new_size.height);
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(gpu);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                self.time.advance(now - self.last_frame);
                self.last_frame = now;
                self.schedule.run(&mut self.world, &mut self.events, &self.time);
                self.update();

                if let (Some(gpu), Some(renderer), Some(cube)) = (&self.gpu, &self.renderer, self.cube_mesh) {
                    // Ground plane (flat cube)
                    let mut commands = vec![
                        DrawCommand {
                            mesh: cube,
                            model_matrix: Mat4::from_scale_rotation_translation(
                                Vec3::new(50.0, 0.1, 50.0),
                                glam::Quat::IDENTITY,
                                Vec3::new(0.0, 0.0, 0.0),
                            ),
                        },
                    ];

                    // Box obstacles (matching physics bodies)
                    for (x, z) in [(5.0_f32, -3.0_f32), (-4.0, -6.0), (2.0, -10.0)] {
                        commands.push(DrawCommand {
                            mesh: cube,
                            model_matrix: Mat4::from_translation(Vec3::new(x, 1.0, z)),
                        });
                    }

                    let light_dir = Vec3::new(0.5, -0.8, -0.3);
                    renderer.render_frame(gpu, &self.camera, &commands, light_dir);
                }

                self.events.flush();
                self.input.end_frame();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: winit::event::DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.cursor_grabbed {
                self.input.mouse.accumulate_delta(delta.0 as f32, delta.1 as f32);
            }
        }
    }
}

fn make_cube() -> MeshData {
    let faces: &[([f32; 3], [f32; 3], [[f32; 3]; 4])] = &[
        ([0.0, 0.0, 1.0],  [0.6, 0.6, 0.7], [[-1.0,-1.0, 1.0],[ 1.0,-1.0, 1.0],[ 1.0, 1.0, 1.0],[-1.0, 1.0, 1.0]]),
        ([0.0, 0.0,-1.0],  [0.5, 0.5, 0.6], [[ 1.0,-1.0,-1.0],[-1.0,-1.0,-1.0],[-1.0, 1.0,-1.0],[ 1.0, 1.0,-1.0]]),
        ([0.0, 1.0, 0.0],  [0.7, 0.7, 0.8], [[-1.0, 1.0, 1.0],[ 1.0, 1.0, 1.0],[ 1.0, 1.0,-1.0],[-1.0, 1.0,-1.0]]),
        ([0.0,-1.0, 0.0],  [0.4, 0.4, 0.5], [[-1.0,-1.0,-1.0],[ 1.0,-1.0,-1.0],[ 1.0,-1.0, 1.0],[-1.0,-1.0, 1.0]]),
        ([1.0, 0.0, 0.0],  [0.55, 0.55, 0.65], [[ 1.0,-1.0, 1.0],[ 1.0,-1.0,-1.0],[ 1.0, 1.0,-1.0],[ 1.0, 1.0, 1.0]]),
        ([-1.0, 0.0, 0.0], [0.5, 0.5, 0.6], [[-1.0,-1.0,-1.0],[-1.0,-1.0, 1.0],[-1.0, 1.0, 1.0],[-1.0, 1.0,-1.0]]),
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (normal, color, verts) in faces {
        let base = vertices.len() as u32;
        for v in verts {
            vertices.push(Vertex { position: *v, color: *color, normal: *normal });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    MeshData { vertices, indices }
}

fn main() {
    env_logger::init();
    log::info!("SpaceAway starting...");
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p spaceaway`
Expected: Compiles.

- [ ] **Step 4: Run clippy on entire workspace**

Run: `cargo clippy --workspace -- -D warnings`
Expected: Clean.

- [ ] **Step 5: Run and verify**

Run: `cargo run -p spaceaway`
Expected:
- Player spawns above ground, falls with gravity, lands on floor
- WASD walks, mouse looks, Space jumps
- Cubes are solid obstacles (can't walk through them)
- Stars visible in the sky
- Click to grab mouse, Escape to release

- [ ] **Step 6: Commit**

```bash
git add crates/spaceaway/
git commit -m "feat(spaceaway): integrate physics and player controller with test scene"
```

---

### Task 6: Final Verification

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass (42 from Phase 1-2 + new sa_physics + sa_player tests).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: Clean.

- [ ] **Step 3: Run the game**

Run: `cargo run -p spaceaway`
Expected: Physics-driven first-person movement on a ground plane with obstacles and star field.
