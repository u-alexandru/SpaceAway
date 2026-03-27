//! CSG boolean operations wrapping the `csgrs` crate.
//!
//! Converts sa_meshgen::Mesh <-> csgrs::mesh::Mesh<()> for boolean operations.
//! These are expensive --- prefer merge() + transform() for simple assembly.

use crate::mesh::{Mesh, MeshVertex};

use csgrs::mesh::Mesh as CsgMesh;
use csgrs::mesh::polygon::Polygon as CsgPolygon;
use csgrs::mesh::vertex::Vertex as CsgVertex;
use csgrs::traits::CSG;
use nalgebra::{Point3, Vector3};

/// Convert our Mesh to a csgrs Mesh<()>.
/// Groups triangles into csgrs Polygons (one polygon per triangle).
fn to_csg(mesh: &Mesh) -> CsgMesh<()> {
    let mut polygons = Vec::with_capacity(mesh.indices.len() / 3);

    for tri in mesh.indices.chunks_exact(3) {
        let verts: Vec<CsgVertex> = tri
            .iter()
            .map(|&i| {
                let v = &mesh.vertices[i as usize];
                CsgVertex::new(
                    Point3::new(v.position[0], v.position[1], v.position[2]),
                    Vector3::new(v.normal[0], v.normal[1], v.normal[2]),
                )
            })
            .collect();
        polygons.push(CsgPolygon::new(verts, None));
    }

    CsgMesh::from_polygons(&polygons, None)
}

/// Convert a csgrs Mesh<()> back to our Mesh.
/// Triangulates the result (csgrs polygons may have >3 vertices after CSG),
/// then emits flat-shaded triangles with a default color.
fn from_csg(csg: &CsgMesh<()>, color: [f32; 3]) -> Mesh {
    let triangulated = csg.triangulate();
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for poly in &triangulated.polygons {
        if poly.vertices.len() < 3 {
            continue;
        }
        // Each triangulated polygon should have exactly 3 vertices
        let base = vertices.len() as u32;
        for cv in &poly.vertices {
            vertices.push(MeshVertex {
                position: [cv.pos.x, cv.pos.y, cv.pos.z],
                color,
                normal: [cv.normal.x, cv.normal.y, cv.normal.z],
            });
        }
        // Fan triangulation for safety (handles polygons with >3 verts)
        for i in 1..(poly.vertices.len() as u32 - 1) {
            indices.extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }

    Mesh { vertices, indices }
}

/// Boolean union of two meshes. Result gets `color` applied to all vertices.
pub fn csg_union(a: &Mesh, b: &Mesh, color: [f32; 3]) -> Mesh {
    let ca = to_csg(a);
    let cb = to_csg(b);
    let result = ca.union(&cb);
    from_csg(&result, color)
}

/// Boolean difference: `a` minus `b`. Result gets `color` applied to all vertices.
pub fn csg_difference(a: &Mesh, b: &Mesh, color: [f32; 3]) -> Mesh {
    let ca = to_csg(a);
    let cb = to_csg(b);
    let result = ca.difference(&cb);
    from_csg(&result, color)
}

/// Boolean intersection of two meshes. Result gets `color` applied to all vertices.
pub fn csg_intersect(a: &Mesh, b: &Mesh, color: [f32; 3]) -> Mesh {
    let ca = to_csg(a);
    let cb = to_csg(b);
    let result = ca.intersection(&cb);
    from_csg(&result, color)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::box_mesh;

    #[test]
    fn union_of_two_boxes_produces_valid_mesh() {
        let a = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        let b = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        // b is at same position as a, union should produce a mesh
        let result = csg_union(&a, &b, [0.5; 3]);
        assert!(!result.vertices.is_empty(), "union should produce vertices");
        assert!(!result.indices.is_empty(), "union should produce indices");
        assert_eq!(result.indices.len() % 3, 0, "indices should be multiple of 3");
    }

    #[test]
    fn difference_reduces_mesh() {
        // Two overlapping boxes: a is 4x4x4, b is 2x2x2 both centered at origin
        let a = box_mesh(4.0, 4.0, 4.0, [1.0; 3]);
        let b = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        let result = csg_difference(&a, &b, [0.5; 3]);
        assert!(!result.vertices.is_empty(), "difference should produce vertices");
        // The bounding box of the difference should still be 4x4x4
        // (we cut a hole inside, but the outer shell remains)
        let (min, max) = result.bounding_box();
        let size = max - min;
        assert!((size.x - 4.0).abs() < 0.1);
        assert!((size.y - 4.0).abs() < 0.1);
    }

    #[test]
    fn intersection_of_overlapping_boxes() {
        use glam::{Mat4, Vec3};
        let a = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        // Shift b by 1 unit on X so they overlap by 1 unit
        let b_raw = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        let b = b_raw.transform(Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)));
        let result = csg_intersect(&a, &b, [0.5; 3]);
        assert!(!result.vertices.is_empty(), "intersection should produce vertices");
        // Intersection should be roughly 1x2x2
        let (min, max) = result.bounding_box();
        let size = max - min;
        assert!((size.x - 1.0).abs() < 0.2, "intersection width should be ~1, got {}", size.x);
    }

    #[test]
    fn csg_result_has_no_orphaned_vertices() {
        let a = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        let b = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        let result = csg_union(&a, &b, [0.5; 3]);
        // Every index should be within bounds
        for &idx in &result.indices {
            assert!(
                (idx as usize) < result.vertices.len(),
                "index {} out of bounds (len={})",
                idx,
                result.vertices.len()
            );
        }
    }

    #[test]
    fn csg_result_color_applied() {
        let a = box_mesh(2.0, 2.0, 2.0, [1.0; 3]);
        let b = box_mesh(2.0, 2.0, 2.0, [0.0; 3]);
        let result = csg_union(&a, &b, [0.5, 0.5, 0.5]);
        for v in &result.vertices {
            assert_eq!(v.color, [0.5, 0.5, 0.5]);
        }
    }
}
