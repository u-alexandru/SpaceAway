// Drive system: Impulse / Cruise / Warp tiers

pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;
pub const METERS_PER_LY: f64 = 9.461e15;
pub const LY_PER_SECOND_AT_C: f64 = 3.169e-8;
pub const CRUISE_MIN_C: f64 = 1.0;
pub const CRUISE_MAX_C: f64 = 500.0;
pub const WARP_MIN_C: f64 = 100_000.0;
pub const WARP_MAX_C: f64 = 5_000_000.0;
pub const WARP_SPOOL_TIME: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveMode {
    Impulse,
    Cruise,
    Warp,
}

#[derive(Debug, Clone, Copy)]
pub enum DriveStatus {
    Idle,
    Spooling(f32),
    Engaged,
}

impl PartialEq for DriveStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DriveStatus::Idle, DriveStatus::Idle) => true,
            (DriveStatus::Engaged, DriveStatus::Engaged) => true,
            (DriveStatus::Spooling(a), DriveStatus::Spooling(b)) => (a - b).abs() < 1e-6,
            _ => false,
        }
    }
}

pub struct DriveController {
    mode: DriveMode,
    status: DriveStatus,
    speed_fraction: f32,
}

impl DriveController {
    pub fn new() -> Self {
        Self {
            mode: DriveMode::Impulse,
            status: DriveStatus::Idle,
            speed_fraction: 0.0,
        }
    }

    pub fn mode(&self) -> DriveMode {
        self.mode
    }

    pub fn status(&self) -> DriveStatus {
        self.status
    }

    pub fn speed_fraction(&self) -> f32 {
        self.speed_fraction
    }

    pub fn set_speed_fraction(&mut self, f: f32) {
        self.speed_fraction = f.clamp(0.0, 1.0);
    }

    /// Returns true if engagement was accepted. Cruise → Engaged immediately.
    /// Warp → Spooling(0.0). Impulse → false (use request_disengage).
    ///
    /// `ship_speed_ms`: current impulse velocity in m/s. Cruise and warp
    /// require near-zero velocity (< 10 m/s) to engage.
    pub fn request_engage(&mut self, mode: DriveMode) -> bool {
        self.request_engage_with_speed(mode, 0.0)
    }

    /// Engage with ship speed check. Rejects if moving too fast in impulse.
    pub fn request_engage_with_speed(&mut self, mode: DriveMode, ship_speed_ms: f32) -> bool {
        match mode {
            DriveMode::Impulse => false,
            DriveMode::Cruise => {
                // Allow from warp (downshift) or from near-stationary impulse
                if self.mode == DriveMode::Impulse && ship_speed_ms > 10.0 {
                    return false;
                }
                self.mode = DriveMode::Cruise;
                self.status = DriveStatus::Engaged;
                true
            }
            DriveMode::Warp => {
                // Ignore if already spooling or engaged in warp
                if self.mode == DriveMode::Warp {
                    return false;
                }
                // Must be near-stationary in impulse, or already in cruise
                if self.mode == DriveMode::Impulse && ship_speed_ms > 10.0 {
                    return false;
                }
                self.mode = DriveMode::Warp;
                self.status = DriveStatus::Spooling(0.0);
                true
            }
        }
    }

    pub fn request_disengage(&mut self) {
        self.mode = DriveMode::Impulse;
        self.status = DriveStatus::Idle;
    }

    /// Advance spool progress; transitions Spooling → Engaged when complete.
    pub fn update(&mut self, dt: f32) {
        if let DriveStatus::Spooling(elapsed) = self.status {
            let new_elapsed = elapsed + dt;
            if new_elapsed >= WARP_SPOOL_TIME - 1e-4 {
                self.status = DriveStatus::Engaged;
            } else {
                self.status = DriveStatus::Spooling(new_elapsed);
            }
        }
    }

    /// Speed in multiples of c. Returns 0.0 unless the drive is Engaged.
    pub fn current_speed_c(&self) -> f64 {
        if !matches!(self.status, DriveStatus::Engaged) {
            return 0.0;
        }
        let f = self.speed_fraction as f64;
        match self.mode {
            DriveMode::Impulse => 0.0,
            DriveMode::Cruise => {
                CRUISE_MIN_C * (CRUISE_MAX_C / CRUISE_MIN_C).powf(f)
            }
            DriveMode::Warp => {
                WARP_MIN_C * (WARP_MAX_C / WARP_MIN_C).powf(f)
            }
        }
    }

    /// Speed in light-years per second.
    pub fn current_speed_ly_s(&self) -> f64 {
        self.current_speed_c() * LY_PER_SECOND_AT_C
    }
}

impl Default for DriveController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_mode_default_is_impulse() {
        let dc = DriveController::new();
        assert_eq!(dc.mode(), DriveMode::Impulse);
        assert_eq!(dc.status(), DriveStatus::Idle);
    }

    #[test]
    fn engage_cruise_from_impulse() {
        let mut dc = DriveController::new();
        let ok = dc.request_engage(DriveMode::Cruise);
        assert!(ok);
        assert_eq!(dc.mode(), DriveMode::Cruise);
        assert_eq!(dc.status(), DriveStatus::Engaged);
    }

    #[test]
    fn engage_warp_starts_spooling() {
        let mut dc = DriveController::new();
        let ok = dc.request_engage(DriveMode::Warp);
        assert!(ok);
        assert_eq!(dc.mode(), DriveMode::Warp);
        assert_eq!(dc.status(), DriveStatus::Spooling(0.0));
    }

    #[test]
    fn warp_spool_completes() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        for _ in 0..300 {
            dc.update(1.0 / 60.0);
        }
        assert_eq!(dc.status(), DriveStatus::Engaged);
    }

    #[test]
    fn disengage_returns_to_impulse() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.request_disengage();
        assert_eq!(dc.mode(), DriveMode::Impulse);
        assert_eq!(dc.status(), DriveStatus::Idle);
    }

    #[test]
    fn cannot_engage_impulse_explicitly() {
        let mut dc = DriveController::new();
        let ok = dc.request_engage(DriveMode::Impulse);
        assert!(!ok);
    }

    #[test]
    fn impulse_speed_is_zero_c() {
        let dc = DriveController::new();
        assert_eq!(dc.current_speed_c(), 0.0);
        assert_eq!(dc.current_speed_ly_s(), 0.0);
    }

    #[test]
    fn cruise_speed_at_min_throttle() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.set_speed_fraction(0.0);
        let speed = dc.current_speed_c();
        assert!((speed - CRUISE_MIN_C).abs() < 1e-9, "speed={speed}");
    }

    #[test]
    fn cruise_speed_at_max_throttle() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.set_speed_fraction(1.0);
        let speed = dc.current_speed_c();
        assert!((speed - CRUISE_MAX_C).abs() < 1e-9, "speed={speed}");
    }

    #[test]
    fn warp_speed_at_max_throttle() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        // spool up fully
        for _ in 0..300 {
            dc.update(1.0 / 60.0);
        }
        dc.set_speed_fraction(1.0);
        let speed = dc.current_speed_c();
        assert!((speed - WARP_MAX_C).abs() < 1.0, "speed={speed}");
    }

    #[test]
    fn warp_spooling_has_zero_speed() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        dc.set_speed_fraction(1.0);
        // still spooling — should be 0
        assert_eq!(dc.current_speed_c(), 0.0);
    }

    #[test]
    fn cruise_ly_s_at_500c() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.set_speed_fraction(1.0);
        let ly_s = dc.current_speed_ly_s();
        // 500c * 3.169e-8 ≈ 1.5845e-5
        assert!((ly_s - 1.585e-5).abs() < 1e-7, "ly_s={ly_s}");
    }

    #[test]
    fn warp_ly_s_at_5m_c() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        for _ in 0..300 {
            dc.update(1.0 / 60.0);
        }
        dc.set_speed_fraction(1.0);
        let ly_s = dc.current_speed_ly_s();
        // 5_000_000c * 3.169e-8 ≈ 0.1585
        assert!((ly_s - 0.1585).abs() < 1e-3, "ly_s={ly_s}");
    }

    #[test]
    fn repeated_warp_engage_ignored() {
        let mut dc = DriveController::new();
        assert!(dc.request_engage(DriveMode::Warp));
        dc.update(1.0); // partial spool
        assert!(!dc.request_engage(DriveMode::Warp), "should reject re-engage while spooling");
        // Spool progress should not have reset
        assert!(matches!(dc.status(), DriveStatus::Spooling(t) if t > 0.5));
    }

    #[test]
    fn warp_engage_while_already_warping_ignored() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        for _ in 0..300 { dc.update(1.0 / 60.0); }
        assert_eq!(dc.status(), DriveStatus::Engaged);
        assert!(!dc.request_engage(DriveMode::Warp), "should reject re-engage while warping");
        assert_eq!(dc.status(), DriveStatus::Engaged);
    }

    #[test]
    fn cruise_to_warp_transition() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.set_speed_fraction(1.0);
        assert!(dc.current_speed_c() > 400.0, "should be at cruise speed");
        // Engage warp — should start spooling, speed drops to 0
        assert!(dc.request_engage(DriveMode::Warp));
        assert_eq!(dc.mode(), DriveMode::Warp);
        assert_eq!(dc.current_speed_c(), 0.0, "speed zero while spooling");
    }
}
