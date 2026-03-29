//! Cube face → sphere mapping using the analytic projection (Nowell 2005).
//!
//! Eliminates corner clustering from naive normalization:
//! x' = x * sqrt(1 - y²/2 - z²/2 + y²z²/3)
//!
//! Reference: Zucker & Higashi, JCGT 2018.

/// The six faces of the cube.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CubeFace {
    PosX = 0,
    NegX = 1,
    PosY = 2,
    NegY = 3,
    PosZ = 4,
    NegZ = 5,
}

impl CubeFace {
    /// All six faces in order.
    pub const ALL: [CubeFace; 6] = [
        CubeFace::PosX, CubeFace::NegX,
        CubeFace::PosY, CubeFace::NegY,
        CubeFace::PosZ, CubeFace::NegZ,
    ];
}

/// Map a point on a cube face to a unit sphere direction.
///
/// `u`, `v` ∈ [-1, +1] are coordinates on the face.
/// Returns a normalized direction vector on the unit sphere.
pub fn cube_to_sphere(face: CubeFace, u: f64, v: f64) -> [f64; 3] {
    let (x, y, z) = match face {
        CubeFace::PosX => ( 1.0,   v,  -u),
        CubeFace::NegX => (-1.0,   v,   u),
        CubeFace::PosY => (  u,  1.0,  -v),
        CubeFace::NegY => (  u, -1.0,   v),
        CubeFace::PosZ => (  u,   v,  1.0),
        CubeFace::NegZ => ( -u,   v, -1.0),
    };

    let x2 = x * x;
    let y2 = y * y;
    let z2 = z * z;

    let sx = x * (1.0 - y2 / 2.0 - z2 / 2.0 + y2 * z2 / 3.0).sqrt();
    let sy = y * (1.0 - x2 / 2.0 - z2 / 2.0 + x2 * z2 / 3.0).sqrt();
    let sz = z * (1.0 - x2 / 2.0 - y2 / 2.0 + x2 * y2 / 3.0).sqrt();

    [sx, sy, sz]
}

/// Compute the sphere-surface position in meters for a point on a cube face.
///
/// Returns position relative to planet center.
pub fn face_point_to_position(face: CubeFace, u: f64, v: f64, radius_m: f64) -> [f64; 3] {
    let dir = cube_to_sphere(face, u, v);
    [dir[0] * radius_m, dir[1] * radius_m, dir[2] * radius_m]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_center_maps_to_axis() {
        let [x, y, z] = cube_to_sphere(CubeFace::PosZ, 0.0, 0.0);
        assert!((x).abs() < 1e-10);
        assert!((y).abs() < 1e-10);
        assert!((z - 1.0).abs() < 1e-10);
    }

    #[test]
    fn face_center_maps_to_correct_axis_all_faces() {
        let expected = [
            (CubeFace::PosX, [1.0, 0.0, 0.0]),
            (CubeFace::NegX, [-1.0, 0.0, 0.0]),
            (CubeFace::PosY, [0.0, 1.0, 0.0]),
            (CubeFace::NegY, [0.0, -1.0, 0.0]),
            (CubeFace::PosZ, [0.0, 0.0, 1.0]),
            (CubeFace::NegZ, [0.0, 0.0, -1.0]),
        ];
        for (face, [ex, ey, ez]) in expected {
            let [x, y, z] = cube_to_sphere(face, 0.0, 0.0);
            assert!((x - ex).abs() < 1e-10, "face {:?} x: got {x}", face);
            assert!((y - ey).abs() < 1e-10, "face {:?} y: got {y}", face);
            assert!((z - ez).abs() < 1e-10, "face {:?} z: got {z}", face);
        }
    }

    #[test]
    fn all_points_on_unit_sphere() {
        for face in CubeFace::ALL {
            for i in 0..=10 {
                for j in 0..=10 {
                    let u = -1.0 + 2.0 * (i as f64 / 10.0);
                    let v = -1.0 + 2.0 * (j as f64 / 10.0);
                    let [x, y, z] = cube_to_sphere(face, u, v);
                    let len = (x * x + y * y + z * z).sqrt();
                    assert!(
                        (len - 1.0).abs() < 1e-10,
                        "face {:?} u={u} v={v}: length={len}",
                        face,
                    );
                }
            }
        }
    }

    #[test]
    fn adjacent_faces_share_edge_points() {
        for i in 0..=10 {
            let v = -1.0 + 2.0 * (i as f64 / 10.0);
            let from_pz = cube_to_sphere(CubeFace::PosZ, 1.0, v);
            let from_px = cube_to_sphere(CubeFace::PosX, -1.0, v);
            assert!(
                (from_pz[0] - from_px[0]).abs() < 1e-10
                    && (from_pz[1] - from_px[1]).abs() < 1e-10
                    && (from_pz[2] - from_px[2]).abs() < 1e-10,
                "edge mismatch at v={v}: pz={:?} px={:?}",
                from_pz,
                from_px,
            );
        }
    }

    #[test]
    fn face_point_to_position_scales_by_radius() {
        let radius = 6_371_000.0;
        let [x, y, z] = face_point_to_position(CubeFace::PosZ, 0.0, 0.0, radius);
        assert!((x).abs() < 1.0);
        assert!((y).abs() < 1.0);
        assert!((z - radius).abs() < 1.0);
    }
}
