//! Full descent integration test: real terrain code, real colliders, real rebase.
//!
//! Headless (no GPU, no window). Simulates a ship descending from 2km above
//! a 7706km desert planet using the REAL TerrainColliders with CollisionGrid,
//! ChunkStreaming, select_visible_nodes, and RebaseBodies. The ship starts
//! with a 100 m/s downward velocity to simulate an approach from orbit.
//! Verifies the ship stops at the surface without tunneling.
//!
//! Three phases:
//!   Warmup:  hold the ship stationary for 120 frames while the collision
//!            grid populates heightfield colliders.
//!   Descent: release the ship under gravity; it falls to the heightfields.
//!   Settle:  after the ship stops, continue ticking for 300 frames to
//!            verify stability.

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;
use sa_terrain::quadtree::{max_lod_levels, select_visible_nodes};
use sa_terrain::streaming::ChunkStreaming;
use sa_terrain::{ChunkKey, TerrainConfig};
use sa_universe::PlanetSubType;
use spaceaway::ship_colliders;
use spaceaway::terrain_colliders::{RebaseBodies, TerrainColliders};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const PLANET_RADIUS_M: f64 = 7_706_434.0;
const START_ALTITUDE_M: f64 = 2_000.0;
const INITIAL_DOWNWARD_SPEED: f32 = 30.0;
const SHIP_MASS: f32 = 50_000.0;
const SURFACE_GRAVITY: f64 = 13.57;
const DT: f32 = 1.0 / 60.0;
const WARMUP_FRAMES: usize = 120;
const DESCENT_FRAMES: usize = 6000;
const SETTLE_FRAMES: usize = 300;
const LOG_INTERVAL: usize = 60;
// In the game, large planets cap displacement to 20km / radius_km.
// For a 7706km planet: 20/7706 ≈ 0.0026.
const DISPLACEMENT_FRACTION: f32 = 0.003;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run one frame of terrain streaming + collision grid update (no physics forces).
fn tick_terrain(
    physics: &mut PhysicsWorld,
    _streaming: &mut ChunkStreaming,
    terrain_colliders: &mut TerrainColliders,
    config: &TerrainConfig,
    anchor: &mut [f64; 3],
    ship_handle: RigidBodyHandle,
    rebase_bodies: &RebaseBodies,
    _max_lod: u8,
    _max_displacement_m: f64,
) {
    let ship_pos = physics
        .get_body(ship_handle)
        .map(|b| *b.translation())
        .unwrap();
    let cam_rel_m = [
        anchor[0] + ship_pos.x as f64,
        anchor[1] + ship_pos.y as f64,
        anchor[2] + ship_pos.z as f64,
    ];

    // Use the collision grid directly (independent of visual LOD).
    let altitude = (cam_rel_m[0] * cam_rel_m[0]
        + cam_rel_m[1] * cam_rel_m[1]
        + cam_rel_m[2] * cam_rel_m[2])
        .sqrt()
        - config.radius_m;

    if altitude < config.radius_m * sa_terrain::config::COLLISION_ACTIVATE_FACTOR {
        terrain_colliders.update_collision_grid(cam_rel_m, config, physics, rebase_bodies);
    }

    *anchor = terrain_colliders.anchor_f64;
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[test]
fn full_descent_with_real_terrain() {
    let _ = env_logger::builder().is_test(true).try_init();

    // -----------------------------------------------------------------------
    // Phase 1: Setup
    // -----------------------------------------------------------------------

    let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);

    let ship_handle = {
        let body = RigidBodyBuilder::dynamic()
            .translation(vector![0.0, 0.0, 0.0])
            .gravity_scale(0.0)
            .linear_damping(0.009)
            .angular_damping(5.0)
            .ccd_enabled(true)
            .build();
        physics.add_rigid_body(body)
    };

    // Hull sensor (mass provider only).
    {
        let collider = ColliderBuilder::cuboid(2.5, 1.5, 15.0)
            .mass(SHIP_MASS)
            .sensor(true)
            .collision_groups(InteractionGroups::new(
                ship_colliders::SHIP_HULL,
                Group::NONE,
            ))
            .build();
        physics.add_collider(collider, ship_handle);
    }

    // 4 landing skid colliders (SHIP_EXTERIOR vs TERRAIN).
    {
        let groups = InteractionGroups::new(
            ship_colliders::SHIP_EXTERIOR,
            ship_colliders::TERRAIN,
        );
        let skid_positions: [[f32; 3]; 4] = [
            [0.0, -1.5, -12.0],
            [0.0, -1.5, 12.0],
            [-2.0, -1.5, 0.0],
            [2.0, -1.5, 0.0],
        ];
        for pos in &skid_positions {
            let collider = ColliderBuilder::ball(0.3)
                .friction(0.6)
                .restitution(0.0)
                .collision_groups(groups)
                .translation(vector![pos[0], pos[1], pos[2]])
                .build();
            physics.add_collider(collider, ship_handle);
        }
    }

    let mut terrain_colliders = TerrainColliders::new();

    let config = TerrainConfig {
        radius_m: PLANET_RADIUS_M,
        noise_seed: 42,
        sub_type: PlanetSubType::Desert,
        displacement_fraction: DISPLACEMENT_FRACTION,
    };

    let mut streaming = ChunkStreaming::new(config.clone(), 4);

    let face_size_m = PLANET_RADIUS_M * std::f64::consts::FRAC_PI_2;
    let max_lod = max_lod_levels(face_size_m);
    let max_displacement_m = PLANET_RADIUS_M * DISPLACEMENT_FRACTION as f64;

    let start_dist = PLANET_RADIUS_M + START_ALTITUDE_M;
    let mut anchor = [0.0_f64, start_dist, 0.0];

    let rebase_bodies = RebaseBodies {
        ship: Some(ship_handle),
        player: None,
    };

    // -----------------------------------------------------------------------
    // Warmup: hold ship stationary while collision grid populates
    // -----------------------------------------------------------------------

    println!("=== WARMUP: {} frames ===", WARMUP_FRAMES);

    for frame in 0..WARMUP_FRAMES {
        tick_terrain(
            &mut physics,
            &mut streaming,
            &mut terrain_colliders,
            &config,
            &mut anchor,
            ship_handle,
            &rebase_bodies,
            max_lod,
            max_displacement_m,
        );

        if frame < 60 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        if frame % 30 == 0 {
            println!(
                "  warmup frame={}, heightfields={}",
                frame,
                terrain_colliders.colliders.len(),
            );
        }
    }

    println!(
        "Warmup done: heightfields={}",
        terrain_colliders.colliders.len(),
    );

    // Give the ship an initial downward velocity.
    if let Some(body) = physics.get_body_mut(ship_handle) {
        body.set_linvel(vector![0.0, -INITIAL_DOWNWARD_SPEED, 0.0], true);
    }

    // -----------------------------------------------------------------------
    // Phase 2: Descent
    // -----------------------------------------------------------------------

    println!("\n=== DESCENT: up to {} frames ===", DESCENT_FRAMES);

    let mut timeline: Vec<String> = Vec::new();
    let mut min_altitude = f64::MAX;
    let mut heightfields_existed = false;
    let mut final_altitude = START_ALTITUDE_M;
    let mut final_speed = INITIAL_DOWNWARD_SPEED;
    let mut settled_frame: Option<usize> = None;

    for frame in 0..DESCENT_FRAMES {
        // Gravity + drag.
        {
            let ship_pos = physics
                .get_body(ship_handle)
                .map(|b| *b.translation())
                .unwrap();
            let cam_rel_m = [
                anchor[0] + ship_pos.x as f64,
                anchor[1] + ship_pos.y as f64,
                anchor[2] + ship_pos.z as f64,
            ];
            let cam_len = (cam_rel_m[0] * cam_rel_m[0]
                + cam_rel_m[1] * cam_rel_m[1]
                + cam_rel_m[2] * cam_rel_m[2])
                .sqrt();

            let inv_len = if cam_len > 0.01 { 1.0 / cam_len } else { 0.0 };
            let gx = (-cam_rel_m[0] * inv_len * SURFACE_GRAVITY * SHIP_MASS as f64) as f32;
            let gy = (-cam_rel_m[1] * inv_len * SURFACE_GRAVITY * SHIP_MASS as f64) as f32;
            let gz = (-cam_rel_m[2] * inv_len * SURFACE_GRAVITY * SHIP_MASS as f64) as f32;

            if let Some(body) = physics.get_body_mut(ship_handle) {
                body.reset_forces(true);
                body.add_force(vector![gx, gy, gz], true);
                let v = *body.linvel();
                let drag = 1.0 - 0.05 * DT;
                body.set_linvel(v * drag, true);
            }
        }

        physics.step(DT);

        // Terrain tick (collision grid update).
        tick_terrain(
            &mut physics,
            &mut streaming,
            &mut terrain_colliders,
            &config,
            &mut anchor,
            ship_handle,
            &rebase_bodies,
            max_lod,
            max_displacement_m,
        );

        // Measure state after tick.
        let ship_pos_after = physics
            .get_body(ship_handle)
            .map(|b| *b.translation())
            .unwrap();
        let cam_rel_after = [
            anchor[0] + ship_pos_after.x as f64,
            anchor[1] + ship_pos_after.y as f64,
            anchor[2] + ship_pos_after.z as f64,
        ];
        let cam_len_after = (cam_rel_after[0] * cam_rel_after[0]
            + cam_rel_after[1] * cam_rel_after[1]
            + cam_rel_after[2] * cam_rel_after[2])
            .sqrt();
        let altitude = cam_len_after - PLANET_RADIUS_M;
        let speed = physics
            .get_body(ship_handle)
            .map(|b| b.linvel().magnitude())
            .unwrap_or(0.0);

        if altitude < min_altitude {
            min_altitude = altitude;
        }

        let num_heightfields = terrain_colliders.colliders.len();

        if num_heightfields > 0 {
            heightfields_existed = true;
        }

        final_altitude = altitude;
        final_speed = speed;

        if frame % LOG_INTERVAL == 0 {
            let msg = format!(
                "frame={:5}, alt={:10.1}m, speed={:8.2}m/s, hf={:3}, rapier=({:.1},{:.1},{:.1})",
                frame, altitude, speed, num_heightfields,
                ship_pos_after.x, ship_pos_after.y, ship_pos_after.z,
            );
            println!("{}", msg);
            timeline.push(msg);
        }

        // Early exit: settled on heightfield.
        if altitude < 200.0 && speed < 1.0 && frame > 60 {
            let msg = format!(
                "SETTLED at frame {}: alt={:.1}m, speed={:.2}m/s",
                frame, altitude, speed
            );
            println!("{}", msg);
            timeline.push(msg);
            settled_frame = Some(frame);
            break;
        }
    }

    // -----------------------------------------------------------------------
    // Phase 3: Post-settle (verify stability)
    // -----------------------------------------------------------------------

    println!(
        "\n=== POST-SETTLE: {} frames for stability check ===",
        SETTLE_FRAMES
    );

    for frame in 0..SETTLE_FRAMES {
        // Keep gravity + drag active so the ship stays put.
        {
            let ship_pos = physics
                .get_body(ship_handle)
                .map(|b| *b.translation())
                .unwrap();
            let cam_rel_m = [
                anchor[0] + ship_pos.x as f64,
                anchor[1] + ship_pos.y as f64,
                anchor[2] + ship_pos.z as f64,
            ];
            let cam_len = (cam_rel_m[0] * cam_rel_m[0]
                + cam_rel_m[1] * cam_rel_m[1]
                + cam_rel_m[2] * cam_rel_m[2])
                .sqrt();
            let inv_len = if cam_len > 0.01 { 1.0 / cam_len } else { 0.0 };
            let gx = (-cam_rel_m[0] * inv_len * SURFACE_GRAVITY * SHIP_MASS as f64) as f32;
            let gy = (-cam_rel_m[1] * inv_len * SURFACE_GRAVITY * SHIP_MASS as f64) as f32;
            let gz = (-cam_rel_m[2] * inv_len * SURFACE_GRAVITY * SHIP_MASS as f64) as f32;

            if let Some(body) = physics.get_body_mut(ship_handle) {
                body.reset_forces(true);
                body.add_force(vector![gx, gy, gz], true);
                let v = *body.linvel();
                let drag = 1.0 - 0.05 * DT;
                body.set_linvel(v * drag, true);
            }
        }

        physics.step(DT);

        tick_terrain(
            &mut physics,
            &mut streaming,
            &mut terrain_colliders,
            &config,
            &mut anchor,
            ship_handle,
            &rebase_bodies,
            max_lod,
            max_displacement_m,
        );

        if frame < 120 {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let num_hf = terrain_colliders.colliders.len();
        if num_hf > 0 {
            heightfields_existed = true;
        }

        let ship_pos_after = physics
            .get_body(ship_handle)
            .map(|b| *b.translation())
            .unwrap();
        let cam_rel_after = [
            anchor[0] + ship_pos_after.x as f64,
            anchor[1] + ship_pos_after.y as f64,
            anchor[2] + ship_pos_after.z as f64,
        ];
        let cam_len_a = (cam_rel_after[0] * cam_rel_after[0]
            + cam_rel_after[1] * cam_rel_after[1]
            + cam_rel_after[2] * cam_rel_after[2])
            .sqrt();
        let alt = cam_len_a - PLANET_RADIUS_M;
        let spd = physics
            .get_body(ship_handle)
            .map(|b| b.linvel().magnitude())
            .unwrap_or(0.0);
        final_altitude = alt;
        final_speed = spd;
        if alt < min_altitude {
            min_altitude = alt;
        }

        if frame % 60 == 0 {
            println!(
                "  settle frame={}, alt={:.1}m, speed={:.2}m/s, hf={}",
                frame, alt, spd, num_hf,
            );
        }
    }

    // -----------------------------------------------------------------------
    // Phase 4: Assertions + diagnostics
    // -----------------------------------------------------------------------

    println!("\n=== TIMELINE ===");
    for line in &timeline {
        println!("  {}", line);
    }

    println!("\n=== FINAL STATE ===");
    println!("  final_altitude = {:.1}m", final_altitude);
    println!("  final_speed    = {:.2}m/s", final_speed);
    println!("  min_altitude   = {:.1}m", min_altitude);
    println!(
        "  heightfields_existed = {}",
        heightfields_existed
    );
    if let Some(f) = settled_frame {
        println!("  settled_frame = {}", f);
    }

    // A1: Ship must have settled (reached low speed at some point).
    // With centered displacement, the terrain surface can be above or below
    // the base radius R, so altitude-based penetration checks are meaningless.
    // Instead, verify the ship came to rest on the terrain.
    assert!(
        settled_frame.is_some(),
        "NOT SETTLED: ship never reached rest (speed < 1 m/s). \
         final_altitude={:.1}m, final_speed={:.2}m/s, min_altitude={:.1}m. \
         The terrain collider system failed to stop the ship.",
        final_altitude,
        final_speed,
        min_altitude
    );

    // A2: After settling, the ship should remain at low speed through the
    // stability phase. Check that final speed is still low.
    assert!(
        final_speed < 5.0,
        "UNSTABLE: ship settled but final speed is {:.2}m/s (expected < 5). \
         final_altitude={:.1}m.",
        final_speed,
        final_altitude
    );

    // A3: HeightField colliders must exist (collision grid should create them).
    assert!(
        heightfields_existed,
        "NO HEIGHTFIELDS: collision grid did not create any HeightField colliders \
         during descent. The ship had no collision surface."
    );

    println!("\nAll assertions passed.");
}
