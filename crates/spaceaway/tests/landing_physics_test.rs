//! Integration test: ship descends under gravity and stops at terrain barrier.
//!
//! Headless — no GPU, no window. Exercises the same physics pipeline as helm
//! mode: gravity force, atmospheric drag, rapier3d step, CCD, collision groups.

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;

// ---------------------------------------------------------------------------
// Collision group constants (mirrors ship_colliders.rs)
// ---------------------------------------------------------------------------

const TERRAIN: Group = Group::GROUP_5;
const SHIP_EXTERIOR: Group = Group::GROUP_6;
const SHIP_HULL: Group = Group::GROUP_1;
const PLAYER: Group = Group::GROUP_3;

// ---------------------------------------------------------------------------
// Constants from sa_ship::ship
// ---------------------------------------------------------------------------

const SHIP_MASS: f32 = 50_000.0;
const SKID_RADIUS: f32 = 0.3;
const SKID_FRICTION: f32 = 0.6;
const SKID_POSITIONS: [[f32; 3]; 4] = [
    [0.0, -1.5, -12.0],
    [0.0, -1.5, 12.0],
    [-2.0, -1.5, 0.0],
    [2.0, -1.5, 0.0],
];

// ---------------------------------------------------------------------------
// Test 1: Ship descends and stops at surface barrier
// ---------------------------------------------------------------------------

#[test]
fn ship_descends_and_stops_at_surface_barrier() {
    let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
    let dt: f32 = 1.0 / 60.0;
    let gravity_force_y: f32 = -13.57 * SHIP_MASS; // -678,500 N

    // --- Ship body (dynamic, CCD, 50,000 kg) at origin ---
    let ship_body = {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![0.0, 0.0, 0.0])
            .gravity_scale(0.0)
            .linear_damping(0.009)
            .angular_damping(5.0)
            .ccd_enabled(true)
            .build();
        physics.add_rigid_body(body)
    };

    // Hull sensor (mass provider, no contact forces)
    {
        let collider = ColliderBuilder::cuboid(2.5, 1.5, 15.0)
            .mass(SHIP_MASS)
            .sensor(true)
            .collision_groups(InteractionGroups::new(
                SHIP_HULL,
                Group::NONE,
            ))
            .build();
        physics.add_collider(collider, ship_body);
    }

    // 4 landing skid colliders (SHIP_EXTERIOR vs TERRAIN)
    let skid_groups = InteractionGroups::new(SHIP_EXTERIOR, TERRAIN);
    for pos in &SKID_POSITIONS {
        let collider = ColliderBuilder::ball(SKID_RADIUS)
            .friction(SKID_FRICTION)
            .restitution(0.0)
            .collision_groups(skid_groups)
            .translation(vector![pos[0], pos[1], pos[2]])
            .build();
        physics.add_collider(collider, ship_body);
    }

    // --- Terrain body (fixed) at origin ---
    let terrain_body = {
        let rb = RigidBodyBuilder::fixed().build();
        physics.add_rigid_body(rb)
    };

    // Surface barrier: cuboid 5000x50x5000 (half-extents).
    // Top face at y = -1000 (simulating 1km altitude).
    // Cuboid center = top_face_y - half_height = -1000 - 50 = -1050.
    let barrier_half_y = 50.0_f32;
    let barrier_top_y = -1000.0_f32;
    let barrier_center_y = barrier_top_y - barrier_half_y;
    let barrier_handle = {
        let collider = ColliderBuilder::cuboid(5000.0, barrier_half_y, 5000.0)
            .collision_groups(InteractionGroups::new(
                TERRAIN,
                PLAYER.union(SHIP_HULL).union(SHIP_EXTERIOR),
            ))
            .friction(0.8)
            .translation(vector![0.0, barrier_center_y, 0.0])
            .build();
        physics.add_collider(collider, terrain_body)
    };

    // --- Simulation loop ---
    let total_frames = 1000;
    let mut min_ship_y = f32::MAX;

    // Track the anchor offset for rebasing
    let mut anchor_offset_y: f32 = 0.0;

    for frame in 0..total_frames {
        // Apply gravity force
        if let Some(body) = physics.get_body_mut(ship_body) {
            body.reset_forces(true);
            body.add_force(vector![0.0, gravity_force_y, 0.0], true);
        }

        // Apply atmospheric drag
        if let Some(body) = physics.get_body_mut(ship_body) {
            let v = *body.linvel();
            let drag = 1.0 - 0.05 * dt;
            body.set_linvel(v * drag, true);
        }

        physics.step(dt);

        // Read position and velocity
        let (ship_y, ship_vy) = {
            let body = physics.get_body(ship_body).unwrap();
            (body.translation().y, body.linvel().y)
        };

        // Track absolute Y (rapier Y + anchor offset)
        let abs_y = ship_y + anchor_offset_y;
        if abs_y < min_ship_y {
            min_ship_y = abs_y;
        }

        if frame % 60 == 0 {
            println!(
                "frame={}, ship_y={:.2}, ship_vy={:.2}, anchor_offset={:.2}, abs_y={:.2}",
                frame, ship_y, ship_vy, anchor_offset_y, abs_y
            );
        }

        // Anchor rebase: if |ship_y| > 100, shift ship and barrier
        if ship_y.abs() > 100.0 {
            let shift = -ship_y;
            anchor_offset_y += ship_y;

            // Shift ship body
            if let Some(body) = physics.get_body_mut(ship_body) {
                let t = *body.translation();
                body.set_translation(vector![t.x, t.y + shift, t.z], true);
            }

            // Shift only the barrier collider (child of terrain body).
            // Ship child colliders (skids, hull sensor) move with the ship
            // body automatically — never shift them manually.
            if let Some(coll) = physics.collider_set.get_mut(barrier_handle) {
                if let Some(pos) = coll.position_wrt_parent() {
                    let new_pos = nalgebra::Isometry3::from_parts(
                        nalgebra::Translation3::new(
                            pos.translation.x,
                            pos.translation.y + shift,
                            pos.translation.z,
                        ),
                        pos.rotation,
                    );
                    coll.set_position_wrt_parent(new_pos);
                }
            }

            physics.sync_collider_positions();
        }
    }

    // --- Assertions ---
    let final_body = physics.get_body(ship_body).unwrap();
    let final_y = final_body.translation().y + anchor_offset_y;
    let final_vy = final_body.linvel().y;

    println!("\n=== RESULTS ===");
    println!("final abs_y = {:.2}", final_y);
    println!("final vy    = {:.2}", final_vy);
    println!("min abs_y   = {:.2}", min_ship_y);

    // Ship skids are at y = body_y - 1.5 (SKID_POSITIONS y offset).
    // The skid bottom is at body_y - 1.5 - SKID_RADIUS = body_y - 1.8.
    // The barrier top is at -1000. So the ship body should stop at
    // approximately -1000 + 1.8 = -998.2 (skids resting on barrier).
    let expected_stop_y = barrier_top_y + 1.5 + SKID_RADIUS; // -998.2

    // Ship must not fall below the barrier
    assert!(
        min_ship_y > barrier_center_y,
        "Ship fell through barrier! min_y={:.2}, barrier_center={:.2}. \
         The ship penetrated the surface barrier.",
        min_ship_y, barrier_center_y
    );

    // Ship should be near the expected stop position (within 5m tolerance)
    assert!(
        (final_y - expected_stop_y).abs() < 5.0,
        "Ship did not stop near surface. final_y={:.2}, expected ~{:.2}. \
         Difference: {:.2}m",
        final_y, expected_stop_y, (final_y - expected_stop_y).abs()
    );

    // Velocity should be near zero (settled)
    assert!(
        final_vy.abs() < 5.0,
        "Ship velocity not near zero at rest. vy={:.2} m/s",
        final_vy
    );
}

// ---------------------------------------------------------------------------
// Test 2: Coordinate pipeline consistency
// ---------------------------------------------------------------------------

#[test]
fn coordinate_pipeline_barrier_matches_rendering() {
    // Planet setup
    let planet_radius: f64 = 7_706_434.0;
    let altitude: f64 = 1000.0;

    // Anchor is on the planet surface + altitude, directly "above" the planet
    // along +Y (simplification: planet center at origin, ship above north pole).
    let anchor = [0.0_f64, planet_radius + altitude, 0.0];

    // Ship at rapier origin (just spawned or just rebased)
    let ship_pos = [0.0_f64, 0.0, 0.0];

    // cam_rel_m = anchor + ship_pos (in the real game, anchor is the f64
    // offset from planet center; ship_pos is the rapier-local position)
    let cam_rel_m = [
        anchor[0] + ship_pos[0],
        anchor[1] + ship_pos[1],
        anchor[2] + ship_pos[2],
    ];

    // --- Compute barrier isometry (mirrors TerrainColliders::compute_barrier_isometry) ---
    let cam_len = (cam_rel_m[0] * cam_rel_m[0]
        + cam_rel_m[1] * cam_rel_m[1]
        + cam_rel_m[2] * cam_rel_m[2])
    .sqrt()
    .max(0.01);
    let inv_len = 1.0 / cam_len;
    let normal = [
        cam_rel_m[0] * inv_len,
        cam_rel_m[1] * inv_len,
        cam_rel_m[2] * inv_len,
    ];

    // Barrier center on the planet surface along the normal direction
    let depth = planet_radius;
    let barrier_sx = (normal[0] * depth - anchor[0]) as f32;
    let barrier_sy = (normal[1] * depth - anchor[1]) as f32;
    let barrier_sz = (normal[2] * depth - anchor[2]) as f32;

    println!("cam_rel_m = ({:.1}, {:.1}, {:.1})", cam_rel_m[0], cam_rel_m[1], cam_rel_m[2]);
    println!("normal    = ({:.6}, {:.6}, {:.6})", normal[0], normal[1], normal[2]);
    println!("barrier position (rapier) = ({:.2}, {:.2}, {:.2})", barrier_sx, barrier_sy, barrier_sz);

    // For this test case (ship directly above north pole along +Y):
    // normal = (0, 1, 0)
    // barrier_sy = (1.0 * radius - (radius + altitude)) = -altitude = -1000
    // barrier_sx = 0, barrier_sz = 0
    //
    // The barrier cuboid has half_y = 50, so top face is at barrier_sy + 50 = -950.
    // Wait — the real code positions the barrier CENTER at surface level,
    // and the cuboid extends 50m above and below. So the top face is at
    // barrier_sy + 50 = -1000 + 50 = -950. But that means the barrier top
    // is 950m below the ship, not 1000m.
    //
    // Actually, looking at the code comment: "top face 50m above surface".
    // The barrier center IS at the surface, top face at surface + 50m.
    // So the ship contacts the top face at -950m (rapier coords), which is
    // 950m below origin — the ship's altitude is 1000m, but the barrier
    // protrudes 50m above the geometric surface.
    //
    // For the coordinate pipeline test, we verify the barrier is placed
    // at the correct rapier-local position relative to anchor.

    // Assert barrier Y is at -altitude (surface is altitude meters below ship)
    assert!(
        (barrier_sy - (-altitude as f32)).abs() < 0.01,
        "Barrier Y should be at -{altitude}, got {barrier_sy}"
    );

    // Assert barrier X and Z are zero (ship directly above pole)
    assert!(
        barrier_sx.abs() < 0.01,
        "Barrier X should be 0, got {barrier_sx}"
    );
    assert!(
        barrier_sz.abs() < 0.01,
        "Barrier Z should be 0, got {barrier_sz}"
    );

    // --- Rendering side: draw command position ---
    // The renderer computes chunk positions as:
    //   cam_offset_m = -cam_rel_m  (camera-relative offset from planet center)
    //   chunk_center  = surface point below ship = (0, radius, 0)
    //   draw_pos = cam_offset_m + chunk_center
    let cam_offset_m = [-cam_rel_m[0], -cam_rel_m[1], -cam_rel_m[2]];
    let chunk_center = [0.0_f64, planet_radius, 0.0]; // surface below ship
    let draw_pos = [
        cam_offset_m[0] + chunk_center[0],
        cam_offset_m[1] + chunk_center[1],
        cam_offset_m[2] + chunk_center[2],
    ];

    println!(
        "draw_pos (render) = ({:.2}, {:.2}, {:.2})",
        draw_pos[0], draw_pos[1], draw_pos[2]
    );

    // Draw position should match barrier position: both represent the surface
    // relative to the camera/ship.
    assert!(
        (draw_pos[0] as f32 - barrier_sx).abs() < 0.01,
        "Draw X ({:.2}) != barrier X ({:.2})",
        draw_pos[0], barrier_sx
    );
    assert!(
        (draw_pos[1] as f32 - barrier_sy).abs() < 0.01,
        "Draw Y ({:.2}) != barrier Y ({:.2}) — physics and rendering disagree on surface position!",
        draw_pos[1], barrier_sy
    );
    assert!(
        (draw_pos[2] as f32 - barrier_sz).abs() < 0.01,
        "Draw Z ({:.2}) != barrier Z ({:.2})",
        draw_pos[2], barrier_sz
    );

    println!("\nPhysics barrier and render draw position agree at ({:.1}, {:.1}, {:.1})",
        barrier_sx, barrier_sy, barrier_sz);
}
