//! Helm seated mode: when the player sits at the helm, mouse controls
//! ship rotation, WASD controls thrust/RCS, and the camera locks to the
//! helm viewpoint.

use crate::ship::Ship;
use glam::Vec3;
use sa_input::InputState;
use sa_physics::PhysicsWorld;
use winit::keyboard::KeyCode;

/// Whether the player is standing or seated at the helm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelmState {
    Standing,
    Seated,
}

/// Mouse sensitivity for ship rotation while seated.
const HELM_MOUSE_SENSITIVITY: f32 = 0.0002;

/// Controls the helm seated mode.
pub struct HelmController {
    pub state: HelmState,
    /// Helm viewpoint offset from ship origin (local space).
    /// This is where the camera goes when seated.
    pub viewpoint_offset: Vec3,
}

impl HelmController {
    /// Create a new helm controller.
    /// `viewpoint_offset` is the camera position relative to the ship body
    /// origin when seated (e.g., cockpit position).
    pub fn new(viewpoint_offset: Vec3) -> Self {
        Self {
            state: HelmState::Standing,
            viewpoint_offset,
        }
    }

    /// Enter seated mode.
    pub fn sit_down(&mut self) {
        self.state = HelmState::Seated;
    }

    /// Exit seated mode.
    pub fn stand_up(&mut self) {
        self.state = HelmState::Standing;
    }

    /// Returns true if seated at the helm.
    pub fn is_seated(&self) -> bool {
        self.state == HelmState::Seated
    }

    /// Process input while seated and apply forces to the ship.
    /// Returns `true` if the player wants to stand up (left click).
    ///
    /// Input mapping while seated:
    /// - Mouse X -> yaw
    /// - Mouse Y -> pitch
    /// - W/S -> forward/backward RCS
    /// - A/D -> lateral RCS
    /// - Space -> thrust up
    /// - ShiftLeft -> thrust down
    /// - Q/E -> roll
    pub fn update_seated(
        &self,
        ship: &Ship,
        physics: &mut PhysicsWorld,
        input: &InputState,
        _dt: f32,
    ) -> bool {
        if self.state != HelmState::Seated {
            return false;
        }

        // Mouse -> rotation
        let (dx, dy) = input.mouse.delta();
        let yaw = -dx * HELM_MOUSE_SENSITIVITY;
        let pitch = dy * HELM_MOUSE_SENSITIVITY;

        let mut roll = 0.0;
        if input.keyboard.is_pressed(KeyCode::KeyQ) {
            roll -= 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyE) {
            roll += 1.0;
        }

        ship.apply_rotation(physics, pitch, yaw, roll);

        // WASD + Space/Shift -> RCS
        let mut longitudinal = 0.0_f32;
        let mut lateral = 0.0_f32;
        let mut vertical = 0.0_f32;

        if input.keyboard.is_pressed(KeyCode::KeyW) {
            longitudinal += 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyS) {
            longitudinal -= 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyA) {
            lateral -= 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyD) {
            lateral += 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::Space) {
            vertical += 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::ShiftLeft) {
            vertical -= 1.0;
        }

        ship.apply_rcs(physics, lateral, vertical, longitudinal);

        // Apply main engine thrust (throttle lever controls magnitude)
        ship.apply_thrust(physics);

        // Stand up on left click
        input.mouse.left_just_pressed()
    }

    /// Compute the camera world position when seated.
    /// Uses the ship body's position and rotation to transform the viewpoint offset.
    pub fn camera_position(&self, physics: &PhysicsWorld, ship: &Ship) -> Option<(f32, f32, f32)> {
        physics.get_body(ship.body_handle).map(|body| {
            let ship_pos = body.translation();
            let ship_rot = body.rotation();
            let offset = nalgebra::Vector3::new(
                self.viewpoint_offset.x,
                self.viewpoint_offset.y,
                self.viewpoint_offset.z,
            );
            let world_offset = ship_rot * offset;
            (
                ship_pos.x + world_offset.x,
                ship_pos.y + world_offset.y,
                ship_pos.z + world_offset.z,
            )
        })
    }

    /// Get the ship's forward direction for camera orientation when seated.
    /// Returns (yaw, pitch) matching the ship's current orientation.
    pub fn camera_orientation(
        &self,
        physics: &PhysicsWorld,
        ship: &Ship,
    ) -> Option<(f32, f32)> {
        physics.get_body(ship.body_handle).map(|body| {
            let rot = body.rotation();
            // Ship forward is -Z in local space
            let forward = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
            let yaw = forward.x.atan2(-forward.z);
            let pitch = forward.y.asin();
            (yaw, pitch)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_standing() {
        let helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));
        assert_eq!(helm.state, HelmState::Standing);
        assert!(!helm.is_seated());
    }

    #[test]
    fn sit_down_and_stand_up() {
        let mut helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));
        helm.sit_down();
        assert!(helm.is_seated());
        helm.stand_up();
        assert!(!helm.is_seated());
    }

    #[test]
    fn camera_position_when_seated() {
        let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        let helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));

        let (cx, cy, cz) = helm.camera_position(&physics, &ship).unwrap();
        // Ship at origin, viewpoint offset (0, 0.8, 1.0), no rotation
        assert!((cx - 0.0).abs() < 0.1, "cx = {cx}");
        assert!((cy - 0.8).abs() < 0.1, "cy = {cy}");
        assert!((cz - 1.0).abs() < 0.1, "cz = {cz}");
    }

    #[test]
    fn update_seated_returns_false_when_standing() {
        let helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));
        let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        let input = InputState::new();
        let wants_stand = helm.update_seated(&ship, &mut physics, &input, 1.0 / 60.0);
        assert!(!wants_stand);
    }
}
