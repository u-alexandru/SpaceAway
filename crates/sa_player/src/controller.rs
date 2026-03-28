use glam::Vec3;
use rapier3d::control::{
    CharacterAutostep, CharacterLength, KinematicCharacterController,
};
use rapier3d::prelude::*;
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use winit::keyboard::KeyCode;

const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HALF_HEIGHT: f32 = 0.6;
const MOVE_SPEED: f32 = 5.0;
const JUMP_SPEED: f32 = 5.0;
const MOUSE_SENSITIVITY: f32 = 0.003;
const GRAVITY: f32 = 9.81;

/// Eye height above the rigid body origin (top of capsule).
const EYE_HEIGHT: f32 = PLAYER_HALF_HEIGHT + PLAYER_RADIUS;

pub struct PlayerController {
    pub body_handle: RigidBodyHandle,
    pub collider_handle: ColliderHandle,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    prev_space_pressed: bool,
    char_controller: KinematicCharacterController,
    /// Vertical velocity tracked manually (kinematic bodies have no physics velocity).
    vertical_velocity: f32,
    /// The capsule shape used for sweep tests in `move_shape`.
    character_shape: SharedShape,
}

impl PlayerController {
    /// Spawns a player capsule rigid body at the given position.
    /// Uses a kinematic-position-based body so the player exerts zero reaction
    /// forces on the environment (no more 785 N gravity counterforce on the ship).
    pub fn spawn(physics: &mut PhysicsWorld, x: f32, y: f32, z: f32) -> Self {
        let body = RigidBodyBuilder::kinematic_position_based()
            .translation(nalgebra::Vector3::new(x, y, z))
            .build();
        let body_handle = physics.add_rigid_body(body);

        let collider = ColliderBuilder::capsule_y(PLAYER_HALF_HEIGHT, PLAYER_RADIUS)
            .friction(0.0)
            .restitution(0.0)
            .collision_groups(InteractionGroups::new(
                Group::GROUP_3,  // PLAYER group
                Group::GROUP_2,  // collide with SHIP_INTERIOR only
            ))
            .build();
        let collider_handle = physics.add_collider(collider, body_handle);

        let char_controller = KinematicCharacterController {
            up: nalgebra::UnitVector3::new_normalize(nalgebra::Vector3::y()),
            offset: CharacterLength::Absolute(0.05), // larger offset reduces ground oscillation
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.3),
                min_width: CharacterLength::Absolute(0.2),
                include_dynamic_bodies: true,
            }),
            snap_to_ground: Some(CharacterLength::Absolute(0.5)), // generous snap prevents ground oscillation
            max_slope_climb_angle: 50_f32.to_radians(),
            min_slope_slide_angle: 30_f32.to_radians(),
            ..Default::default()
        };

        let character_shape = SharedShape::capsule_y(PLAYER_HALF_HEIGHT, PLAYER_RADIUS);

        Self {
            body_handle,
            collider_handle,
            yaw: 0.0,
            pitch: 0.0,
            grounded: true,
            prev_space_pressed: false,
            char_controller,
            vertical_velocity: 0.0,
            character_shape,
        }
    }

    /// Updates the player based on input: mouse look, WASD movement, and jumping.
    ///
    /// `ship_position`: the ship body's world position (translation vector).
    /// `ship_rotation`: the ship body's world rotation (unit quaternion).
    ///
    /// Collision detection runs in SHIP-LOCAL space where all colliders are
    /// stationary. This means:
    /// - Zero AABB updates regardless of ship speed
    /// - Query pipeline never needs rebuilding
    /// - move_shape sweeps only the walk distance (0-0.08m), not ship velocity
    /// - Performance is O(1) regardless of ship speed or collider count
    pub fn update(
        &mut self,
        physics: &mut PhysicsWorld,
        input: &InputState,
        dt: f32,
        ship_position: nalgebra::Vector3<f32>,
        ship_rotation: nalgebra::UnitQuaternion<f32>,
    ) {
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

        // Rising-edge jump detection: only jump on the frame Space is first pressed
        let space_pressed = input.keyboard.is_pressed(KeyCode::Space);
        let jump_requested = space_pressed && !self.prev_space_pressed;
        self.prev_space_pressed = space_pressed;

        // Vertical velocity: gravity when airborne, jump impulse, reset when grounded
        if self.grounded {
            // Snap vertical velocity to zero when on the ground.
            // Gravity is handled by snap_to_ground in the character controller.
            self.vertical_velocity = 0.0;
            if jump_requested {
                self.vertical_velocity = JUMP_SPEED;
            }
        } else {
            self.vertical_velocity -= GRAVITY * dt;
        }

        // SHIP-LOCAL COLLISION DETECTION
        //
        // Interior colliders are at world origin, ROTATED to match the ship
        // (rotation synced each frame in the game loop). We transform the player
        // to ship-local space (subtract position, undo rotation), sweep there,
        // then transform back.
        //
        // Player.yaw is WORLD space. Walk direction is world-space, then
        // transformed to ship-local for collision. Gravity in ship-local -Y
        // (always toward the ship's floor regardless of roll).
        //
        // Performance: O(1) regardless of ship speed.

        let ship_rot_inv = ship_rotation.inverse();

        // Player's current WORLD position
        let player_world = physics
            .get_body(self.body_handle)
            .map(|b| b.translation().clone_owned())
            .unwrap_or(nalgebra::Vector3::zeros());

        // Transform player to ship-local space
        let player_local = ship_rot_inv * (player_world - ship_position);

        // Walk direction in world space, transformed to ship-local for collision.
        // Gravity in ship-local Y (toward ship's floor regardless of roll).
        let walk_vel = move_dir * MOVE_SPEED;
        let walk_world = nalgebra::Vector3::new(walk_vel.x * dt, 0.0, walk_vel.z * dt);
        let mut walk_local = ship_rot_inv * walk_world;
        walk_local.y += self.vertical_velocity * dt;

        // Sweep in ship-local space (colliders are rotated to match)
        let local_isometry = Isometry::new(player_local, nalgebra::Vector3::zeros());

        let filter = QueryFilter::default()
            .exclude_rigid_body(self.body_handle)
            .groups(InteractionGroups::new(
                Group::GROUP_3,
                Group::GROUP_2,
            ));

        let output = self.char_controller.move_shape(
            dt,
            &physics.rigid_body_set,
            &physics.collider_set,
            &physics.query_pipeline,
            self.character_shape.as_ref(),
            &local_isometry,
            walk_local,
            filter,
            |_collision| {},
        );

        // Transform result back to world space
        let new_local = player_local + output.translation;
        let new_world = ship_position + ship_rotation * new_local;

        if let Some(body) = physics.get_body_mut(self.body_handle) {
            body.set_translation(new_world, true);
        }

        // Update grounded state. Use snap_to_ground result but also keep
        // grounded=true when the vertical velocity is near zero (prevents
        // oscillation from sub-millimeter ground separation).
        let was_grounded = self.grounded;
        self.grounded = output.grounded || (was_grounded && self.vertical_velocity.abs() < 0.5);

        // If grounded, kill vertical velocity (gravity handled by snap_to_ground)
        if self.grounded {
            self.vertical_velocity = 0.0;
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
            WorldPos::new(t.x as f64, (t.y + EYE_HEIGHT) as f64, t.z as f64)
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
    fn player_body_is_kinematic() {
        let mut physics = PhysicsWorld::new();
        let player = PlayerController::spawn(&mut physics, 0.0, 5.0, 0.0);
        let body = physics.get_body(player.body_handle).unwrap();
        assert!(
            body.is_kinematic(),
            "player body should be kinematic, not dynamic"
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
