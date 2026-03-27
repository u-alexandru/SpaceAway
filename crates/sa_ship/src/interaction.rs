//! Raycast-based interaction system for ship interactables.
//!
//! Each frame: cast a ray from camera forward, detect hover, handle
//! click/drag for levers, click for buttons/switches, click for helm seat.

use crate::interactable::Interactable;
use rapier3d::prelude::*;
use std::collections::HashMap;

/// Unique identifier for an interactable (index into the interactables vec).
pub type InteractableId = usize;

/// The drag state machine for lever interaction.
#[derive(Clone, Debug)]
enum DragState {
    /// Not dragging anything.
    Idle,
    /// Currently dragging a lever. Stores the lever ID and its position
    /// at the start of the drag.
    Dragging {
        target: InteractableId,
        #[allow(dead_code)]
        start_position: f32,
    },
}

/// The interaction system manages raycasting and interactable state.
pub struct InteractionSystem {
    /// All registered interactables, keyed by ID.
    interactables: Vec<Interactable>,
    /// Map from collider handle to interactable ID for fast lookup after raycast.
    collider_to_id: HashMap<ColliderHandle, InteractableId>,
    /// Currently hovered interactable (if any).
    hovered: Option<InteractableId>,
    /// Current drag state.
    drag: DragState,
    /// Maximum raycast distance in meters.
    max_range: f32,
    /// Mouse Y sensitivity for lever dragging (position change per pixel).
    lever_sensitivity: f32,
}

impl InteractionSystem {
    pub fn new() -> Self {
        Self {
            interactables: Vec::new(),
            collider_to_id: HashMap::new(),
            hovered: None,
            drag: DragState::Idle,
            max_range: 1.5,
            lever_sensitivity: 0.003,
        }
    }

    /// Register an interactable and return its ID.
    pub fn register(&mut self, interactable: Interactable) -> InteractableId {
        let id = self.interactables.len();
        self.collider_to_id
            .insert(interactable.collider_handle, id);
        self.interactables.push(interactable);
        id
    }

    /// Get a reference to an interactable by ID.
    pub fn get(&self, id: InteractableId) -> Option<&Interactable> {
        self.interactables.get(id)
    }

    /// Get a mutable reference to an interactable by ID.
    pub fn get_mut(&mut self, id: InteractableId) -> Option<&mut Interactable> {
        self.interactables.get_mut(id)
    }

    /// Currently hovered interactable ID.
    pub fn hovered(&self) -> Option<InteractableId> {
        self.hovered
    }

    /// Returns true if currently dragging a lever.
    pub fn is_dragging(&self) -> bool {
        matches!(self.drag, DragState::Dragging { .. })
    }

    /// Update the interaction system. Call once per frame.
    ///
    /// - `ray_origin`: camera eye position (world space)
    /// - `ray_dir`: camera forward direction (normalized)
    /// - `mouse_dy`: mouse Y delta this frame (pixels)
    /// - `left_just_pressed`: true on the frame left mouse button was pressed
    /// - `left_pressed`: true while left mouse button is held
    /// - `left_just_released`: true on the frame left mouse button was released
    /// - `physics`: physics world (for raycasting)
    ///
    /// Returns `Some(InteractableId)` if a helm seat was clicked (caller
    /// should enter seated mode).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        ray_origin: [f32; 3],
        ray_dir: [f32; 3],
        mouse_dy: f32,
        left_just_pressed: bool,
        left_pressed: bool,
        left_just_released: bool,
        physics: &sa_physics::PhysicsWorld,
    ) -> Option<InteractableId> {
        let mut helm_seat_clicked = None;

        // --- Raycast for hover ---
        let origin = nalgebra::Point3::new(ray_origin[0], ray_origin[1], ray_origin[2]);
        let direction = nalgebra::Vector3::new(ray_dir[0], ray_dir[1], ray_dir[2]);

        // Only hit sensors (interactable colliders are sensors)
        let filter = QueryFilter::default().predicate(&|_handle, collider: &Collider| {
            collider.is_sensor()
        });

        let hit = physics.cast_ray(origin, direction, self.max_range, true, filter);

        self.hovered = hit.and_then(|(collider_handle, _toi)| {
            self.collider_to_id.get(&collider_handle).copied()
        });

        // --- Handle drag state machine ---
        match &self.drag {
            DragState::Idle => {
                if left_just_pressed
                    && let Some(id) = self.hovered
                {
                    let interactable = &self.interactables[id];
                    match &interactable.kind {
                        crate::interactable::InteractableKind::Lever { position } => {
                            self.drag = DragState::Dragging {
                                target: id,
                                start_position: *position,
                            };
                        }
                        crate::interactable::InteractableKind::Button { .. } => {
                            self.interactables[id].press_button();
                        }
                        crate::interactable::InteractableKind::Switch { .. } => {
                            self.interactables[id].cycle_switch();
                        }
                        crate::interactable::InteractableKind::HelmSeat => {
                            helm_seat_clicked = Some(id);
                        }
                        crate::interactable::InteractableKind::Screen { .. } => {
                            // Screens are not interactive
                        }
                    }
                }
            }
            DragState::Dragging { target, .. } => {
                let target_id = *target;
                if left_pressed {
                    // Update lever position from mouse Y delta.
                    // Negative dy = mouse moves up = lever goes up = position increases.
                    if let Some(interactable) = self.interactables.get_mut(target_id) {
                        let current = interactable.lever_position().unwrap_or(0.0);
                        let new_pos = current - mouse_dy * self.lever_sensitivity;
                        interactable.set_lever_position(new_pos);
                    }
                }
                if left_just_released || !left_pressed {
                    // Release: momentary buttons depress, lever stays
                    self.drag = DragState::Idle;
                }
            }
        }

        // Release momentary buttons when mouse is released (even if not dragging)
        if left_just_released {
            for interactable in &mut self.interactables {
                interactable.release_button();
            }
        }

        helm_seat_clicked
    }
}

impl Default for InteractionSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactable::Interactable;

    fn dummy_handle(idx: u32) -> ColliderHandle {
        ColliderHandle::from_raw_parts(idx, 0)
    }

    #[test]
    fn register_and_get() {
        let mut sys = InteractionSystem::new();
        let lever = Interactable::lever(dummy_handle(0), "throttle");
        let id = sys.register(lever);
        assert!(sys.get(id).is_some());
        assert_eq!(sys.get(id).unwrap().label, "throttle");
    }

    #[test]
    fn drag_delta_maps_to_lever_position() {
        let mut sys = InteractionSystem::new();
        let lever = Interactable::lever(dummy_handle(0), "throttle");
        let id = sys.register(lever);

        // Simulate: start drag, then drag mouse up (negative dy)
        sys.hovered = Some(id);
        sys.drag = DragState::Dragging {
            target: id,
            start_position: 0.0,
        };

        // Drag mouse up by 100 pixels -> position increases by 100 * 0.003 = 0.3
        let physics = sa_physics::PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        sys.update(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            -100.0, // mouse moved up
            false,   // not just pressed
            true,    // held
            false,   // not just released
            &physics,
        );

        let pos = sys.get(id).unwrap().lever_position().unwrap();
        assert!(
            (pos - 0.3).abs() < 0.01,
            "lever should be at ~0.3 after dragging up 100px, got {pos}"
        );
    }

    #[test]
    fn lever_position_clamped_during_drag() {
        let mut sys = InteractionSystem::new();
        let mut lever = Interactable::lever(dummy_handle(0), "throttle");
        lever.set_lever_position(0.95);
        let id = sys.register(lever);

        sys.drag = DragState::Dragging {
            target: id,
            start_position: 0.95,
        };

        let physics = sa_physics::PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        // Drag way up -> should clamp to 1.0
        sys.update(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            -1000.0,
            false,
            true,
            false,
            &physics,
        );

        let pos = sys.get(id).unwrap().lever_position().unwrap();
        assert!(
            (pos - 1.0).abs() < f32::EPSILON,
            "lever should clamp to 1.0, got {pos}"
        );
    }
}
