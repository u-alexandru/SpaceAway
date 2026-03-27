//! Auto-collider generation from mesh geometry.
//!
//! Converts sa_meshgen Mesh data into the formats rapier3d needs for
//! collision shape construction.

use crate::mesh::Mesh;

/// Extract vertex positions as nalgebra Point3 array for rapier3d.
///
/// Used by `ColliderBuilder::convex_hull()` and `ColliderBuilder::convex_decomposition()`.
pub fn mesh_to_points(mesh: &Mesh) -> Vec<nalgebra::Point3<f32>> {
    mesh.vertices
        .iter()
        .map(|v| nalgebra::Point3::new(v.position[0], v.position[1], v.position[2]))
        .collect()
}

/// Extract triangle indices as `[u32; 3]` arrays for rapier3d.
///
/// Used by `ColliderBuilder::convex_decomposition()` and `ColliderBuilder::trimesh()`.
pub fn mesh_to_indices(mesh: &Mesh) -> Vec<[u32; 3]> {
    mesh.indices
        .chunks_exact(3)
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect()
}

/// Compute the axis-aligned bounding box half-extents of a mesh.
///
/// Returns `(center, half_extents)` suitable for `ColliderBuilder::cuboid()`.
pub fn mesh_to_aabb(mesh: &Mesh) -> ([f32; 3], [f32; 3]) {
    let (min, max) = mesh.bounding_box();
    let center = [
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    ];
    let half = [
        (max.x - min.x) * 0.5,
        (max.y - min.y) * 0.5,
        (max.z - min.z) * 0.5,
    ];
    (center, half)
}

/// Build a convex hull from raw position arrays (no Mesh needed).
///
/// Useful for constructing wall segment colliders from hex ring vertices
/// where the full Mesh struct would be overkill.
pub fn points_from_positions(positions: &[[f32; 3]]) -> Vec<nalgebra::Point3<f32>> {
    positions
        .iter()
        .map(|p| nalgebra::Point3::new(p[0], p[1], p[2]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{Mesh, MeshVertex};

    fn simple_triangle() -> Mesh {
        Mesh {
            vertices: vec![
                MeshVertex {
                    position: [0.0, 0.0, 0.0],
                    color: [1.0; 3],
                    normal: [0.0, 1.0, 0.0],
                },
                MeshVertex {
                    position: [1.0, 0.0, 0.0],
                    color: [1.0; 3],
                    normal: [0.0, 1.0, 0.0],
                },
                MeshVertex {
                    position: [0.0, 1.0, 0.0],
                    color: [1.0; 3],
                    normal: [0.0, 1.0, 0.0],
                },
            ],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn mesh_to_points_count() {
        let mesh = simple_triangle();
        let points = mesh_to_points(&mesh);
        assert_eq!(points.len(), 3);
        assert!((points[0].x - 0.0).abs() < 1e-5);
        assert!((points[1].x - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mesh_to_indices_count() {
        let mesh = simple_triangle();
        let indices = mesh_to_indices(&mesh);
        assert_eq!(indices.len(), 1);
        assert_eq!(indices[0], [0, 1, 2]);
    }

    #[test]
    fn mesh_to_aabb_correct() {
        let mesh = simple_triangle();
        let (center, half) = mesh_to_aabb(&mesh);
        assert!((center[0] - 0.5).abs() < 1e-5);
        assert!((center[1] - 0.5).abs() < 1e-5);
        assert!((half[0] - 0.5).abs() < 1e-5);
        assert!((half[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn points_from_positions_works() {
        let pts = points_from_positions(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        assert_eq!(pts.len(), 2);
        assert!((pts[0].x - 1.0).abs() < 1e-5);
        assert!((pts[1].z - 6.0).abs() < 1e-5);
    }
}
