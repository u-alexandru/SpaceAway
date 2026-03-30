//! Maps DriveController state to galactic_position deltas and visual parameters.

use sa_ship::drive::{DriveController, DriveMode, DriveStatus};

/// Visual parameters derived from drive state. Passed to the renderer
/// as continuous floats — shaders have no knowledge of drive modes.
#[derive(Debug, Clone, Copy)]
pub struct DriveVisuals {
    /// Velocity direction (normalized, world space).
    pub velocity_dir: [f32; 3],
    /// Speed as fraction of c for relativistic aberration (0.0–0.99).
    pub beta: f32,
    /// Star streak length in pixels (0.0 = point, 300.0 = full warp).
    pub streak_factor: f32,
    /// Sky tunnel/vignette intensity (0.0–1.0).
    pub warp_intensity: f32,
    /// Additive white flash for drive transitions (0.0–1.0).
    pub flash_intensity: f32,
}

impl Default for DriveVisuals {
    fn default() -> Self {
        Self {
            velocity_dir: [0.0, 0.0, -1.0],
            beta: 0.0,
            streak_factor: 0.0,
            warp_intensity: 0.0,
            flash_intensity: 0.0,
        }
    }
}

/// Persistent state for smooth visual transitions.
pub struct DriveVisualState {
    pub visuals: DriveVisuals,
    prev_mode: DriveMode,
    flash_timer: f32,
}

impl DriveVisualState {
    pub fn new() -> Self {
        Self {
            visuals: DriveVisuals::default(),
            prev_mode: DriveMode::Impulse,
            flash_timer: 0.0,
        }
    }

    /// Update visual parameters from current drive state. Call every frame.
    pub fn update(
        &mut self,
        drive: &DriveController,
        direction: [f32; 3],
        dt: f32,
    ) {
        let mode = drive.mode();
        let speed_c = drive.current_speed_c();

        // Detect mode transitions for flash
        if mode != self.prev_mode {
            // Flash on warp engage (Spooling→Engaged handled below)
            if mode == DriveMode::Warp && matches!(drive.status(), DriveStatus::Engaged) {
                self.flash_timer = 0.3;
            }
            // Flash on warp disengage
            if self.prev_mode == DriveMode::Warp && mode == DriveMode::Impulse {
                self.flash_timer = 0.15;
            }
            self.prev_mode = mode;
        }

        // Detect spool completion (Spooling→Engaged within warp)
        if mode == DriveMode::Warp
            && matches!(drive.status(), DriveStatus::Engaged)
            && self.visuals.warp_intensity < 0.01
        {
            // Just engaged warp — big flash
            self.flash_timer = 0.3;
        }

        // Flash decay
        self.flash_timer = (self.flash_timer - dt).max(0.0);
        self.visuals.flash_intensity = self.flash_timer / 0.3;

        // Direction
        self.visuals.velocity_dir = direction;

        // Target values based on speed
        let target_beta: f32;
        let target_streak: f32;
        let target_warp: f32;

        if speed_c < 0.001 {
            // Impulse or spooling — no effects
            target_beta = 0.0;
            target_streak = 0.0;
            target_warp = 0.0;
        } else if speed_c <= 5_000.0 {
            // Cruise range (1c–5,000c)
            target_beta = 0.99;
            // Logarithmic streak: 0 at 1c, ~80 at 5,000c
            let t = (speed_c.max(1.0) as f32).log2() / (5_000.0_f32).log2();
            target_streak = t * 80.0;
            target_warp = 0.0;
        } else {
            // Warp range (100,000c–5,000,000c)
            target_beta = 0.99;
            // Streak: 150 at 100kc, 300 at 5Mc
            let t = ((speed_c / 100_000.0).max(1.0) as f32).log2()
                / (50.0_f32).log2(); // 5M/100k = 50
            target_streak = 150.0 + t.min(1.0) * 150.0;
            // Warp intensity: 0.3 at 100kc, 1.0 at 5Mc
            target_warp = 0.3 + t.min(1.0) * 0.7;
        }

        // Smooth transitions (lerp toward targets)
        let ramp_speed = 4.0 * dt; // ~0.25s to reach target
        self.visuals.beta += (target_beta - self.visuals.beta) * ramp_speed.min(1.0);
        self.visuals.streak_factor += (target_streak - self.visuals.streak_factor) * ramp_speed.min(1.0);
        self.visuals.warp_intensity += (target_warp - self.visuals.warp_intensity) * ramp_speed.min(1.0);

        // Snap small values to zero to avoid perpetual drift
        if self.visuals.beta < 0.001 { self.visuals.beta = 0.0; }
        if self.visuals.streak_factor < 0.1 { self.visuals.streak_factor = 0.0; }
        if self.visuals.warp_intensity < 0.005 { self.visuals.warp_intensity = 0.0; }
    }
}

/// Compute the galactic position delta for this frame.
///
/// `direction`: normalized travel direction (ship forward in world space).
/// `dt`: frame delta time in seconds.
///
/// Returns the delta to add to `galactic_position` in light-years.
#[allow(dead_code)]
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

/// Compute the warp deceleration multiplier based on distance to locked target.
/// Returns 1.0 (full speed) when far, ramps down near target.
/// Warp auto-disengages at WARP_DISENGAGE_LY (cruise can cover the rest).
pub fn warp_deceleration(distance_to_target_ly: f64) -> f64 {
    if distance_to_target_ly > 1.0 {
        1.0
    } else if distance_to_target_ly > 0.1 {
        0.1 + 0.9 * ((distance_to_target_ly - 0.1) / 0.9)
    } else if distance_to_target_ly > WARP_DISENGAGE_LY {
        0.05 + 0.05 * ((distance_to_target_ly - WARP_DISENGAGE_LY) / (0.1 - WARP_DISENGAGE_LY))
    } else {
        0.05
    }
}

/// Distance at which warp auto-disengages to cruise (~0.01 ly ≈ 630 AU).
/// Cruise can comfortably cover this distance.
pub const WARP_DISENGAGE_LY: f64 = 0.01;

/// Distance at which cruise auto-disengages to impulse.
/// ~100,000 km — reachable in ~3 minutes at impulse max speed (10 km/s).
/// Previous value (50 AU = 7.5 billion km) was unreachable by impulse.
/// 100,000 km / 9.461e12 km/ly ≈ 1.057e-8 ly.
pub const CRUISE_DISENGAGE_LY: f64 = 1.057e-8; // ~100,000 km in ly

/// Cruise deceleration multiplier. Ramps down smoothly as we approach a planet.
/// Designed for planet approach: full speed far out, gradual slowdown, gentle
/// arrival at ~100km altitude where cruise auto-disengages to impulse.
pub fn cruise_deceleration(distance_to_target_ly: f64) -> f64 {
    let km_in_ly: f64 = 9.461e12;
    let dist_km = distance_to_target_ly * km_in_ly;
    if dist_km > 10_000_000.0 {
        1.0 // > 10M km: full speed
    } else if dist_km > 1_000_000.0 {
        // 10M → 1M km: ramp from 100% to 20%
        0.2 + 0.8 * ((dist_km - 1_000_000.0) / 9_000_000.0)
    } else if dist_km > 100_000.0 {
        // 1M → 100K km: ramp from 20% to 5%
        0.05 + 0.15 * ((dist_km - 100_000.0) / 900_000.0)
    } else if dist_km > 1_000.0 {
        // 100K → 1K km: ramp from 5% to 0.1%
        0.001 + 0.049 * ((dist_km - 1_000.0) / 99_000.0)
    } else if dist_km > 100.0 {
        // 1K → 100 km: final approach, crawl to disengage point
        0.0005 + 0.0005 * ((dist_km - 100.0) / 900.0)
    } else {
        0.0001 // < 100 km: near-stop, about to disengage
    }
}

/// Like galactic_position_delta but with deceleration toward a target.
/// `target_distance_ly`: distance to locked target (None = no deceleration).
/// Returns both the position delta and the effective speed (ly/s).
pub fn galactic_position_delta_decel(
    drive: &DriveController,
    direction: [f64; 3],
    dt: f64,
    target_distance_ly: Option<f64>,
) -> ([f64; 3], f64) {
    let base_speed = drive.current_speed_ly_s();
    if base_speed < 1e-20 {
        return ([0.0, 0.0, 0.0], 0.0);
    }

    let decel = target_distance_ly
        .map(|d| match drive.mode() {
            DriveMode::Warp => warp_deceleration(d),
            DriveMode::Cruise => cruise_deceleration(d),
            _ => 1.0,
        })
        .unwrap_or(1.0);
    let effective_speed = base_speed * decel;

    let len = (direction[0]*direction[0]
        + direction[1]*direction[1]
        + direction[2]*direction[2]).sqrt();
    if len < 1e-10 {
        return ([0.0, 0.0, 0.0], 0.0);
    }
    let d = [direction[0]/len, direction[1]/len, direction[2]/len];

    let delta = [
        d[0] * effective_speed * dt,
        d[1] * effective_speed * dt,
        d[2] * effective_speed * dt,
    ];
    (delta, effective_speed)
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
        dc.set_speed_fraction(1.0); // 5000c
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        assert!(delta[2] < -1e-6, "should move in -Z, got {}", delta[2]);
        assert!((delta[2].abs() - 1.585e-4).abs() < 1e-6, "delta={}", delta[2]);
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

    #[test]
    fn deceleration_full_speed_far() {
        assert!((warp_deceleration(5.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn deceleration_reduced_close() {
        let d = warp_deceleration(0.5);
        assert!(d < 1.0 && d > 0.1, "at 0.5ly should be between 0.1 and 1.0, got {d}");
    }

    #[test]
    fn deceleration_very_slow_near() {
        let d = warp_deceleration(0.01);
        assert!(d < 0.15, "at 0.01ly should be very slow, got {d}");
    }
}
