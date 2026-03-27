//! Connection-point-based assembly system for snapping parts together.
//!
//! A `Part` has a mesh and named connection points. `attach()` computes the
//! transform needed to align two parts at their connection points, then merges
//! the meshes.

use crate::mesh::Mesh;
use glam::{Mat4, Quat, Vec3};

/// A named connection point on a part.
#[derive(Clone, Debug)]
pub struct ConnectPoint {
    /// Unique name, e.g. "fore", "aft", "port_door"
    pub id: &'static str,
    /// Position in the part's local space
    pub position: Vec3,
    /// Outward-facing direction (which way this opening faces)
    pub normal: Vec3,
}

/// A mesh with named connection points.
#[derive(Clone, Debug)]
pub struct Part {
    pub mesh: Mesh,
    pub connections: Vec<ConnectPoint>,
}

impl Part {
    /// Look up a connection point by id. Panics if not found.
    pub fn connection(&self, id: &str) -> &ConnectPoint {
        self.connections
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("connection point '{}' not found", id))
    }

    /// Look up a connection point by id, returning None if not found.
    pub fn try_connection(&self, id: &str) -> Option<&ConnectPoint> {
        self.connections.iter().find(|c| c.id == id)
    }
}

/// Compute the rotation quaternion that aligns `from` direction to `to` direction.
fn rotation_between(from: Vec3, to: Vec3) -> Quat {
    let from = from.normalize();
    let to = to.normalize();
    let dot = from.dot(to);

    if dot > 0.9999 {
        // Already aligned
        return Quat::IDENTITY;
    }
    if dot < -0.9999 {
        // Opposite directions: rotate 180 degrees around any perpendicular axis
        let perp = if from.x.abs() < 0.9 {
            from.cross(Vec3::X).normalize()
        } else {
            from.cross(Vec3::Y).normalize()
        };
        return Quat::from_axis_angle(perp, std::f32::consts::PI);
    }

    let axis = from.cross(to).normalize();
    let angle = dot.acos();
    Quat::from_axis_angle(axis, angle)
}

/// Attach `attach_part` to `base_part` by aligning connection points.
///
/// The attach part is:
/// 1. Rotated so its connection normal faces opposite to the base connection normal
///    (the two parts face each other at the join).
/// 2. Translated so the connection points coincide.
///
/// Returns a new Part with the merged mesh and all remaining connection points
/// (both base and transformed attach, minus the two used for joining).
pub fn attach(
    base: &Part,
    base_conn_id: &str,
    attach_part: &Part,
    attach_conn_id: &str,
) -> Part {
    let base_conn = base.connection(base_conn_id);
    let attach_conn = attach_part.connection(attach_conn_id);

    // Step 1: Rotate so attach_conn.normal faces opposite to base_conn.normal.
    // We want: rotated(attach_conn.normal) == -base_conn.normal
    let target_dir = -base_conn.normal;
    let rot = rotation_between(attach_conn.normal, target_dir);

    // Step 2: After rotation, compute where the attach connection point ended up,
    // then translate so it meets the base connection point.
    let rotated_attach_pos = rot * attach_conn.position;
    let translation = base_conn.position - rotated_attach_pos;

    // Build the full transform matrix: translate * rotate
    let transform = Mat4::from_translation(translation) * Mat4::from_quat(rot);

    // Transform the attach mesh
    let transformed_mesh = attach_part.mesh.transform(transform);

    // Merge meshes
    let merged_mesh = Mesh::merge(&[base.mesh.clone(), transformed_mesh]);

    // Collect connection points: base's (minus used one) + transformed attach's (minus used one)
    let mut connections: Vec<ConnectPoint> = base
        .connections
        .iter()
        .filter(|c| c.id != base_conn_id)
        .cloned()
        .collect();

    for conn in &attach_part.connections {
        if conn.id == attach_conn_id {
            continue;
        }
        let new_pos = transform.transform_point3(conn.position);
        let new_normal = (rot * conn.normal).normalize();
        connections.push(ConnectPoint {
            id: conn.id,
            position: new_pos,
            normal: new_normal,
        });
    }

    Part {
        mesh: merged_mesh,
        connections,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::box_mesh;

    fn corridor_part() -> Part {
        // A simple 3x2x2 box with "fore" (+Z) and "aft" (-Z) connections
        let mesh = box_mesh(3.0, 2.0, 2.0, [0.5; 3]);
        Part {
            mesh,
            connections: vec![
                ConnectPoint {
                    id: "fore",
                    position: Vec3::new(0.0, 0.0, 1.0),
                    normal: Vec3::Z,
                },
                ConnectPoint {
                    id: "aft",
                    position: Vec3::new(0.0, 0.0, -1.0),
                    normal: Vec3::NEG_Z,
                },
            ],
        }
    }

    #[test]
    fn attach_doubles_vertices() {
        let a = corridor_part();
        let b = corridor_part();
        let result = attach(&a, "fore", &b, "aft");
        // Two corridors merged: 24 + 24 = 48 vertices
        assert_eq!(result.mesh.vertices.len(), 48);
    }

    #[test]
    fn attach_removes_used_connections() {
        let a = corridor_part();
        let b = corridor_part();
        let result = attach(&a, "fore", &b, "aft");
        // a had fore+aft, b had fore+aft
        // Used: a.fore and b.aft -> remaining: a.aft + b.fore = 2
        assert_eq!(result.connections.len(), 2);
    }

    #[test]
    fn attach_aligns_connection_points() {
        let a = corridor_part();
        let b = corridor_part();
        let result = attach(&a, "fore", &b, "aft");

        // b's "aft" should now coincide with a's "fore" position
        // The aft connection of a is still at (0,0,-1)
        let aft = result.connection("aft");
        assert!((aft.position - Vec3::new(0.0, 0.0, -1.0)).length() < 1e-4);

        // b's "fore" connection should be at a.fore.position + 2 units along +Z
        // (because b is 2 units deep, and b.aft was at z=-1 of b, b.fore at z=+1)
        // After attaching: b.aft aligns to a.fore(z=1), so b.fore ends up at z=3
        let fore = result.connection("fore");
        assert!(
            (fore.position.z - 3.0).abs() < 1e-4,
            "b's fore should be at z=3 after attachment, got z={}",
            fore.position.z
        );
    }

    #[test]
    fn attach_normals_face_opposite_at_join() {
        let a = corridor_part();
        let b = corridor_part();
        let _result = attach(&a, "fore", &b, "aft");

        // At the join point, a.fore faces +Z, b.aft (after transform) faces -Z.
        // They should be anti-parallel (dot product = -1).
        let a_fore = a.connection("fore");
        let b_aft = b.connection("aft");
        // b.aft's normal (-Z) after the rotation should become -a.fore.normal = -Z
        // And a.fore.normal = +Z. So the normals at the join are +Z and -Z: anti-parallel.
        let dot = a_fore.normal.dot(b_aft.normal);
        // They start as +Z and -Z, so dot = -1
        assert!((dot - (-1.0)).abs() < 1e-4, "normals should be anti-parallel");
    }

    #[test]
    fn continuous_mesh_no_gap() {
        let a = corridor_part();
        let b = corridor_part();
        let result = attach(&a, "fore", &b, "aft");
        let (min, max) = result.mesh.bounding_box();
        // a: z from -1 to +1, b attached at fore: z from +1 to +3
        // Total Z span should be 4.0 (from -1 to +3)
        let z_span = max.z - min.z;
        assert!(
            (z_span - 4.0).abs() < 1e-4,
            "z span should be 4.0 (continuous), got {}",
            z_span
        );
    }
}
