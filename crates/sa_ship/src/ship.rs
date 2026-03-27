//! Ship entity: physics body, propulsion state, force application.

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;

/// Ship physics and propulsion state.
pub struct Ship {
    /// The rapier3d rigid body handle for the ship.
    pub body_handle: RigidBodyHandle,
    /// Current throttle level (0.0 to 1.0), set by the thrust lever.
    pub throttle: f32,
    /// Whether the engine is on, set by the engine button.
    pub engine_on: bool,
    /// Maximum forward thrust in Newtons.
    pub max_thrust: f32,
    /// Maximum lateral/vertical thrust (RCS) in Newtons.
    pub max_rcs_thrust: f32,
    /// Maximum rotational torque in Nm.
    pub max_torque: f32,
}

impl Ship {
    /// Ship physics constants.
    pub const MASS: f32 = 50_000.0;
    pub const DEFAULT_MAX_THRUST: f32 = 500_000.0;
    pub const DEFAULT_MAX_RCS: f32 = 50_000.0;
    pub const DEFAULT_MAX_TORQUE: f32 = 100_000.0;

    /// Create a new ship and register its rigid body in the physics world.
    /// The ship spawns at the given position with zero velocity.
    /// The body has no gravity, no linear damping, and no angular damping
    /// (Newtonian behavior --- momentum persists until countered).
    pub fn new(physics: &mut PhysicsWorld, x: f32, y: f32, z: f32) -> Self {
        let body = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector3::new(x, y, z))
            .gravity_scale(0.0) // No gravity on the ship (space)
            .linear_damping(0.0) // No drag
            .angular_damping(0.5) // Slight angular damping for controllability
            .ccd_enabled(true)
            .build();
        let body_handle = physics.add_rigid_body(body);

        // Hull collider is a SENSOR — provides mass/inertia but doesn't
        // generate contact forces. The player walks on a separate floor
        // collider added in ship_setup. A solid hull collider would
        // trap/eject the player since they spawn inside it.
        let collider = ColliderBuilder::cuboid(2.5, 1.5, 15.0)
            .mass(Self::MASS)
            .sensor(true)
            .build();
        physics.add_collider(collider, body_handle);

        Self {
            body_handle,
            throttle: 0.0,
            engine_on: false,
            max_thrust: Self::DEFAULT_MAX_THRUST,
            max_rcs_thrust: Self::DEFAULT_MAX_RCS,
            max_torque: Self::DEFAULT_MAX_TORQUE,
        }
    }

    /// Reset accumulated forces and torques on the ship body.
    /// Call at the start of each frame before applying new forces.
    /// Rapier3d `add_force`/`add_torque` accumulate persistently;
    /// this clears them so only the current frame's inputs take effect.
    pub fn reset_forces(&self, physics: &mut PhysicsWorld) {
        if let Some(body) = physics.get_body_mut(self.body_handle) {
            body.reset_forces(true);
            body.reset_torques(true);
        }
    }

    /// Apply forward thrust along the ship's local -Z axis (nose direction).
    /// Effective thrust = throttle * max_thrust * engine_on.
    /// Call once per frame while helm is active.
    pub fn apply_thrust(&self, physics: &mut PhysicsWorld) {
        if !self.engine_on {
            return;
        }
        let force_magnitude = self.throttle * self.max_thrust;
        if force_magnitude.abs() < 0.01 {
            return;
        }
        // Read rotation first, then apply force with mutable borrow.
        let rotation = match physics.get_body(self.body_handle) {
            Some(body) => *body.rotation(),
            None => return,
        };
        // Ship forward is local -Z (nose points toward -Z in ship space).
        let forward = rotation * nalgebra::Vector3::new(0.0, 0.0, -1.0);
        let force = forward * force_magnitude;
        if let Some(body) = physics.get_body_mut(self.body_handle) {
            body.add_force(force, true);
        }
    }

    /// Apply RCS (lateral/vertical) thrust.
    /// `lateral`: -1.0 (left) to 1.0 (right)
    /// `vertical`: -1.0 (down) to 1.0 (up)
    /// `longitudinal`: -1.0 (backward) to 1.0 (forward) --- direct W/S when at helm
    pub fn apply_rcs(
        &self,
        physics: &mut PhysicsWorld,
        lateral: f32,
        vertical: f32,
        longitudinal: f32,
    ) {
        let lat = lateral.clamp(-1.0, 1.0);
        let vert = vertical.clamp(-1.0, 1.0);
        let longi = longitudinal.clamp(-1.0, 1.0);
        if lat.abs() < 0.01 && vert.abs() < 0.01 && longi.abs() < 0.01 {
            return;
        }

        let rotation = match physics.get_body(self.body_handle) {
            Some(body) => *body.rotation(),
            None => return,
        };
        let right = rotation * nalgebra::Vector3::new(1.0, 0.0, 0.0);
        let up = rotation * nalgebra::Vector3::new(0.0, 1.0, 0.0);
        let forward = rotation * nalgebra::Vector3::new(0.0, 0.0, -1.0);

        let force = right * lat * self.max_rcs_thrust
            + up * vert * self.max_rcs_thrust
            + forward * longi * self.max_rcs_thrust;
        if let Some(body) = physics.get_body_mut(self.body_handle) {
            body.add_force(force, true);
        }
    }

    /// Apply rotational torque.
    /// `pitch`: -1.0 (nose down) to 1.0 (nose up)
    /// `yaw`: -1.0 (nose left) to 1.0 (nose right)
    /// `roll`: -1.0 (roll left) to 1.0 (roll right)
    pub fn apply_rotation(
        &self,
        physics: &mut PhysicsWorld,
        pitch: f32,
        yaw: f32,
        roll: f32,
    ) {
        let p = pitch.clamp(-1.0, 1.0);
        let y = yaw.clamp(-1.0, 1.0);
        let r = roll.clamp(-1.0, 1.0);
        if p.abs() < 0.01 && y.abs() < 0.01 && r.abs() < 0.01 {
            return;
        }

        let rotation = match physics.get_body(self.body_handle) {
            Some(body) => *body.rotation(),
            None => return,
        };
        // Torque axes in ship local space:
        // pitch = rotation around local X
        // yaw = rotation around local Y
        // roll = rotation around local Z
        let local_torque = nalgebra::Vector3::new(
            p * self.max_torque,
            y * self.max_torque,
            r * self.max_torque,
        );
        let world_torque = rotation * local_torque;
        if let Some(body) = physics.get_body_mut(self.body_handle) {
            body.add_torque(world_torque, true);
        }
    }

    /// Get the ship's current position.
    pub fn position(&self, physics: &PhysicsWorld) -> Option<(f32, f32, f32)> {
        physics.get_body(self.body_handle).map(|b| {
            let t = b.translation();
            (t.x, t.y, t.z)
        })
    }

    /// Get the ship's current rotation as a quaternion (x, y, z, w).
    pub fn rotation(&self, physics: &PhysicsWorld) -> Option<(f32, f32, f32, f32)> {
        physics.get_body(self.body_handle).map(|b| {
            let r = b.rotation();
            (r.i, r.j, r.k, r.w)
        })
    }

    /// Get the ship's current linear velocity magnitude (m/s).
    pub fn speed(&self, physics: &PhysicsWorld) -> f32 {
        physics
            .get_body(self.body_handle)
            .map(|b| b.linvel().magnitude())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_g_world() -> PhysicsWorld {
        PhysicsWorld::with_gravity(0.0, 0.0, 0.0)
    }

    #[test]
    fn ship_creates_body() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        assert!(physics.get_body(ship.body_handle).is_some());
    }

    #[test]
    fn ship_has_correct_mass() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        let body = physics.get_body(ship.body_handle).unwrap();
        let mass = body.mass();
        // Mass should be ~50000 (additional_mass + collider mass)
        assert!(
            mass > 49_000.0,
            "ship mass should be ~50000, got {mass}"
        );
    }

    #[test]
    fn thrust_applies_force_when_engine_on() {
        let mut physics = zero_g_world();
        let mut ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        ship.engine_on = true;
        ship.throttle = 1.0;

        ship.apply_thrust(&mut physics);
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        let vel = body.linvel();
        // Ship forward is -Z, so velocity Z should be negative
        assert!(
            vel.z < 0.0,
            "ship should move in -Z direction, vel.z = {}",
            vel.z
        );
    }

    #[test]
    fn no_thrust_when_engine_off() {
        let mut physics = zero_g_world();
        let mut ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        ship.engine_on = false;
        ship.throttle = 1.0;

        ship.apply_thrust(&mut physics);
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        let speed = body.linvel().magnitude();
        assert!(
            speed < 0.001,
            "ship should not move with engine off, speed = {speed}"
        );
    }

    #[test]
    fn momentum_persists_no_drag() {
        let mut physics = zero_g_world();
        let mut ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        ship.engine_on = true;
        ship.throttle = 1.0;

        // Apply thrust for one frame
        ship.apply_thrust(&mut physics);
        physics.step(1.0 / 60.0);

        let speed_after_thrust = physics
            .get_body(ship.body_handle)
            .unwrap()
            .linvel()
            .magnitude();

        // Coast for 60 frames with no thrust
        ship.engine_on = false;
        ship.reset_forces(&mut physics);
        for _ in 0..60 {
            physics.step(1.0 / 60.0);
        }

        let speed_after_coast = physics
            .get_body(ship.body_handle)
            .unwrap()
            .linvel()
            .magnitude();

        assert!(
            (speed_after_coast - speed_after_thrust).abs() < 0.1,
            "momentum should persist: thrust={speed_after_thrust}, coast={speed_after_coast}"
        );
    }

    #[test]
    fn rcs_lateral_moves_sideways() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);

        ship.apply_rcs(&mut physics, 1.0, 0.0, 0.0); // right
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        assert!(
            body.linvel().x > 0.0,
            "ship should move right, vel.x = {}",
            body.linvel().x
        );
    }

    #[test]
    fn rotation_torque_spins_ship() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);

        ship.apply_rotation(&mut physics, 0.0, 1.0, 0.0); // yaw right
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        let angvel = body.angvel();
        assert!(
            angvel.magnitude() > 0.0,
            "ship should have angular velocity after torque"
        );
    }
}
