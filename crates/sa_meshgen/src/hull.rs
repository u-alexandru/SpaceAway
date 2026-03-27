//! Shared hexagonal hull helpers for ship construction.
//!
//! All ship sections share a hexagonal cross-section. This module provides
//! functions to build hull segments, interior panels, door frames, and consoles.

use crate::mesh::{Mesh, MeshVertex};
use crate::primitives::{face_normal, push_quad};

/// Build the hex profile vertices at a given Z position and width.
///
/// Returns 6 vertices forming the hexagonal cross-section:
/// ```text
///   [0]----[1]      top-left, top-right
///  /          \
/// [5]        [2]    left, right
///  \          /
///   [4]----[3]      bottom-left, bottom-right
/// ```
pub fn hex_ring(width: f32, height: f32, z: f32) -> [[f32; 3]; 6] {
    let w = width;
    let h = height;
    [
        [-w * 0.375, h * 0.5, z],  // [0] top-left
        [w * 0.375, h * 0.5, z],   // [1] top-right
        [w * 0.5, 0.0, z],         // [2] right
        [w * 0.375, -h * 0.5, z],  // [3] bottom-right
        [-w * 0.375, -h * 0.5, z], // [4] bottom-left
        [-w * 0.5, 0.0, z],        // [5] left
    ]
}

/// Build a hexagonal hull section.
///
/// The hex profile has 6 sides connecting the front ring to the back ring.
/// `front_width` and `back_width` allow tapering (for cockpit/engine).
/// `length` is along the Z axis (from z=0 to z=length).
/// Returns exterior-facing mesh (normals point outward).
pub fn hex_hull(
    front_width: f32,
    back_width: f32,
    height: f32,
    length: f32,
    color: [f32; 3],
) -> Mesh {
    let front = hex_ring(front_width, height, 0.0);
    let back = hex_ring(back_width, height, length);

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let interior_color: [f32; 3] = [0.52, 0.54, 0.56];

    // Connect front ring to back ring with 6 quad strips (one per hex side).
    // Each quad connects front[i], front[next], back[next], back[i].
    for i in 0..6 {
        let next = (i + 1) % 6;
        // Outward-facing quad: front[i], back[i], back[next], front[next]
        // (winding order for outward normal when viewed from outside)
        let corners = [front[i], back[i], back[next], front[next]];
        let normal = face_normal(corners[0], corners[1], corners[2]);
        push_quad(&mut vertices, &mut indices, corners, normal, color);

        // Interior-facing quad: offset inward by 0.05m to avoid Z-fighting,
        // reversed winding, inward normal, interior color.
        let inset = 0.05;
        let inner_corners = [
            [front[next][0] - normal[0] * inset, front[next][1] - normal[1] * inset, front[next][2] - normal[2] * inset],
            [back[next][0] - normal[0] * inset, back[next][1] - normal[1] * inset, back[next][2] - normal[2] * inset],
            [back[i][0] - normal[0] * inset, back[i][1] - normal[1] * inset, back[i][2] - normal[2] * inset],
            [front[i][0] - normal[0] * inset, front[i][1] - normal[1] * inset, front[i][2] - normal[2] * inset],
        ];
        let inner_normal = [-normal[0], -normal[1], -normal[2]];
        push_quad(&mut vertices, &mut indices, inner_corners, inner_normal, interior_color);
    }

    Mesh { vertices, indices }
}

/// Build a hex cap face (front or back) as a fan of triangles.
/// `ring` is the 6 vertices. If `flip` is true, winding is reversed (for back cap).
pub fn hex_cap(ring: &[[f32; 3]; 6], color: [f32; 3], flip: bool) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Center of the ring
    let cx: f32 = ring.iter().map(|v| v[0]).sum::<f32>() / 6.0;
    let cy: f32 = ring.iter().map(|v| v[1]).sum::<f32>() / 6.0;
    let cz: f32 = ring.iter().map(|v| v[2]).sum::<f32>() / 6.0;
    let center = [cx, cy, cz];

    for i in 0..6 {
        let next = (i + 1) % 6;
        let (a, b) = if flip {
            (ring[next], ring[i])
        } else {
            (ring[i], ring[next])
        };
        let normal = face_normal(center, a, b);
        let base = vertices.len() as u32;
        vertices.push(MeshVertex {
            position: center,
            color,
            normal,
        });
        vertices.push(MeshVertex {
            position: a,
            color,
            normal,
        });
        vertices.push(MeshVertex {
            position: b,
            color,
            normal,
        });
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    Mesh { vertices, indices }
}

/// Build an interior floor panel (flat quad at floor height).
/// Spans from z=0 to z=length, centered on X.
pub fn interior_floor(width: f32, length: f32, floor_y: f32, color: [f32; 3]) -> Mesh {
    let hw = width / 2.0;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Floor faces upward (+Y)
    let corners = [
        [-hw, floor_y, 0.0],
        [hw, floor_y, 0.0],
        [hw, floor_y, length],
        [-hw, floor_y, length],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        corners,
        [0.0, 1.0, 0.0],
        color,
    );

    Mesh { vertices, indices }
}

/// Build interior ceiling panel.
/// Spans from z=0 to z=length, centered on X.
pub fn interior_ceiling(width: f32, length: f32, ceiling_y: f32, color: [f32; 3]) -> Mesh {
    let hw = width / 2.0;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Ceiling faces downward (-Y)
    let corners = [
        [-hw, ceiling_y, length],
        [hw, ceiling_y, length],
        [hw, ceiling_y, 0.0],
        [-hw, ceiling_y, 0.0],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        corners,
        [0.0, -1.0, 0.0],
        color,
    );

    Mesh { vertices, indices }
}

/// Build interior side walls (two vertical panels, inset from hull).
/// Spans from z=0 to z=length, from floor_y to ceiling_y.
pub fn interior_walls(
    width: f32,
    floor_y: f32,
    ceiling_y: f32,
    length: f32,
    wall_inset: f32,
    color: [f32; 3],
) -> Mesh {
    let hw = width / 2.0 - wall_inset;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Left wall: faces inward (+X)
    let left = [
        [-hw, floor_y, length],
        [-hw, floor_y, 0.0],
        [-hw, ceiling_y, 0.0],
        [-hw, ceiling_y, length],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        left,
        [1.0, 0.0, 0.0],
        color,
    );

    // Right wall: faces inward (-X)
    let right = [
        [hw, floor_y, 0.0],
        [hw, floor_y, length],
        [hw, ceiling_y, length],
        [hw, ceiling_y, 0.0],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        right,
        [-1.0, 0.0, 0.0],
        color,
    );

    Mesh { vertices, indices }
}

/// Build a door frame (rectangular frame around a doorway).
/// The frame sits at z=0 in the XY plane.
pub fn door_frame_mesh(
    door_w: f32,
    door_h: f32,
    frame_thickness: f32,
    color: [f32; 3],
) -> Mesh {
    let hw = door_w / 2.0;
    let ft = frame_thickness;
    let depth = 0.1; // thin frame depth along Z
    let hd = depth / 2.0;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Top bar
    let top_corners = [
        [-hw - ft, door_h, -hd],
        [hw + ft, door_h, -hd],
        [hw + ft, door_h + ft, -hd],
        [-hw - ft, door_h + ft, -hd],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        top_corners,
        [0.0, 0.0, -1.0],
        color,
    );

    // Left bar
    let left_corners = [
        [-hw - ft, 0.0, -hd],
        [-hw, 0.0, -hd],
        [-hw, door_h + ft, -hd],
        [-hw - ft, door_h + ft, -hd],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        left_corners,
        [0.0, 0.0, -1.0],
        color,
    );

    // Right bar
    let right_corners = [
        [hw, 0.0, -hd],
        [hw + ft, 0.0, -hd],
        [hw + ft, door_h + ft, -hd],
        [hw, door_h + ft, -hd],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        right_corners,
        [0.0, 0.0, -1.0],
        color,
    );

    Mesh { vertices, indices }
}

/// Build a console/workstation (angled surface with screen).
/// Sits on the floor, screen faces +Z.
pub fn console_mesh(width: f32, accent_color: [f32; 3]) -> Mesh {
    let w = width;
    let hw = w / 2.0;
    let base_h = 0.6;
    let screen_h = 0.4;
    let depth = 0.5;
    let screen_tilt = 0.15; // how far the screen top leans back

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let base_color = [0.40, 0.40, 0.42];

    // Base box front face
    let base_front = [
        [-hw, 0.0, 0.0],
        [hw, 0.0, 0.0],
        [hw, base_h, 0.0],
        [-hw, base_h, 0.0],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        base_front,
        [0.0, 0.0, -1.0],
        base_color,
    );

    // Base box top
    let base_top = [
        [-hw, base_h, 0.0],
        [hw, base_h, 0.0],
        [hw, base_h, depth],
        [-hw, base_h, depth],
    ];
    push_quad(
        &mut vertices,
        &mut indices,
        base_top,
        [0.0, 1.0, 0.0],
        base_color,
    );

    // Screen face (angled, faces toward viewer)
    let screen_bottom_z = 0.0;
    let screen_top_z = screen_tilt;
    let screen = [
        [-hw, base_h, screen_bottom_z],
        [hw, base_h, screen_bottom_z],
        [hw, base_h + screen_h, screen_top_z],
        [-hw, base_h + screen_h, screen_top_z],
    ];
    let screen_normal = face_normal(screen[0], screen[1], screen[2]);
    push_quad(
        &mut vertices,
        &mut indices,
        screen,
        screen_normal,
        accent_color,
    );

    Mesh { vertices, indices }
}

/// Build a bulkhead wall that fills the interior cross-section with a door opening.
///
/// The bulkhead sits in the XY plane at z=0 (the caller translates it to the
/// correct Z position). It spans from `-interior_width/2` to `+interior_width/2`
/// in X, and from `floor_y` to `ceiling_y` in Y.
///
/// A rectangular doorway is cut out: `door_w` wide (centered on X=0) and
/// `door_h` tall (bottom at `floor_y`).
///
/// Built as a hex-shaped wall matching the hull cross-section exactly,
/// with a rectangular door opening. Never extends beyond the hull, never
/// leaves gaps at angled hex corners.
///
/// `hull_width` and `hull_height` must match the hex_hull at this boundary.
pub fn bulkhead_with_door(
    hull_width: f32,
    hull_height: f32,
    floor_y: f32,
    _ceiling_y: f32,
    door_w: f32,
    door_h: f32,
    color: [f32; 3],
) -> Mesh {
    // Get the hex ring vertices at this cross-section
    let ring = hex_ring(hull_width, hull_height, 0.0);
    // ring: [0]=top-left, [1]=top-right, [2]=right, [3]=bot-right, [4]=bot-left, [5]=left

    let hdw = door_w / 2.0;
    let door_bottom = floor_y;
    let door_top = floor_y + door_h;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // R-TS6: single-sided, shader handles back-face lighting
    let n: [f32; 3] = [0.0, 0.0, -1.0];

    // Helper to push a triangle (inline to avoid borrow issues)
    macro_rules! tri {
        ($a:expr, $b:expr, $c:expr) => {{
            let base = vertices.len() as u32;
            vertices.push(MeshVertex { position: $a, color, normal: n });
            vertices.push(MeshVertex { position: $b, color, normal: n });
            vertices.push(MeshVertex { position: $c, color, normal: n });
            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }};
    }

    // Build the hex wall in sections, working around the door opening.
    // Door is centered at x=0, from y=door_bottom to y=door_top.
    //
    // We fill the hex cross-section with panels:
    // 1. Top section: above door_top (top hex triangle + lintel area)
    // 2. Left section: left of door, between door_bottom and door_top
    // 3. Right section: right of door, between door_bottom and door_top
    // 4. Bottom section: below door_bottom (bottom hex area)

    let tl = ring[0]; // top-left
    let tr = ring[1]; // top-right
    let r  = ring[2]; // right
    let br = ring[3]; // bottom-right
    let bl = ring[4]; // bottom-left
    let l  = ring[5]; // left

    // --- TOP SECTION (above door) ---
    // Top triangle: tl, tr, center-top area
    // Fill from door_top up to the hex top
    let dt_l = [-hdw, door_top, 0.0]; // door top-left corner
    let dt_r = [hdw, door_top, 0.0];  // door top-right corner

    // Lintel: rectangle above door
    push_quad(&mut vertices, &mut indices, [dt_l, dt_r, tr, tl], n, color);
    // Top triangle of hex (above the tl-tr line) — this is flat since tl and tr are at same Y
    // Actually tl and tr are already the top of the hex, so nothing above them.

    // --- LEFT SECTION (left of door, door height) ---
    let db_l = [-hdw, door_bottom, 0.0]; // door bottom-left
    // Left panel: from door edge to hex left side, at door height
    // This is a quad from the left hex wall to the door edge
    let l_at_door_top = [-hdw, door_top, 0.0];
    let l_at_door_bot = [-hdw, door_bottom, 0.0];

    // Left side has hex vertices [5]=left at y=0, [0]=top-left, [4]=bottom-left
    // We need to fill from door left edge to the hex outline
    // Between door_bottom and door_top on the left side:
    // Hex left edge x at y=0 is -hull_width/2 = l[0]
    // Hex left edge x at y=door_top: interpolate between [5] and [0]
    let hex_x_at_y = |y: f32, from: [f32; 3], to: [f32; 3]| -> f32 {
        if (to[1] - from[1]).abs() < 0.001 { return from[0]; }
        let t = (y - from[1]) / (to[1] - from[1]);
        from[0] + t * (to[0] - from[0])
    };

    // Left wall at door_top height (between vertex 5 and vertex 0)
    let lx_top = hex_x_at_y(door_top, l, tl);
    let lx_bot = hex_x_at_y(door_bottom, l, bl);
    let left_hex_top = [lx_top, door_top, 0.0];
    let left_hex_bot = [lx_bot, door_bottom, 0.0];
    push_quad(&mut vertices, &mut indices,
        [left_hex_bot, l_at_door_bot, l_at_door_top, left_hex_top], n, color);

    // --- RIGHT SECTION (right of door, door height) ---
    let r_at_door_top = [hdw, door_top, 0.0];
    let r_at_door_bot = [hdw, door_bottom, 0.0];
    let rx_top = hex_x_at_y(door_top, r, tr);
    let rx_bot = hex_x_at_y(door_bottom, r, br);
    let right_hex_top = [rx_top, door_top, 0.0];
    let right_hex_bot = [rx_bot, door_bottom, 0.0];
    push_quad(&mut vertices, &mut indices,
        [r_at_door_bot, right_hex_bot, right_hex_top, r_at_door_top], n, color);

    // --- BOTTOM SECTION (below door) ---
    let db_r = [hdw, door_bottom, 0.0];
    // Fill from hex bottom up to door bottom with a single quad
    push_quad(&mut vertices, &mut indices, [bl, br, db_r, db_l], n, color);

    // --- CORNER TRIANGLES ---
    // Top-left corner: between tl, left_hex_top, and l_at_door_top up to tl
    // These fill the angled hex corners above/below the door side panels
    // Upper-left: tl -> left_hex_top -> dt_l (triangle)
    if left_hex_top[1] < tl[1] {
        tri!(tl, left_hex_top, dt_l);
    }
    // Upper-right: tr -> dt_r -> right_hex_top
    if right_hex_top[1] < tr[1] {
        tri!(tr, dt_r, right_hex_top);
    }
    // Lower-left: bl -> db_l -> left_hex_bot
    if left_hex_bot[1] > bl[1] {
        tri!(bl, db_l, left_hex_bot);
    }
    // Lower-right: br -> right_hex_bot -> db_r
    if right_hex_bot[1] > br[1] {
        tri!(br, right_hex_bot, db_r);
    }

    // Left mid triangle: l -> left_hex_bot -> left_hex_top (fills between hex side vertex and door-height panels)
    tri!(l, left_hex_bot, left_hex_top);
    // Right mid triangle: r -> right_hex_top -> right_hex_bot
    tri!(r, right_hex_top, right_hex_bot);

    Mesh { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn hex_ring_has_six_vertices() {
        let ring = hex_ring(4.0, 3.0, 0.0);
        assert_eq!(ring.len(), 6);
    }

    #[test]
    fn hex_ring_symmetry() {
        let ring = hex_ring(4.0, 3.0, 5.0);
        // Top-left and top-right should be symmetric about X=0
        assert!((ring[0][0] + ring[1][0]).abs() < 1e-5);
        // All at z=5
        for v in &ring {
            assert!((v[2] - 5.0).abs() < 1e-5);
        }
    }

    #[test]
    fn hex_hull_not_empty() {
        let m = hex_hull(4.0, 4.0, 3.0, 5.0, [0.5; 3]);
        assert!(!m.vertices.is_empty());
        assert!(!m.indices.is_empty());
        // 6 exterior quads + 6 interior quads = 24 triangles
        assert_eq!(m.triangle_count(), 24);
    }

    #[test]
    fn hex_hull_tapered() {
        let m = hex_hull(2.0, 4.0, 3.0, 5.0, [0.5; 3]);
        let (min, max) = m.bounding_box();
        // Front (z=0) is narrower than back (z=5)
        assert!(max.z >= 4.9);
        assert!(min.z <= 0.1);
        // Width at back should be about 4.0
        assert!((max.x - 2.0).abs() < 0.1);
    }

    #[test]
    fn hex_hull_no_degenerate_triangles() {
        let m = hex_hull(4.0, 4.0, 3.0, 5.0, [0.5; 3]);
        for tri in m.indices.chunks_exact(3) {
            let a = Vec3::from(m.vertices[tri[0] as usize].position);
            let b = Vec3::from(m.vertices[tri[1] as usize].position);
            let c = Vec3::from(m.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-6, "triangle should have non-zero area");
        }
    }

    #[test]
    fn hex_cap_has_six_triangles() {
        let ring = hex_ring(4.0, 3.0, 0.0);
        let m = hex_cap(&ring, [0.5; 3], false);
        assert_eq!(m.triangle_count(), 6);
    }

    #[test]
    fn interior_floor_is_flat() {
        let m = interior_floor(3.0, 5.0, -1.0, [0.5; 3]);
        assert_eq!(m.triangle_count(), 2);
        // All vertices at y=-1.0
        for v in &m.vertices {
            assert!((v.position[1] - (-1.0)).abs() < 1e-5);
        }
    }

    #[test]
    fn interior_ceiling_faces_down() {
        let m = interior_ceiling(3.0, 5.0, 1.2, [0.5; 3]);
        assert_eq!(m.triangle_count(), 2);
        for v in &m.vertices {
            assert!(v.normal[1] < -0.9, "ceiling normal should face down");
        }
    }

    #[test]
    fn interior_walls_two_panels() {
        let m = interior_walls(4.0, -1.0, 1.2, 5.0, 0.15, [0.5; 3]);
        // 2 quads = 4 triangles
        assert_eq!(m.triangle_count(), 4);
    }

    #[test]
    fn door_frame_not_empty() {
        let m = door_frame_mesh(1.2, 2.0, 0.1, [0.5; 3]);
        assert!(!m.vertices.is_empty());
    }

    #[test]
    fn console_mesh_not_empty() {
        let m = console_mesh(1.2, [0.2, 0.4, 0.7]);
        assert!(!m.vertices.is_empty());
        assert!(m.triangle_count() >= 3);
    }

    #[test]
    fn bulkhead_with_door_not_empty() {
        let m = bulkhead_with_door(4.0, 3.0, -1.0, 1.2, 1.2, 2.0, [0.5; 3]);
        assert!(!m.vertices.is_empty());
        assert!(m.triangle_count() >= 6, "hex bulkhead should have at least 6 triangles");
    }

    #[test]
    fn bulkhead_no_degenerate_triangles() {
        let m = bulkhead_with_door(4.0, 3.0, -1.0, 1.2, 1.2, 2.0, [0.5; 3]);
        for tri_idx in m.indices.chunks_exact(3) {
            let a = Vec3::from(m.vertices[tri_idx[0] as usize].position);
            let b = Vec3::from(m.vertices[tri_idx[1] as usize].position);
            let c = Vec3::from(m.vertices[tri_idx[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-6, "bulkhead triangle should have non-zero area, got {area}");
        }
    }

    #[test]
    fn bulkhead_all_vertices_within_hex() {
        // All vertices should be within the hex ring bounding box
        let m = bulkhead_with_door(4.0, 3.0, -1.0, 1.2, 1.2, 2.0, [0.5; 3]);
        for v in &m.vertices {
            assert!(v.position[0].abs() <= 2.01, "x out of hex bounds: {}", v.position[0]);
            assert!(v.position[1].abs() <= 1.51, "y out of hex bounds: {}", v.position[1]);
        }
    }
}
