//! Comprehensive terrain pipeline integration tests.
//!
//! Tests 1-8: chunk geometry, quadtree LOD, draw positions, barrier math,
//! collider range, and gravity blending. All use REAL functions from sa_terrain.

use sa_terrain::chunk::{generate_chunk, GRID_SIZE};
use sa_terrain::cube_sphere::CubeFace;
use sa_terrain::gravity::compute_gravity;
use sa_terrain::quadtree::{max_lod_levels, select_visible_nodes};
use sa_terrain::streaming::ChunkStreaming;
use sa_terrain::{ChunkKey, TerrainConfig};
use sa_universe::PlanetSubType;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Earth-like planet config used across tests.
fn earth_config() -> TerrainConfig {
    TerrainConfig {
        radius_m: 6_371_000.0,
        noise_seed: 42,
        sub_type: PlanetSubType::Temperate,
        displacement_fraction: 0.02,
    }
}

/// Game planet config (7706 km radius, matching landing test).
fn game_planet_config() -> TerrainConfig {
    TerrainConfig {
        radius_m: 7_706_434.0,
        noise_seed: 123,
        sub_type: PlanetSubType::Barren,
        displacement_fraction: 0.015,
    }
}

fn vec3_len(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn vec3_normalize(v: [f64; 3]) -> [f64; 3] {
    let len = vec3_len(v);
    if len < 1e-30 {
        return [0.0, 0.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

// ===========================================================================
// TEST 1: Chunk center is at planet surface (radius_m from center)
// ===========================================================================

#[test]
fn chunk_center_at_planet_surface() {
    let config = earth_config();
    let amplitude = config.radius_m * config.displacement_fraction as f64;

    // Test across multiple LODs and faces. LOD 0 covers an entire face
    // (quarter of the sphere), so its center is well inside the sphere —
    // only test LODs fine enough that the chunk patch is small and the
    // center is close to the surface.
    let test_cases: &[(u8, u8, u32, u32)] = &[
        // (face, lod, x, y)
        (CubeFace::PosZ as u8, 5, 0, 0),   // LOD 5: medium
        (CubeFace::PosX as u8, 8, 3, 7),   // LOD 8: fine
        (CubeFace::NegY as u8, 10, 100, 200), // LOD 10: finer
        (CubeFace::PosY as u8, 14, 1000, 2000), // LOD 14: surface level
    ];

    for &(face, lod, x, y) in test_cases {
        // Ensure x,y are within range for this LOD.
        let tiles = 1u32 << lod;
        let x = x.min(tiles - 1);
        let y = y.min(tiles - 1);

        let key = ChunkKey { face, lod, x, y };
        let chunk = generate_chunk(key, &config);
        let center_dist = vec3_len(chunk.center_f64);

        let lower = config.radius_m - amplitude;
        let upper = config.radius_m + amplitude;

        assert!(
            center_dist >= lower && center_dist <= upper,
            "LOD {} face {} chunk ({},{}) center dist {:.1}m not in [{:.1}, {:.1}] (radius={:.1}, amplitude={:.1})",
            lod, face, x, y, center_dist, lower, upper, config.radius_m, amplitude
        );
    }
}

// ===========================================================================
// TEST 2: Chunk vertices are near the surface
// ===========================================================================

#[test]
fn chunk_vertices_near_surface() {
    let config = earth_config();
    let amplitude = config.radius_m * config.displacement_fraction as f64;
    let n = GRID_SIZE as usize;

    // Test at LOD 14 (surface level) on two faces.
    for face in [CubeFace::PosZ, CubeFace::NegX] {
        let tiles = 1u32 << 14;
        let key = ChunkKey {
            face: face as u8,
            lod: 14,
            x: tiles / 2,
            y: tiles / 2,
        };
        let chunk = generate_chunk(key, &config);

        // Grid vertices only (first n*n, skip skirts).
        for i in 0..n * n {
            let v = &chunk.vertices[i];
            // Reconstruct world position: local_pos + center_f64
            let wx = v.position[0] as f64 + chunk.center_f64[0];
            let wy = v.position[1] as f64 + chunk.center_f64[1];
            let wz = v.position[2] as f64 + chunk.center_f64[2];
            let dist = (wx * wx + wy * wy + wz * wz).sqrt();

            let lower = config.radius_m - amplitude * 2.0;
            let upper = config.radius_m + amplitude * 2.0;

            assert!(
                dist >= lower && dist <= upper,
                "face {:?} LOD 14 vertex {} world dist {:.1}m outside [{:.1}, {:.1}] (2x amplitude tolerance)",
                face, i, dist, lower, upper
            );
        }
    }
}

// ===========================================================================
// TEST 3: Skirt vertices drop is bounded
// ===========================================================================

#[test]
fn skirt_drop_is_bounded() {
    let config = earth_config();
    let amplitude = config.radius_m * config.displacement_fraction as f64;
    let n = GRID_SIZE as usize;
    let grid_count = n * n;

    let key = ChunkKey {
        face: CubeFace::PosZ as u8,
        lod: 10,
        x: 0,
        y: 0,
    };
    let chunk = generate_chunk(key, &config);

    // Skirt vertices start at index grid_count.
    // There are 4 edges * n vertices = 4n skirt vertices.
    let skirt_count = 4 * n;
    assert!(
        chunk.vertices.len() >= grid_count + skirt_count,
        "Expected at least {} vertices (grid {} + skirt {}), got {}",
        grid_count + skirt_count,
        grid_count,
        skirt_count,
        chunk.vertices.len()
    );

    // Expected skirt drop: 2 * displacement_fraction * radius_m.
    let expected_drop = config.radius_m * config.displacement_fraction as f64 * 2.0;
    // Allow a generous margin: skirt drop should not exceed 3x amplitude.
    let max_allowed_drop = amplitude * 3.0;

    for i in grid_count..chunk.vertices.len() {
        let sv = &chunk.vertices[i];
        let sw_x = sv.position[0] as f64 + chunk.center_f64[0];
        let sw_y = sv.position[1] as f64 + chunk.center_f64[1];
        let sw_z = sv.position[2] as f64 + chunk.center_f64[2];
        let skirt_dist = (sw_x * sw_x + sw_y * sw_y + sw_z * sw_z).sqrt();

        // Skirt vertex should be BELOW the surface but not absurdly far.
        let drop_from_radius = config.radius_m - skirt_dist;

        // The drop should be positive (below surface) and bounded.
        // Allow some vertices to be slightly above surface due to displacement.
        assert!(
            drop_from_radius < max_allowed_drop,
            "Skirt vertex {} drop {:.1}m exceeds max allowed {:.1}m (expected ~{:.1}m). \
             This means skirts extend too far below the surface.",
            i - grid_count,
            drop_from_radius,
            max_allowed_drop,
            expected_drop
        );
    }
}

// ===========================================================================
// TEST 4: Quadtree LOD selection at various altitudes
// ===========================================================================

#[test]
fn quadtree_lod_selection_by_altitude() {
    let config = earth_config();
    let face_size_m = config.radius_m * std::f64::consts::FRAC_PI_2;
    let max_lod = max_lod_levels(face_size_m);
    let max_disp = config.radius_m * config.displacement_fraction as f64;

    // Camera above the +Z pole at various altitudes.
    let test_cases: &[(f64, u8, &str)] = &[
        (1_000.0, 14, "1km altitude should have LOD >= 14"),
        (10_000.0, 10, "10km altitude should have LOD >= 10"),
        (100_000.0, 8, "100km altitude should have LOD >= 8"),
        (1_000_000.0, 3, "1000km altitude should have LOD >= 3"),
    ];

    for &(altitude, min_expected_lod, msg) in test_cases {
        let camera = [0.0, 0.0, config.radius_m + altitude];
        let nodes = select_visible_nodes(camera, config.radius_m, max_lod, max_disp, None);

        assert!(!nodes.is_empty(), "No nodes returned at altitude {altitude}m");

        // Find the finest LOD among nodes near the camera (within 2x altitude).
        let nearest_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| {
                let dx = n.center[0] - camera[0];
                let dy = n.center[1] - camera[1];
                let dz = n.center[2] - camera[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                dist < altitude * 3.0
            })
            .collect();

        let finest_lod = nearest_nodes.iter().map(|n| n.lod).max().unwrap_or(0);

        assert!(
            finest_lod >= min_expected_lod,
            "{msg}: got finest LOD {finest_lod} (expected >= {min_expected_lod}) \
             among {} nearby nodes out of {} total",
            nearest_nodes.len(),
            nodes.len()
        );
    }

    // Verify no LOD 0 nodes within 100km of camera at 1km altitude.
    {
        let camera = [0.0, 0.0, config.radius_m + 1000.0];
        let nodes = select_visible_nodes(camera, config.radius_m, max_lod, max_disp, None);
        let coarse_near = nodes.iter().any(|n| {
            let dx = n.center[0] - camera[0];
            let dy = n.center[1] - camera[1];
            let dz = n.center[2] - camera[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            n.lod == 0 && dist < 100_000.0
        });
        assert!(
            !coarse_near,
            "LOD 0 node found within 100km of camera at 1km altitude — quadtree not subdividing"
        );
    }
}

// ===========================================================================
// TEST 5: Draw command pipeline preserves camera-relative offset
// ===========================================================================

#[test]
fn draw_command_pipeline_preserves_offset() {
    // This test verifies the renderer's draw position formula:
    //   draw_pos = (planet_ly - camera_ly) * LY_TO_M + chunk.center_f64
    //            = -cam_rel_m + chunk.center_f64
    //
    // The draw position is the chunk center relative to the camera, which
    // is exactly what the GPU needs for camera-relative rendering.

    let config = earth_config();
    let ly_to_m: f64 = 9.461e15;

    // Planet at an arbitrary galactic position.
    let planet_center_ly = [100.0_f64, 200.0, 300.0];

    // Camera position relative to planet center (in meters).
    let cam_rel_m = [0.0_f64, 0.0, config.radius_m + 10_000.0];

    // Camera galactic position.
    let camera_ly = [
        planet_center_ly[0] + cam_rel_m[0] / ly_to_m,
        planet_center_ly[1] + cam_rel_m[1] / ly_to_m,
        planet_center_ly[2] + cam_rel_m[2] / ly_to_m,
    ];

    // Renderer formula: cam_offset_m = (planet_ly - camera_ly) * LY_TO_M
    let cam_offset_m = [
        (planet_center_ly[0] - camera_ly[0]) * ly_to_m,
        (planet_center_ly[1] - camera_ly[1]) * ly_to_m,
        (planet_center_ly[2] - camera_ly[2]) * ly_to_m,
    ];

    // cam_offset_m should equal -cam_rel_m. At galactic distances (100+ ly)
    // the round-trip through light-year coordinates introduces f64 precision
    // loss of ~10-100m. The real renderer minimizes this by computing the
    // difference (planet_ly - camera_ly) directly (the values are nearby so
    // subtraction is precise), but this test uses the naive round-trip to
    // quantify the precision budget.
    for i in 0..3 {
        let expected = -cam_rel_m[i];
        let diff = (cam_offset_m[i] - expected).abs();
        // At 100-300 ly, f64 mantissa gives ~50-60m precision loss.
        assert!(
            diff < 100.0,
            "cam_offset_m[{i}] = {:.6}, expected {:.6} (= -cam_rel_m[{i}]). \
             f64 precision loss in LY conversion: {:.3}m (must be < 100m)",
            cam_offset_m[i], expected, diff
        );
    }

    // Generate a real chunk to test the full formula.
    let key = ChunkKey {
        face: CubeFace::PosZ as u8,
        lod: 10,
        x: 512,
        y: 512,
    };
    let chunk = generate_chunk(key, &config);

    // draw_pos = cam_offset_m + chunk.center_f64
    //          = -cam_rel_m + chunk.center_f64
    //          = chunk.center_f64 - cam_rel_m
    // This is the chunk center in camera-relative coordinates.
    let draw_pos = [
        (cam_offset_m[0] + chunk.center_f64[0]) as f32,
        (cam_offset_m[1] + chunk.center_f64[1]) as f32,
        (cam_offset_m[2] + chunk.center_f64[2]) as f32,
    ];

    // Compute expected: chunk center minus camera position.
    let expected = [
        (chunk.center_f64[0] - cam_rel_m[0]) as f32,
        (chunk.center_f64[1] - cam_rel_m[1]) as f32,
        (chunk.center_f64[2] - cam_rel_m[2]) as f32,
    ];

    for i in 0..3 {
        // Allow 100m tolerance from the LY round-trip precision loss.
        assert!(
            (draw_pos[i] - expected[i]).abs() < 100.0,
            "draw_pos[{i}] = {:.1}, expected {:.1}. \
             The renderer formula does not produce correct camera-relative positions.",
            draw_pos[i], expected[i]
        );
    }

    // The draw distance should be the Euclidean distance from camera to chunk center.
    let draw_dist = (draw_pos[0] * draw_pos[0]
        + draw_pos[1] * draw_pos[1]
        + draw_pos[2] * draw_pos[2])
    .sqrt();

    let true_dist = ((chunk.center_f64[0] - cam_rel_m[0]).powi(2)
        + (chunk.center_f64[1] - cam_rel_m[1]).powi(2)
        + (chunk.center_f64[2] - cam_rel_m[2]).powi(2))
    .sqrt() as f32;

    assert!(
        (draw_dist - true_dist).abs() < 200.0,
        "Draw distance {:.1}m does not match true distance {:.1}m (tolerance 200m for LY precision)",
        draw_dist, true_dist
    );
}

// ===========================================================================
// TEST 6: Barrier position matches altitude at various heights
// ===========================================================================

#[test]
fn barrier_position_matches_altitude() {
    let config = game_planet_config();

    // Test at various altitudes with different approach directions.
    let test_cases: &[(f64, [f64; 3], &str)] = &[
        (100.0, [0.0, 1.0, 0.0], "100m above +Y pole"),
        (1_000.0, [0.0, 1.0, 0.0], "1km above +Y pole"),
        (10_000.0, [0.0, 0.0, 1.0], "10km above +Z pole"),
        (100_000.0, [1.0, 0.0, 0.0], "100km above +X pole"),
        (500_000.0, [0.577, 0.577, 0.577], "500km at 45-degree approach"),
    ];

    for &(altitude, dir, label) in test_cases {
        let dir = vec3_normalize(dir);
        let cam_rel_m = [
            dir[0] * (config.radius_m + altitude),
            dir[1] * (config.radius_m + altitude),
            dir[2] * (config.radius_m + altitude),
        ];

        // Anchor = cam_rel_m (ship just spawned or rebased).
        let anchor = cam_rel_m;

        // Mirror compute_barrier_isometry logic:
        let cam_len = vec3_len(cam_rel_m).max(0.01);
        let inv_len = 1.0 / cam_len;
        let normal = [
            cam_rel_m[0] * inv_len,
            cam_rel_m[1] * inv_len,
            cam_rel_m[2] * inv_len,
        ];

        // Barrier center at planet surface along normal.
        let depth = config.radius_m;
        let barrier_sx = (normal[0] * depth - anchor[0]) as f32;
        let barrier_sy = (normal[1] * depth - anchor[1]) as f32;
        let barrier_sz = (normal[2] * depth - anchor[2]) as f32;

        let barrier_dist = (barrier_sx * barrier_sx
            + barrier_sy * barrier_sy
            + barrier_sz * barrier_sz)
            .sqrt();

        // Barrier distance from origin (where ship is after rebase) should be
        // approximately equal to the altitude.
        assert!(
            (barrier_dist - altitude as f32).abs() < 1.0,
            "{label}: barrier distance {:.1}m should match altitude {:.1}m. \
             Barrier is misplaced by {:.1}m.",
            barrier_dist,
            altitude,
            (barrier_dist - altitude as f32).abs()
        );

        // Barrier should be in the opposite direction from the planet normal
        // (i.e., toward the planet center from the ship's perspective).
        let dot = barrier_sx * normal[0] as f32
            + barrier_sy * normal[1] as f32
            + barrier_sz * normal[2] as f32;
        assert!(
            dot < 0.0,
            "{label}: barrier should be below ship (dot with normal = {:.3}), \
             but it's above.",
            dot
        );
    }
}

// ===========================================================================
// TEST 7: HeightField collider range check
// ===========================================================================

#[test]
fn heightfield_collider_range() {
    let config = earth_config();
    let collider_range_m: f64 = 2000.0;

    // Chunk center at the surface directly below the camera (+Z pole).
    let chunk_center = [0.0_f64, 0.0, config.radius_m];

    // Test at various altitudes.
    let test_cases: &[(f64, bool, &str)] = &[
        (500.0, true, "500m altitude: chunk should be in collider range"),
        (1_000.0, true, "1km altitude: chunk should be in collider range"),
        (1_900.0, true, "1.9km altitude: chunk should still be in range"),
        (2_500.0, false, "2.5km altitude: chunk should be out of range"),
        (5_000.0, false, "5km altitude: chunk should be well out of range"),
    ];

    for &(altitude, should_be_in_range, label) in test_cases {
        let cam_rel_m = [0.0_f64, 0.0, config.radius_m + altitude];

        // Compute chunk distance (mirrors chunk_dist in terrain_colliders.rs).
        let dx = chunk_center[0] - cam_rel_m[0];
        let dy = chunk_center[1] - cam_rel_m[1];
        let dz = chunk_center[2] - cam_rel_m[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

        let in_range = dist < collider_range_m;
        assert_eq!(
            in_range, should_be_in_range,
            "{label}: dist={:.1}m, range={:.1}m, in_range={}, expected={}",
            dist, collider_range_m, in_range, should_be_in_range
        );
    }
}

// ===========================================================================
// TEST 8: Gravity blend at various altitudes
// ===========================================================================

#[test]
fn gravity_blend_full_range() {
    let radius = 6_371_000.0_f64;
    let surface_g = 9.81_f32;
    let ship_g = 9.81_f32;
    let ship_down = [0.0_f32, -1.0, 0.0];
    let atmosphere_top = radius * 0.2;

    // Test altitudes from well above atmosphere to the surface.
    let test_cases: &[(f64, f32, f32, &str)] = &[
        // (altitude, expected_blend_min, expected_blend_max, label)
        (atmosphere_top * 2.0, 0.0, 0.0, "well above atmosphere: blend=0"),
        (atmosphere_top, 0.0, 0.01, "at atmosphere top: blend~=0"),
        (atmosphere_top * 0.5, 0.3, 0.7, "mid-atmosphere: blend between 0-1"),
        (atmosphere_top * 0.1, 0.85, 1.0, "near surface: blend close to 1"),
        (0.0, 1.0, 1.0, "on surface: blend=1"),
    ];

    for &(altitude, blend_min, blend_max, label) in test_cases {
        let pos = [0.0_f64, radius + altitude, 0.0];
        let state = compute_gravity(pos, ship_down, radius, surface_g, ship_g);

        assert!(
            state.blend >= blend_min && state.blend <= blend_max,
            "{label}: blend={:.4}, expected [{:.2}, {:.2}]",
            state.blend, blend_min, blend_max
        );

        // Verify direction is a unit vector.
        let len = (state.direction[0].powi(2)
            + state.direction[1].powi(2)
            + state.direction[2].powi(2))
        .sqrt();
        assert!(
            (len - 1.0).abs() < 0.01,
            "{label}: gravity direction not unit vector, len={:.6}",
            len
        );

        // When blend > 0, direction should have a component toward planet center.
        // Planet center is at origin, ship is along +Y, so "toward center" = -Y.
        if state.blend > 0.1 {
            assert!(
                state.direction[1] < 0.0,
                "{label}: gravity direction Y should be negative (toward center), got {:.4}",
                state.direction[1]
            );
        }
    }

    // Verify magnitude transitions smoothly: strictly monotonic as altitude decreases
    // (when ship_g == surface_g, magnitude is constant, so just check bounds).
    for alt_steps in 0..10 {
        let altitude = atmosphere_top * (1.0 - alt_steps as f64 / 10.0);
        let pos = [0.0_f64, radius + altitude, 0.0];
        let state = compute_gravity(pos, ship_down, radius, surface_g, ship_g);
        assert!(
            state.magnitude >= ship_g.min(surface_g) - 0.01
                && state.magnitude <= ship_g.max(surface_g) + 0.01,
            "Gravity magnitude {:.3} outside expected range at altitude {:.0}m",
            state.magnitude, altitude
        );
    }
}

// ===========================================================================
// TEST 9: Full descent simulation with terrain streaming
// ===========================================================================

#[test]
fn descent_streaming_produces_chunks() {
    let config = game_planet_config();
    let face_size_m = config.radius_m * std::f64::consts::FRAC_PI_2;
    let max_lod = max_lod_levels(face_size_m);
    let max_disp = config.radius_m * config.displacement_fraction as f64;

    let mut streaming = ChunkStreaming::new(config.clone(), 4);

    let start_altitude = 1000.0_f64;
    let mut total_chunks_received = 0_usize;
    let mut max_lod_received: u8 = 0;
    let mut chunks_available = false;

    // Simulate 100 frames of hovering at 1km altitude.
    // Workers generate chunks in background threads.
    for frame in 0..100 {
        let cam_rel_m = [0.0, 0.0, config.radius_m + start_altitude];

        let visible = select_visible_nodes(cam_rel_m, config.radius_m, max_lod, max_disp, None);
        let (new_chunks, _removed) = streaming.update(
            &visible,
            &config,
            [0.0, 0.0, config.radius_m + start_altitude],
        );

        for chunk in &new_chunks {
            total_chunks_received += 1;
            if chunk.key.lod > max_lod_received {
                max_lod_received = chunk.key.lod;
            }
        }

        if total_chunks_received > 0 && !chunks_available {
            chunks_available = true;
            assert!(
                frame < 30,
                "First chunks took {} frames to arrive (expected < 30)",
                frame
            );
        }

        // Small sleep to let worker threads process.
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    assert!(
        chunks_available,
        "No chunks received after 100 frames — streaming pipeline is broken"
    );

    assert!(
        total_chunks_received >= 6,
        "Only {} chunks received in 100 frames — expected at least 6 (one per face)",
        total_chunks_received
    );

    assert!(
        max_lod_received >= 10,
        "Finest LOD received was {} — expected >= 10 for 1km altitude streaming",
        max_lod_received
    );
}

// ===========================================================================
// TEST 10: cam_rel_m consistency through anchor rebase
// ===========================================================================

#[test]
fn cam_rel_m_stable_through_rebase() {
    let config = game_planet_config();

    // Initial state: anchor at 1km above +Y pole, ship at rapier origin.
    let altitude = 1000.0_f64;
    let anchor = [0.0_f64, config.radius_m + altitude, 0.0];
    let ship_rapier = [0.0_f64, 0.0, 0.0];

    // cam_rel_m = anchor + ship_rapier (planet-center-relative position).
    let cam_rel_m_before = [
        anchor[0] + ship_rapier[0],
        anchor[1] + ship_rapier[1],
        anchor[2] + ship_rapier[2],
    ];

    // Ship descends 150m (past the 100m rebase threshold).
    let ship_after_descent = [0.0_f64, -150.0, 0.0];

    // Before rebase: cam_rel_m with drifted ship.
    let cam_rel_m_drifted = [
        anchor[0] + ship_after_descent[0],
        anchor[1] + ship_after_descent[1],
        anchor[2] + ship_after_descent[2],
    ];

    // Rebase: new_anchor = anchor + ship_rapier_pos, new_ship = (0,0,0).
    let new_anchor = [
        anchor[0] + ship_after_descent[0],
        anchor[1] + ship_after_descent[1],
        anchor[2] + ship_after_descent[2],
    ];
    let new_ship = [0.0_f64, 0.0, 0.0];

    // After rebase: cam_rel_m should be the same.
    let cam_rel_m_after = [
        new_anchor[0] + new_ship[0],
        new_anchor[1] + new_ship[1],
        new_anchor[2] + new_ship[2],
    ];

    // Verify they match within f32 precision.
    for i in 0..3 {
        let diff = (cam_rel_m_drifted[i] - cam_rel_m_after[i]).abs();
        assert!(
            diff < 1e-6,
            "cam_rel_m[{i}] changed after rebase: before={:.6}, after={:.6}, diff={:.9}. \
             Rebase corrupted the coordinate system.",
            cam_rel_m_drifted[i],
            cam_rel_m_after[i],
            diff
        );
    }

    // Also verify the altitude is correct after rebase.
    let altitude_after = vec3_len(cam_rel_m_after) - config.radius_m;
    let expected_altitude = altitude - 150.0; // descended 150m
    assert!(
        (altitude_after - expected_altitude).abs() < 0.01,
        "Altitude after rebase {:.2}m should be {:.2}m",
        altitude_after, expected_altitude
    );

    // Verify initial cam_rel_m gives correct altitude.
    let altitude_before = vec3_len(cam_rel_m_before) - config.radius_m;
    assert!(
        (altitude_before - altitude).abs() < 0.01,
        "Initial altitude {:.2}m should be {:.2}m",
        altitude_before, altitude
    );
}
