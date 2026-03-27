//! Ship part catalog: functions that return Part (mesh + connection points).
//!
//! Each part uses primitives + merge + transform to compose geometry.
//! All dimensions in meters, centered at origin unless noted.

use crate::assembly::{ConnectPoint, Part};
use crate::colors;
use crate::mesh::Mesh;
use crate::primitives::{box_mesh, cylinder_mesh};
use crate::primitives_ext::wedge_mesh;
use glam::{Mat4, Vec3};

/// Rectangular corridor tube: 3m wide, 2.5m tall, 2m deep.
/// Open both ends (just the walls, floor, ceiling).
/// Connections: "fore" (+Z), "aft" (-Z).
pub fn hull_corridor() -> Part {
    let w = 3.0;
    let h = 2.5;
    let d = 2.0;
    let wall = 0.1; // wall thickness

    // Build as a box with the full exterior
    let exterior = box_mesh(w, h, d, colors::HULL_EXTERIOR);

    // Floor panel (thin box at the bottom)
    let floor = box_mesh(w - wall * 2.0, 0.05, d, colors::FLOOR);
    let floor = floor.transform(Mat4::from_translation(Vec3::new(
        0.0,
        -h / 2.0 + wall + 0.025,
        0.0,
    )));

    let mesh = Mesh::merge(&[exterior, floor]);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "fore",
                position: Vec3::new(0.0, 0.0, d / 2.0),
                normal: Vec3::Z,
            },
            ConnectPoint {
                id: "aft",
                position: Vec3::new(0.0, 0.0, -d / 2.0),
                normal: Vec3::NEG_Z,
            },
        ],
    }
}

/// Box room: 5m wide, 5m deep, 3m tall.
/// Connections: "fore" (+Z), "aft" (-Z), "port" (-X), "starboard" (+X).
pub fn hull_room() -> Part {
    let w = 5.0;
    let h = 3.0;
    let d = 5.0;
    let wall = 0.1;

    let exterior = box_mesh(w, h, d, colors::HULL_EXTERIOR);

    // Interior walls (slightly smaller, flipped normals)
    let mut interior = box_mesh(
        w - wall * 2.0,
        h - wall * 2.0,
        d - wall * 2.0,
        colors::INTERIOR_WALL,
    );
    interior.flip_normals();
    interior.flip_winding();

    let floor = box_mesh(w - wall * 2.0, 0.05, d - wall * 2.0, colors::FLOOR);
    let floor = floor.transform(Mat4::from_translation(Vec3::new(
        0.0,
        -h / 2.0 + wall + 0.025,
        0.0,
    )));

    let mesh = Mesh::merge(&[exterior, interior, floor]);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "fore",
                position: Vec3::new(0.0, 0.0, d / 2.0),
                normal: Vec3::Z,
            },
            ConnectPoint {
                id: "aft",
                position: Vec3::new(0.0, 0.0, -d / 2.0),
                normal: Vec3::NEG_Z,
            },
            ConnectPoint {
                id: "port",
                position: Vec3::new(-w / 2.0, 0.0, 0.0),
                normal: Vec3::NEG_X,
            },
            ConnectPoint {
                id: "starboard",
                position: Vec3::new(w / 2.0, 0.0, 0.0),
                normal: Vec3::X,
            },
        ],
    }
}

/// Cockpit: 5m wide, 4m deep, 2.5m tall. Tapered front with angled panels.
/// Connection: "aft" (-Z).
pub fn hull_cockpit() -> Part {
    let w = 5.0;
    let h = 2.5;
    let d = 4.0;

    // Main body is a box for the rear section
    let body = box_mesh(w, h, d * 0.6, colors::HULL_EXTERIOR);
    let body = body.transform(Mat4::from_translation(Vec3::new(0.0, 0.0, -d * 0.2)));

    // Tapered front: wedge
    let nose = wedge_mesh(w, h, d * 0.4, colors::HULL_EXTERIOR);
    // Rotate the wedge so it points forward (+Z)
    let nose = nose.transform(
        Mat4::from_translation(Vec3::new(0.0, 0.0, d * 0.2))
            * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );

    // Window panel (thin colored strip on the front)
    let window = box_mesh(w * 0.6, h * 0.3, 0.02, colors::ACCENT_HELM);
    let window = window.transform(Mat4::from_translation(Vec3::new(0.0, h * 0.1, d * 0.35)));

    let mesh = Mesh::merge(&[body, nose, window]);

    Part {
        mesh,
        connections: vec![ConnectPoint {
            id: "aft",
            position: Vec3::new(0.0, 0.0, -d / 2.0),
            normal: Vec3::NEG_Z,
        }],
    }
}

/// Engine section: 4m wide, 3m tall, 3m deep. Rear section with nacelle geometry.
/// Connection: "fore" (+Z).
pub fn hull_engine() -> Part {
    let w = 4.0;
    let h = 3.0;
    let d = 3.0;

    // Main housing
    let housing = box_mesh(w, h, d, colors::HULL_EXTERIOR);

    // Engine nacelles (two cylinders at the back)
    let nacelle_r = 0.5;
    let nacelle_len = 1.5;
    let nacelle = cylinder_mesh(nacelle_r, nacelle_len, 8, colors::ACCENT_ENGINE);

    // Rotate nacelles to point along -Z and position them
    let rot = Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let nacelle_left = nacelle.transform(
        Mat4::from_translation(Vec3::new(-w * 0.3, 0.0, -d / 2.0 - nacelle_len / 2.0)) * rot,
    );
    let nacelle_right = nacelle.transform(
        Mat4::from_translation(Vec3::new(w * 0.3, 0.0, -d / 2.0 - nacelle_len / 2.0)) * rot,
    );

    let mesh = Mesh::merge(&[housing, nacelle_left, nacelle_right]);

    Part {
        mesh,
        connections: vec![ConnectPoint {
            id: "fore",
            position: Vec3::new(0.0, 0.0, d / 2.0),
            normal: Vec3::Z,
        }],
    }
}

/// Airlock: 2m wide, 2.5m tall, 2m deep. Small room with two door frames.
/// Connections: "inner" (+Z), "outer" (-Z).
pub fn hull_airlock() -> Part {
    let w = 2.0;
    let h = 2.5;
    let d = 2.0;

    let exterior = box_mesh(w, h, d, colors::AIRLOCK_WARNING);

    // Inner door frame accent
    let frame_inner = door_frame();
    let frame_inner =
        frame_inner
            .mesh
            .transform(Mat4::from_translation(Vec3::new(0.0, 0.0, d / 2.0 - 0.1)));

    // Outer door frame accent
    let frame_outer = door_frame();
    let frame_outer = frame_outer.mesh.transform(Mat4::from_translation(Vec3::new(
        0.0,
        0.0,
        -d / 2.0 + 0.1,
    )));

    let mesh = Mesh::merge(&[exterior, frame_inner, frame_outer]);

    Part {
        mesh,
        connections: vec![
            ConnectPoint {
                id: "inner",
                position: Vec3::new(0.0, 0.0, d / 2.0),
                normal: Vec3::Z,
            },
            ConnectPoint {
                id: "outer",
                position: Vec3::new(0.0, 0.0, -d / 2.0),
                normal: Vec3::NEG_Z,
            },
        ],
    }
}

/// Angled control panel: 1.2m wide, 0.8m tall, 0.6m deep.
/// No connections (furniture, not structural).
pub fn console() -> Part {
    let w = 1.2;
    let h = 0.8;
    let d = 0.6;

    // Base: a box for the lower body
    let base = box_mesh(w, h * 0.6, d, colors::INTERIOR_WALL);
    let base = base.transform(Mat4::from_translation(Vec3::new(0.0, -h * 0.2, 0.0)));

    // Screen: angled top panel (wedge)
    let screen = wedge_mesh(w, h * 0.4, d * 0.8, colors::CONSOLE_SCREEN);
    let screen = screen.transform(Mat4::from_translation(Vec3::new(0.0, h * 0.2, 0.0)));

    let mesh = Mesh::merge(&[base, screen]);

    Part {
        mesh,
        connections: vec![],
    }
}

/// Door frame: 1.5m wide, 2m tall, 0.2m deep. Rectangular frame.
/// No connections (decorative).
pub fn door_frame() -> Part {
    let w = 1.5;
    let h = 2.0;
    let frame_thickness = 0.15;
    let d = 0.2;

    // Four frame pieces (top, bottom, left, right)
    let top = box_mesh(w, frame_thickness, d, colors::INTERIOR_WALL);
    let top = top.transform(Mat4::from_translation(Vec3::new(
        0.0,
        h / 2.0 - frame_thickness / 2.0,
        0.0,
    )));

    let bottom = box_mesh(w, frame_thickness, d, colors::INTERIOR_WALL);
    let bottom = bottom.transform(Mat4::from_translation(Vec3::new(
        0.0,
        -h / 2.0 + frame_thickness / 2.0,
        0.0,
    )));

    let left = box_mesh(frame_thickness, h, d, colors::INTERIOR_WALL);
    let left = left.transform(Mat4::from_translation(Vec3::new(
        -w / 2.0 + frame_thickness / 2.0,
        0.0,
        0.0,
    )));

    let right = box_mesh(frame_thickness, h, d, colors::INTERIOR_WALL);
    let right = right.transform(Mat4::from_translation(Vec3::new(
        w / 2.0 - frame_thickness / 2.0,
        0.0,
        0.0,
    )));

    let mesh = Mesh::merge(&[top, bottom, left, right]);

    Part {
        mesh,
        connections: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corridor_has_two_connections() {
        let p = hull_corridor();
        assert_eq!(p.connections.len(), 2);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
    }

    #[test]
    fn corridor_mesh_not_empty() {
        let p = hull_corridor();
        assert!(!p.mesh.vertices.is_empty());
        assert!(!p.mesh.indices.is_empty());
    }

    #[test]
    fn room_has_four_connections() {
        let p = hull_room();
        assert_eq!(p.connections.len(), 4);
        assert!(p.try_connection("fore").is_some());
        assert!(p.try_connection("aft").is_some());
        assert!(p.try_connection("port").is_some());
        assert!(p.try_connection("starboard").is_some());
    }

    #[test]
    fn room_mesh_not_empty() {
        let p = hull_room();
        assert!(!p.mesh.vertices.is_empty());
    }

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
    }

    #[test]
    fn engine_has_fore_connection() {
        let p = hull_engine();
        assert_eq!(p.connections.len(), 1);
        assert!(p.try_connection("fore").is_some());
    }

    #[test]
    fn engine_mesh_not_empty() {
        let p = hull_engine();
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn airlock_has_inner_outer_connections() {
        let p = hull_airlock();
        assert_eq!(p.connections.len(), 2);
        assert!(p.try_connection("inner").is_some());
        assert!(p.try_connection("outer").is_some());
    }

    #[test]
    fn console_has_no_connections() {
        let p = console();
        assert_eq!(p.connections.len(), 0);
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn door_frame_has_no_connections() {
        let p = door_frame();
        assert_eq!(p.connections.len(), 0);
        assert!(!p.mesh.vertices.is_empty());
    }

    #[test]
    fn all_parts_have_valid_indices() {
        let parts: Vec<Part> = vec![
            hull_corridor(),
            hull_room(),
            hull_cockpit(),
            hull_engine(),
            hull_airlock(),
            console(),
            door_frame(),
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
    fn corridor_connections_face_opposite_directions() {
        let p = hull_corridor();
        let fore = p.connection("fore");
        let aft = p.connection("aft");
        let dot = fore.normal.dot(aft.normal);
        assert!(
            (dot - (-1.0)).abs() < 1e-4,
            "fore and aft should face opposite"
        );
    }
}
