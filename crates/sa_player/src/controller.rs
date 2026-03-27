use glam::Vec3;
use rapier3d::prelude::*;
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use winit::keyboard::KeyCode;

const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HALF_HEIGHT: f32 = 0.6;
const MOVE_SPEED: f32 = 5.0;
const JUMP_IMPULSE: f32 = 5.0;
const MOUSE_SENSITIVITY: f32 = 0.003;

pub struct PlayerController {
    pub body_handle: RigidBodyHandle,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    prev_space_pressed: bool,
}

impl PlayerController {
    /// Spawns a player capsule rigid body at the given position.
    /// The body has locked rotations and linear damping for responsive movement.
    pub fn spawn(physics: &mut PhysicsWorld, x: f32, y: f32, z: f32) -> Self {
        let body = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector3::new(x, y, z))
            .lock_rotations()
            .linear_damping(0.0)
            .ccd_enabled(true)
            .can_sleep(false) // Player must never sleep — always responsive to input
            .build();
        let body_handle = physics.add_rigid_body(body);

        let collider = ColliderBuilder::capsule_y(PLAYER_HALF_HEIGHT, PLAYER_RADIUS)
            .friction(0.0)
            .restitution(0.0)
            .mass(80.0) // 80 kg player — must match the mag-boot gravity force (785 N ≈ 80kg × 9.81)
            .build();
        physics.add_collider(collider, body_handle);

        Self {
            body_handle,
            yaw: 0.0,
            pitch: 0.0,
            grounded: true,
            prev_space_pressed: false,
        }
    }

    /// Updates the player based on input: mouse look, WASD movement, and jumping.
    pub fn update(&mut self, physics: &mut PhysicsWorld, input: &InputState, _dt: f32) {
        // Mouse look
        let (dx, dy) = input.mouse.delta();
        self.yaw += dx * MOUSE_SENSITIVITY;
        self.pitch -= dy * MOUSE_SENSITIVITY;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);

        // Movement direction from WASD (horizontal plane only)
        let forward_dir = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos()).normalize_or_zero();
        let right_dir = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize_or_zero();

        let mut move_dir = Vec3::ZERO;
        if input.keyboard.is_pressed(KeyCode::KeyW) {
            move_dir += forward_dir;
        }
        if input.keyboard.is_pressed(KeyCode::KeyS) {
            move_dir -= forward_dir;
        }
        if input.keyboard.is_pressed(KeyCode::KeyD) {
            move_dir += right_dir;
        }
        if input.keyboard.is_pressed(KeyCode::KeyA) {
            move_dir -= right_dir;
        }
        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        let target_vel = move_dir * MOVE_SPEED;

        // Preserve vertical velocity, set horizontal directly
        if let Some(body) = physics.get_body_mut(self.body_handle) {
            let current_vel = *body.linvel();
            let new_vel =
                nalgebra::Vector3::new(target_vel.x, current_vel.y, target_vel.z);
            body.set_linvel(new_vel, true);

            // Grounded check with wider margin to catch landing frame
            self.grounded = current_vel.y.abs() < 0.5;
        }

        // Rising-edge jump detection: only jump on the frame Space is first pressed
        let space_pressed = input.keyboard.is_pressed(KeyCode::Space);
        let jump_requested = space_pressed && !self.prev_space_pressed;
        self.prev_space_pressed = space_pressed;

        if jump_requested
            && self.grounded
            && let Some(body) = physics.get_body_mut(self.body_handle)
        {
            body.apply_impulse(nalgebra::Vector3::new(0.0, JUMP_IMPULSE, 0.0), true);
        }
    }

    /// Returns the camera-style forward vector from yaw and pitch.
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize()
    }

    /// Returns the player eye position (body translation offset up by capsule height).
    pub fn position(&self, physics: &PhysicsWorld) -> WorldPos {
        if let Some(body) = physics.get_body(self.body_handle) {
            let t = body.translation();
            WorldPos::new(
                t.x as f64,
                (t.y + PLAYER_HALF_HEIGHT + PLAYER_RADIUS) as f64,
                t.z as f64,
            )
        } else {
            WorldPos::ORIGIN
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_creates_body() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        assert!(
            physics.get_body(player.body_handle).is_some(),
            "player body should exist in physics world"
        );
    }

    #[test]
    fn player_does_not_tip_over() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        let body = physics.get_body(player.body_handle).unwrap();
        // lock_rotations() locks all rotation axes
        let locked = body.is_rotation_locked();
        assert!(
            locked == [true, true, true],
            "all rotations should be locked, got {locked:?}"
        );
    }

    #[test]
    fn initial_state() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        assert!(
            (player.yaw - 0.0).abs() < f32::EPSILON,
            "initial yaw should be 0"
        );
        assert!(
            (player.pitch - 0.0).abs() < f32::EPSILON,
            "initial pitch should be 0"
        );
        assert!(player.grounded, "initial grounded should be true");
    }
}
