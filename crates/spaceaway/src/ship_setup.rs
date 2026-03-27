//! Ship scene setup: creates the ship body, meshes, and interactables.

use glam::Vec3;
use rapier3d::prelude::*;
use sa_meshgen::interactables;
use sa_physics::PhysicsWorld;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_ship::station::{cockpit_layout, PlacementKind};

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

    let mut interaction = InteractionSystem::new();
    let layout = cockpit_layout();

    let mut throttle_lever = 0;
    let mut engine_button = 0;
    let mut speed_screen = 0;
    let mut helm_seat = 0;

    for placement in &layout.interactables {
        // Create a sensor collider for raycast detection.
        // Sensors don't generate contact forces --- they only detect raycasts.
        let half = placement.collider_half_extents;
        let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
            .translation(nalgebra::Vector3::new(
                placement.position.x,
                placement.position.y,
                placement.position.z,
            ))
            .sensor(true)
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
