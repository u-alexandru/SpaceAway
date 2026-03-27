use rapier3d::prelude::*;

use crate::world::PhysicsWorld;

/// Spawns a dynamic rigid body at the given position with additional mass.
pub fn spawn_dynamic_body(
    world: &mut PhysicsWorld,
    x: f32,
    y: f32,
    z: f32,
    mass: f32,
) -> RigidBodyHandle {
    let body = RigidBodyBuilder::dynamic()
        .translation(nalgebra::Vector3::new(x, y, z))
        .additional_mass(mass)
        .build();
    world.add_rigid_body(body)
}

/// Spawns a static (fixed) rigid body at the given position.
pub fn spawn_static_body(
    world: &mut PhysicsWorld,
    x: f32,
    y: f32,
    z: f32,
) -> RigidBodyHandle {
    let body = RigidBodyBuilder::fixed()
        .translation(nalgebra::Vector3::new(x, y, z))
        .build();
    world.add_rigid_body(body)
}

/// Spawns a kinematic position-based rigid body at the given position.
pub fn spawn_kinematic_body(
    world: &mut PhysicsWorld,
    x: f32,
    y: f32,
    z: f32,
) -> RigidBodyHandle {
    let body = RigidBodyBuilder::kinematic_position_based()
        .translation(nalgebra::Vector3::new(x, y, z))
        .build();
    world.add_rigid_body(body)
}

/// Returns the position of a rigid body as (x, y, z), or None if the handle is invalid.
pub fn body_position(world: &PhysicsWorld, handle: RigidBodyHandle) -> Option<(f32, f32, f32)> {
    world.get_body(handle).map(|b| {
        let t = b.translation();
        (t.x, t.y, t.z)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_dynamic_body() {
        let mut world = PhysicsWorld::new();
        let h = spawn_dynamic_body(&mut world, 0.0, 10.0, 0.0, 1.0);
        let body = world.get_body(h).unwrap();
        assert!(body.is_dynamic());
    }

    #[test]
    fn create_static_body() {
        let mut world = PhysicsWorld::new();
        let h = spawn_static_body(&mut world, 0.0, 0.0, 0.0);
        let body = world.get_body(h).unwrap();
        assert!(body.is_fixed());
    }

    #[test]
    fn dynamic_body_falls_with_gravity() {
        let mut world = PhysicsWorld::new();
        let h = spawn_dynamic_body(&mut world, 0.0, 10.0, 0.0, 1.0);
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let (_, y, _) = body_position(&world, h).unwrap();
        assert!(y < 10.0, "body should have fallen, y = {y}");
    }

    #[test]
    fn kinematic_body_does_not_fall() {
        let mut world = PhysicsWorld::new();
        let h = spawn_kinematic_body(&mut world, 0.0, 5.0, 0.0);
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let (_, y, _) = body_position(&world, h).unwrap();
        assert!(
            (y - 5.0).abs() < 0.001,
            "kinematic body should stay at 5.0, got {y}"
        );
    }
}
