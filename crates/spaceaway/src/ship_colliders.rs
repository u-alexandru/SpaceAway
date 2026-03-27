//! Ship interior colliders — simple and reliable.
//!
//! Uses a small number of static box colliders that approximate the ship
//! interior. Prioritizes reliability over hex-precision. The player walks
//! on a flat floor and is bounded by walls — we don't need per-face hex
//! colliders, just enough to keep the player inside.

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;

const DOOR_W: f32 = 1.2;
const DOOR_H: f32 = 2.0;
const FLOOR_Y: f32 = -1.0;

/// Add a single static box collider to the physics world.
fn add_static_box(
    physics: &mut PhysicsWorld,
    x: f32, y: f32, z: f32,
    hx: f32, hy: f32, hz: f32,
) {
    let body = RigidBodyBuilder::fixed()
        .translation(nalgebra::Vector3::new(x, y, z))
        .build();
    let handle = physics.add_rigid_body(body);
    let collider = ColliderBuilder::cuboid(hx, hy, hz)
        .friction(0.5)
        .restitution(0.0)
        .build();
    physics.add_collider(collider, handle);
}

/// Build all ship interior colliders.
///
/// Ship spans z=0 (cockpit front) to z=29 (engine back).
/// We use simple box colliders: floor, ceiling, left/right walls,
/// front/back endcaps, and bulkheads at section boundaries with
/// door openings.
pub fn build_ship_colliders(physics: &mut PhysicsWorld) {
    let ship_len = 29.0;
    let ship_center_z = ship_len / 2.0; // 14.5

    // Floor: spans entire ship length
    // y=-1.1 (just below floor mesh at y=-1.0), thin
    add_static_box(physics, 0.0, FLOOR_Y - 0.1, ship_center_z, 2.5, 0.1, ship_len / 2.0);

    // Ceiling: spans entire ship length
    add_static_box(physics, 0.0, 1.3, ship_center_z, 2.5, 0.1, ship_len / 2.0);

    // Left wall: full length, from floor to ceiling
    add_static_box(physics, -2.0, 0.1, ship_center_z, 0.1, 1.2, ship_len / 2.0);

    // Right wall: full length
    add_static_box(physics, 2.0, 0.1, ship_center_z, 0.1, 1.2, ship_len / 2.0);

    // Front endcap (cockpit nose)
    add_static_box(physics, 0.0, 0.1, -0.1, 2.5, 1.5, 0.1);

    // Back endcap (engine tail)
    add_static_box(physics, 0.0, 0.1, ship_len + 0.1, 2.5, 1.5, 0.1);

    // Bulkheads at section boundaries WITH door openings.
    // Each bulkhead = 2 colliders (left of door + right of door) + 1 lintel above door.
    let bulkhead_zs = [4.0, 7.0, 8.0, 13.0, 14.0, 17.0, 18.0, 23.0, 24.0];
    let hdw = DOOR_W / 2.0; // 0.6

    for &bz in &bulkhead_zs {
        // Left of door: from left wall (-2.0) to door edge (-0.6)
        let left_center_x = (-2.0 + (-hdw)) / 2.0; // -1.3
        let left_half_w = (2.0 - hdw) / 2.0; // 0.7
        add_static_box(physics, left_center_x, 0.1, bz, left_half_w, 1.2, 0.05);

        // Right of door: from door edge (+0.6) to right wall (+2.0)
        let right_center_x = (hdw + 2.0) / 2.0; // 1.3
        add_static_box(physics, right_center_x, 0.1, bz, left_half_w, 1.2, 0.05);

        // Lintel above door: spans door width, from door top to ceiling
        let door_top_y = FLOOR_Y + DOOR_H; // 1.0
        let lintel_center_y = (door_top_y + 1.3) / 2.0; // 1.15
        let lintel_half_h = (1.3 - door_top_y) / 2.0; // 0.15
        if lintel_half_h > 0.01 {
            add_static_box(physics, 0.0, lintel_center_y, bz, hdw, lintel_half_h, 0.05);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_colliders_does_not_panic() {
        let mut physics = PhysicsWorld::new();
        build_ship_colliders(&mut physics);
        // Should have a reasonable number of colliders (not 142!)
        assert!(physics.collider_set.len() < 50, "Too many colliders: {}", physics.collider_set.len());
        assert!(physics.collider_set.len() > 10, "Too few colliders: {}", physics.collider_set.len());
    }
}
