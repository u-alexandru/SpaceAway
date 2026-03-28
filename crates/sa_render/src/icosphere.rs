//! Icosphere mesh generation with subdivision levels for LOD.
//!
//! LOD 0 = 20 faces (icosahedron), each subdivision multiplies by 4.
//! LOD 4 = 5,120 faces, LOD 5 = 20,480 faces.
//! All vertices lie on the unit sphere. Caller scales to planet radius.

use std::collections::HashMap;

/// Raw icosphere data (positions on unit sphere + triangle indices).
pub struct IcosphereData {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Generate a unit icosphere at the given subdivision level.
pub fn generate_icosphere(subdivisions: u32) -> IcosphereData {
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;

    let mut positions: Vec<[f32; 3]> = vec![
        normalize([-1.0, t, 0.0]),
        normalize([1.0, t, 0.0]),
        normalize([-1.0, -t, 0.0]),
        normalize([1.0, -t, 0.0]),
        normalize([0.0, -1.0, t]),
        normalize([0.0, 1.0, t]),
        normalize([0.0, -1.0, -t]),
        normalize([0.0, 1.0, -t]),
        normalize([t, 0.0, -1.0]),
        normalize([t, 0.0, 1.0]),
        normalize([-t, 0.0, -1.0]),
        normalize([-t, 0.0, 1.0]),
    ];

    let mut indices: Vec<u32> = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6,
        7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6,
        7, 9, 8, 1,
    ];

    let mut cache: HashMap<(u32, u32), u32> = HashMap::new();

    for _ in 0..subdivisions {
        let mut new_indices: Vec<u32> = Vec::with_capacity(indices.len() * 4);

        for tri in indices.chunks_exact(3) {
            let a = tri[0];
            let b = tri[1];
            let c = tri[2];

            let ab = get_midpoint(a, b, &mut positions, &mut cache);
            let bc = get_midpoint(b, c, &mut positions, &mut cache);
            let ca = get_midpoint(c, a, &mut positions, &mut cache);

            new_indices.extend_from_slice(&[a, ab, ca]);
            new_indices.extend_from_slice(&[b, bc, ab]);
            new_indices.extend_from_slice(&[c, ca, bc]);
            new_indices.extend_from_slice(&[ab, bc, ca]);
        }

        indices = new_indices;
    }

    IcosphereData { positions, indices }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

fn get_midpoint(
    a: u32,
    b: u32,
    positions: &mut Vec<[f32; 3]>,
    cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = (a.min(b), a.max(b));
    if let Some(&idx) = cache.get(&key) {
        return idx;
    }

    let va = positions[a as usize];
    let vb = positions[b as usize];
    let mid = normalize([
        (va[0] + vb[0]) / 2.0,
        (va[1] + vb[1]) / 2.0,
        (va[2] + vb[2]) / 2.0,
    ]);

    let idx = positions.len() as u32;
    positions.push(mid);
    cache.insert(key, idx);
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icosphere_lod0_has_20_faces() {
        let mesh = generate_icosphere(0);
        assert_eq!(mesh.indices.len() / 3, 20);
    }

    #[test]
    fn icosphere_lod1_has_80_faces() {
        let mesh = generate_icosphere(1);
        assert_eq!(mesh.indices.len() / 3, 80);
    }

    #[test]
    fn icosphere_lod4_has_5120_faces() {
        let mesh = generate_icosphere(4);
        assert_eq!(mesh.indices.len() / 3, 5120);
    }

    #[test]
    fn icosphere_vertices_on_unit_sphere() {
        let mesh = generate_icosphere(2);
        for v in &mesh.positions {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-4,
                "vertex should be on unit sphere, len={len}"
            );
        }
    }

    #[test]
    fn icosphere_no_degenerate_triangles() {
        let mesh = generate_icosphere(3);
        for tri in mesh.indices.chunks_exact(3) {
            let a = glam::Vec3::from(mesh.positions[tri[0] as usize]);
            let b = glam::Vec3::from(mesh.positions[tri[1] as usize]);
            let c = glam::Vec3::from(mesh.positions[tri[2] as usize]);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-8, "degenerate triangle found");
        }
    }

    #[test]
    fn icosphere_vertex_count_formula() {
        // V = 10 * 4^n + 2 for subdivision level n
        for n in 0..5 {
            let mesh = generate_icosphere(n);
            let expected = 10 * 4u32.pow(n) + 2;
            assert_eq!(
                mesh.positions.len() as u32,
                expected,
                "LOD {n}: expected {expected} vertices, got {}",
                mesh.positions.len()
            );
        }
    }
}
