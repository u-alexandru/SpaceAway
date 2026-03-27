use crate::mesh::{Mesh, MeshVertex};
use crate::primitives::{face_normal, push_quad};

/// Triangular prism (wedge) centered at origin.
///
/// The triangular cross-section is in the XY plane:
///   - Bottom-left:  (-w/2, -h/2)
///   - Bottom-right: (+w/2, -h/2)
///   - Top-center:   (0,    +h/2)
///
/// Extruded along Z from -depth/2 to +depth/2.
///
/// 5 faces: 2 triangular end caps, 3 rectangular sides.
/// Vertices: 2*3 (end caps) + 3*4 (side quads) = 18
/// Indices:  2*3 (end caps) + 3*6 (side quads) = 24
pub fn wedge_mesh(width: f32, height: f32, depth: f32, color: [f32; 3]) -> Mesh {
    let hw = width / 2.0;
    let hh = height / 2.0;
    let hd = depth / 2.0;

    let mut vertices = Vec::with_capacity(18);
    let mut indices = Vec::with_capacity(24);

    // The 6 corner points of the wedge
    // Front face (z = +hd)
    let fl = [-hw, -hh, hd]; // front-left
    let fr = [hw, -hh, hd]; // front-right
    let ft = [0.0, hh, hd]; // front-top

    // Back face (z = -hd)
    let bl = [-hw, -hh, -hd]; // back-left
    let br = [hw, -hh, -hd]; // back-right
    let bt = [0.0, hh, -hd]; // back-top

    // --- Front triangular face (z = +hd, normal +Z) ---
    {
        let n = [0.0, 0.0, 1.0];
        let base = vertices.len() as u32;
        vertices.push(MeshVertex { position: fl, color, normal: n });
        vertices.push(MeshVertex { position: fr, color, normal: n });
        vertices.push(MeshVertex { position: ft, color, normal: n });
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    // --- Back triangular face (z = -hd, normal -Z) ---
    {
        let n = [0.0, 0.0, -1.0];
        let base = vertices.len() as u32;
        // Reversed winding for -Z normal
        vertices.push(MeshVertex { position: br, color, normal: n });
        vertices.push(MeshVertex { position: bl, color, normal: n });
        vertices.push(MeshVertex { position: bt, color, normal: n });
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    // --- Bottom face (y = -hh, normal -Y) ---
    push_quad(
        &mut vertices,
        &mut indices,
        [bl, br, fr, fl],
        [0.0, -1.0, 0.0],
        color,
    );

    // --- Left slope face (fl -> ft -> bt -> bl) ---
    {
        let n = face_normal(fl, ft, bt);
        push_quad(&mut vertices, &mut indices, [fl, ft, bt, bl], n, color);
    }

    // --- Right slope face (fr -> br -> bt -> ft) ---
    {
        let n = face_normal(fr, br, bt);
        push_quad(&mut vertices, &mut indices, [fr, br, bt, ft], n, color);
    }

    Mesh { vertices, indices }
}

/// Curved wall section (arc). Creates a hollow curved wall centered at origin.
/// - `inner_r`: inner radius
/// - `outer_r`: outer radius
/// - `height`: wall height (along Y, from -height/2 to +height/2)
/// - `angle_deg`: angular sweep in degrees (e.g. 90.0 for a quarter circle)
/// - `sides`: number of segments along the arc
///
/// The arc sweeps in the XZ plane, starting at +X, going toward +Z.
pub fn arc_mesh(
    inner_r: f32,
    outer_r: f32,
    height: f32,
    angle_deg: f32,
    sides: u32,
    color: [f32; 3],
) -> Mesh {
    let sides = sides.max(1);
    let hh = height / 2.0;
    let angle_rad = angle_deg.to_radians();
    let angle_step = angle_rad / sides as f32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for i in 0..sides {
        let a0 = angle_step * i as f32;
        let a1 = angle_step * (i + 1) as f32;

        let cos0 = a0.cos();
        let sin0 = a0.sin();
        let cos1 = a1.cos();
        let sin1 = a1.sin();

        // Inner ring positions
        let ib0 = [inner_r * cos0, -hh, inner_r * sin0]; // inner bottom
        let it0 = [inner_r * cos0, hh, inner_r * sin0]; // inner top
        let ib1 = [inner_r * cos1, -hh, inner_r * sin1];
        let it1 = [inner_r * cos1, hh, inner_r * sin1];

        // Outer ring positions
        let ob0 = [outer_r * cos0, -hh, outer_r * sin0];
        let ot0 = [outer_r * cos0, hh, outer_r * sin0];
        let ob1 = [outer_r * cos1, -hh, outer_r * sin1];
        let ot1 = [outer_r * cos1, hh, outer_r * sin1];

        // --- Outer wall (faces outward) ---
        {
            let n = face_normal(ob0, ob1, ot1);
            push_quad(&mut vertices, &mut indices, [ob0, ob1, ot1, ot0], n, color);
        }

        // --- Inner wall (faces inward, toward center) ---
        {
            let n = face_normal(ib1, ib0, it0);
            push_quad(&mut vertices, &mut indices, [ib1, ib0, it0, it1], n, color);
        }

        // --- Top face (y = +hh) ---
        {
            let n = [0.0, 1.0, 0.0];
            push_quad(&mut vertices, &mut indices, [it0, ot0, ot1, it1], n, color);
        }

        // --- Bottom face (y = -hh) ---
        {
            let n = [0.0, -1.0, 0.0];
            push_quad(&mut vertices, &mut indices, [ib1, ob1, ob0, ib0], n, color);
        }
    }

    // --- End caps (flat faces at the start and end of the arc) ---
    {
        let a_start = 0.0_f32;
        let a_end = angle_rad;

        // Start cap (at angle=0)
        let cs = a_start.cos();
        let ss = a_start.sin();
        let n_start = face_normal(
            [inner_r * cs, -hh, inner_r * ss],
            [outer_r * cs, -hh, outer_r * ss],
            [outer_r * cs, hh, outer_r * ss],
        );
        push_quad(
            &mut vertices,
            &mut indices,
            [
                [inner_r * cs, -hh, inner_r * ss],
                [outer_r * cs, -hh, outer_r * ss],
                [outer_r * cs, hh, outer_r * ss],
                [inner_r * cs, hh, inner_r * ss],
            ],
            n_start,
            color,
        );

        // End cap (at angle=angle_rad)
        let ce = a_end.cos();
        let se = a_end.sin();
        let n_end = face_normal(
            [outer_r * ce, -hh, outer_r * se],
            [inner_r * ce, -hh, inner_r * se],
            [inner_r * ce, hh, inner_r * se],
        );
        push_quad(
            &mut vertices,
            &mut indices,
            [
                [outer_r * ce, -hh, outer_r * se],
                [inner_r * ce, -hh, inner_r * se],
                [inner_r * ce, hh, inner_r * se],
                [outer_r * ce, hh, outer_r * se],
            ],
            n_end,
            color,
        );
    }

    Mesh { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn wedge_face_count() {
        let m = wedge_mesh(2.0, 1.5, 3.0, [1.0; 3]);
        // 2 triangular caps (2 tri) + 3 rectangular sides (6 tri) = 8 triangles
        assert_eq!(m.triangle_count(), 8);
    }

    #[test]
    fn wedge_vertex_count() {
        let m = wedge_mesh(2.0, 1.5, 3.0, [1.0; 3]);
        // 2 caps * 3 verts + 3 quads * 4 verts = 6 + 12 = 18
        assert_eq!(m.vertices.len(), 18);
    }

    #[test]
    fn wedge_bounding_box() {
        let m = wedge_mesh(2.0, 3.0, 4.0, [1.0; 3]);
        let (min, max) = m.bounding_box();
        assert!((max.x - min.x - 2.0).abs() < 1e-5);
        assert!((max.y - min.y - 3.0).abs() < 1e-5);
        assert!((max.z - min.z - 4.0).abs() < 1e-5);
    }

    #[test]
    fn wedge_no_degenerate_triangles() {
        let m = wedge_mesh(1.0, 1.0, 1.0, [1.0; 3]);
        for tri in m.indices.chunks_exact(3) {
            let a = Vec3::from(m.vertices[tri[0] as usize].position);
            let b = Vec3::from(m.vertices[tri[1] as usize].position);
            let c = Vec3::from(m.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-6);
        }
    }

    #[test]
    fn arc_covers_specified_angle() {
        // 90-degree arc, outer_r=2
        let m = arc_mesh(1.0, 2.0, 1.0, 90.0, 8, [1.0; 3]);
        let (min, max) = m.bounding_box();
        // Arc goes from +X toward +Z, so:
        // X range: inner_r..outer_r (roughly 1..2)
        // Z range: 0..outer_r (roughly 0..2)
        assert!(max.x > 1.5, "arc should extend along +X");
        assert!(max.z > 1.5, "90-degree arc should extend along +Z");
        assert!(min.z >= -0.1, "90-degree arc should not go into -Z");
    }

    #[test]
    fn arc_has_end_caps() {
        let m = arc_mesh(1.0, 2.0, 1.0, 90.0, 4, [1.0; 3]);
        // Should have non-zero triangle count
        assert!(m.triangle_count() > 0);
        // 4 segments * 4 faces (outer, inner, top, bottom) + 2 end caps
        // = 4*4 + 2 = 18 quads = 36 triangles
        assert_eq!(m.triangle_count(), 36);
    }

    #[test]
    fn arc_full_circle() {
        let m = arc_mesh(1.0, 2.0, 1.0, 360.0, 12, [1.0; 3]);
        // 12 segments * 4 faces + 2 end caps = 50 quads = 100 triangles
        assert_eq!(m.triangle_count(), 100);
    }
}
