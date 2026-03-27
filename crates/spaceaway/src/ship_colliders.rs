//! Auto-generated ship interior colliders from hex hull geometry.
//!
//! Instead of manually placing box colliders, this module generates colliders
//! directly from the hex ring vertices used by sa_meshgen. Each hull section
//! gets wall colliders built as convex hulls of the actual hex face geometry,
//! plus flat cuboids for floors and ceilings.
//!
//! Collision groups follow the standards in docs/collision-system-standards.md.

use rapier3d::prelude::*;
use sa_meshgen::auto_collider::points_from_positions;
use sa_meshgen::hull::hex_ring;
use sa_physics::PhysicsWorld;

// ---------------------------------------------------------------------------
// Collision group constants (from collision-system-standards.md)
// ---------------------------------------------------------------------------

/// Ship hull sensor: mass/inertia only, no contact forces.
#[allow(dead_code)]
pub const SHIP_HULL: Group = Group::GROUP_1; // 0x0001

/// Interior walkable surfaces (walls, floors, ceilings, bulkheads).
pub const SHIP_INTERIOR: Group = Group::GROUP_2; // 0x0002

/// Player capsule collider.
#[allow(dead_code)]
pub const PLAYER: Group = Group::GROUP_3; // 0x0004

/// Interactable sensor volumes (raycast-only detection).
pub const INTERACTABLE: Group = Group::GROUP_4; // 0x0008

// Membership + filter helpers:
// SHIP_INTERIOR colliders are in SHIP_INTERIOR, collide with PLAYER.
// PLAYER is in PLAYER, collides with SHIP_INTERIOR | WORLD.
// INTERACTABLE is in INTERACTABLE, collides with nothing (sensor, raycast only).

fn interior_groups() -> InteractionGroups {
    InteractionGroups::new(SHIP_INTERIOR, PLAYER)
}

#[allow(dead_code)]
fn interactable_groups() -> InteractionGroups {
    InteractionGroups::new(INTERACTABLE, Group::NONE)
}

// ---------------------------------------------------------------------------
// Ship section definitions
// ---------------------------------------------------------------------------

/// A ship section: its hull width at fore/aft ends, height, and Z extent.
struct Section {
    fore_width: f32,
    aft_width: f32,
    height: f32,
    z_start: f32,
    length: f32,
    /// Whether there is a bulkhead with door at the fore end.
    bulkhead_fore: bool,
    /// Whether there is a bulkhead with door at the aft end.
    bulkhead_aft: bool,
}

/// The full ship layout matching assemble_ship() in main.rs:
/// cockpit(4m) + corridor(3m) + transition(1m) + nav_room(5m)
/// + transition(1m) + corridor(3m) + transition(1m) + eng_room(5m)
/// + transition(1m) + engine(5m)
fn ship_sections() -> Vec<Section> {
    let h = 3.0; // STD_HEIGHT
    let mut sections = Vec::new();
    let mut z = 0.0;

    // Cockpit: 2.0 -> 4.0, length 4.0, bulkhead at aft only
    sections.push(Section {
        fore_width: 2.0, aft_width: 4.0, height: h,
        z_start: z, length: 4.0,
        bulkhead_fore: false, bulkhead_aft: true,
    });
    z += 4.0;

    // Corridor 1: 4.0 -> 4.0, length 3.0
    sections.push(Section {
        fore_width: 4.0, aft_width: 4.0, height: h,
        z_start: z, length: 3.0,
        bulkhead_fore: true, bulkhead_aft: true,
    });
    z += 3.0;

    // Transition 1: 4.0 -> 5.0, length 1.0
    sections.push(Section {
        fore_width: 4.0, aft_width: 5.0, height: h,
        z_start: z, length: 1.0,
        bulkhead_fore: false, bulkhead_aft: false,
    });
    z += 1.0;

    // Nav room: 5.0 -> 5.0, length 5.0
    sections.push(Section {
        fore_width: 5.0, aft_width: 5.0, height: h,
        z_start: z, length: 5.0,
        bulkhead_fore: true, bulkhead_aft: true,
    });
    z += 5.0;

    // Transition 2: 5.0 -> 4.0, length 1.0
    sections.push(Section {
        fore_width: 5.0, aft_width: 4.0, height: h,
        z_start: z, length: 1.0,
        bulkhead_fore: false, bulkhead_aft: false,
    });
    z += 1.0;

    // Corridor 2: 4.0 -> 4.0, length 3.0
    sections.push(Section {
        fore_width: 4.0, aft_width: 4.0, height: h,
        z_start: z, length: 3.0,
        bulkhead_fore: true, bulkhead_aft: true,
    });
    z += 3.0;

    // Transition 3: 4.0 -> 5.0, length 1.0
    sections.push(Section {
        fore_width: 4.0, aft_width: 5.0, height: h,
        z_start: z, length: 1.0,
        bulkhead_fore: false, bulkhead_aft: false,
    });
    z += 1.0;

    // Engineering room: 5.0 -> 5.0, length 5.0
    sections.push(Section {
        fore_width: 5.0, aft_width: 5.0, height: h,
        z_start: z, length: 5.0,
        bulkhead_fore: true, bulkhead_aft: true,
    });
    z += 5.0;

    // Transition 4: 5.0 -> 3.5, length 1.0
    sections.push(Section {
        fore_width: 5.0, aft_width: 3.5, height: h,
        z_start: z, length: 1.0,
        bulkhead_fore: false, bulkhead_aft: false,
    });
    z += 1.0;

    // Engine section: 3.5 -> 2.0, length 5.0
    sections.push(Section {
        fore_width: 3.5, aft_width: 2.0, height: h,
        z_start: z, length: 5.0,
        bulkhead_fore: true, bulkhead_aft: false,
    });

    sections
}

// ---------------------------------------------------------------------------
// Collider builders
// ---------------------------------------------------------------------------

const FLOOR_Y: f32 = -1.0;
const CEILING_Y: f32 = 1.2;
const WALL_THICKNESS: f32 = 0.15;
const DOOR_W: f32 = 1.2;
const DOOR_H: f32 = 2.0;

/// Add a fixed rigid body with a collider at the given position.
fn add_fixed_collider(physics: &mut PhysicsWorld, collider: Collider) {
    let body = RigidBodyBuilder::fixed().build();
    let handle = physics.add_rigid_body(body);
    physics.add_collider(collider, handle);
}

/// Build a convex hull wall collider from a hex face.
///
/// Given the four corner positions of one hex face (fore_a, fore_b at z_front;
/// aft_a, aft_b at z_back), creates a thin convex hull by offsetting inward
/// by `thickness` along the face normal.
fn hex_face_wall_collider(
    fore_a: [f32; 3],
    fore_b: [f32; 3],
    aft_a: [f32; 3],
    aft_b: [f32; 3],
    thickness: f32,
) -> Option<Collider> {
    // Compute the inward-facing normal of this face.
    let ax = aft_a[0] - fore_a[0];
    let ay = aft_a[1] - fore_a[1];
    let az = aft_a[2] - fore_a[2];
    let bx = fore_b[0] - fore_a[0];
    let by = fore_b[1] - fore_a[1];
    let bz = fore_b[2] - fore_a[2];
    // Cross product (a x b) gives outward normal for CCW winding
    let nx = ay * bz - az * by;
    let ny = az * bx - ax * bz;
    let nz = ax * by - ay * bx;
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len < 1e-6 {
        return None;
    }
    // Inward normal (negate the outward)
    let inx = -nx / len * thickness;
    let iny = -ny / len * thickness;
    let inz = -nz / len * thickness;

    let positions = [
        // Outer face
        fore_a, fore_b, aft_a, aft_b,
        // Inner face (offset inward)
        [fore_a[0] + inx, fore_a[1] + iny, fore_a[2] + inz],
        [fore_b[0] + inx, fore_b[1] + iny, fore_b[2] + inz],
        [aft_a[0] + inx, aft_a[1] + iny, aft_a[2] + inz],
        [aft_b[0] + inx, aft_b[1] + iny, aft_b[2] + inz],
    ];

    let points = points_from_positions(&positions);
    ColliderBuilder::convex_hull(&points).map(|b| {
        b.friction(0.5)
            .restitution(0.0)
            .collision_groups(interior_groups())
            .build()
    })
}

/// Generate hex wall colliders for one ship section.
///
/// Creates 6 convex hull colliders (one per hex face) connecting the fore
/// ring to the aft ring. Only generates colliders for faces that are above
/// the floor level (the bottom face is handled by the floor cuboid).
fn section_wall_colliders(physics: &mut PhysicsWorld, section: &Section) {
    let fore_ring = hex_ring(section.fore_width, section.height, section.z_start);
    let aft_ring = hex_ring(
        section.aft_width,
        section.height,
        section.z_start + section.length,
    );

    // hex_ring vertices: [0]=top-left, [1]=top-right, [2]=right,
    // [3]=bottom-right, [4]=bottom-left, [5]=left
    // Connect each face: fore[i] to fore[next], aft[i] to aft[next]
    for i in 0..6 {
        let next = (i + 1) % 6;

        // Skip the bottom face (indices 3->4, which is bottom-right to bottom-left).
        // The floor cuboid handles that boundary.
        if i == 3 {
            continue;
        }

        if let Some(collider) = hex_face_wall_collider(
            fore_ring[i],
            fore_ring[next],
            aft_ring[i],
            aft_ring[next],
            WALL_THICKNESS,
        ) {
            add_fixed_collider(physics, collider);
        }
    }
}

/// Add a floor cuboid for a section.
fn section_floor_collider(physics: &mut PhysicsWorld, section: &Section) {
    // Use the narrower of fore/aft widths for safe floor extent
    let w = section.fore_width.min(section.aft_width);
    let hw = w / 2.0 - 0.05; // slightly inset from hull
    let hl = section.length / 2.0;
    let center_z = section.z_start + hl;

    let collider = ColliderBuilder::cuboid(hw, 0.1, hl)
        .translation(nalgebra::Vector3::new(0.0, FLOOR_Y - 0.1, center_z))
        .friction(0.8)
        .restitution(0.0)
        .collision_groups(interior_groups())
        .build();
    add_fixed_collider(physics, collider);
}

/// Add a ceiling cuboid for a section.
fn section_ceiling_collider(physics: &mut PhysicsWorld, section: &Section) {
    let w = section.fore_width.min(section.aft_width);
    let hw = w / 2.0 - 0.05;
    let hl = section.length / 2.0;
    let center_z = section.z_start + hl;

    let collider = ColliderBuilder::cuboid(hw, 0.1, hl)
        .translation(nalgebra::Vector3::new(0.0, CEILING_Y + 0.1, center_z))
        .friction(0.3)
        .restitution(0.0)
        .collision_groups(interior_groups())
        .build();
    add_fixed_collider(physics, collider);
}

/// Add a bulkhead with door opening at a given Z position.
///
/// Creates three colliders: left of door, right of door, and lintel above door.
/// All shaped as convex hulls from the hex cross-section at that Z.
fn bulkhead_colliders(physics: &mut PhysicsWorld, width: f32, z: f32) {
    let hdw = DOOR_W / 2.0; // 0.6
    let hw = width / 2.0;
    let door_bottom = FLOOR_Y;
    let door_top = FLOOR_Y + DOOR_H;
    let bulkhead_thickness = 0.1;
    let ht = bulkhead_thickness / 2.0;

    // Left of door: from -hw to -hdw, floor to ceiling
    let left_points = points_from_positions(&[
        [-hw, door_bottom, z - ht],
        [-hdw, door_bottom, z - ht],
        [-hdw, CEILING_Y, z - ht],
        [-hw, CEILING_Y, z - ht],
        [-hw, door_bottom, z + ht],
        [-hdw, door_bottom, z + ht],
        [-hdw, CEILING_Y, z + ht],
        [-hw, CEILING_Y, z + ht],
    ]);
    if let Some(builder) = ColliderBuilder::convex_hull(&left_points) {
        let collider = builder
            .friction(0.5)
            .restitution(0.0)
            .collision_groups(interior_groups())
            .build();
        add_fixed_collider(physics, collider);
    }

    // Right of door: from hdw to hw, floor to ceiling
    let right_points = points_from_positions(&[
        [hdw, door_bottom, z - ht],
        [hw, door_bottom, z - ht],
        [hw, CEILING_Y, z - ht],
        [hdw, CEILING_Y, z - ht],
        [hdw, door_bottom, z + ht],
        [hw, door_bottom, z + ht],
        [hw, CEILING_Y, z + ht],
        [hdw, CEILING_Y, z + ht],
    ]);
    if let Some(builder) = ColliderBuilder::convex_hull(&right_points) {
        let collider = builder
            .friction(0.5)
            .restitution(0.0)
            .collision_groups(interior_groups())
            .build();
        add_fixed_collider(physics, collider);
    }

    // Lintel above door: spans door width, from door_top to ceiling
    if CEILING_Y > door_top + 0.01 {
        let lintel_points = points_from_positions(&[
            [-hdw, door_top, z - ht],
            [hdw, door_top, z - ht],
            [hdw, CEILING_Y, z - ht],
            [-hdw, CEILING_Y, z - ht],
            [-hdw, door_top, z + ht],
            [hdw, door_top, z + ht],
            [hdw, CEILING_Y, z + ht],
            [-hdw, CEILING_Y, z + ht],
        ]);
        if let Some(builder) = ColliderBuilder::convex_hull(&lintel_points) {
            let collider = builder
                .friction(0.5)
                .restitution(0.0)
                .collision_groups(interior_groups())
                .build();
            add_fixed_collider(physics, collider);
        }
    }
}

/// Add an endcap wall (solid, no door) at a given Z position.
fn endcap_collider(physics: &mut PhysicsWorld, width: f32, z: f32) {
    let hw = width / 2.0;
    let ht = 0.1;

    let points = points_from_positions(&[
        [-hw, FLOOR_Y, z - ht],
        [hw, FLOOR_Y, z - ht],
        [hw, CEILING_Y, z - ht],
        [-hw, CEILING_Y, z - ht],
        [-hw, FLOOR_Y, z + ht],
        [hw, FLOOR_Y, z + ht],
        [hw, CEILING_Y, z + ht],
        [-hw, CEILING_Y, z + ht],
    ]);
    if let Some(builder) = ColliderBuilder::convex_hull(&points) {
        let collider = builder
            .friction(0.5)
            .restitution(0.0)
            .collision_groups(interior_groups())
            .build();
        add_fixed_collider(physics, collider);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build all ship interior colliders from hex hull geometry.
///
/// For each ship section:
/// - 5 hex face wall colliders (convex hulls from actual hex vertices, skipping bottom)
/// - 1 floor cuboid (Tier 1 -- flat surface)
/// - 1 ceiling cuboid (Tier 1 -- flat surface)
/// - Bulkhead colliders at section boundaries with door openings
/// - Endcap colliders at the nose and tail
///
/// All colliders use the SHIP_INTERIOR collision group so they interact
/// with PLAYER but not with interaction raycasts or projectiles.
pub fn build_ship_colliders(physics: &mut PhysicsWorld) {
    let sections = ship_sections();

    for section in &sections {
        // Hex face walls (5 faces, skipping bottom)
        section_wall_colliders(physics, section);

        // Floor and ceiling
        section_floor_collider(physics, section);
        section_ceiling_collider(physics, section);

        // Bulkheads at section boundaries
        if section.bulkhead_fore {
            let width = section.fore_width;
            bulkhead_colliders(physics, width, section.z_start);
        }
        if section.bulkhead_aft {
            let width = section.aft_width;
            bulkhead_colliders(physics, width, section.z_start + section.length);
        }
    }

    // Endcaps at nose (z=0) and tail (z=29)
    let first = &sections[0];
    endcap_collider(physics, first.fore_width, first.z_start);

    let last = &sections[sections.len() - 1];
    endcap_collider(physics, last.aft_width, last.z_start + last.length);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_colliders_does_not_panic() {
        let mut physics = PhysicsWorld::new();
        build_ship_colliders(&mut physics);
        // Should have a reasonable number of colliders
        let count = physics.collider_set.len();
        assert!(
            count > 20,
            "Too few colliders: {count} (expected >20 for hex walls + floors + bulkheads)"
        );
        assert!(
            count < 200,
            "Too many colliders: {count}"
        );
    }

    #[test]
    fn all_colliders_are_in_interior_group() {
        let mut physics = PhysicsWorld::new();
        build_ship_colliders(&mut physics);

        for (_, collider) in physics.collider_set.iter() {
            let groups = collider.collision_groups();
            assert_eq!(
                groups.memberships, SHIP_INTERIOR,
                "all interior colliders should be in SHIP_INTERIOR group"
            );
            assert_eq!(
                groups.filter, PLAYER,
                "interior colliders should only interact with PLAYER"
            );
        }
    }

    #[test]
    fn hex_face_wall_creates_valid_collider() {
        // A simple vertical quad face
        let collider = hex_face_wall_collider(
            [-1.0, -1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [-1.0, -1.0, 5.0],
            [-1.0, 1.0, 5.0],
            0.15,
        );
        assert!(collider.is_some(), "should produce a valid convex hull");
    }

    #[test]
    fn ship_sections_total_length() {
        let sections = ship_sections();
        let total: f32 = sections.iter().map(|s| s.length).sum();
        // cockpit(4) + corr(3) + trans(1) + nav(5) + trans(1) + corr(3)
        // + trans(1) + eng(5) + trans(1) + engine(5) = 29
        assert!(
            (total - 29.0).abs() < 0.01,
            "total ship length should be 29m, got {total}"
        );
    }

    #[test]
    fn sections_are_contiguous() {
        let sections = ship_sections();
        for i in 1..sections.len() {
            let prev_end = sections[i - 1].z_start + sections[i - 1].length;
            let curr_start = sections[i].z_start;
            assert!(
                (prev_end - curr_start).abs() < 0.01,
                "gap between section {} and {}: prev ends at {prev_end}, next starts at {curr_start}",
                i - 1,
                i
            );
        }
    }
}
