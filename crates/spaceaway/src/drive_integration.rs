//! Maps DriveController state to galactic_position deltas.

use sa_ship::drive::DriveController;

/// Compute the galactic position delta for this frame.
///
/// `direction`: normalized travel direction (ship forward in world space).
/// `dt`: frame delta time in seconds.
///
/// Returns the delta to add to `galactic_position` in light-years.
pub fn galactic_position_delta(
    drive: &DriveController,
    direction: [f64; 3],
    dt: f64,
) -> [f64; 3] {
    let speed_ly_s = drive.current_speed_ly_s();
    if speed_ly_s < 1e-20 {
        return [0.0, 0.0, 0.0];
    }
    // Normalize direction to guard against floating-point drift
    let len = (direction[0] * direction[0]
        + direction[1] * direction[1]
        + direction[2] * direction[2])
    .sqrt();
    if len < 1e-10 {
        return [0.0, 0.0, 0.0];
    }
    let d = [direction[0] / len, direction[1] / len, direction[2] / len];
    [
        d[0] * speed_ly_s * dt,
        d[1] * speed_ly_s * dt,
        d[2] * speed_ly_s * dt,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sa_ship::DriveMode;

    #[test]
    fn impulse_delta_is_zero() {
        let dc = DriveController::new();
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0 / 60.0);
        assert!(delta[0].abs() < 1e-20);
        assert!(delta[1].abs() < 1e-20);
        assert!(delta[2].abs() < 1e-20);
    }

    #[test]
    fn cruise_delta_moves_forward() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.set_speed_fraction(1.0); // 500c
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        assert!(delta[2] < -1e-6, "should move in -Z, got {}", delta[2]);
        assert!((delta[2].abs() - 1.585e-5).abs() < 1e-7, "delta={}", delta[2]);
    }

    #[test]
    fn warp_delta_is_large() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        for _ in 0..300 { dc.update(1.0 / 60.0); }
        dc.set_speed_fraction(1.0);
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        assert!((delta[2].abs() - 0.1585).abs() < 0.001, "delta={}", delta[2]);
    }

    #[test]
    fn spooling_warp_has_zero_delta() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        dc.set_speed_fraction(1.0);
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        assert!(delta[2].abs() < 1e-20);
    }
}
