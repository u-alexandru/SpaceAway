//! Frustum culling: extract planes from a view-projection matrix
//! and test bounding spheres against them.

/// Six frustum planes extracted from a view-projection matrix.
/// Each plane is `[nx, ny, nz, d]` where `nx*x + ny*y + nz*z + d >= 0` is inside.
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    pub planes: [[f64; 4]; 6],
}

impl Frustum {
    /// Extract frustum planes from a column-major 4×4 view-projection matrix.
    /// Uses the Gribb/Hartmann method: each plane is a row combination of the VP matrix.
    ///
    /// `vp` is a flat [f64; 16] in column-major order (OpenGL/glam convention):
    /// indices [0..3] = column 0, [4..7] = column 1, [8..11] = column 2, [12..15] = column 3.
    pub fn from_vp_matrix(vp: [f64; 16]) -> Self {
        // Row vectors from the column-major matrix:
        // row[i] = [vp[i], vp[4+i], vp[8+i], vp[12+i]]
        let row = |i: usize| -> [f64; 4] {
            [vp[i], vp[4 + i], vp[8 + i], vp[12 + i]]
        };

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        let add = |a: [f64; 4], b: [f64; 4]| -> [f64; 4] {
            [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
        };
        let sub = |a: [f64; 4], b: [f64; 4]| -> [f64; 4] {
            [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
        };

        let mut planes = [
            add(r3, r0),  // Left
            sub(r3, r0),  // Right
            add(r3, r1),  // Bottom
            sub(r3, r1),  // Top
            add(r3, r2),  // Near
            sub(r3, r2),  // Far
        ];

        // Normalize each plane
        for plane in &mut planes {
            let len = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
            if len > 1e-12 {
                plane[0] /= len;
                plane[1] /= len;
                plane[2] /= len;
                plane[3] /= len;
            }
        }

        Self { planes }
    }

    /// Test if a sphere is at least partially inside the frustum.
    /// Returns false if the sphere is completely outside any plane.
    pub fn contains_sphere(&self, center: [f64; 3], radius: f64) -> bool {
        for plane in &self.planes {
            let dist = plane[0] * center[0]
                + plane[1] * center[1]
                + plane[2] * center[2]
                + plane[3];
            if dist < -radius {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_at_origin_inside_large_frustum() {
        // Identity-like VP — everything should be inside
        let mut vp = [0.0f64; 16];
        // Column-major identity with large far plane
        vp[0] = 1.0;
        vp[5] = 1.0;
        vp[10] = 1.0;
        vp[15] = 1.0;

        let frustum = Frustum::from_vp_matrix(vp);
        assert!(frustum.contains_sphere([0.0, 0.0, 0.0], 1.0));
    }

    #[test]
    fn sphere_far_away_outside_frustum() {
        // Simple perspective-like VP
        let mut vp = [0.0f64; 16];
        vp[0] = 1.0;  // x scale
        vp[5] = 1.0;  // y scale
        vp[10] = -1.0; // z
        vp[14] = -1.0; // perspective divide
        vp[11] = -1.0; // w = -z

        let frustum = Frustum::from_vp_matrix(vp);
        // Very far to the right — should be outside
        assert!(!frustum.contains_sphere([1000.0, 0.0, -1.0], 1.0));
    }
}
