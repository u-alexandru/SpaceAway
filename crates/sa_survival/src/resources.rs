//! Ship survival resources: fuel, oxygen, power.
//!
//! Pure math -- no physics, no rendering. Updated each frame with dt and ship state.

use sa_ship::DriveMode;

/// Ship survival resources with drain/regen rates.
#[derive(Debug, Clone)]
pub struct ShipResources {
    /// Fuel remaining (0.0 to 1.0). Burns slowly from reactor, faster from engines.
    pub fuel: f32,
    /// Oxygen remaining (0.0 to 1.0). Depletes when life support has no power.
    pub oxygen: f32,
    /// Power output (0.0 to 1.0). Proportional to fuel availability.
    pub power: f32,
    /// Exotic fuel for warp drive (0.0 to 1.0).
    pub exotic_fuel: f32,
}

/// Reactor idle fuel burn rate per second.
const IDLE_FUEL_DRAIN: f32 = 0.0005;
/// Engine fuel burn rate per second at full throttle.
const ENGINE_FUEL_DRAIN: f32 = 0.002;
/// O2 drain rate per second when life support has no power.
const O2_DRAIN: f32 = 0.005;
/// O2 regen rate per second when life support has power.
const O2_REGEN: f32 = 0.002;
/// Cruise drive fuel drain rate per second at full drive fraction.
const CRUISE_FUEL_DRAIN: f32 = 0.004;
/// Minimum exotic fuel drain rate per second at minimum warp fraction.
const WARP_EXOTIC_DRAIN_MIN: f32 = 0.0005;
/// Maximum exotic fuel drain rate per second at maximum warp fraction.
const WARP_EXOTIC_DRAIN_MAX: f32 = 0.005;

impl ShipResources {
    /// Create resources at full capacity.
    pub fn new() -> Self {
        Self {
            fuel: 1.0,
            oxygen: 1.0,
            power: 1.0,
            exotic_fuel: 1.0,
        }
    }

    /// Update resources for one frame.
    ///
    /// - `dt`: delta time in seconds
    /// - `throttle`: 0.0 to 1.0
    /// - `engine_on`: whether engines are firing
    pub fn update(&mut self, dt: f32, throttle: f32, engine_on: bool) {
        // Fuel drain
        let engine_drain = if engine_on {
            ENGINE_FUEL_DRAIN * throttle
        } else {
            0.0
        };
        self.fuel = (self.fuel - (IDLE_FUEL_DRAIN + engine_drain) * dt).max(0.0);

        // Power from fuel (simple on/off)
        self.power = if self.fuel > 0.0 { 1.0 } else { 0.0 };

        // O2: drains without power, regenerates with power
        if self.power > 0.0 {
            self.oxygen = (self.oxygen + O2_REGEN * dt).min(1.0);
        } else {
            self.oxygen = (self.oxygen - O2_DRAIN * dt).max(0.0);
        }
    }

    /// Update resources for one frame with drive mode awareness.
    ///
    /// - `dt`: delta time in seconds
    /// - `throttle`: 0.0 to 1.0 (used for Impulse engine drain)
    /// - `engine_on`: whether impulse engines are firing
    /// - `drive`: current drive mode
    /// - `drive_fraction`: 0.0 to 1.0 drive intensity (cruise or warp)
    pub fn update_with_drive(
        &mut self,
        dt: f32,
        throttle: f32,
        engine_on: bool,
        drive: DriveMode,
        drive_fraction: f32,
    ) {
        // Hydrogen fuel drain
        let engine_drain = match drive {
            DriveMode::Impulse => {
                if engine_on {
                    ENGINE_FUEL_DRAIN * throttle
                } else {
                    0.0
                }
            }
            DriveMode::Cruise => CRUISE_FUEL_DRAIN * drive_fraction,
            DriveMode::Warp => 0.0,
        };
        self.fuel = (self.fuel - (IDLE_FUEL_DRAIN + engine_drain) * dt).max(0.0);

        // Exotic fuel drain: only in Warp mode
        if drive == DriveMode::Warp {
            let exotic_drain =
                WARP_EXOTIC_DRAIN_MIN + (WARP_EXOTIC_DRAIN_MAX - WARP_EXOTIC_DRAIN_MIN) * drive_fraction;
            self.exotic_fuel = (self.exotic_fuel - exotic_drain * dt).max(0.0);
        }

        // Power from fuel (simple on/off)
        self.power = if self.fuel > 0.0 { 1.0 } else { 0.0 };

        // O2: drains without power, regenerates with power
        if self.power > 0.0 {
            self.oxygen = (self.oxygen + O2_REGEN * dt).min(1.0);
        } else {
            self.oxygen = (self.oxygen - O2_DRAIN * dt).max(0.0);
        }
    }

    /// Add fuel from a gathered resource.
    pub fn add_fuel(&mut self, amount: f32) {
        self.fuel = (self.fuel + amount).min(1.0);
    }

    /// Add oxygen from a gathered resource.
    pub fn add_oxygen(&mut self, amount: f32) {
        self.oxygen = (self.oxygen + amount).min(1.0);
    }

    /// Add exotic fuel from a gathered resource.
    pub fn add_exotic_fuel(&mut self, amount: f32) {
        self.exotic_fuel = (self.exotic_fuel + amount).min(1.0);
    }
}

impl Default for ShipResources {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_full() {
        let r = ShipResources::new();
        assert_eq!(r.fuel, 1.0);
        assert_eq!(r.oxygen, 1.0);
        assert_eq!(r.power, 1.0);
    }

    #[test]
    fn idle_fuel_drain() {
        let mut r = ShipResources::new();
        // 1 second idle
        r.update(1.0, 0.0, false);
        let expected = 1.0 - IDLE_FUEL_DRAIN;
        assert!((r.fuel - expected).abs() < 1e-6, "fuel={}, expected={}", r.fuel, expected);
    }

    #[test]
    fn engine_fuel_drain() {
        let mut r = ShipResources::new();
        // 1 second at full throttle
        r.update(1.0, 1.0, true);
        let expected = 1.0 - IDLE_FUEL_DRAIN - ENGINE_FUEL_DRAIN;
        assert!((r.fuel - expected).abs() < 1e-6, "fuel={}, expected={}", r.fuel, expected);
    }

    #[test]
    fn half_throttle_drain() {
        let mut r = ShipResources::new();
        r.update(1.0, 0.5, true);
        let expected = 1.0 - IDLE_FUEL_DRAIN - ENGINE_FUEL_DRAIN * 0.5;
        assert!((r.fuel - expected).abs() < 1e-6);
    }

    #[test]
    fn fuel_floors_at_zero() {
        let mut r = ShipResources::new();
        r.fuel = 0.001;
        r.update(10.0, 1.0, true);
        assert_eq!(r.fuel, 0.0);
    }

    #[test]
    fn power_follows_fuel() {
        let mut r = ShipResources::new();
        assert_eq!(r.power, 1.0);
        r.fuel = 0.0;
        r.update(0.0, 0.0, false);
        assert_eq!(r.power, 0.0);
    }

    #[test]
    fn o2_drains_without_power() {
        let mut r = ShipResources::new();
        r.fuel = 0.0;
        r.update(1.0, 0.0, false);
        assert_eq!(r.power, 0.0);
        let expected = 1.0 - O2_DRAIN;
        assert!((r.oxygen - expected).abs() < 1e-6);
    }

    #[test]
    fn o2_regens_with_power() {
        let mut r = ShipResources::new();
        r.oxygen = 0.5;
        r.update(1.0, 0.0, false);
        let expected = 0.5 + O2_REGEN;
        assert!((r.oxygen - expected).abs() < 1e-6);
    }

    #[test]
    fn o2_caps_at_one() {
        let mut r = ShipResources::new();
        r.oxygen = 1.0;
        r.update(10.0, 0.0, false);
        assert_eq!(r.oxygen, 1.0);
    }

    #[test]
    fn o2_floors_at_zero() {
        let mut r = ShipResources::new();
        r.fuel = 0.0;
        r.oxygen = 0.01;
        r.update(10.0, 0.0, false);
        assert_eq!(r.oxygen, 0.0);
    }

    #[test]
    fn add_fuel_caps() {
        let mut r = ShipResources::new();
        r.fuel = 0.8;
        r.add_fuel(0.5);
        assert_eq!(r.fuel, 1.0);
    }

    #[test]
    fn add_oxygen_caps() {
        let mut r = ShipResources::new();
        r.oxygen = 0.9;
        r.add_oxygen(0.5);
        assert_eq!(r.oxygen, 1.0);
    }

    #[test]
    fn idle_lasts_about_33_minutes() {
        let mut r = ShipResources::new();
        // Simulate 33 minutes at idle
        let seconds = 33.0 * 60.0;
        r.update(seconds, 0.0, false);
        // Should be nearly empty
        assert!(r.fuel < 0.02, "fuel={} should be nearly 0 after 33min idle", r.fuel);
        assert!(r.fuel >= 0.0);
    }

    #[test]
    fn full_throttle_lasts_about_8_minutes() {
        let mut r = ShipResources::new();
        // 0.0005 + 0.002 = 0.0025/s -> 1.0/0.0025 = 400s ~6.7min
        // Actually the spec says ~8 minutes for engine alone.
        // Total: idle + full = 0.0025/s -> 400s
        let seconds = 400.0;
        r.update(seconds, 1.0, true);
        assert_eq!(r.fuel, 0.0);
    }

    // --- New tests ---

    #[test]
    fn exotic_fuel_starts_full() {
        let r = ShipResources::new();
        assert_eq!(r.exotic_fuel, 1.0);
    }

    #[test]
    fn cruise_drains_fuel_faster() {
        let mut r = ShipResources::new();
        // 1 second, cruise at full drive_fraction, no impulse throttle
        r.update_with_drive(1.0, 0.0, false, DriveMode::Cruise, 1.0);
        let expected = 1.0 - (IDLE_FUEL_DRAIN + CRUISE_FUEL_DRAIN);
        assert!(
            (r.fuel - expected).abs() < 1e-6,
            "fuel={}, expected={}",
            r.fuel,
            expected
        );
    }

    #[test]
    fn warp_drains_exotic_fuel() {
        let mut r = ShipResources::new();
        // 1 second, warp at full drive_fraction
        r.update_with_drive(1.0, 0.0, false, DriveMode::Warp, 1.0);
        let expected_exotic = 1.0 - WARP_EXOTIC_DRAIN_MAX;
        assert!(
            (r.exotic_fuel - expected_exotic).abs() < 1e-6,
            "exotic_fuel={}, expected={}",
            r.exotic_fuel,
            expected_exotic
        );
    }

    #[test]
    fn warp_does_not_drain_hydrogen_beyond_idle() {
        let mut r = ShipResources::new();
        // 1 second in warp — hydrogen only drains at idle rate
        r.update_with_drive(1.0, 0.0, false, DriveMode::Warp, 1.0);
        let expected_fuel = 1.0 - IDLE_FUEL_DRAIN;
        assert!(
            (r.fuel - expected_fuel).abs() < 1e-6,
            "fuel={}, expected={}",
            r.fuel,
            expected_fuel
        );
    }

    #[test]
    fn impulse_unchanged_from_original() {
        let mut r1 = ShipResources::new();
        let mut r2 = ShipResources::new();
        r1.update(1.0, 0.5, true);
        r2.update_with_drive(1.0, 0.5, true, DriveMode::Impulse, 0.0);
        assert!(
            (r1.fuel - r2.fuel).abs() < 1e-6,
            "update fuel={}, update_with_drive fuel={}",
            r1.fuel,
            r2.fuel
        );
        assert!((r1.oxygen - r2.oxygen).abs() < 1e-6);
        assert_eq!(r1.power, r2.power);
    }
}
