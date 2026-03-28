use glam::Vec3;
use rapier3d::control::{
    CharacterAutostep, CharacterLength, KinematicCharacterController,
};
use rapier3d::prelude::*;
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use winit::keyboard::KeyCode;

const PLAYER_RADIUS: f32 = 0.2;
const PLAYER_HALF_HEIGHT: f32 = 0.7;
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
                max_height: CharacterLength::Absolute(0.15), // small steps only
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
    /// Collision runs in ORIGIN-CENTERED, SHIP-ROTATED space. Interior colliders
    /// sit at `(origin, ship_rot)`, so their world positions = `ship_rot * local`.
    /// The player (after carry) is at `ship_pos + ship_rot * local`, so
    /// `player - ship_pos = ship_rot * local` — the same rotated space.
    ///
    /// We subtract ship_position (NO rotation undo), sweep against rotated
    /// colliders, then add ship_position back. Walk direction and gravity are
    /// in this same rotated frame (world-space vectors).
    ///
    /// The `char_controller.up` is set to `ship_rot * Y` each frame so
    /// snap-to-ground and slope detection align with the ship's floor.
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

        // Ship's "up" direction in world space (rotated Y axis).
        // Used for gravity, jump, and character controller slope detection.
        let ship_up = ship_rotation * nalgebra::Vector3::new(0.0, 1.0, 0.0);

        // Compute yaw-offset from ship heading for walk direction.
        // Player.yaw is world-space. Ship heading = atan2 of ship's forward.
        let ship_fwd = ship_rotation * nalgebra::Vector3::new(0.0, 0.0, -1.0);
        let ship_yaw = ship_fwd.x.atan2(-ship_fwd.z);
        let yaw_offset = self.yaw - ship_yaw;

        // Walk direction: rotate by yaw_offset in the ship's floor plane,
        // then apply ship rotation. This keeps WASD relative to where the
        // player is looking, projected onto the ship's floor.
        let local_fwd = nalgebra::Vector3::new(yaw_offset.sin(), 0.0, -yaw_offset.cos());
        let local_right = nalgebra::Vector3::new(yaw_offset.cos(), 0.0, yaw_offset.sin());
        let mut move_local = nalgebra::Vector3::zeros();
        if input.keyboard.is_pressed(KeyCode::KeyW) {
            move_local += local_fwd;
        }
        if input.keyboard.is_pressed(KeyCode::KeyS) {
            move_local -= local_fwd;
        }
        if input.keyboard.is_pressed(KeyCode::KeyD) {
            move_local += local_right;
        }
        if input.keyboard.is_pressed(KeyCode::KeyA) {
            move_local -= local_right;
        }
        if move_local.norm_squared() > 0.0 {
            move_local = move_local.normalize();
        }
        // Rotate walk direction into world/collision space
        let walk_dir = ship_rotation * move_local;

        // Rising-edge jump detection: only jump on the frame Space is first pressed
        let space_pressed = input.keyboard.is_pressed(KeyCode::Space);
        let jump_requested = space_pressed && !self.prev_space_pressed;
        self.prev_space_pressed = space_pressed;

        // Vertical velocity: gravity when airborne, jump impulse, reset when grounded
        if self.grounded {
            self.vertical_velocity = 0.0;
            if jump_requested {
                self.vertical_velocity = JUMP_SPEED;
            }
        } else {
            self.vertical_velocity -= GRAVITY * dt;
        }

        // ORIGIN-CENTERED COLLISION
        //
        // Interior colliders are on a fixed body at (origin, ship_rot).
        // Collider world positions = ship_rot * collider_local.
        // Player at origin = player_world - ship_position = ship_rot * player_local.
        // Both share the same rotated coordinate system → move_shape works.
        //
        // Walk vector and gravity are world-space vectors (already rotated).
        // char_controller.up is set to ship_up so slopes/snap work correctly.

        // Set character controller "up" to match ship orientation
        self.char_controller.up =
            nalgebra::UnitVector3::new_normalize(nalgebra::Vector3::new(
                ship_up.x, ship_up.y, ship_up.z,
            ));

        // Player's current WORLD position
        let player_world = physics
            .get_body(self.body_handle)
            .map(|b| b.translation().clone_owned())
            .unwrap_or(nalgebra::Vector3::zeros());

        // Translate to origin (NO rotation undo — keep in rotated space)
        let player_at_origin = player_world - ship_position;

        // Walk translation: horizontal walk + vertical (gravity/jump) along ship_up
        let walk_translation =
            walk_dir * (MOVE_SPEED * dt) + ship_up * (self.vertical_velocity * dt);

        // Sweep in origin-centered rotated space
        let sweep_isometry = Isometry::new(player_at_origin, nalgebra::Vector3::zeros());

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
            &sweep_isometry,
            walk_translation,
            filter,
            |_collision| {},
        );

        // Transform result back to world space (just add ship position)
        let new_world = ship_position + player_at_origin + output.translation;

        if let Some(body) = physics.get_body_mut(self.body_handle) {
            body.set_translation(new_world, true);
        }

        // Update grounded state. Use snap_to_ground result but also keep
        // grounded=true when the vertical velocity is near zero (prevents
        // oscillation from sub-millimeter ground separation).
        let was_grounded = self.grounded;
        self.grounded = output.grounded || (was_grounded && self.vertical_velocity.abs() < 0.5);

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
    /// Uses world Y as up — only correct when ship has no roll/pitch.
    pub fn position(&self, physics: &PhysicsWorld) -> WorldPos {
        if let Some(body) = physics.get_body(self.body_handle) {
            let t = body.translation();
            WorldPos::new(t.x as f64, (t.y + EYE_HEIGHT) as f64, t.z as f64)
        } else {
            WorldPos::ORIGIN
        }
    }

    /// Returns the player eye position offset along the ship's up direction.
    /// Correct after any ship roll/pitch/yaw.
    pub fn position_ship_up(
        &self,
        physics: &PhysicsWorld,
        ship_rotation: nalgebra::UnitQuaternion<f32>,
    ) -> WorldPos {
        if let Some(body) = physics.get_body(self.body_handle) {
            let t = body.translation();
            let up = ship_rotation * nalgebra::Vector3::new(0.0, EYE_HEIGHT, 0.0);
            WorldPos::new(
                (t.x + up.x) as f64,
                (t.y + up.y) as f64,
                (t.z + up.z) as f64,
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
