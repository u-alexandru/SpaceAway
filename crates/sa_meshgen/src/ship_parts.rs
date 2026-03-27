//! Ship part catalog: functions that return Part (mesh + connection points).
//!
//! Every part has BOTH an exterior hexagonal hull AND interior detail
//! (floor, ceiling, walls, consoles, door frames). All dimensions in meters.
//! Parts are built starting at z=0, extending along +Z.

use crate::assembly::{ConnectPoint, Part};
use crate::colors;
use crate::hull;
use crate::mesh::Mesh;
use crate::primitives::{box_mesh, cone_mesh, cylinder_mesh};
use glam::{Mat4, Vec3};

// ---------------------------------------------------------------------------
// Shared constants
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
/// Interior: floor, ceiling, helm console at front, window panels.
/// Connection: "aft" at z=4.0 (facing +Z).
pub fn hull_cockpit() -> Part {
    let front_w = 2.0;
    let back_w = STD_WIDTH;
    let length = 4.0;

    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(front_w, back_w, STD_HEIGHT, length, colors::HULL_EXTERIOR));

    // Front cap (nose)
    let front_ring = hex_ring_at(front_w, STD_HEIGHT, 0.0);
    meshes.push(hull::hex_cap(&front_ring, colors::WINDOW_GLASS, false));

    // Interior: floor, ceiling, walls
    let interior_w = back_w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));
    meshes.push(hull::interior_walls(
        back_w, FLOOR_Y, CEILING_Y, length, WALL_INSET, colors::INTERIOR_WALL,
    ));

    // Helm console near the front
    let console = hull::console_mesh(1.2, colors::ACCENT_HELM);
    let console = console.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.8)));
    meshes.push(console);

    // Door frame at aft
    let frame = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame = frame.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, length)));
    meshes.push(frame);

    // --- Structural features ---

    // Antenna array: thin cylinder extending forward from top of cockpit
    let antenna = cylinder_mesh(0.03, 2.5, 6, colors::ANTENNA);
    // Cylinder is along Y by default; rotate to point along -Z (forward)
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
        }],
    }
}

// ---------------------------------------------------------------------------
// Corridor
// ---------------------------------------------------------------------------

/// Standard hex corridor, width=4.0, variable length.
/// Interior: floor, ceiling, walls, door frames at both ends.
/// Connections: "fore" at z=0 (facing -Z), "aft" at z=length (facing +Z).
pub fn hull_corridor(length: f32) -> Part {
    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(STD_WIDTH, STD_WIDTH, STD_HEIGHT, length, colors::HULL_EXTERIOR));

    // Interior
    let interior_w = STD_WIDTH - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));
    meshes.push(hull::interior_walls(
        STD_WIDTH, FLOOR_Y, CEILING_Y, length, WALL_INSET, colors::INTERIOR_WALL,
    ));

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
            },
            ConnectPoint {
                id: "aft",
                position: Vec3::new(0.0, 0.0, length),
                normal: Vec3::Z,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Transition
// ---------------------------------------------------------------------------

/// Transition piece: tapers from `from_width` to `to_width` over `length` along Z.
/// Connections: "fore" at z=0 (facing -Z), "aft" at z=length (facing +Z).
pub fn hull_transition(from_width: f32, to_width: f32, length: f32) -> Part {
    let mut meshes = Vec::new();

    meshes.push(hull::hex_hull(from_width, to_width, STD_HEIGHT, length, colors::HULL_ACCENT));

    // Interior floor at the wider width
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
            },
            ConnectPoint {
                id: "aft",
                position: Vec3::new(0.0, 0.0, length),
                normal: Vec3::Z,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Room
// ---------------------------------------------------------------------------

/// Wider hex room section (width=5.0, length=5.0).
/// Includes built-in transition pieces on fore and aft (5.0 -> 4.0).
/// Interior: floor, ceiling, walls, console with accent color, door frames.
/// Connections: "fore" at z=0, "aft" at z=length.
/// Optionally "port" and/or "starboard" if `side_doors` contains those names.
pub fn hull_room(
    name: &str,
    accent_color: [f32; 3],
    side_doors: &[&str],
) -> Part {
    let room_len = 5.0;
    let trans_len = 1.0;
    let total_len = trans_len + room_len + trans_len; // 7.0 total

    let mut meshes = Vec::new();

    // Fore transition: STD_WIDTH -> ROOM_WIDTH (z=0..1)
    meshes.push(hull::hex_hull(
        STD_WIDTH, ROOM_WIDTH, STD_HEIGHT, trans_len, colors::HULL_ACCENT,
    ));

    // Main room hull (z=1..6)
    let room_hull = hull::hex_hull(ROOM_WIDTH, ROOM_WIDTH, STD_HEIGHT, room_len, colors::HULL_EXTERIOR);
    let room_hull = room_hull.transform(Mat4::from_translation(Vec3::new(0.0, 0.0, trans_len)));
    meshes.push(room_hull);

    // Aft transition: ROOM_WIDTH -> STD_WIDTH (z=6..7)
    let aft_trans = hull::hex_hull(ROOM_WIDTH, STD_WIDTH, STD_HEIGHT, trans_len, colors::HULL_ACCENT);
    let aft_trans = aft_trans.transform(Mat4::from_translation(Vec3::new(
        0.0, 0.0, trans_len + room_len,
    )));
    meshes.push(aft_trans);

    // Interior for the full length
    let interior_w = ROOM_WIDTH - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, total_len, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, total_len, CEILING_Y, colors::CEILING));
    meshes.push(hull::interior_walls(
        ROOM_WIDTH, FLOOR_Y, CEILING_Y, total_len, WALL_INSET, colors::INTERIOR_WALL,
    ));

    // Console on one wall with accent color (placed mid-room)
    let console = hull::console_mesh(1.5, accent_color);
    let console = console.transform(
        Mat4::from_translation(Vec3::new(-1.5, FLOOR_Y, trans_len + room_len * 0.5))
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2),
    );
    meshes.push(console);

    // Door frames at fore and aft
    let frame_fore = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_fore = frame_fore.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.0)));
    meshes.push(frame_fore);

    let frame_aft = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame_aft = frame_aft.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, total_len)));
    meshes.push(frame_aft);

    // --- Structural features ---

    // Feature: radiator fins on engineering rooms
    if name == "eng" {
        let fin_length = 3.0;
        let fin_height = 2.0;
        let fin_thick = 0.05;
        let mid_z = trans_len + room_len * 0.5;
        let fin_offset_x = ROOM_WIDTH * 0.5 + fin_length * 0.5;
        // Angle the fins slightly (15 degrees outward from vertical)
        let fin_angle = 0.26; // ~15 degrees

        // Port (left) radiator fin
        let fin = box_mesh(fin_length, fin_height, fin_thick, colors::RADIATOR_FIN);
        let port_fin = fin.transform(
            Mat4::from_translation(Vec3::new(-fin_offset_x, 0.2, mid_z))
                * Mat4::from_rotation_z(-fin_angle),
        );
        meshes.push(port_fin);

        // Starboard (right) radiator fin
        let fin = box_mesh(fin_length, fin_height, fin_thick, colors::RADIATOR_FIN);
        let starboard_fin = fin.transform(
            Mat4::from_translation(Vec3::new(fin_offset_x, 0.2, mid_z))
                * Mat4::from_rotation_z(fin_angle),
        );
        meshes.push(starboard_fin);
    }

    // Feature: sensor dish on nav/sensors room
    if name == "nav" || name == "sensors" {
        let dish_z = trans_len + room_len * 0.5;
        let dorsal_y = STD_HEIGHT * 0.5;
        // Small cylinder as the dish base
        let dish_base = cylinder_mesh(0.15, 0.4, 8, colors::ANTENNA);
        let dish_base = dish_base.transform(
            Mat4::from_translation(Vec3::new(0.0, dorsal_y + 0.2, dish_z)),
        );
        meshes.push(dish_base);
        // Cone on top as the dish
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
        },
        ConnectPoint {
            id: "aft",
            position: Vec3::new(0.0, 0.0, total_len),
            normal: Vec3::Z,
        },
    ];

    for &side in side_doors {
        match side {
            "port" => connections.push(ConnectPoint {
                id: "port",
                position: Vec3::new(-ROOM_WIDTH / 2.0, 0.0, trans_len + room_len / 2.0),
                normal: Vec3::NEG_X,
            }),
            "starboard" => connections.push(ConnectPoint {
                id: "starboard",
                position: Vec3::new(ROOM_WIDTH / 2.0, 0.0, trans_len + room_len / 2.0),
                normal: Vec3::X,
            }),
            _ => {}
        }
    }

    Part { mesh, connections }
}

// ---------------------------------------------------------------------------
// Engine section
// ---------------------------------------------------------------------------

/// Engine section: tapered hull front_width=4.0 to back_width=2.5, length=5.0.
/// Two engine nacelles (cylinders) extending from back, with cones at tips.
/// Interior: floor, engine console.
/// Connection: "fore" at z=0 (facing -Z).
pub fn hull_engine_section() -> Part {
    let front_w = STD_WIDTH;
    let back_w = 2.5;
    let length = 5.0;

    let mut meshes = Vec::new();

    // Exterior hull
    meshes.push(hull::hex_hull(front_w, back_w, STD_HEIGHT, length, colors::HULL_EXTERIOR));

    // Interior
    let interior_w = front_w - WALL_INSET * 2.0;
    meshes.push(hull::interior_floor(interior_w, length, FLOOR_Y, colors::FLOOR));
    meshes.push(hull::interior_ceiling(interior_w, length, CEILING_Y, colors::CEILING));
    meshes.push(hull::interior_walls(
        front_w, FLOOR_Y, CEILING_Y, length, WALL_INSET, colors::INTERIOR_WALL,
    ));

    // Engine console
    let console = hull::console_mesh(1.2, colors::ACCENT_ENGINE);
    let console = console.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 1.5)));
    meshes.push(console);

    // Door frame at fore
    let frame = hull::door_frame_mesh(DOOR_W, DOOR_H, FRAME_THICKNESS, colors::INTERIOR_WALL);
    let frame = frame.transform(Mat4::from_translation(Vec3::new(0.0, FLOOR_Y, 0.0)));
    meshes.push(frame);

    // Engine nacelles: two cylinders extending from the back
    let nacelle_r = 0.5;
    let nacelle_len = 3.0;
    let nacelle = cylinder_mesh(nacelle_r, nacelle_len, 8, colors::HULL_ACCENT);
    // Cylinders are along Y by default; rotate to point along Z
    let rot_z = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);

    let nacelle_left = nacelle.transform(
        Mat4::from_translation(Vec3::new(-0.8, -0.3, length + nacelle_len / 2.0)) * rot_z,
    );
    meshes.push(nacelle_left);

    let nacelle_right = nacelle.transform(
        Mat4::from_translation(Vec3::new(0.8, -0.3, length + nacelle_len / 2.0)) * rot_z,
    );
    meshes.push(nacelle_right);

    // Engine cones at the tips of the nacelles
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
        }],
    }
}

// ---------------------------------------------------------------------------
// Airlock
// ---------------------------------------------------------------------------

/// Airlock: small hex hull width=2.5, height=3.0, length=2.5.
/// Two door frames (inner and outer), yellow warning accent strips.
/// Connection: "inner" at z=0 (facing -Z).
pub fn hull_airlock() -> Part {
    let w = 2.5;
    let length = 2.5;

    let mut meshes = Vec::new();

    // Exterior hull with warning accent
    meshes.push(hull::hex_hull(w, w, STD_HEIGHT, length, colors::AIRLOCK_WARNING));

    // Outer cap (sealed end)
    let outer_ring = hex_ring_at(w, STD_HEIGHT, length);
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
        }],
    }
}

// ---------------------------------------------------------------------------
// Helper: generate hex ring at a position (re-exports hull internal logic)
// ---------------------------------------------------------------------------

/// Generate hex ring vertices at a given width, height, and z position.
/// This duplicates the logic from hull::hex_ring which is private.
fn hex_ring_at(width: f32, height: f32, z: f32) -> [[f32; 3]; 6] {
    let w = width;
    let h = height;
    [
        [-w * 0.375, h * 0.5, z],
        [w * 0.375, h * 0.5, z],
        [w * 0.5, 0.0, z],
        [w * 0.375, -h * 0.5, z],
        [-w * 0.375, -h * 0.5, z],
        [-w * 0.5, 0.0, z],
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cockpit_has_aft_connection() {
        let p = hull_cockpit();
        assert_eq!(p.connections.len(), 1);
        assert!(p.try_connection("aft").is_some());
    }

    #[test]
    fn cockpit_mesh_not_empty() {
        let p = hull_cockpit();
        assert!(!p.mesh.vertices.is_empty());
        assert!(!p.mesh.indices.is_empty());
    }

    #[test]
    fn corridor_has_two_connections() {
        let p = hull_corridor(3.0);
        assert_eq!(p.connections.len(), 2);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
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
    fn transition_has_two_connections() {
        let p = hull_transition(4.0, 5.0, 1.0);
        assert_eq!(p.connections.len(), 2);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
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
    fn full_ship_bounding_box_roughly_correct() {
        use crate::assembly::attach;

        let cockpit = hull_cockpit();
        let corr1 = hull_corridor(3.0);
        let trans1 = hull_transition(STD_WIDTH, ROOM_WIDTH, 1.0);
        let nav_room = hull_room("nav", colors::ACCENT_NAVIGATION, &[]);
        let trans2 = hull_transition(ROOM_WIDTH, STD_WIDTH, 1.0);
        let corr2 = hull_corridor(3.0);
        let trans3 = hull_transition(STD_WIDTH, ROOM_WIDTH, 1.0);
        let eng_room = hull_room("eng", colors::ACCENT_ENGINEERING, &["starboard"]);
        let trans4 = hull_transition(ROOM_WIDTH, STD_WIDTH, 1.0);
        let engine = hull_engine_section();
        let airlock = hull_airlock();

        let ship = attach(&cockpit, "aft", &corr1, "fore");
        let ship = attach(&ship, "aft", &trans1, "fore");
        let ship = attach(&ship, "aft", &nav_room, "fore");
        let ship = attach(&ship, "aft", &trans2, "fore");
        let ship = attach(&ship, "aft", &corr2, "fore");
        let ship = attach(&ship, "aft", &trans3, "fore");
        let ship = attach(&ship, "aft", &eng_room, "fore");
        let ship = attach(&ship, "aft", &trans4, "fore");
        let ship = attach(&ship, "aft", &engine, "fore");
        let ship = if ship.try_connection("starboard").is_some() {
            attach(&ship, "starboard", &airlock, "inner")
        } else {
            ship
        };

        let (min, max) = ship.mesh.bounding_box();
        let length = max.z - min.z;
        // Ship should be roughly 29m + nacelles, so allow some tolerance
        assert!(
            length > 25.0 && length < 45.0,
            "ship length {length} should be roughly 28-40m"
        );
    }
}
