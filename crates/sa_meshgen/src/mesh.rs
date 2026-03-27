use glam::{Mat4, Vec3};

/// A single vertex in a generated mesh. No GPU dependencies.
/// The game binary converts this to sa_render::Vertex via a simple field copy.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
}

/// A CPU-side mesh: vertices + triangle indices. No GPU types.
/// The game binary converts this to sa_render::MeshData.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    /// Create an empty mesh.
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Merge multiple meshes into one. Concatenates vertices, offsets indices.
    pub fn merge(meshes: &[Mesh]) -> Mesh {
        let total_verts: usize = meshes.iter().map(|m| m.vertices.len()).sum();
        let total_idx: usize = meshes.iter().map(|m| m.indices.len()).sum();
        let mut vertices = Vec::with_capacity(total_verts);
        let mut indices = Vec::with_capacity(total_idx);
        for mesh in meshes {
            let offset = vertices.len() as u32;
            vertices.extend_from_slice(&mesh.vertices);
            indices.extend(mesh.indices.iter().map(|i| i + offset));
        }
        Mesh { vertices, indices }
    }

    /// Apply a 4x4 transform to all vertex positions and normals.
    /// Normals are transformed by the inverse-transpose of the upper-left 3x3,
    /// then renormalized.
    pub fn transform(&self, matrix: Mat4) -> Mesh {
        // For normals: use the inverse-transpose of the 3x3 part.
        // If the matrix has only rotation+translation (no non-uniform scale),
        // the 3x3 part itself works. We handle the general case.
        let normal_matrix = matrix.inverse().transpose();

        let vertices = self
            .vertices
            .iter()
            .map(|v| {
                let pos = Vec3::from(v.position);
                let norm = Vec3::from(v.normal);

                let new_pos = matrix.transform_point3(pos);
                let new_norm = normal_matrix
                    .transform_vector3(norm)
                    .normalize_or_zero();

                MeshVertex {
                    position: new_pos.into(),
                    color: v.color,
                    normal: if new_norm == Vec3::ZERO {
                        v.normal
                    } else {
                        new_norm.into()
                    },
                }
            })
            .collect();

        Mesh {
            vertices,
            indices: self.indices.clone(),
        }
    }

    /// Recolor all vertices.
    pub fn color_all(&mut self, color: [f32; 3]) {
        for v in &mut self.vertices {
            v.color = color;
        }
    }

    /// Flip all normals (for interior faces of rooms).
    pub fn flip_normals(&mut self) {
        for v in &mut self.vertices {
            v.normal = [
                -v.normal[0],
                -v.normal[1],
                -v.normal[2],
            ];
        }
    }

    /// Reverse winding order of all triangles (swap index 1 and 2 in each triple).
    /// Useful when flipping normals to keep backface culling consistent.
    pub fn flip_winding(&mut self) {
        for tri in self.indices.chunks_exact_mut(3) {
            tri.swap(1, 2);
        }
    }

    /// Compute axis-aligned bounding box. Returns (min, max).
    pub fn bounding_box(&self) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        (min, max)
    }

    /// Number of triangles in the mesh.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn dummy_tri(offset: f32, color: [f32; 3]) -> Mesh {
        Mesh {
            vertices: vec![
                MeshVertex {
                    position: [offset, 0.0, 0.0],
                    color,
                    normal: [0.0, 1.0, 0.0],
                },
                MeshVertex {
                    position: [offset + 1.0, 0.0, 0.0],
                    color,
                    normal: [0.0, 1.0, 0.0],
                },
                MeshVertex {
                    position: [offset, 1.0, 0.0],
                    color,
                    normal: [0.0, 1.0, 0.0],
                },
            ],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn merge_doubles_vertex_count() {
        let a = dummy_tri(0.0, [1.0; 3]);
        let b = dummy_tri(5.0, [0.5; 3]);
        let merged = Mesh::merge(&[a.clone(), b.clone()]);
        assert_eq!(merged.vertices.len(), 6);
        assert_eq!(merged.indices.len(), 6);
        // Second triangle's indices should be offset by 3
        assert_eq!(merged.indices[3], 3);
        assert_eq!(merged.indices[4], 4);
        assert_eq!(merged.indices[5], 5);
    }

    #[test]
    fn merge_empty_produces_empty() {
        let merged = Mesh::merge(&[]);
        assert_eq!(merged.vertices.len(), 0);
        assert_eq!(merged.indices.len(), 0);
    }

    #[test]
    fn transform_translates_positions() {
        let tri = dummy_tri(0.0, [1.0; 3]);
        let translated = tri.transform(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        assert!((translated.vertices[0].position[0] - 10.0).abs() < 1e-5);
        assert!((translated.vertices[1].position[0] - 11.0).abs() < 1e-5);
    }

    #[test]
    fn transform_preserves_normals_for_translation() {
        let tri = dummy_tri(0.0, [1.0; 3]);
        let translated = tri.transform(Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        // Normals should be unchanged by pure translation
        for v in &translated.vertices {
            assert!((v.normal[1] - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn color_all_changes_every_vertex() {
        let mut tri = dummy_tri(0.0, [1.0; 3]);
        tri.color_all([0.5, 0.5, 0.5]);
        for v in &tri.vertices {
            assert_eq!(v.color, [0.5, 0.5, 0.5]);
        }
    }

    #[test]
    fn flip_normals_inverts_direction() {
        let mut tri = dummy_tri(0.0, [1.0; 3]);
        tri.flip_normals();
        for v in &tri.vertices {
            assert_eq!(v.normal, [0.0, -1.0, 0.0]);
        }
    }

    #[test]
    fn bounding_box_matches_vertices() {
        let tri = dummy_tri(0.0, [1.0; 3]);
        let (min, max) = tri.bounding_box();
        assert!((min.x - 0.0).abs() < 1e-5);
        assert!((max.x - 1.0).abs() < 1e-5);
        assert!((max.y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn triangle_count_correct() {
        let tri = dummy_tri(0.0, [1.0; 3]);
        assert_eq!(tri.triangle_count(), 1);
        let merged = Mesh::merge(&[tri.clone(), tri]);
        assert_eq!(merged.triangle_count(), 2);
    }
}
