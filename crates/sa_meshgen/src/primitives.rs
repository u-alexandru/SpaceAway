use crate::mesh::{Mesh, MeshVertex};

/// Helper: push a quad (two triangles) with flat-shading normal onto a mesh.
/// Vertices are specified counter-clockwise when viewed from the front (normal side).
/// Each face gets its own 4 vertices for flat shading (no shared vertices between faces).
fn push_quad(
    vertices: &mut Vec<MeshVertex>,
    indices: &mut Vec<u32>,
    corners: [[f32; 3]; 4],
    normal: [f32; 3],
    color: [f32; 3],
) {
    let base = vertices.len() as u32;
    for &pos in &corners {
        vertices.push(MeshVertex {
            position: pos,
            color,
            normal,
        });
    }
    // Two triangles: 0-1-2 and 0-2-3
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Compute flat-shading normal from three points (counter-clockwise winding).
#[allow(dead_code)]
fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    use glam::Vec3;
    let va = Vec3::from(a);
    let vb = Vec3::from(b);
    let vc = Vec3::from(c);
    let n = (vb - va).cross(vc - va).normalize_or_zero();
    n.into()
}

/// Axis-aligned box centered at origin. 24 vertices (4 per face), 36 indices (12 triangles).
///
/// Produces flat-shaded faces with correct outward normals.
pub fn box_mesh(width: f32, height: f32, depth: f32, color: [f32; 3]) -> Mesh {
    let hw = width / 2.0;
    let hh = height / 2.0;
    let hd = depth / 2.0;

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    // +Z face (front)
    push_quad(
        &mut vertices,
        &mut indices,
        [
            [-hw, -hh, hd],
            [hw, -hh, hd],
            [hw, hh, hd],
            [-hw, hh, hd],
        ],
        [0.0, 0.0, 1.0],
        color,
    );

    // -Z face (back)
    push_quad(
        &mut vertices,
        &mut indices,
        [
            [hw, -hh, -hd],
            [-hw, -hh, -hd],
            [-hw, hh, -hd],
            [hw, hh, -hd],
        ],
        [0.0, 0.0, -1.0],
        color,
    );

    // +Y face (top)
    push_quad(
        &mut vertices,
        &mut indices,
        [
            [-hw, hh, hd],
            [hw, hh, hd],
            [hw, hh, -hd],
            [-hw, hh, -hd],
        ],
        [0.0, 1.0, 0.0],
        color,
    );

    // -Y face (bottom)
    push_quad(
        &mut vertices,
        &mut indices,
        [
            [-hw, -hh, -hd],
            [hw, -hh, -hd],
            [hw, -hh, hd],
            [-hw, -hh, hd],
        ],
        [0.0, -1.0, 0.0],
        color,
    );

    // +X face (right)
    push_quad(
        &mut vertices,
        &mut indices,
        [
            [hw, -hh, hd],
            [hw, -hh, -hd],
            [hw, hh, -hd],
            [hw, hh, hd],
        ],
        [1.0, 0.0, 0.0],
        color,
    );

    // -X face (left)
    push_quad(
        &mut vertices,
        &mut indices,
        [
            [-hw, -hh, -hd],
            [-hw, -hh, hd],
            [-hw, hh, hd],
            [-hw, hh, -hd],
        ],
        [-1.0, 0.0, 0.0],
        color,
    );

    Mesh { vertices, indices }
}

/// N-sided cylinder centered at origin, extending from -height/2 to +height/2 along Y.
/// Produces flat-shaded faces. Includes top and bottom caps.
///
/// Total vertices: sides * 4 (side quads) + sides * 3 * 2 (cap triangles)
/// = sides * 10
pub fn cylinder_mesh(radius: f32, height: f32, sides: u32, color: [f32; 3]) -> Mesh {
    cone_mesh(radius, radius, height, sides, color)
}

/// Frustum / cone centered at origin, from -height/2 to +height/2 along Y.
/// `base_radius` is at -height/2, `top_radius` at +height/2.
/// Set `top_radius = 0.0` for a pointed cone.
///
/// Flat-shaded: each side quad and each cap triangle gets its own vertices.
pub fn cone_mesh(
    base_radius: f32,
    top_radius: f32,
    height: f32,
    sides: u32,
    color: [f32; 3],
) -> Mesh {
    use std::f32::consts::TAU;

    let sides = sides.max(3);
    let hh = height / 2.0;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // Precompute ring positions
    let angle_step = TAU / sides as f32;

    // --- Side faces ---
    // Each side is a quad (or triangle if top_radius == 0) between two ring vertices.
    for i in 0..sides {
        let a0 = angle_step * i as f32;
        let a1 = angle_step * ((i + 1) % sides) as f32;

        let cos0 = a0.cos();
        let sin0 = a0.sin();
        let cos1 = a1.cos();
        let sin1 = a1.sin();

        // Bottom ring (at y = -hh)
        let b0 = [base_radius * cos0, -hh, base_radius * sin0];
        let b1 = [base_radius * cos1, -hh, base_radius * sin1];
        // Top ring (at y = +hh)
        let t0 = [top_radius * cos0, hh, top_radius * sin0];
        let t1 = [top_radius * cos1, hh, top_radius * sin1];

        // Compute face normal for the side quad
        let normal = face_normal(b0, b1, t1);

        if top_radius > 1e-6 {
            // Quad: b0, b1, t1, t0
            push_quad(&mut vertices, &mut indices, [b0, b1, t1, t0], normal, color);
        } else {
            // Triangle (cone tip): b0, b1, t0 (t0 == t1 == apex)
            let apex = [0.0, hh, 0.0];
            let base = vertices.len() as u32;
            vertices.push(MeshVertex { position: b0, color, normal });
            vertices.push(MeshVertex { position: b1, color, normal });
            vertices.push(MeshVertex { position: apex, color, normal });
            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }

    // --- Bottom cap (y = -hh, normal pointing -Y) ---
    {
        let cap_normal = [0.0, -1.0, 0.0];
        let center = [0.0, -hh, 0.0];
        for i in 0..sides {
            let a0 = angle_step * i as f32;
            let a1 = angle_step * ((i + 1) % sides) as f32;
            // Winding: center, next, current (so normal faces -Y)
            let p0 = [base_radius * a1.cos(), -hh, base_radius * a1.sin()];
            let p1 = [base_radius * a0.cos(), -hh, base_radius * a0.sin()];

            let base_idx = vertices.len() as u32;
            vertices.push(MeshVertex { position: center, color, normal: cap_normal });
            vertices.push(MeshVertex { position: p0, color, normal: cap_normal });
            vertices.push(MeshVertex { position: p1, color, normal: cap_normal });
            indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        }
    }

    // --- Top cap (y = +hh, normal pointing +Y) ---
    if top_radius > 1e-6 {
        let cap_normal = [0.0, 1.0, 0.0];
        let center = [0.0, hh, 0.0];
        for i in 0..sides {
            let a0 = angle_step * i as f32;
            let a1 = angle_step * ((i + 1) % sides) as f32;
            // Winding: center, current, next (so normal faces +Y)
            let p0 = [top_radius * a0.cos(), hh, top_radius * a0.sin()];
            let p1 = [top_radius * a1.cos(), hh, top_radius * a1.sin()];

            let base_idx = vertices.len() as u32;
            vertices.push(MeshVertex { position: center, color, normal: cap_normal });
            vertices.push(MeshVertex { position: p0, color, normal: cap_normal });
            vertices.push(MeshVertex { position: p1, color, normal: cap_normal });
            indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        }
    }

    Mesh { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn box_vertex_and_index_count() {
        let m = box_mesh(2.0, 3.0, 4.0, [1.0; 3]);
        assert_eq!(m.vertices.len(), 24, "6 faces * 4 verts = 24");
        assert_eq!(m.indices.len(), 36, "6 faces * 6 indices = 36");
    }

    #[test]
    fn box_bounding_box_matches_dimensions() {
        let m = box_mesh(2.0, 3.0, 4.0, [1.0; 3]);
        let (min, max) = m.bounding_box();
        assert!((max.x - min.x - 2.0).abs() < 1e-5);
        assert!((max.y - min.y - 3.0).abs() < 1e-5);
        assert!((max.z - min.z - 4.0).abs() < 1e-5);
    }

    #[test]
    fn box_centered_at_origin() {
        let m = box_mesh(2.0, 4.0, 6.0, [1.0; 3]);
        let (min, max) = m.bounding_box();
        let center = (min + max) / 2.0;
        assert!(center.length() < 1e-5, "box should be centered at origin");
    }

    #[test]
    fn box_normals_point_outward() {
        let m = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        // For each face (4 vertices share a normal), the normal should point
        // away from center. Since box is centered at origin, the dot product
        // of normal with any vertex position on that face should be > 0.
        for face_start in (0..24).step_by(4) {
            let normal = Vec3::from(m.vertices[face_start].normal);
            let pos = Vec3::from(m.vertices[face_start].position);
            assert!(
                normal.dot(pos) > 0.0,
                "normal {:?} should point away from origin for vertex at {:?}",
                normal,
                pos
            );
        }
    }

    #[test]
    fn box_no_degenerate_triangles() {
        let m = box_mesh(1.0, 1.0, 1.0, [1.0; 3]);
        for tri in m.indices.chunks_exact(3) {
            let a = Vec3::from(m.vertices[tri[0] as usize].position);
            let b = Vec3::from(m.vertices[tri[1] as usize].position);
            let c = Vec3::from(m.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-6, "triangle should have non-zero area");
        }
    }

    #[test]
    fn box_color_applied() {
        let color = [0.3, 0.5, 0.7];
        let m = box_mesh(1.0, 1.0, 1.0, color);
        for v in &m.vertices {
            assert_eq!(v.color, color);
        }
    }

    #[test]
    fn cylinder_triangle_count() {
        // sides=8: 8 side quads (16 tri) + 8 bottom cap + 8 top cap = 32 triangles
        let m = cylinder_mesh(1.0, 2.0, 8, [1.0; 3]);
        assert_eq!(m.triangle_count(), 32);
    }

    #[test]
    fn cylinder_has_caps() {
        let m = cylinder_mesh(1.0, 2.0, 8, [1.0; 3]);
        // Check there are vertices with normal [0,1,0] (top) and [0,-1,0] (bottom)
        let has_top = m.vertices.iter().any(|v| v.normal[1] > 0.9);
        let has_bottom = m.vertices.iter().any(|v| v.normal[1] < -0.9);
        assert!(has_top, "should have top cap");
        assert!(has_bottom, "should have bottom cap");
    }

    #[test]
    fn cylinder_bounding_box() {
        let m = cylinder_mesh(1.5, 3.0, 16, [1.0; 3]);
        let (min, max) = m.bounding_box();
        // Height along Y
        assert!((max.y - min.y - 3.0).abs() < 1e-4);
        // Radius along X and Z (approximately, due to polygon approximation)
        assert!((max.x - 1.5).abs() < 0.1);
        assert!((max.z - 1.5).abs() < 0.1);
    }

    #[test]
    fn cone_pointed_has_no_top_cap() {
        // top_radius=0 means pointed cone, no top cap
        let m = cone_mesh(1.0, 0.0, 2.0, 8, [1.0; 3]);
        // 8 side triangles + 8 bottom cap = 16 triangles
        assert_eq!(m.triangle_count(), 16);
        // Should NOT have any vertices with normal [0,1,0]
        let has_top_cap = m.vertices.iter().any(|v| v.normal[1] > 0.9);
        assert!(!has_top_cap, "pointed cone should not have top cap");
    }

    #[test]
    fn cone_frustum_has_both_caps() {
        let m = cone_mesh(2.0, 1.0, 3.0, 6, [1.0; 3]);
        // 6 side quads (12 tri) + 6 bottom + 6 top = 24
        assert_eq!(m.triangle_count(), 24);
    }

    #[test]
    fn cylinder_no_degenerate_triangles() {
        let m = cylinder_mesh(1.0, 2.0, 12, [1.0; 3]);
        for tri in m.indices.chunks_exact(3) {
            let a = Vec3::from(m.vertices[tri[0] as usize].position);
            let b = Vec3::from(m.vertices[tri[1] as usize].position);
            let c = Vec3::from(m.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-6, "triangle should have non-zero area");
        }
    }
}
