//! GLTF model loader — converts GLTF meshes to our MeshData format.
//!
//! Usage: convert FBX assets to GLTF via Blender, then load at runtime.

use crate::mesh::MeshData;
use crate::vertex::Vertex;

/// Load a GLTF/GLB file and extract all meshes as MeshData.
/// Returns flat-shaded MeshData with per-face normals and vertex colors.
pub fn load_gltf(path: &str) -> Result<Vec<MeshData>, String> {
    let (document, buffers, _images) = gltf::import(path)
        .map_err(|e| format!("Failed to load GLTF {:?}: {}", path, e))?;

    let mut meshes = Vec::new();

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .map(|iter| iter.collect())
                .unwrap_or_default();

            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|iter| iter.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

            let colors: Vec<[f32; 3]> = reader
                .read_colors(0)
                .map(|c| c.into_rgb_f32().collect())
                .unwrap_or_else(|| vec![[0.6, 0.6, 0.6]; positions.len()]);

            let indices: Vec<u32> = reader
                .read_indices()
                .map(|iter| iter.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());

            let mut vertices = Vec::with_capacity(positions.len());
            for i in 0..positions.len() {
                vertices.push(Vertex {
                    position: positions[i],
                    normal: if i < normals.len() { normals[i] } else { [0.0, 1.0, 0.0] },
                    color: if i < colors.len() { colors[i] } else { [0.6, 0.6, 0.6] },
                });
            }

            meshes.push(MeshData { vertices, indices });
        }
    }

    Ok(meshes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_gltf("nonexistent.glb");
        assert!(result.is_err());
    }
}
