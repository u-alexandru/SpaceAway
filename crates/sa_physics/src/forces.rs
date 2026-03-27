use rapier3d::prelude::*;

use crate::world::PhysicsWorld;

/// Applies a continuous force to a rigid body (wake = true).
pub fn apply_force(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    fx: f32,
    fy: f32,
    fz: f32,
) {
    if let Some(body) = world.get_body_mut(handle) {
        body.add_force(nalgebra::Vector3::new(fx, fy, fz), true);
    }
}

/// Applies an instantaneous impulse to a rigid body.
pub fn apply_impulse(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    ix: f32,
    iy: f32,
    iz: f32,
) {
    if let Some(body) = world.get_body_mut(handle) {
        body.apply_impulse(nalgebra::Vector3::new(ix, iy, iz), true);
    }
}

/// Applies a continuous torque to a rigid body.
pub fn apply_torque(
    world: &mut PhysicsWorld,
    handle: RigidBodyHandle,
    tx: f32,
    ty: f32,
    tz: f32,
) {
    if let Some(body) = world.get_body_mut(handle) {
        body.add_torque(nalgebra::Vector3::new(tx, ty, tz), true);
    }
}

/// Returns the linear velocity of a rigid body as (x, y, z), or None if the handle is invalid.
pub fn linear_velocity(world: &PhysicsWorld, handle: RigidBodyHandle) -> Option<(f32, f32, f32)> {
    world.get_body(handle).map(|b| {
        let v = b.linvel();
        (v.x, v.y, v.z)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bodies::spawn_dynamic_body;
    use crate::colliders::attach_sphere_collider;

    #[test]
    fn apply_force_changes_velocity() {
        let mut world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let h = spawn_dynamic_body(&mut world, 0.0, 0.0, 0.0, 1.0);
        attach_sphere_collider(&mut world, h, 0.5);
        apply_force(&mut world, h, 100.0, 0.0, 0.0);
        world.step(1.0 / 60.0);
        let (vx, _, _) = linear_velocity(&world, h).unwrap();
        assert!(vx > 0.0, "velocity x should be positive after +X force, got {vx}");
    }

    #[test]
    fn no_drag_momentum_persists() {
        let mut world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let h = spawn_dynamic_body(&mut world, 0.0, 0.0, 0.0, 1.0);
        attach_sphere_collider(&mut world, h, 0.5);

        // Apply impulse for one instant
        apply_impulse(&mut world, h, 10.0, 0.0, 0.0);
        world.step(1.0 / 60.0);
        let (vx_after_impulse, _, _) = linear_velocity(&world, h).unwrap();

        // Coast for 60 more frames with no forces
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let (vx_after_coast, _, _) = linear_velocity(&world, h).unwrap();

        // Velocity should be essentially unchanged (Newtonian: no drag)
        assert!(
            (vx_after_coast - vx_after_impulse).abs() < 0.01,
            "momentum should persist: impulse vel = {vx_after_impulse}, coast vel = {vx_after_coast}"
        );
    }

    #[test]
    fn apply_torque_changes_angular_velocity() {
        let mut world = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let h = spawn_dynamic_body(&mut world, 0.0, 0.0, 0.0, 1.0);
        attach_sphere_collider(&mut world, h, 0.5);
        apply_torque(&mut world, h, 0.0, 100.0, 0.0);
        world.step(1.0 / 60.0);

        let body = world.get_body(h).unwrap();
        let angvel = body.angvel();
        assert!(
            angvel.y.abs() > 0.0,
            "angular velocity y should be non-zero after torque, got {}",
            angvel.y
        );
    }
}
