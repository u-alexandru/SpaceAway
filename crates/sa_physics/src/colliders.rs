use rapier3d::prelude::*;

use crate::bodies::{spawn_static_body};
use crate::world::PhysicsWorld;

/// Attaches a box (cuboid) collider to a rigid body. Restitution 0.2.
pub fn attach_box_collider(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    hx: f32,
    hy: f32,
    hz: f32,
) -> ColliderHandle {
    let collider = ColliderBuilder::cuboid(hx, hy, hz)
        .restitution(0.2)
        .build();
    world.add_collider(collider, body)
}

/// Attaches a sphere (ball) collider to a rigid body. Restitution 0.2.
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

/// Attaches a capsule collider (Y-aligned) to a rigid body. Friction 0.5, restitution 0.0.
pub fn attach_capsule_collider(
    world: &mut PhysicsWorld,
    body: RigidBodyHandle,
    half_height: f32,
    radius: f32,
) -> ColliderHandle {
    let collider = ColliderBuilder::capsule_y(half_height, radius)
        .friction(0.5)
        .restitution(0.0)
        .build();
    world.add_collider(collider, body)
}

/// Adds a ground plane: a static body at height `y` with a large box collider (500x0.1x500).
pub fn add_ground(world: &mut PhysicsWorld, y: f32) -> (RigidBodyHandle, ColliderHandle) {
    let body_handle = spawn_static_body(world, 0.0, y, 0.0);
    let collider = ColliderBuilder::cuboid(500.0, 0.1, 500.0)
        .restitution(0.2)
        .build();
    let collider_handle = world.add_collider(collider, body_handle);
    (body_handle, collider_handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bodies::spawn_dynamic_body;

    #[test]
    fn add_box_collider() {
        let mut world = PhysicsWorld::new();
        let body = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        attach_box_collider(&mut world, body, 0.5, 0.5, 0.5);
    }

    #[test]
    fn add_sphere_collider() {
        let mut world = PhysicsWorld::new();
        let body = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        attach_sphere_collider(&mut world, body, 0.5);
    }

    #[test]
    fn floor_stops_falling_body() {
        let mut world = PhysicsWorld::new();
        // Ground at y=0
        add_ground(&mut world, 0.0);
        // Ball at y=5
        let ball = spawn_dynamic_body(&mut world, 0.0, 5.0, 0.0, 1.0);
        attach_sphere_collider(&mut world, ball, 0.5);

        for _ in 0..180 {
            world.step(1.0 / 60.0);
        }

        let (_, y, _) = crate::bodies::body_position(&world, ball).unwrap();
        // Ball should rest near ground (center at ~0.6 = ground top 0.1 + radius 0.5)
        assert!(
            y < 2.0 && y > -1.0,
            "ball should rest near the ground, got y = {y}"
        );
    }

    #[test]
    fn add_ground_plane() {
        let mut world = PhysicsWorld::new();
        add_ground(&mut world, 0.0);
    }
}
