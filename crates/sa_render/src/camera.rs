use glam::{Mat4, Quat, Vec3};
use sa_math::WorldPos;

pub struct Camera {
    pub position: WorldPos,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// When set, view_matrix() uses this orientation instead of yaw/pitch.
    /// Used by helm mode to track full ship rotation (including roll).
    pub orientation_override: Option<Quat>,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: WorldPos::ORIGIN,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: f32::INFINITY,
            orientation_override: None,
        }
    }

    pub fn forward(&self) -> Vec3 {
        if let Some(q) = self.orientation_override {
            // Quaternion mode: forward is -Z rotated by orientation
            q * Vec3::new(0.0, 0.0, -1.0)
        } else {
            Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                -self.yaw.cos() * self.pitch.cos(),
            )
            .normalize()
        }
    }

    pub fn right(&self) -> Vec3 {
        if let Some(q) = self.orientation_override {
            q * Vec3::new(1.0, 0.0, 0.0)
        } else {
            self.forward().cross(Vec3::Y).normalize()
        }
    }

    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch += delta_pitch;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);
    }

    pub fn move_forward(&mut self, amount: f32) {
        let fwd = self.forward();
        self.position.x += fwd.x as f64 * amount as f64;
        self.position.y += fwd.y as f64 * amount as f64;
        self.position.z += fwd.z as f64 * amount as f64;
    }

    pub fn move_right(&mut self, amount: f32) {
        let r = self.right();
        self.position.x += r.x as f64 * amount as f64;
        self.position.y += r.y as f64 * amount as f64;
        self.position.z += r.z as f64 * amount as f64;
    }

    pub fn move_up(&mut self, amount: f32) {
        self.position.y += amount as f64;
    }

    pub fn view_matrix(&self) -> Mat4 {
        if let Some(q) = self.orientation_override {
            // Helm mode: full quaternion orientation (includes roll)
            Mat4::from_quat(q.conjugate()) // conjugate = inverse for unit quaternion
        } else {
            // Normal mode: yaw/pitch only
            Mat4::look_to_rh(Vec3::ZERO, self.forward(), Vec3::Y)
        }
    }

    /// Reversed-Z infinite projection matrix.
    /// Depth 1.0 at near plane, 0.0 at infinity.
    /// Combined with Depth32Float and GreaterEqual compare, this gives
    /// sub-millimeter precision near the camera AND renders objects at
    /// billions of meters (planets at AU distances). No far clip plane.
    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        let f = 1.0 / (self.fov_y / 2.0).tan();
        glam::Mat4::from_cols(
            glam::Vec4::new(f / aspect_ratio, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, f, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, 0.0, -1.0),
            glam::Vec4::new(0.0, 0.0, self.near, 0.0),
        )
    }

    pub fn view_projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        self.projection_matrix(aspect_ratio) * self.view_matrix()
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_looks_at_negative_z() {
        let cam = Camera::new();
        let forward = cam.forward();
        assert!(forward.z < -0.9);
        assert!(forward.x.abs() < 0.01);
    }

    #[test]
    fn move_forward_changes_position() {
        let mut cam = Camera::new();
        let before = cam.position;
        cam.move_forward(1.0);
        assert!(cam.position.z < before.z);
    }

    #[test]
    fn yaw_rotates_horizontally() {
        let mut cam = Camera::new();
        cam.rotate(std::f32::consts::FRAC_PI_2, 0.0);
        let forward = cam.forward();
        assert!(forward.x.abs() > 0.9);
    }

    #[test]
    fn pitch_clamped() {
        let mut cam = Camera::new();
        cam.rotate(0.0, 100.0);
        assert!(cam.pitch.abs() < std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn view_projection_is_valid() {
        let cam = Camera::new();
        let vp = cam.view_projection_matrix(16.0 / 9.0);
        for val in vp.to_cols_array() {
            assert!(!val.is_nan());
        }
    }
}
