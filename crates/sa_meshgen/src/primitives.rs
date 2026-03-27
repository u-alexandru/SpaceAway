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
}
