//! CDLOD quadtree: LOD range selection, node traversal, frustum culling.
//!
//! Per the Strugar (2010) CDLOD paper: LOD ranges double per level,
//! nodes subdivide when camera is closer than range, morph at 50%.

use crate::cube_sphere::{CubeFace, cube_to_sphere};

/// Minimum range for finest LOD level (meters).
const MIN_RANGE: f64 = 50.0;

/// Hard cap on total visible nodes to prevent runaway subdivision.
/// 800 ensures the face under the camera gets enough fine-LOD nodes
/// even after other faces consume nodes for the far hemisphere.
const MAX_VISIBLE_NODES: usize = 800;

/// A visible terrain node selected by the quadtree traversal.
#[derive(Debug, Clone)]
pub struct VisibleNode {
    pub face: CubeFace,
    pub lod: u8,
    pub x: u32,
    pub y: u32,
    /// Node center on sphere surface (planet-relative meters).
    pub center: [f64; 3],
    /// Morph factor for this node (0.0 = full detail, 1.0 = fully morphed to parent).
    pub morph_factor: f32,
}

/// Compute LOD range for a given level. Camera farther than this = LOD is sufficient.
pub fn lod_range(level: u8) -> f64 {
    MIN_RANGE * (1u64 << level) as f64
}

/// Compute the maximum number of LOD levels needed for a planet.
/// `face_size_m`: approximate size of one cube face on the sphere surface.
pub fn max_lod_levels(face_size_m: f64) -> u8 {
    let ratio = face_size_m / MIN_RANGE;
    if ratio <= 1.0 {
        return 1;
    }
    // Cap at 30 to prevent 1u32 << lod overflow (u32 max shift is 31).
    (ratio.log2().ceil() as u8).clamp(1, 30)
}

/// Select visible nodes for rendering. Returns nodes sorted coarsest-first.
///
/// `camera_pos`: camera position in planet-relative meters.
/// `planet_radius_m`: planet radius for sphere-surface calculations.
/// `max_lod`: finest LOD level (from `max_lod_levels`).
#[profiling::function]
pub fn select_visible_nodes(
    camera_pos: [f64; 3],
    planet_radius_m: f64,
    max_lod: u8,
    max_displacement: f64,
) -> Vec<VisibleNode> {
    // When the camera is inside the planet (e.g., cruise overshoot or physics
    // glitch), project it onto the surface so LOD selection still produces
    // fine nodes for the nearest terrain. Without this, the camera-to-node
    // distances are all ~radius and the quadtree returns only coarse LOD 0-1
    // nodes, making the terrain look like a smaller sphere of flat panels.
    // The MAX_VISIBLE_NODES cap (500) prevents runaway subdivision.
    let cam_dist = (camera_pos[0] * camera_pos[0]
        + camera_pos[1] * camera_pos[1]
        + camera_pos[2] * camera_pos[2]).sqrt();
    let effective_cam = if cam_dist < planet_radius_m && cam_dist > 1.0 {
        let scale = (planet_radius_m * 1.01) / cam_dist;
        [camera_pos[0] * scale, camera_pos[1] * scale, camera_pos[2] * scale]
    } else {
        camera_pos
    };

    // Sort faces by proximity to camera so the face directly under the
    // camera is visited first. Without this, the fixed iteration order
    // (PosX, NegX, PosY, NegY, PosZ, NegZ) can exhaust the node cap on
    // other faces before reaching the critical face, producing gaps.
    let mut face_dists: [(CubeFace, f64); 6] = CubeFace::ALL.map(|face| {
        let dir = cube_to_sphere(face, 0.0, 0.0);
        let center = [dir[0] * planet_radius_m, dir[1] * planet_radius_m, dir[2] * planet_radius_m];
        let dx = effective_cam[0] - center[0];
        let dy = effective_cam[1] - center[1];
        let dz = effective_cam[2] - center[2];
        (face, dx * dx + dy * dy + dz * dz)
    });
    face_dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut nodes = Vec::with_capacity(512);
    for &(face, _) in &face_dists {
        if nodes.len() >= MAX_VISIBLE_NODES { break; }
        select_recursive(
            face, 0, 0, 0,
            effective_cam, planet_radius_m, max_lod, max_displacement,
            &mut nodes,
        );
    }
    nodes
}

#[allow(clippy::too_many_arguments)]
fn select_recursive(
    face: CubeFace,
    lod: u8,
    x: u32,
    y: u32,
    camera_pos: [f64; 3],
    radius: f64,
    max_lod: u8,
    max_displacement: f64,
    out: &mut Vec<VisibleNode>,
) {
    // Hard cap: stop subdivision if we've already collected enough nodes.
    if out.len() >= MAX_VISIBLE_NODES { return; }

    // Compute node center on sphere surface
    let subdivs = 1u32 << lod;
    let u = -1.0 + (2.0 * x as f64 + 1.0) / subdivs as f64;
    let v = -1.0 + (2.0 * y as f64 + 1.0) / subdivs as f64;
    let dir = cube_to_sphere(face, u, v);
    let center = [dir[0] * radius, dir[1] * radius, dir[2] * radius];

    // Distance from camera to node center
    let dx = camera_pos[0] - center[0];
    let dy = camera_pos[1] - center[1];
    let dz = camera_pos[2] - center[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    // Node bounding radius: half the face-diagonal at this LOD, inflated by displacement.
    // Displacement scales with face_size (finer LODs have proportionally smaller height
    // variation), not the planet-wide maximum. Using the full max_displacement at fine
    // LODs would inflate tiny nodes by 100+ km, causing the quadtree to produce
    // millions of nodes and freeze the game.
    let face_size = 2.0 * radius / subdivs as f64;
    let displacement_at_lod = max_displacement.min(face_size * 0.5);
    let node_radius = face_size * std::f64::consts::FRAC_1_SQRT_2 + displacement_at_lod;

    let range = lod_range(lod);

    // If far enough, or at finest level, emit this node
    if dist > range + node_radius || lod == max_lod {
        let morph_start = range * 0.5;
        let morph = if dist > morph_start {
            ((dist - morph_start) / (range - morph_start)).min(1.0) as f32
        } else {
            0.0
        };

        out.push(VisibleNode {
            face,
            lod,
            x,
            y,
            center,
            morph_factor: morph,
        });
        return;
    }

    // Subdivide into 4 children, nearest-first so the node cap doesn't
    // starve the quadrant directly under the camera.
    let child_lod = lod + 1;
    let cx = x * 2;
    let cy = y * 2;
    let child_subdivs = 1u32 << child_lod;
    let children = [(cx, cy), (cx + 1, cy), (cx, cy + 1), (cx + 1, cy + 1)];
    let mut child_dists: [(u32, u32, f64); 4] = children.map(|(cxi, cyi)| {
        let cu = -1.0 + (2.0 * cxi as f64 + 1.0) / child_subdivs as f64;
        let cv = -1.0 + (2.0 * cyi as f64 + 1.0) / child_subdivs as f64;
        let cdir = cube_to_sphere(face, cu, cv);
        let ccx = cdir[0] * radius;
        let ccy = cdir[1] * radius;
        let ccz = cdir[2] * radius;
        let ddx = camera_pos[0] - ccx;
        let ddy = camera_pos[1] - ccy;
        let ddz = camera_pos[2] - ccz;
        (cxi, cyi, ddx * ddx + ddy * ddy + ddz * ddz)
    });
    child_dists.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    for &(cxi, cyi, _) in &child_dists {
        select_recursive(face, child_lod, cxi, cyi, camera_pos, radius, max_lod, max_displacement, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_range_doubles_per_level() {
        assert!((lod_range(0) - 50.0).abs() < 1e-6);
        assert!((lod_range(1) - 100.0).abs() < 1e-6);
        assert!((lod_range(2) - 200.0).abs() < 1e-6);
        assert!((lod_range(17) - 50.0 * 131072.0).abs() < 1e-6);
    }

    #[test]
    fn max_lod_for_earth() {
        let levels = max_lod_levels(10_000_000.0);
        assert_eq!(levels, 18);
    }

    #[test]
    fn max_lod_for_small_moon() {
        let levels = max_lod_levels(500_000.0);
        assert_eq!(levels, 14);
    }

    #[test]
    fn camera_at_surface_produces_finest_nodes() {
        let radius = 6_371_000.0;
        let camera = [0.0, 0.0, radius];
        let max_lod = max_lod_levels(radius * 1.57);
        let nodes = select_visible_nodes(camera, radius, max_lod, radius * 0.02);
        let finest = nodes.iter().filter(|n| n.lod == max_lod).count();
        assert!(finest > 0, "expected finest-LOD nodes near camera, got 0");
    }

    #[test]
    fn camera_far_away_produces_coarse_nodes() {
        let radius = 6_371_000.0;
        let camera = [0.0, 0.0, radius * 10.0];
        let max_lod = max_lod_levels(radius * 1.57);
        let nodes = select_visible_nodes(camera, radius, max_lod, radius * 0.02);
        let max_lod_seen = nodes.iter().map(|n| n.lod).max().unwrap_or(0);
        assert!(max_lod_seen < 5, "expected coarse nodes far away, got max lod {max_lod_seen}");
    }

    #[test]
    fn all_six_faces_represented() {
        let radius = 1_000_000.0;
        let camera = [0.0, 0.0, 0.0];
        let max_lod = 10;
        let nodes = select_visible_nodes(camera, radius, max_lod, 0.0);
        let mut faces_seen = std::collections::HashSet::new();
        for n in &nodes {
            faces_seen.insert(n.face);
        }
        assert_eq!(faces_seen.len(), 6, "expected all 6 faces, got {}", faces_seen.len());
    }

    #[test]
    fn morph_factor_zero_near_camera() {
        let radius = 6_371_000.0;
        let camera = [0.0, 0.0, radius];
        let max_lod = 18;
        let nodes = select_visible_nodes(camera, radius, max_lod, radius * 0.02);
        let nearest = nodes.iter()
            .filter(|n| n.lod == max_lod)
            .min_by(|a, b| {
                let da = (a.center[2] - camera[2]).abs();
                let db = (b.center[2] - camera[2]).abs();
                da.partial_cmp(&db).unwrap()
            });
        if let Some(n) = nearest {
            assert!(n.morph_factor < 0.5, "nearest finest node should have low morph, got {}", n.morph_factor);
        }
    }
}
