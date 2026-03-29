//! Ship parts catalog v2: larger ship, windowed cockpit, thicker bulkheads.
//!
//! v2 dimensions:
//! - STD_WIDTH: 5.0 (was 4.0)
//! - ROOM_WIDTH: 6.5 (was 5.0)
//! - Cockpit: 6.5m wide, 7.0m long, glass panels on all upper hex faces
//! - Bulkheads: 0.3m thick (was paper-thin)
//! - All other conventions unchanged (hex profile, connection point system)

use crate::assembly::{ConnectPoint, Part};
use crate::colors;
use crate::hull;
use crate::mesh::Mesh;
use crate::primitives::box_mesh;
use glam::{Mat4, Vec3};

// ---------------------------------------------------------------------------
// v2 dimensions
// ---------------------------------------------------------------------------

const STD_WIDTH: f32 = 5.0;
const STD_HEIGHT: f32 = 3.0;
const ROOM_WIDTH: f32 = 6.5;
const FLOOR_Y: f32 = -1.0;
const CEILING_Y: f32 = 1.2;
const WALL_INSET: f32 = 0.15;
const DOOR_W: f32 = 1.4;
const DOOR_H: f32 = 2.1;
/// Bulkhead thickness along Z — visible frame depth.
const BULKHEAD_DEPTH: f32 = 0.3;

// ---------------------------------------------------------------------------
// Cockpit v2 — wide, windowed command bridge
// ---------------------------------------------------------------------------

/// Cockpit v2: wide windowed bridge with space for 2 crew.
///
/// Dimensions: 3.5m (nose) → 6.5m (aft), length 7.0m.
/// Glass panels on upper hex faces (top-left, top, top-right) for panoramic view.
/// Lower hex faces are solid hull. Nose cap is glass.
/// Two chair positions side by side, console panel at front.
pub fn hull_cockpit_v2() -> Part {
    let front_w = 3.5;
    let back_w = ROOM_WIDTH;
    let length = 7.0;

    let mut meshes = Vec::new();

    // Build hull selectively: solid bottom panels, glass upper panels
    let front_ring = hull::hex_ring(front_w, STD_HEIGHT, 0.0);
    let back_ring = hull::hex_ring(back_w, STD_HEIGHT, length);

    // Hex sides: [0-1]=top, [1-2]=top-right, [2-3]=bottom-right,
    //            [3-4]=bottom, [4-5]=bottom-left, [5-0]=top-left
    // Windows: top (0-1), top-right (1-2), top-left (5-0) — NO mesh, fully open
    // Solid: bottom-right (2-3), bottom (3-4), bottom-left (4-5)

    // Solid hull panels (lower 3 faces only)
    for &(a, b) in &[(2, 3), (3, 4), (4, 5)] {
        meshes.push(hull::hex_hull_panel(
            &front_ring, &back_ring, a, b, colors::HULL_EXTERIOR,
        ));
    }

    // Upper 3 faces are open windows — no mesh generated.
    // No nose cap either — fully open forward view.

    // Window frame edges: thin strips along the open window boundaries.
    // Uses exact hex ring vertices so connection validation passes (V-P10).
    meshes.push(hull::window_frame_edges(&front_ring, &back_ring, &[(0, 1), (1, 2), (5, 0)], colors::HULL_ACCENT));

    // Interior floor and ceiling
    let interior_w = front_w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    // Thick bulkhead at aft
    meshes.push(thick_bulkhead(back_w, STD_HEIGHT, length));

    // Console shelf across the front (below window line)
    let console_w = front_w * 0.7;
    let console = box_mesh(console_w, 0.05, 0.5, colors::HULL_ACCENT);
    let console = console.transform(Mat4::from_translation(Vec3::new(0.0, -0.15, 0.8)));
    meshes.push(console);

    // Antenna array extending forward from top
    let antenna = crate::primitives::cylinder_mesh(0.03, 2.5, 6, colors::ANTENNA);
    let antenna = antenna.transform(
        Mat4::from_translation(Vec3::new(0.0, STD_HEIGHT * 0.45, -1.0))
            * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );
    meshes.push(antenna);

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![ConnectPoint {
            id: "aft",
            position: Vec3::new(0.0, 0.0, length),
            normal: Vec3::Z,
            width: back_w,
            height: STD_HEIGHT,
        }],
    }
}

// ---------------------------------------------------------------------------
// Corridor v2 — wider
// ---------------------------------------------------------------------------

/// Standard corridor, width=5.0, variable length. Thick bulkheads at both ends.
pub fn hull_corridor_v2(length: f32) -> Part {
    let mut meshes = Vec::new();

    meshes.push(hull::hex_hull(
        STD_WIDTH, STD_WIDTH, STD_HEIGHT, length, colors::HULL_EXTERIOR,
    ));

    let interior_w = STD_WIDTH - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    // Thick bulkheads at both ends
    meshes.push(thick_bulkhead(STD_WIDTH, STD_HEIGHT, 0.0));
    meshes.push(thick_bulkhead(STD_WIDTH, STD_HEIGHT, length));

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "fore",
                position: Vec3::ZERO,
                normal: Vec3::NEG_Z,
                width: STD_WIDTH,
                height: STD_HEIGHT,
            },
            ConnectPoint {
                id: "aft",
                position: Vec3::new(0.0, 0.0, length),
                normal: Vec3::Z,
                width: STD_WIDTH,
                height: STD_HEIGHT,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Transition v2
// ---------------------------------------------------------------------------

/// Transition piece: tapers between widths.
pub fn hull_transition_v2(from_width: f32, to_width: f32, length: f32) -> Part {
    let mut meshes = Vec::new();

    meshes.push(hull::hex_hull(
        from_width, to_width, STD_HEIGHT, length, colors::HULL_ACCENT,
    ));

    let floor_w = from_width.min(to_width) - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(floor_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(floor_w, length, CEILING_Y, colors::CEILING));

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "fore",
                position: Vec3::ZERO,
                normal: Vec3::NEG_Z,
                width: from_width,
                height: STD_HEIGHT,
            },
            ConnectPoint {
                id: "aft",
                position: Vec3::new(0.0, 0.0, length),
                normal: Vec3::Z,
                width: to_width,
                height: STD_HEIGHT,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Room v2 — wider
// ---------------------------------------------------------------------------

/// Wider room section, width=6.5, length=6.0. Thick bulkheads.
pub fn hull_room_v2(
    name: &str,
    _accent_color: [f32; 3],
    side_doors: &[&str],
) -> Part {
    let room_len = 6.0;

    let mut meshes = Vec::new();

    meshes.push(hull::hex_hull(
        ROOM_WIDTH, ROOM_WIDTH, STD_HEIGHT, room_len, colors::HULL_EXTERIOR,
    ));

    let interior_w = ROOM_WIDTH - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, room_len, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, room_len, CEILING_Y, colors::CEILING));

    // Thick bulkheads at both ends
    meshes.push(thick_bulkhead(ROOM_WIDTH, STD_HEIGHT, 0.0));
    meshes.push(thick_bulkhead(ROOM_WIDTH, STD_HEIGHT, room_len));

    // Structural features
    if name == "eng" {
        let fin_length = 3.5;
        let fin_height = 2.5;
        let fin_thick = 0.05;
        let mid_z = room_len * 0.5;
        let fin_offset_x = ROOM_WIDTH * 0.5 + fin_length * 0.5;
        let fin_angle = 0.26;

        let fin = box_mesh(fin_length, fin_height, fin_thick, colors::RADIATOR_FIN);
        let port = fin.transform(
            Mat4::from_translation(Vec3::new(-fin_offset_x, 0.2, mid_z))
                * Mat4::from_rotation_z(-fin_angle),
        );
        meshes.push(port);

        let fin = box_mesh(fin_length, fin_height, fin_thick, colors::RADIATOR_FIN);
        let starboard = fin.transform(
            Mat4::from_translation(Vec3::new(fin_offset_x, 0.2, mid_z))
                * Mat4::from_rotation_z(fin_angle),
        );
        meshes.push(starboard);
    }

    if name == "nav" || name == "sensors" {
        let dish_z = room_len * 0.5;
        let dorsal_y = STD_HEIGHT * 0.5;
        let dish_base = crate::primitives::cylinder_mesh(0.15, 0.4, 8, colors::ANTENNA);
        let dish_base = dish_base.transform(
            Mat4::from_translation(Vec3::new(0.0, dorsal_y + 0.2, dish_z)),
        );
        meshes.push(dish_base);
        let dish = crate::primitives::cone_mesh(0.5, 0.1, 0.3, 8, colors::HULL_ACCENT);
        let dish = dish.transform(
            Mat4::from_translation(Vec3::new(0.0, dorsal_y + 0.55, dish_z)),
        );
        meshes.push(dish);
    }

    let mesh = Mesh::merge(&meshes);

    let mut connections = vec![
        ConnectPoint {
            id: "fore",
            position: Vec3::ZERO,
            normal: Vec3::NEG_Z,
            width: ROOM_WIDTH,
            height: STD_HEIGHT,
        },
        ConnectPoint {
            id: "aft",
            position: Vec3::new(0.0, 0.0, room_len),
            normal: Vec3::Z,
            width: ROOM_WIDTH,
            height: STD_HEIGHT,
        },
    ];

    for &side in side_doors {
        match side {
            "port" => connections.push(ConnectPoint {
                id: "port",
                position: Vec3::new(-ROOM_WIDTH / 2.0, 0.0, room_len / 2.0),
                normal: Vec3::NEG_X,
                width: 2.5,
                height: STD_HEIGHT,
            }),
            "starboard" => connections.push(ConnectPoint {
                id: "starboard",
                position: Vec3::new(ROOM_WIDTH / 2.0, 0.0, room_len / 2.0),
                normal: Vec3::X,
                width: 2.5,
                height: STD_HEIGHT,
            }),
            _ => {}
        }
    }

    Part { mesh, connections }
}

// ---------------------------------------------------------------------------
// Engine section v2
// ---------------------------------------------------------------------------

/// Engine section: tapered from 4.0 to 2.5, length 6.0.
pub fn hull_engine_section_v2() -> Part {
    let front_w = 4.0;
    let back_w = 2.5;
    let length = 6.0;

    let mut meshes = Vec::new();

    meshes.push(hull::hex_hull(
        front_w, back_w, STD_HEIGHT, length, colors::HULL_EXTERIOR,
    ));

    let back_ring = hull::hex_ring(back_w, STD_HEIGHT, length);
    meshes.push(hull::hex_cap(&back_ring, colors::HULL_EXTERIOR, true));

    let interior_w = front_w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    meshes.push(thick_bulkhead(front_w, STD_HEIGHT, 0.0));

    // Engine nacelles
    let nacelle_r = 0.6;
    let nacelle_len = 3.5;
    let nacelle = crate::primitives::cylinder_mesh(nacelle_r, nacelle_len, 8, colors::HULL_ACCENT);
    let rot_z = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);

    let nacelle_left = nacelle.transform(
        Mat4::from_translation(Vec3::new(-1.0, -0.3, length + nacelle_len / 2.0)) * rot_z,
    );
    meshes.push(nacelle_left);

    let nacelle_right = nacelle.transform(
        Mat4::from_translation(Vec3::new(1.0, -0.3, length + nacelle_len / 2.0)) * rot_z,
    );
    meshes.push(nacelle_right);

    let cone = crate::primitives::cone_mesh(nacelle_r, 0.0, 1.2, 8, colors::ACCENT_ENGINE);
    let cone_left = cone.transform(
        Mat4::from_translation(Vec3::new(-1.0, -0.3, length + nacelle_len + 0.6)) * rot_z,
    );
    meshes.push(cone_left);

    let cone_right = cone.transform(
        Mat4::from_translation(Vec3::new(1.0, -0.3, length + nacelle_len + 0.6)) * rot_z,
    );
    meshes.push(cone_right);

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![ConnectPoint {
            id: "fore",
            position: Vec3::ZERO,
            normal: Vec3::NEG_Z,
            width: front_w,
            height: STD_HEIGHT,
        }],
    }
}

// ---------------------------------------------------------------------------
// Thick bulkhead helper
// ---------------------------------------------------------------------------

/// Builds a bulkhead with visible depth (BULKHEAD_DEPTH) centered on z_pos.
/// Extends half the depth each direction so it doesn't clip into adjacent sections.
/// Two faces (fore and aft) with direct quads forming the door frame inner walls
/// (no box_mesh overlaps — avoids Z-fighting at door edges).
fn thick_bulkhead(hull_width: f32, hull_height: f32, z_pos: f32) -> Mesh {
    let half = BULKHEAD_DEPTH / 2.0;
    let z_fore = z_pos - half;
    let z_aft = z_pos + half;
    let mut meshes = Vec::new();

    // Fore face
    let fore = hull::bulkhead_with_door(
        hull_width, hull_height, FLOOR_Y, CEILING_Y, DOOR_W, DOOR_H, colors::BULKHEAD,
    );
    let fore = fore.transform(Mat4::from_translation(Vec3::new(0.0, 0.0, z_fore)));
    meshes.push(fore);

    // Aft face
    let aft = hull::bulkhead_with_door(
        hull_width, hull_height, FLOOR_Y, CEILING_Y, DOOR_W, DOOR_H, colors::BULKHEAD,
    );
    let aft = aft.transform(Mat4::from_translation(Vec3::new(0.0, 0.0, z_aft)));
    meshes.push(aft);

    // Door frame inner walls: direct quads connecting fore/aft door edges.
    // No box_mesh — vertices sit exactly at the door opening corners,
    // eliminating coplanar overlap with the bulkhead face geometry.
    let hdw = DOOR_W / 2.0;
    let door_bottom = FLOOR_Y;
    let door_top = FLOOR_Y + DOOR_H;
    let frame_color = colors::INTERIOR_WALL;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Door opening corners (fore and aft)
    let fl_b = [-hdw, door_bottom, z_fore]; // fore-left-bottom
    let fl_t = [-hdw, door_top, z_fore];    // fore-left-top
    let fr_b = [hdw, door_bottom, z_fore];  // fore-right-bottom
    let fr_t = [hdw, door_top, z_fore];     // fore-right-top
    let al_b = [-hdw, door_bottom, z_aft];  // aft-left-bottom
    let al_t = [-hdw, door_top, z_aft];     // aft-left-top
    let ar_b = [hdw, door_bottom, z_aft];   // aft-right-bottom
    let ar_t = [hdw, door_top, z_aft];      // aft-right-top

    // Left jamb: quad from fore-left to aft-left (normal facing right, into door)
    let n_left = [1.0, 0.0, 0.0_f32];
    crate::primitives::push_quad(
        &mut vertices, &mut indices,
        [fl_b, al_b, al_t, fl_t], n_left, frame_color,
    );

    // Right jamb: quad from fore-right to aft-right (normal facing left, into door)
    let n_right = [-1.0, 0.0, 0.0_f32];
    crate::primitives::push_quad(
        &mut vertices, &mut indices,
        [ar_b, fr_b, fr_t, ar_t], n_right, frame_color,
    );

    // Lintel: quad from fore-top to aft-top (normal facing down, into door)
    let n_top = [0.0, -1.0, 0.0_f32];
    crate::primitives::push_quad(
        &mut vertices, &mut indices,
        [fl_t, al_t, ar_t, fr_t], n_top, frame_color,
    );

    meshes.push(Mesh { vertices, indices });

    Mesh::merge(&meshes)
}

// ---------------------------------------------------------------------------
// Full v2 assembly
// ---------------------------------------------------------------------------

/// Assemble the complete v2 ship: cockpit → corridor → rooms → engine.
///
/// Layout (bow to stern):
/// - Cockpit v2:  7.0m  (3.5→6.5, windowed bridge)
/// - Corridor 1:  4.0m  (5.0 width)
/// - Transition:  1.0m  (5.0→6.5)
/// - Nav room:    6.0m  (6.5 width)
/// - Transition:  1.0m  (6.5→5.0)
/// - Corridor 2:  4.0m  (5.0 width)
/// - Transition:  1.0m  (5.0→6.5)
/// - Eng room:    6.0m  (6.5 width)
/// - Transition:  1.0m  (6.5→4.0)
/// - Engine:      6.0m  (4.0→2.5)
///   Total: ~37m
pub fn assemble_ship_v2() -> Mesh {
    use crate::assembly::attach;

    let cockpit = hull_cockpit_v2();
    let trans0 = hull_transition_v2(ROOM_WIDTH, STD_WIDTH, 1.0); // cockpit 6.5 → corridor 5.0
    let corr1 = hull_corridor_v2(4.0);
    let trans1 = hull_transition_v2(STD_WIDTH, ROOM_WIDTH, 1.0);
    let nav_room = hull_room_v2("nav", colors::ACCENT_NAVIGATION, &[]);
    let trans2 = hull_transition_v2(ROOM_WIDTH, STD_WIDTH, 1.0);
    let corr2 = hull_corridor_v2(4.0);
    let trans3 = hull_transition_v2(STD_WIDTH, ROOM_WIDTH, 1.0);
    let eng_room = hull_room_v2("eng", colors::ACCENT_ENGINEERING, &[]);
    let trans4 = hull_transition_v2(ROOM_WIDTH, 4.0, 1.0);
    let engine = hull_engine_section_v2();

    let ship = attach(&cockpit, "aft", &trans0, "fore");
    let ship = attach(&ship, "aft", &corr1, "fore");
    let ship = attach(&ship, "aft", &trans1, "fore");
    let ship = attach(&ship, "aft", &nav_room, "fore");
    let ship = attach(&ship, "aft", &trans2, "fore");
    let ship = attach(&ship, "aft", &corr2, "fore");
    let ship = attach(&ship, "aft", &trans3, "fore");
    let ship = attach(&ship, "aft", &eng_room, "fore");
    let ship = attach(&ship, "aft", &trans4, "fore");
    let ship = attach(&ship, "aft", &engine, "fore");

    ship.mesh
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::validate_part;

    #[test]
    fn cockpit_v2_validates() {
        let p = hull_cockpit_v2();
        validate_part(&p).unwrap();
    }

    #[test]
    fn cockpit_v2_has_aft_connection() {
        let p = hull_cockpit_v2();
        assert_eq!(p.connections.len(), 1);
        assert!(p.try_connection("aft").is_some());
        assert!((p.connection("aft").width - ROOM_WIDTH).abs() < 1e-4);
    }

    #[test]
    fn corridor_v2_validates() {
        let p = hull_corridor_v2(4.0);
        validate_part(&p).unwrap();
    }

    #[test]
    fn room_v2_validates() {
        let p = hull_room_v2("nav", colors::ACCENT_NAVIGATION, &[]);
        validate_part(&p).unwrap();
    }

    #[test]
    fn engine_v2_validates() {
        let p = hull_engine_section_v2();
        validate_part(&p).unwrap();
    }

    #[test]
    fn full_v2_assembly() {
        let ship = assemble_ship_v2();
        let (min, max) = ship.bounding_box();
        let length = max.z - min.z;
        assert!(
            length > 30.0 && length < 55.0,
            "v2 ship length {length} should be roughly 35-50m"
        );
    }

    #[test]
    fn v2_mesh_valid_indices() {
        let ship = assemble_ship_v2();
        for &idx in &ship.indices {
            assert!(
                (idx as usize) < ship.vertices.len(),
                "index {} out of bounds (len={})", idx, ship.vertices.len()
            );
        }
    }
}
