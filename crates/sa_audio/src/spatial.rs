//! 3D audio positioning: listener, emitters, distance attenuation.

use glam::Vec3;

/// Listener state (camera position + orientation).
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
        }
    }
}

/// Compute volume and pan for a sound at `source_pos` relative to the listener.
/// Returns (volume_multiplier, pan) where pan is -1.0 (left) to 1.0 (right).
pub fn spatial_params(listener: &Listener, source_pos: Vec3, max_range: f32) -> (f32, f32) {
    let to_source = source_pos - listener.position;
    let distance = to_source.length();

    if distance < 0.001 {
        return (1.0, 0.0); // at listener position
    }

    // Distance attenuation: linear falloff
    let volume = (1.0 - distance / max_range).clamp(0.0, 1.0);

    // Stereo pan: project onto listener's right vector
    let right = listener.forward.cross(listener.up).normalize();
    let dir = to_source / distance;
    let pan = dir.dot(right).clamp(-1.0, 1.0);

    (volume, pan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_listener_full_volume_center_pan() {
        let listener = Listener::default();
        let (vol, pan) = spatial_params(&listener, Vec3::ZERO, 30.0);
        assert!((vol - 1.0).abs() < 0.01);
        assert!(pan.abs() < 0.01);
    }

    #[test]
    fn beyond_range_silent() {
        let listener = Listener::default();
        let (vol, _) = spatial_params(&listener, Vec3::new(0.0, 0.0, -50.0), 30.0);
        assert!(vol < 0.01);
    }

    #[test]
    fn right_side_positive_pan() {
        let listener = Listener::default();
        let (_, pan) = spatial_params(&listener, Vec3::new(10.0, 0.0, 0.0), 30.0);
        assert!(pan > 0.3, "right-side sound should have positive pan, got {pan}");
    }

    #[test]
    fn left_side_negative_pan() {
        let listener = Listener::default();
        let (_, pan) = spatial_params(&listener, Vec3::new(-10.0, 0.0, 0.0), 30.0);
        assert!(pan < -0.3, "left-side sound should have negative pan, got {pan}");
    }
}
