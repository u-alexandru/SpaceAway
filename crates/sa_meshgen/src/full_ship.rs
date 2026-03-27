//! Single-function ship builder — the entire vessel as one continuous mesh.
//!
//! No connection points, no attach(), no inter-part transforms.
//! Every piece is placed at exact absolute (x, y, z) coordinates and merged.

use glam::{Mat4, Vec3};

use crate::colors;
use crate::hull::{
    console_mesh, door_frame_mesh, hex_cap, hex_hull, hex_ring, interior_ceiling, interior_floor,
};
use crate::mesh::Mesh;
use crate::primitives::{cone_mesh, cylinder_mesh};

// ---------------------------------------------------------------------------
// Ship geometry constants
// ---------------------------------------------------------------------------

const HEX_HEIGHT: f32 = 3.0;
const FLOOR_Y: f32 = -1.0;
const CEILING_Y: f32 = 0.2;
const DOOR_W: f32 = 1.2;
const DOOR_H: f32 = 2.0;
const DOOR_FRAME_THICK: f32 = 0.1;
const NACELLE_COLOR: [f32; 3] = [0.30, 0.30, 0.35];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Translate a mesh to an absolute Z position (plus optional x/y).
fn at(mesh: Mesh, x: f32, y: f32, z: f32) -> Mesh {
    mesh.transform(Mat4::from_translation(Vec3::new(x, y, z)))
}

/// Translate only in Z.
fn at_z(mesh: Mesh, z: f32) -> Mesh {
    at(mesh, 0.0, 0.0, z)
}

/// Interior floor for a section — width inset from hull so it doesn't poke through.
fn section_floor(hull_width: f32, length: f32, z: f32) -> Mesh {
    let floor_w = hull_width - 0.5;
    at_z(interior_floor(floor_w, length, FLOOR_Y, colors::FLOOR), z)
}

/// Interior ceiling for a section.
fn section_ceiling(hull_width: f32, length: f32, z: f32) -> Mesh {
    let ceil_w = hull_width - 0.5;
    at_z(
        interior_ceiling(ceil_w, length, CEILING_Y, colors::CEILING),
        z,
    )
}

/// Door frame at a Z boundary.
fn door_at_z(z: f32) -> Mesh {
    let frame = door_frame_mesh(DOOR_W, DOOR_H, DOOR_FRAME_THICK, colors::HULL_ACCENT);
    at(frame, 0.0, FLOOR_Y, z)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the entire ship as one continuous mesh.
///
/// Layout along Z (bow at z=0, stern at z=29):
///
/// ```text
/// Z= 0.. 4  COCKPIT          tapered 2.0 → 4.0
/// Z= 4.. 7  CORRIDOR          4.0
/// Z= 7.. 8  TRANSITION        4.0 → 5.0
/// Z= 8..13  NAV/SENSORS ROOM  5.0
/// Z=13..14  TRANSITION        5.0 → 4.0
/// Z=14..17  CORRIDOR           4.0
/// Z=17..18  TRANSITION        4.0 → 5.0
/// Z=18..23  ENGINEERING ROOM  5.0
/// Z=23..24  TRANSITION        5.0 → 3.5
/// Z=24..29  ENGINE SECTION    tapered 3.5 → 2.0
/// ```
#[allow(clippy::vec_init_then_push)]
pub fn build_ship() -> Mesh {
    let mut parts: Vec<Mesh> = Vec::new();

    // -----------------------------------------------------------------------
    // HULL SEGMENTS — each hex_hull builds from z=0..length, then translate.
    // Boundaries share the same width so rings match exactly.
    // -----------------------------------------------------------------------

    // Z=0..4  Cockpit (tapered 2.0 → 4.0)
    parts.push(at_z(
        hex_hull(2.0, 4.0, HEX_HEIGHT, 4.0, colors::HULL_EXTERIOR),
        0.0,
    ));

    // Z=4..7  Corridor (4.0)
    parts.push(at_z(
        hex_hull(4.0, 4.0, HEX_HEIGHT, 3.0, colors::HULL_EXTERIOR),
        4.0,
    ));

    // Z=7..8  Transition (4.0 → 5.0)
    parts.push(at_z(
        hex_hull(4.0, 5.0, HEX_HEIGHT, 1.0, colors::HULL_EXTERIOR),
        7.0,
    ));

    // Z=8..13  Nav/Sensors Room (5.0)
    parts.push(at_z(
        hex_hull(5.0, 5.0, HEX_HEIGHT, 5.0, colors::HULL_EXTERIOR),
        8.0,
    ));

    // Z=13..14  Transition (5.0 → 4.0)
    parts.push(at_z(
        hex_hull(5.0, 4.0, HEX_HEIGHT, 1.0, colors::HULL_EXTERIOR),
        13.0,
    ));

    // Z=14..17  Corridor (4.0)
    parts.push(at_z(
        hex_hull(4.0, 4.0, HEX_HEIGHT, 3.0, colors::HULL_EXTERIOR),
        14.0,
    ));

    // Z=17..18  Transition (4.0 → 5.0)
    parts.push(at_z(
        hex_hull(4.0, 5.0, HEX_HEIGHT, 1.0, colors::HULL_EXTERIOR),
        17.0,
    ));

    // Z=18..23  Engineering Room (5.0)
    parts.push(at_z(
        hex_hull(5.0, 5.0, HEX_HEIGHT, 5.0, colors::HULL_EXTERIOR),
        18.0,
    ));

    // Z=23..24  Transition (5.0 → 3.5)
    parts.push(at_z(
        hex_hull(5.0, 3.5, HEX_HEIGHT, 1.0, colors::HULL_EXTERIOR),
        23.0,
    ));

    // Z=24..29  Engine Section (tapered 3.5 → 2.0)
    parts.push(at_z(
        hex_hull(3.5, 2.0, HEX_HEIGHT, 5.0, colors::HULL_EXTERIOR),
        24.0,
    ));

    // -----------------------------------------------------------------------
    // CAPS
    // -----------------------------------------------------------------------

    // Cockpit front cap (glass) at z=0, width=2.0
    let front_ring = hex_ring(2.0, HEX_HEIGHT, 0.0);
    parts.push(hex_cap(&front_ring, colors::WINDOW_GLASS, false));

    // Engine back cap at z=29, width=2.0
    let back_ring = hex_ring(2.0, HEX_HEIGHT, 29.0);
    parts.push(hex_cap(&back_ring, colors::HULL_EXTERIOR, true));

    // -----------------------------------------------------------------------
    // INTERIOR FLOORS & CEILINGS
    // -----------------------------------------------------------------------

    // Cockpit (z=0..4, w=variable — use narrower average ~3.0)
    parts.push(section_floor(3.0, 4.0, 0.0));
    parts.push(section_ceiling(3.0, 4.0, 0.0));

    // Corridor 1 (z=4..7, w=4.0)
    parts.push(section_floor(4.0, 3.0, 4.0));
    parts.push(section_ceiling(4.0, 3.0, 4.0));

    // Transition (z=7..8, w~4.5)
    parts.push(section_floor(4.0, 1.0, 7.0));
    parts.push(section_ceiling(4.0, 1.0, 7.0));

    // Nav room (z=8..13, w=5.0)
    parts.push(section_floor(5.0, 5.0, 8.0));
    parts.push(section_ceiling(5.0, 5.0, 8.0));

    // Transition (z=13..14, w~4.5)
    parts.push(section_floor(4.0, 1.0, 13.0));
    parts.push(section_ceiling(4.0, 1.0, 13.0));

    // Corridor 2 (z=14..17, w=4.0)
    parts.push(section_floor(4.0, 3.0, 14.0));
    parts.push(section_ceiling(4.0, 3.0, 14.0));

    // Transition (z=17..18, w~4.5)
    parts.push(section_floor(4.0, 1.0, 17.0));
    parts.push(section_ceiling(4.0, 1.0, 17.0));

    // Engineering room (z=18..23, w=5.0)
    parts.push(section_floor(5.0, 5.0, 18.0));
    parts.push(section_ceiling(5.0, 5.0, 18.0));

    // Transition (z=23..24, w~4.25)
    parts.push(section_floor(3.5, 1.0, 23.0));
    parts.push(section_ceiling(3.5, 1.0, 23.0));

    // -----------------------------------------------------------------------
    // DOOR FRAMES at section boundaries
    // -----------------------------------------------------------------------
    for &z in &[4.0, 7.0, 8.0, 13.0, 14.0, 17.0, 18.0, 23.0] {
        parts.push(door_at_z(z));
    }

    // -----------------------------------------------------------------------
    // CONSOLES
    // -----------------------------------------------------------------------

    // Cockpit: helm console at z=1, facing aft (+Z)
    parts.push(at(
        console_mesh(1.0, colors::ACCENT_HELM),
        0.0,
        FLOOR_Y,
        1.0,
    ));

    // Nav room: navigation console at port (left) wall, z=10
    // Rotate 90 degrees to face starboard (+X)
    {
        let nav_console = console_mesh(1.0, colors::ACCENT_NAVIGATION);
        let rotated = nav_console.transform(Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2));
        parts.push(at(rotated, -2.0, FLOOR_Y, 10.0));
    }

    // Nav room: sensors console at starboard (right) wall, z=10
    // Rotate -90 degrees to face port (-X)
    {
        let sens_console = console_mesh(1.0, colors::ACCENT_SENSORS);
        let rotated = sens_console.transform(Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2));
        parts.push(at(rotated, 2.0, FLOOR_Y, 10.0));
    }

    // Engineering: console at aft wall, z=22, facing fore (-Z)
    {
        let eng_console = console_mesh(1.2, colors::ACCENT_ENGINEERING);
        let rotated = eng_console.transform(Mat4::from_rotation_y(std::f32::consts::PI));
        parts.push(at(rotated, 0.0, FLOOR_Y, 22.0));
    }

    // -----------------------------------------------------------------------
    // ENGINE NACELLES — two cylinders extending from z=29 to z=32
    // -----------------------------------------------------------------------

    // Nacelles are along Z, but cylinder_mesh builds along Y. Rotate -90 deg
    // around X so +Y becomes +Z.
    let nacelle_rot = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    // Left nacelle: x = -0.8
    {
        let cyl = cylinder_mesh(0.5, 3.0, 12, NACELLE_COLOR);
        let rotated = cyl.transform(nacelle_rot);
        // cylinder center at origin after rotation → center at z=0, shift to z=30.5
        parts.push(at(rotated, -0.8, 0.0, 30.5));
    }

    // Right nacelle: x = +0.8
    {
        let cyl = cylinder_mesh(0.5, 3.0, 12, NACELLE_COLOR);
        let rotated = cyl.transform(nacelle_rot);
        parts.push(at(rotated, 0.8, 0.0, 30.5));
    }

    // -----------------------------------------------------------------------
    // ENGINE NOZZLES — cones at the tips of the nacelles (z=32..33)
    // -----------------------------------------------------------------------

    // Left nozzle
    {
        let nozzle = cone_mesh(0.5, 0.0, 1.0, 12, colors::ACCENT_ENGINE);
        let rotated = nozzle.transform(nacelle_rot);
        parts.push(at(rotated, -0.8, 0.0, 32.5));
    }

    // Right nozzle
    {
        let nozzle = cone_mesh(0.5, 0.0, 1.0, 12, colors::ACCENT_ENGINE);
        let rotated = nozzle.transform(nacelle_rot);
        parts.push(at(rotated, 0.8, 0.0, 32.5));
    }

    // -----------------------------------------------------------------------
    // RADIATOR FINS — flat boxes extending from engineering room sides
    // -----------------------------------------------------------------------
    {
        use crate::primitives::box_mesh;
        // Left fin: extends from hull surface to the left
        let fin_mesh = box_mesh(2.0, 0.05, 4.0, colors::RADIATOR_FIN);
        parts.push(at(fin_mesh.clone(), -3.5, 0.0, 20.5));
        // Right fin
        parts.push(at(fin_mesh, 3.5, 0.0, 20.5));
    }

    // -----------------------------------------------------------------------
    // COCKPIT ANTENNA — thin cylinder on top at z=1, extending 1.5m upward
    // -----------------------------------------------------------------------
    {
        let antenna = cylinder_mesh(0.03, 1.5, 6, colors::ANTENNA);
        // cylinder along Y, center at origin → top at 0.75. Shift up so base
        // sits on hull top. Hull top ≈ HEX_HEIGHT/2 = 1.5
        parts.push(at(antenna, 0.0, 1.5 + 0.75, 1.0));
    }

    // -----------------------------------------------------------------------
    // SENSOR DISH — cylinder + cone on top of nav room at z=10
    // -----------------------------------------------------------------------
    {
        // Dish mast
        let mast = cylinder_mesh(0.06, 0.8, 6, colors::HULL_ACCENT);
        parts.push(at(mast, 0.0, 1.5 + 0.4, 10.0));

        // Dish (cone, wider at top)
        let dish = cone_mesh(0.5, 0.1, 0.3, 12, colors::HULL_ACCENT);
        parts.push(at(dish, 0.0, 1.5 + 0.8 + 0.15, 10.0));
    }

    // -----------------------------------------------------------------------
    // MERGE everything
    // -----------------------------------------------------------------------
    Mesh::merge(&parts)
}

// ---------------------------------------------------------------------------
// Individual sections for preview (key-6 cycling)
// ---------------------------------------------------------------------------

/// Cockpit section alone (z=0..4).
pub fn build_cockpit() -> Mesh {
    let mut parts = Vec::new();
    parts.push(hex_hull(2.0, 4.0, HEX_HEIGHT, 4.0, colors::HULL_EXTERIOR));
    let front_ring = hex_ring(2.0, HEX_HEIGHT, 0.0);
    parts.push(hex_cap(&front_ring, colors::WINDOW_GLASS, false));
    parts.push(section_floor(3.0, 4.0, 0.0));
    parts.push(section_ceiling(3.0, 4.0, 0.0));
    parts.push(console_mesh(1.0, colors::ACCENT_HELM));
    Mesh::merge(&parts)
}

/// Nav/Sensors room alone (z=0..5, local coords).
pub fn build_nav_room() -> Mesh {
    let mut parts = Vec::new();
    parts.push(hex_hull(5.0, 5.0, HEX_HEIGHT, 5.0, colors::HULL_EXTERIOR));
    parts.push(section_floor(5.0, 5.0, 0.0));
    parts.push(section_ceiling(5.0, 5.0, 0.0));
    {
        let nav_console = console_mesh(1.0, colors::ACCENT_NAVIGATION);
        let rotated = nav_console.transform(Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2));
        parts.push(at(rotated, -2.0, FLOOR_Y, 2.5));
    }
    {
        let sens_console = console_mesh(1.0, colors::ACCENT_SENSORS);
        let rotated = sens_console.transform(Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2));
        parts.push(at(rotated, 2.0, FLOOR_Y, 2.5));
    }
    Mesh::merge(&parts)
}

/// Engine section alone (z=0..5 hull + nacelles + nozzles, local coords).
pub fn build_engine_section() -> Mesh {
    let mut parts = Vec::new();
    parts.push(hex_hull(
        3.5,
        2.0,
        HEX_HEIGHT,
        5.0,
        colors::HULL_EXTERIOR,
    ));
    let back_ring = hex_ring(2.0, HEX_HEIGHT, 5.0);
    parts.push(hex_cap(&back_ring, colors::HULL_EXTERIOR, true));

    let nacelle_rot = Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for &x in &[-0.8_f32, 0.8] {
        let cyl = cylinder_mesh(0.5, 3.0, 12, NACELLE_COLOR);
        parts.push(at(cyl.transform(nacelle_rot), x, 0.0, 6.5));

        let nozzle = cone_mesh(0.5, 0.0, 1.0, 12, colors::ACCENT_ENGINE);
        parts.push(at(nozzle.transform(nacelle_rot), x, 0.0, 8.5));
    }

    Mesh::merge(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_ship_not_empty() {
        let ship = build_ship();
        assert!(!ship.vertices.is_empty());
        assert!(!ship.indices.is_empty());
        assert!(
            ship.triangle_count() > 100,
            "full ship should have many triangles, got {}",
            ship.triangle_count()
        );
    }

    #[test]
    fn build_ship_spans_z_axis() {
        let ship = build_ship();
        let (min, max) = ship.bounding_box();
        // Ship goes from z=0 to z≈33 (nacelle nozzle tips)
        assert!(min.z < 0.5, "ship should start near z=0, got {}", min.z);
        assert!(max.z > 29.0, "ship should extend past z=29, got {}", max.z);
    }

    #[test]
    fn build_ship_no_degenerate_triangles() {
        let ship = build_ship();
        for tri in ship.indices.chunks_exact(3) {
            let a = glam::Vec3::from(ship.vertices[tri[0] as usize].position);
            let b = glam::Vec3::from(ship.vertices[tri[1] as usize].position);
            let c = glam::Vec3::from(ship.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-7, "triangle should have non-zero area");
        }
    }

    #[test]
    fn preview_sections_not_empty() {
        let cockpit = build_cockpit();
        assert!(cockpit.triangle_count() > 10);

        let nav = build_nav_room();
        assert!(nav.triangle_count() > 10);

        let engine = build_engine_section();
        assert!(engine.triangle_count() > 10);
    }
}
