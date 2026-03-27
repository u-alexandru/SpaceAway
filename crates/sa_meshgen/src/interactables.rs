//! Mesh generators for ship interactable objects.
//!
//! Each function returns a `Mesh` centered at origin. The caller positions
//! them in ship space using `mesh.transform()`.

use crate::colors;
use crate::mesh::Mesh;
use crate::primitives::box_mesh;
use glam::{Mat4, Vec3};

/// Lever mesh: a visible handle on a track with a base.
///
/// - Handle: box 0.06 x 0.15 x 0.06 (grippable knob)
/// - Shaft: box 0.03 x 0.25 x 0.03 (connects handle to base)
/// - Track: box 0.08 x 0.5 x 0.04 (visual guide showing travel range)
/// - Base: box 0.15 x 0.04 x 0.10 (mounted on console)
///
/// The handle slides along the track's 0.5m travel range.
pub fn lever_mesh(lever_position: f32) -> Mesh {
    let pos = lever_position.clamp(0.0, 1.0);
    let travel = 0.5;
    let rod_y_offset = -travel / 2.0 + pos * travel;

    let mut meshes = Vec::new();

    // Track (vertical guide rail)
    let track = box_mesh(0.08, travel, 0.04, colors::HULL_ACCENT);
    meshes.push(track);

    // Base plate
    let base = box_mesh(0.15, 0.04, 0.10, colors::FLOOR);
    let base = base.transform(Mat4::from_translation(Vec3::new(
        0.0,
        -travel / 2.0 - 0.02,
        0.0,
    )));
    meshes.push(base);

    // Shaft (vertical rod connecting handle to track)
    let shaft = box_mesh(0.03, 0.20, 0.03, colors::ACCENT_HELM);
    let shaft = shaft.transform(Mat4::from_translation(Vec3::new(0.0, rod_y_offset, 0.04)));
    meshes.push(shaft);

    // Handle knob (grippable top)
    let handle = box_mesh(0.06, 0.06, 0.06, colors::ACCENT_ENGINE);
    let handle = handle.transform(Mat4::from_translation(Vec3::new(0.0, rod_y_offset + 0.13, 0.04)));
    meshes.push(handle);

    Mesh::merge(&meshes)
}

/// Button mesh: a visible raised box on a base plate.
///
/// - Button face: box 0.12 x 0.12 x 0.06 (clearly visible)
/// - Base plate: box 0.18 x 0.04 x 0.18
///
/// `pressed`: if true, the button face is depressed (lower Y offset).
pub fn button_mesh(pressed: bool) -> Mesh {
    let mut meshes = Vec::new();

    // Base plate
    let base = box_mesh(0.18, 0.04, 0.18, colors::FLOOR);
    meshes.push(base);

    // Button face
    let button_y = if pressed { 0.025 } else { 0.05 };
    let button = box_mesh(0.12, 0.06, 0.12, colors::ACCENT_ENGINE);
    let button = button.transform(Mat4::from_translation(Vec3::new(0.0, button_y, 0.0)));
    meshes.push(button);

    Mesh::merge(&meshes)
}

/// Switch mesh: an angled handle on a base.
///
/// - Base: box 0.1 x 0.02 x 0.06
/// - Handle: box 0.04 x 0.12 x 0.03, angled based on position
///
/// `position`: current position (0 to num_positions-1)
/// `num_positions`: total number of positions
pub fn switch_mesh(position: u8, num_positions: u8) -> Mesh {
    let mut meshes = Vec::new();

    // Base
    let base = box_mesh(0.1, 0.02, 0.06, colors::FLOOR);
    meshes.push(base);

    // Handle angle: spread positions across -30 to +30 degrees
    let max_angle = std::f32::consts::FRAC_PI_6; // 30 degrees
    let t = if num_positions > 1 {
        position as f32 / (num_positions - 1) as f32
    } else {
        0.5
    };
    let angle = -max_angle + t * 2.0 * max_angle;

    let handle = box_mesh(0.04, 0.12, 0.03, colors::ACCENT_ENGINEERING);
    let handle = handle
        .transform(Mat4::from_translation(Vec3::new(0.0, 0.07, 0.0)) * Mat4::from_rotation_z(angle));
    meshes.push(handle);

    Mesh::merge(&meshes)
}

/// Screen mesh: a flat panel with a colored face.
///
/// - Panel: box width x height x 0.02
/// - Screen face is colored CONSOLE_SCREEN
pub fn screen_mesh(width: f32, height: f32) -> Mesh {
    box_mesh(width, height, 0.02, colors::CONSOLE_SCREEN)
}

/// Helm seat mesh: a box seat with a box back.
///
/// - Seat: box 0.5 x 0.1 x 0.5
/// - Back: box 0.5 x 0.6 x 0.1, positioned behind and above seat
pub fn helm_seat_mesh() -> Mesh {
    let mut meshes = Vec::new();

    // Seat (horizontal surface)
    let seat = box_mesh(0.5, 0.1, 0.5, colors::HULL_ACCENT);
    meshes.push(seat);

    // Back (vertical surface behind seat, toward aft/+Z since person faces -Z/forward)
    let back = box_mesh(0.5, 0.6, 0.1, colors::HULL_ACCENT);
    let back = back.transform(Mat4::from_translation(Vec3::new(0.0, 0.35, 0.3)));
    meshes.push(back);

    Mesh::merge(&meshes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn assert_mesh_valid(mesh: &Mesh, name: &str) {
        assert!(!mesh.vertices.is_empty(), "{name} should have vertices");
        assert!(!mesh.indices.is_empty(), "{name} should have indices");
        // All indices valid
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.vertices.len(),
                "{name}: index {idx} out of bounds (len={})",
                mesh.vertices.len()
            );
        }
        // No degenerate triangles
        for tri in mesh.indices.chunks_exact(3) {
            let a = Vec3::from(mesh.vertices[tri[0] as usize].position);
            let b = Vec3::from(mesh.vertices[tri[1] as usize].position);
            let c = Vec3::from(mesh.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(
                area > 1e-8,
                "{name}: degenerate triangle with area {area}"
            );
        }
    }

    #[test]
    fn lever_mesh_valid() {
        assert_mesh_valid(&lever_mesh(0.0), "lever_0");
        assert_mesh_valid(&lever_mesh(0.5), "lever_0.5");
        assert_mesh_valid(&lever_mesh(1.0), "lever_1");
    }

    #[test]
    fn button_mesh_valid() {
        assert_mesh_valid(&button_mesh(false), "button_up");
        assert_mesh_valid(&button_mesh(true), "button_down");
    }

    #[test]
    fn switch_mesh_valid() {
        assert_mesh_valid(&switch_mesh(0, 3), "switch_0");
        assert_mesh_valid(&switch_mesh(1, 3), "switch_1");
        assert_mesh_valid(&switch_mesh(2, 3), "switch_2");
    }

    #[test]
    fn screen_mesh_valid() {
        assert_mesh_valid(&screen_mesh(0.4, 0.25), "screen");
    }

    #[test]
    fn helm_seat_mesh_valid() {
        assert_mesh_valid(&helm_seat_mesh(), "helm_seat");
    }

    #[test]
    fn lever_rod_moves_with_position() {
        let low = lever_mesh(0.0);
        let high = lever_mesh(1.0);
        let (_, low_max) = low.bounding_box();
        let (_, high_max) = high.bounding_box();
        // The rod at position 1.0 should be higher than at 0.0
        assert!(
            high_max.y > low_max.y,
            "lever at 1.0 should be taller: high_max.y={}, low_max.y={}",
            high_max.y, low_max.y
        );
    }

    #[test]
    fn button_depresses_when_pressed() {
        let up = button_mesh(false);
        let down = button_mesh(true);
        let (_, up_max) = up.bounding_box();
        let (_, down_max) = down.bounding_box();
        assert!(
            up_max.y > down_max.y,
            "unpressed button should be taller: up={}, down={}",
            up_max.y, down_max.y
        );
    }

    #[test]
    fn helm_seat_has_back() {
        let mesh = helm_seat_mesh();
        let (_, max) = mesh.bounding_box();
        // Back extends above the seat
        assert!(
            max.y > 0.3,
            "helm seat should have a back, max.y = {}",
            max.y
        );
    }
}
