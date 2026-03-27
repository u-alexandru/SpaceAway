//! Ship part catalog: functions that return Part (mesh + connection points).
//!
//! Every part follows the modular ship construction standards:
//! - Hull spans z=0 to z=length in local space
//! - Fore connection at (0,0,0) with normal (0,0,-1)
//! - Aft connection at (0,0,length) with normal (0,0,+1)
//! - Width transitions ONLY in hull_transition() parts
//! - Interior: floor at y=-1.0, ceiling at y=0.2
//! - Connection points carry width/height metadata

use crate::assembly::{ConnectPoint, Part};
use crate::colors;
use crate::hull;
use crate::mesh::Mesh;
use crate::primitives::{box_mesh, cone_mesh, cylinder_mesh};
use glam::{Mat4, Vec3};

// ---------------------------------------------------------------------------
// Shared constants (per standards Appendix B)
// ---------------------------------------------------------------------------

const STD_WIDTH: f32 = 4.0;
const STD_HEIGHT: f32 = 3.0;
const ROOM_WIDTH: f32 = 5.0;
const FLOOR_Y: f32 = -1.0;
const CEILING_Y: f32 = 0.2;
const WALL_INSET: f32 = 0.15;
const DOOR_W: f32 = 1.2;
const DOOR_H: f32 = 2.0;
const FRAME_THICKNESS: f32 = 0.1;

// ---------------------------------------------------------------------------
// Cockpit
// ---------------------------------------------------------------------------

/// Cockpit: tapered hex hull, front_width=2.0 to back_width=4.0, length=4.0.
/// Interior: floor, ceiling, helm console at front, glass cap at nose.
/// Connection: "aft" at z=4.0 (facing +Z), width=4.0.
pub fn hull_cockpit() -> Part {
    let front_w = 2.0;
    let back_w = STD_WIDTH;
    let length = 4.0;

    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(front_w, back_w, STD_HEIGHT, length, colors::HULL_EXTERIOR));

    // Front cap (glass nose) -- terminal face, rule 4.5.1
    let front_ring = hull::hex_ring(front_w, STD_HEIGHT, 0.0);
    meshes.push(hull::hex_cap(&front_ring, colors::WINDOW_GLASS, false));

    // Interior: floor, ceiling (use narrower width per rule 4.3.4)
    let interior_w = front_w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    // Helm console near the front
    let console = hull::console_mesh(1.2, colors::ACCENT_HELM);
    let console = console.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.8)));
    meshes.push(console);

    // Door frame at aft
    let frame = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame = frame.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, length)));
    meshes.push(frame);

    // Antenna array extending forward from top
    let antenna = cylinder_mesh(0.03, 2.5, 6, colors::ANTENNA);
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
// Corridor
// ---------------------------------------------------------------------------

/// Standard hex corridor, width=4.0, variable length.
/// Interior: floor, ceiling, door frames at both ends.
/// Connections: "fore" at z=0, "aft" at z=length, both width=4.0.
pub fn hull_corridor(length: f32) -> Part {
    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(STD_WIDTH, STD_WIDTH, STD_HEIGHT, length, colors::HULL_EXTERIOR));

    // Interior
    let interior_w = STD_WIDTH - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    // Door frames at both ends
    let frame_fore = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_fore = frame_fore.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.0)));
    meshes.push(frame_fore);

    let frame_aft = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_aft = frame_aft.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, length)));
    meshes.push(frame_aft);

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "fore",
                position: Vec3::new(0.0, 0.0, 0.0),
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
// Transition
// ---------------------------------------------------------------------------

/// Transition piece: tapers from `from_width` to `to_width` over 1.0m along Z.
/// Connections: "fore" at z=0 with from_width, "aft" at z=1.0 with to_width.
pub fn hull_transition(from_width: f32, to_width: f32, length: f32) -> Part {
    let mut meshes = Vec::new();

    meshes.push(hull::hex_hull(from_width, to_width, STD_HEIGHT, length, colors::HULL_ACCENT));

    // Interior floor at the narrower width (rule 4.3.4)
    let floor_w = from_width.min(to_width) - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(floor_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(floor_w, length, CEILING_Y, colors::CEILING));

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "fore",
                position: Vec3::new(0.0, 0.0, 0.0),
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
// Room
// ---------------------------------------------------------------------------

/// Wider hex room section (constant width=5.0, length=5.0).
/// NO embedded transitions (rule 4.4). Caller inserts transition pieces.
/// Interior: floor, ceiling, console with accent color, door frames.
/// Connections: "fore" at z=0 width=5.0, "aft" at z=5.0 width=5.0.
pub fn hull_room(
    name: &str,
    accent_color: [f32; 3],
    side_doors: &[&str],
) -> Part {
    let room_len = 5.0;

    let mut meshes = Vec::new();

    // Main room hull: constant ROOM_WIDTH from z=0 to z=room_len
    meshes.push(hull::hex_hull(
        ROOM_WIDTH, ROOM_WIDTH, STD_HEIGHT, room_len, colors::HULL_EXTERIOR,
    ));

    // Interior
    let interior_w = ROOM_WIDTH - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, room_len, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, room_len, CEILING_Y, colors::CEILING));

    // Console on one wall with accent color (placed mid-room)
    let console = hull::console_mesh(1.5, accent_color);
    let console = console.transform(
        Mat4::from_translation(Vec3::new(-1.5, FLOOR_Y, room_len * 0.5))
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2),
    );
    meshes.push(console);

    // Door frames at fore and aft
    let frame_fore = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_fore = frame_fore.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.0)));
    meshes.push(frame_fore);

    let frame_aft = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_aft = frame_aft.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, room_len)));
    meshes.push(frame_aft);

    // --- Structural features ---

    // Feature: radiator fins on engineering rooms
    if name == "eng" {
        let fin_length = 3.0;
        let fin_height = 2.0;
        let fin_thick = 0.05;
        let mid_z = room_len * 0.5;
        let fin_offset_x = ROOM_WIDTH * 0.5 + fin_length * 0.5;
        let fin_angle = 0.26; // ~15 degrees

        let fin = box_mesh(fin_length, fin_height, fin_thick, colors::RADIATOR_FIN);
        let port_fin = fin.transform(
            Mat4::from_translation(Vec3::new(-fin_offset_x, 0.2, mid_z))
                * Mat4::from_rotation_z(-fin_angle),
        );
        meshes.push(port_fin);

        let fin = box_mesh(fin_length, fin_height, fin_thick, colors::RADIATOR_FIN);
        let starboard_fin = fin.transform(
            Mat4::from_translation(Vec3::new(fin_offset_x, 0.2, mid_z))
                * Mat4::from_rotation_z(fin_angle),
        );
        meshes.push(starboard_fin);
    }

    // Feature: sensor dish on nav/sensors room
    if name == "nav" || name == "sensors" {
        let dish_z = room_len * 0.5;
        let dorsal_y = STD_HEIGHT * 0.5;
        let dish_base = cylinder_mesh(0.15, 0.4, 8, colors::ANTENNA);
        let dish_base = dish_base.transform(
            Mat4::from_translation(Vec3::new(0.0, dorsal_y + 0.2, dish_z)),
        );
        meshes.push(dish_base);
        let dish = cone_mesh(0.5, 0.1, 0.3, 8, colors::HULL_ACCENT);
        let dish = dish.transform(
            Mat4::from_translation(Vec3::new(0.0, dorsal_y + 0.55, dish_z)),
        );
        meshes.push(dish);
    }

    let mesh = Mesh::merge(&meshes);

    let mut connections = vec![
        ConnectPoint {
            id: "fore",
            position: Vec3::new(0.0, 0.0, 0.0),
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

    // Side connections (structural attachment only -- no hull cutouts per Section 7.5)
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
// Engine section
// ---------------------------------------------------------------------------

/// Engine section: tapered hull front_width=3.5 to back_width=2.0, length=5.0.
/// Two engine nacelles + nozzles extending from back.
/// Connection: "fore" at z=0 (facing -Z), width=3.5.
pub fn hull_engine_section() -> Part {
    let front_w = 3.5;
    let back_w = 2.0;
    let length = 5.0;

    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(front_w, back_w, STD_HEIGHT, length, colors::HULL_EXTERIOR));

    // Back cap (terminal face, rule 4.5.1)
    let back_ring = hull::hex_ring(back_w, STD_HEIGHT, length);
    meshes.push(hull::hex_cap(&back_ring, colors::HULL_EXTERIOR, true));

    // Interior
    let interior_w = front_w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    // Engine console
    let console = hull::console_mesh(1.2, colors::ACCENT_ENGINE);
    let console = console.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 1.5)));
    meshes.push(console);

    // Door frame at fore
    let frame = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame = frame.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.0)));
    meshes.push(frame);

    // Engine nacelles
    let nacelle_r = 0.5;
    let nacelle_len = 3.0;
    let nacelle = cylinder_mesh(nacelle_r, nacelle_len, 8, colors::HULL_ACCENT);
    let rot_z = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);

    let nacelle_left = nacelle.transform(
        Mat4::from_translation(Vec3::new(-0.8, -0.3, length + nacelle_len / 2.0)) * rot_z,
    );
    meshes.push(nacelle_left);

    let nacelle_right = nacelle.transform(
        Mat4::from_translation(Vec3::new(0.8, -0.3, length + nacelle_len / 2.0)) * rot_z,
    );
    meshes.push(nacelle_right);

    // Engine cones at tips
    let cone = cone_mesh(nacelle_r, 0.0, 1.0, 8, colors::ACCENT_ENGINE);
    let cone_left = cone.transform(
        Mat4::from_translation(Vec3::new(-0.8, -0.3, length + nacelle_len + 0.5)) * rot_z,
    );
    meshes.push(cone_left);

    let cone_right = cone.transform(
        Mat4::from_translation(Vec3::new(0.8, -0.3, length + nacelle_len + 0.5)) * rot_z,
    );
    meshes.push(cone_right);

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![ConnectPoint {
            id: "fore",
            position: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::NEG_Z,
            width: front_w,
            height: STD_HEIGHT,
        }],
    }
}

// ---------------------------------------------------------------------------
// Airlock
// ---------------------------------------------------------------------------

/// Airlock: small hex hull width=2.5, height=3.0, length=2.5.
/// Outer cap sealed. Inner connection at z=0, width=2.5.
pub fn hull_airlock() -> Part {
    let w = 2.5;
    let length = 2.5;

    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(w, w, STD_HEIGHT, length, colors::AIRLOCK_WARNING));

    // Outer cap (sealed end, rule 4.5.1)
    let outer_ring = hull::hex_ring(w, STD_HEIGHT, length);
    meshes.push(hull::hex_cap(&outer_ring, colors::HULL_ACCENT, true));

    // Interior
    let interior_w = w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));

    // Inner door frame
    let frame_inner = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_inner = frame_inner.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.0)));
    meshes.push(frame_inner);

    // Outer door frame
    let frame_outer = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::AIRLOCK_WARNING);
    let frame_outer = frame_outer.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, length)));
    meshes.push(frame_outer);

    let mesh = Mesh::merge(&meshes);

    Part {
        mesh,
        connections: vec![ConnectPoint {
            id: "inner",
            position: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::NEG_Z,
            width: w,
            height: STD_HEIGHT,
        }],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::validate_part;

    #[test]
    fn cockpit_has_aft_connection() {
        let p = hull_cockpit();
        assert_eq!(p.connections.len(), 1);
        assert!(p.try_connection("aft").is_some());
    }

    #[test]
    fn cockpit_validates() {
        let p = hull_cockpit();
        validate_part(&p).unwrap();
    }

    #[test]
    fn cockpit_mesh_not_empty() {
        let p = hull_cockpit();
        assert!(!p.mesh.vertices.is_empty());
        assert!(!p.mesh.indices.is_empty());
    }

    #[test]
    fn cockpit_aft_width() {
        let p = hull_cockpit();
        let aft = p.connection("aft");
        assert!((aft.width - STD_WIDTH).abs() < 1e-4);
        assert!((aft.position.z - 4.0).abs() < 1e-4);
    }

    #[test]
    fn corridor_has_two_connections() {
        let p = hull_corridor(3.0);
        assert_eq!(p.connections.len(), 2);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
    }

    #[test]
    fn corridor_validates() {
        let p = hull_corridor(3.0);
        validate_part(&p).unwrap();
    }

    #[test]
    fn corridor_mesh_not_empty() {
        let p = hull_corridor(3.0);
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn corridor_connections_face_opposite() {
        let p = hull_corridor(3.0);
        let fore = p.connection("fore");
        let aft = p.connection("aft");
        let dot = fore.normal.dot(aft.normal);
        assert!(
            (dot - (-1.0)).abs() < 1e-4,
            "fore and aft should face opposite"
        );
    }

    #[test]
    fn corridor_widths_correct() {
        let p = hull_corridor(3.0);
        assert!((p.connection("fore").width - STD_WIDTH).abs() < 1e-4);
        assert!((p.connection("aft").width - STD_WIDTH).abs() < 1e-4);
    }

    #[test]
    fn transition_has_two_connections() {
        let p = hull_transition(4.0, 5.0, 1.0);
        assert_eq!(p.connections.len(), 2);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
    }

    #[test]
    fn transition_validates() {
        let p = hull_transition(4.0, 5.0, 1.0);
        validate_part(&p).unwrap();
    }

    #[test]
    fn transition_widths_correct() {
        let p = hull_transition(4.0, 5.0, 1.0);
        assert!((p.connection("fore").width - 4.0).abs() < 1e-4);
        assert!((p.connection("aft").width - 5.0).abs() < 1e-4);
    }

    #[test]
    fn transition_mesh_not_empty() {
        let p = hull_transition(4.0, 5.0, 1.0);
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn room_has_fore_aft_connections() {
        let p = hull_room("nav", colors::ACCENT_NAVIGATION, &[]);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
    }

    #[test]
    fn room_validates() {
        let p = hull_room("nav", colors::ACCENT_NAVIGATION, &[]);
        validate_part(&p).unwrap();
    }

    #[test]
    fn room_widths_are_room_width() {
        let p = hull_room("nav", colors::ACCENT_NAVIGATION, &[]);
        assert!((p.connection("fore").width - ROOM_WIDTH).abs() < 1e-4);
        assert!((p.connection("aft").width - ROOM_WIDTH).abs() < 1e-4);
    }

    #[test]
    fn room_no_embedded_transitions() {
        // Room fore and aft should both be at ROOM_WIDTH (5.0), not STD_WIDTH (4.0)
        let p = hull_room("test", [0.5; 3], &[]);
        let fore = p.connection("fore");
        let aft = p.connection("aft");
        assert!((fore.width - ROOM_WIDTH).abs() < 1e-4,
            "fore width should be ROOM_WIDTH, got {}", fore.width);
        assert!((aft.width - ROOM_WIDTH).abs() < 1e-4,
            "aft width should be ROOM_WIDTH, got {}", aft.width);
        // Length should be 5.0 (room only), not 7.0 (room + transitions)
        assert!((aft.position.z - 5.0).abs() < 1e-4,
            "aft z should be 5.0, got {}", aft.position.z);
    }

    #[test]
    fn room_with_side_doors() {
        let p = hull_room("eng", colors::ACCENT_ENGINEERING, &["port", "starboard"]);
        assert_eq!(p.connections.len(), 4);
        assert!(p.try_connection("port").is_some());
        assert!(p.try_connection("starboard").is_some());
    }

    #[test]
    fn room_mesh_not_empty() {
        let p = hull_room("test", [0.5; 3], &[]);
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn engine_section_has_fore_connection() {
        let p = hull_engine_section();
        assert_eq!(p.connections.len(), 1);
        assert!(p.try_connection("fore").is_some());
    }

    #[test]
    fn engine_section_validates() {
        let p = hull_engine_section();
        validate_part(&p).unwrap();
    }

    #[test]
    fn engine_fore_width() {
        let p = hull_engine_section();
        assert!((p.connection("fore").width - 3.5).abs() < 1e-4);
    }

    #[test]
    fn engine_section_mesh_not_empty() {
        let p = hull_engine_section();
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn airlock_has_inner_connection() {
        let p = hull_airlock();
        assert_eq!(p.connections.len(), 1);
        assert!(p.try_connection("inner").is_some());
    }

    #[test]
    fn airlock_validates() {
        let p = hull_airlock();
        validate_part(&p).unwrap();
    }

    #[test]
    fn airlock_mesh_not_empty() {
        let p = hull_airlock();
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn all_parts_have_valid_indices() {
        let parts: Vec<Part> = vec![
            hull_cockpit(),
            hull_corridor(3.0),
            hull_transition(4.0, 5.0, 1.0),
            hull_room("test", [0.5; 3], &[]),
            hull_engine_section(),
            hull_airlock(),
        ];
        for part in &parts {
            for &idx in &part.mesh.indices {
                assert!(
                    (idx as usize) < part.mesh.vertices.len(),
                    "index {} out of bounds (len={})",
                    idx,
                    part.mesh.vertices.len()
                );
            }
        }
    }

    #[test]
    fn all_parts_no_degenerate_triangles() {
        let parts: Vec<Part> = vec![
            hull_cockpit(),
            hull_corridor(3.0),
            hull_transition(4.0, 5.0, 1.0),
            hull_room("test", [0.5; 3], &[]),
            hull_engine_section(),
            hull_airlock(),
        ];
        for part in &parts {
            for tri in part.mesh.indices.chunks_exact(3) {
                let a = Vec3::from(part.mesh.vertices[tri[0] as usize].position);
                let b = Vec3::from(part.mesh.vertices[tri[1] as usize].position);
                let c = Vec3::from(part.mesh.vertices[tri[2] as usize].position);
                let area = (b - a).cross(c - a).length() / 2.0;
                assert!(area > 1e-6, "triangle should have non-zero area");
            }
        }
    }

    #[test]
    fn full_ship_assembly_with_validation() {
        use crate::assembly::attach;
        use crate::validate::{validate_connection, validate_part};

        let cockpit = hull_cockpit();
        let corr1 = hull_corridor(3.0);
        let trans1 = hull_transition(STD_WIDTH, ROOM_WIDTH, 1.0);
        let nav_room = hull_room("nav", colors::ACCENT_NAVIGATION, &[]);
        let trans2 = hull_transition(ROOM_WIDTH, STD_WIDTH, 1.0);
        let corr2 = hull_corridor(3.0);
        let trans3 = hull_transition(STD_WIDTH, ROOM_WIDTH, 1.0);
        let eng_room = hull_room("eng", colors::ACCENT_ENGINEERING, &[]);
        let trans4 = hull_transition(ROOM_WIDTH, 3.5, 1.0);
        let engine = hull_engine_section();

        // Validate all parts
        validate_part(&cockpit).unwrap();
        validate_part(&corr1).unwrap();
        validate_part(&trans1).unwrap();
        validate_part(&nav_room).unwrap();
        validate_part(&trans2).unwrap();
        validate_part(&corr2).unwrap();
        validate_part(&trans3).unwrap();
        validate_part(&eng_room).unwrap();
        validate_part(&trans4).unwrap();
        validate_part(&engine).unwrap();

        // Validate connections
        validate_connection(&cockpit, "aft", &corr1, "fore").unwrap();
        validate_connection(&corr1, "aft", &trans1, "fore").unwrap();
        validate_connection(&trans1, "aft", &nav_room, "fore").unwrap();
        validate_connection(&nav_room, "aft", &trans2, "fore").unwrap();
        validate_connection(&trans2, "aft", &corr2, "fore").unwrap();
        validate_connection(&corr2, "aft", &trans3, "fore").unwrap();
        validate_connection(&trans3, "aft", &eng_room, "fore").unwrap();
        validate_connection(&eng_room, "aft", &trans4, "fore").unwrap();
        validate_connection(&trans4, "aft", &engine, "fore").unwrap();

        // Assemble
        let ship = attach(&cockpit, "aft", &corr1, "fore");
        let ship = attach(&ship, "aft", &trans1, "fore");
        let ship = attach(&ship, "aft", &nav_room, "fore");
        let ship = attach(&ship, "aft", &trans2, "fore");
        let ship = attach(&ship, "aft", &corr2, "fore");
        let ship = attach(&ship, "aft", &trans3, "fore");
        let ship = attach(&ship, "aft", &eng_room, "fore");
        let ship = attach(&ship, "aft", &trans4, "fore");
        let ship = attach(&ship, "aft", &engine, "fore");

        let (min, max) = ship.mesh.bounding_box();
        let length = max.z - min.z;
        // Expected hull length: 4+3+1+5+1+3+1+5+1+5 = 29m + nacelles
        assert!(
            length > 25.0 && length < 45.0,
            "ship length {length} should be roughly 28-40m"
        );
    }
}
