//! Ship scene setup: creates the ship body, meshes, and interactables.

use glam::Vec3;
use rapier3d::prelude::*;
use sa_meshgen::interactables;
use sa_physics::PhysicsWorld;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_ship::station::{cockpit_layout, PlacementKind};

use crate::ship_colliders::INTERACTABLE;

/// IDs for the key interactables, so the game loop can read their state.
#[allow(dead_code)]
pub struct ShipInteractableIds {
    pub throttle_lever: usize,
    pub engine_button: usize,
    pub speed_screen: usize,
    pub helm_seat: usize,
}

/// All the meshes needed for the ship scene.
pub struct ShipMeshes {
    /// The assembled ship hull mesh.
    pub hull: sa_meshgen::Mesh,
    /// Interactable meshes, paired with their positions in ship space.
    pub interactable_meshes: Vec<(sa_meshgen::Mesh, Vec3)>,
}

/// Create the ship physics body and register interactables.
///
/// Returns the Ship, the InteractionSystem with registered interactables,
/// and the interactable IDs for game-loop wiring.
pub fn create_ship_and_interactables(
    physics: &mut PhysicsWorld,
) -> (Ship, InteractionSystem, ShipInteractableIds) {
    // Create ship body at world origin
    let ship = Ship::new(physics, 0.0, 0.0, 0.0);

    // Build precise hex-hull interior colliders (walls, bulkheads, endcaps).
    // These match the actual hexagonal cross-section of each ship section,
    // replacing the old simple box colliders that left gaps at angled hull faces.
    crate::ship_colliders::build_ship_colliders(physics);

    let mut interaction = InteractionSystem::new();
    let layout = cockpit_layout();

    let mut throttle_lever = 0;
    let mut engine_button = 0;
    let mut speed_screen = 0;
    let mut helm_seat = 0;

    for placement in &layout.interactables {
        // Generate the interactable mesh and create a CONVEX HULL sensor
        // from its actual geometry. This is much more precise than box
        // approximations — the collision shape matches the visual exactly.
        let mesh = match &placement.kind {
            PlacementKind::Lever => interactables::lever_mesh(0.0),
            PlacementKind::ToggleButton | PlacementKind::MomentaryButton => {
                interactables::button_mesh(false)
            }
            PlacementKind::Switch { num_positions } => interactables::switch_mesh(0, *num_positions),
            PlacementKind::Screen { width, height } => {
                interactables::screen_mesh(*width, *height)
            }
            PlacementKind::HelmSeat => interactables::helm_seat_mesh(),
        };

        // Convert mesh vertices to points and build convex hull collider
        let points = sa_meshgen::auto_collider::mesh_to_points(&mesh);
        let collider = if let Some(builder) = ColliderBuilder::convex_hull(&points) {
            builder
        } else {
            // Fallback to bounding box if convex hull fails
            let (_center, half) = sa_meshgen::auto_collider::mesh_to_aabb(&mesh);
            ColliderBuilder::cuboid(half[0].max(0.05), half[1].max(0.05), half[2].max(0.05))
        };

        let collider = collider
            .translation(nalgebra::Vector3::new(
                placement.position.x,
                placement.position.y,
                placement.position.z,
            ))
            .sensor(true)
            .collision_groups(InteractionGroups::new(
                INTERACTABLE,
                INTERACTABLE,
            ))
            .build();
        let collider_handle = physics.add_collider(collider, ship.body_handle);

        let id = match &placement.kind {
            PlacementKind::Lever => {
                let id = interaction.register(
                    sa_ship::Interactable::lever(collider_handle, &placement.label),
                );
                throttle_lever = id;
                id
            }
            PlacementKind::ToggleButton => {
                let id = interaction.register(
                    sa_ship::Interactable::toggle_button(collider_handle, &placement.label),
                );
                engine_button = id;
                id
            }
            PlacementKind::MomentaryButton => interaction.register(
                sa_ship::Interactable::momentary_button(collider_handle, &placement.label),
            ),
            PlacementKind::Switch { num_positions } => interaction.register(
                sa_ship::Interactable::switch(collider_handle, *num_positions, &placement.label),
            ),
            PlacementKind::Screen { .. } => {
                let id = interaction.register(
                    sa_ship::Interactable::screen(collider_handle, &placement.label),
                );
                speed_screen = id;
                id
            }
            PlacementKind::HelmSeat => {
                let id = interaction.register(
                    sa_ship::Interactable::helm_seat(collider_handle, &placement.label),
                );
                helm_seat = id;
                id
            }
        };
        let _ = id; // suppress unused warning for non-tracked ones
    }

    let ids = ShipInteractableIds {
        throttle_lever,
        engine_button,
        speed_screen,
        helm_seat,
    };

    (ship, interaction, ids)
}

/// Generate all meshes for the ship scene.
pub fn generate_ship_meshes() -> ShipMeshes {
    // Assemble the hull
    let hull = crate::assemble_ship();

    // Generate interactable meshes at cockpit positions
    let layout = cockpit_layout();
    let mut interactable_meshes = Vec::new();

    for placement in &layout.interactables {
        let mesh = match &placement.kind {
            PlacementKind::Lever => interactables::lever_mesh(0.0),
            PlacementKind::ToggleButton | PlacementKind::MomentaryButton => {
                interactables::button_mesh(false)
            }
            PlacementKind::Switch { num_positions } => interactables::switch_mesh(0, *num_positions),
            PlacementKind::Screen { width, height } => {
                interactables::screen_mesh(*width, *height)
            }
            PlacementKind::HelmSeat => interactables::helm_seat_mesh(),
        };
        interactable_meshes.push((mesh, placement.position));
    }

    ShipMeshes {
        hull,
        interactable_meshes,
    }
}
