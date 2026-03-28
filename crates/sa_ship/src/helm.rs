//! Helm seated mode: when the player sits at the helm, WASD controls
//! ship rotation (pitch/yaw), Q controls roll, and the camera can
//! free-look independently. E exits the seat.

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

/// Rotation rate for keyboard-driven ship rotation (normalized input * this).
const HELM_ROTATION_RATE: f32 = 1.0;

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

    /// Process input while seated and apply rotation to the ship.
    /// Returns `true` if the player wants to stand up (E key).
    ///
    /// Input mapping while seated:
    /// - W/S -> pitch (nose up/down)
    /// - A/D -> yaw (turn left/right)
    /// - Q/E -> roll (left/right)
    /// - F -> stand up
    ///
    /// Mouse does NOT control the ship; it controls the camera (handled
    /// in the game loop). Thrust comes from the throttle lever and engine
    /// button, applied every frame in the game loop regardless of seating.
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

        // WASD -> ship rotation
        let mut pitch = 0.0_f32;
        let mut yaw = 0.0_f32;
        let mut roll = 0.0_f32;

        if input.keyboard.is_pressed(KeyCode::KeyW) {
            pitch += HELM_ROTATION_RATE;
        }
        if input.keyboard.is_pressed(KeyCode::KeyS) {
            pitch -= HELM_ROTATION_RATE;
        }
        if input.keyboard.is_pressed(KeyCode::KeyA) {
            yaw -= HELM_ROTATION_RATE;
        }
        if input.keyboard.is_pressed(KeyCode::KeyD) {
            yaw += HELM_ROTATION_RATE;
        }
        if input.keyboard.is_pressed(KeyCode::KeyQ) {
            roll -= HELM_ROTATION_RATE;
        }
        if input.keyboard.is_pressed(KeyCode::KeyE) {
            roll += HELM_ROTATION_RATE;
        }

        ship.apply_rotation(physics, pitch, yaw, roll);

        // F key = stand up (exit seat)
        input.keyboard.just_pressed(KeyCode::KeyF)
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
