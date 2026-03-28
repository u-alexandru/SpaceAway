//! Suit resources: emergency oxygen and power when ship systems fail.

/// Suit survival resources — last line of defense.
#[derive(Debug, Clone)]
pub struct SuitResources {
    /// Oxygen remaining (0.0 to 1.0). Drains when ship O2 unavailable.
    pub oxygen: f32,
    /// Battery power remaining (0.0 to 1.0). Drains when ship power unavailable.
    pub power: f32,
}

/// Suit O2 drain rate per second when ship O2 is unavailable.
const SUIT_O2_DRAIN: f32 = 0.002; // ~8 minutes
/// Suit power drain rate per second when ship power is unavailable.
const SUIT_POWER_DRAIN: f32 = 0.001; // ~16 minutes
/// Suit recharge rate per second when ship resources are available.
const SUIT_RECHARGE: f32 = 0.01; // fast recharge from ship

impl SuitResources {
    pub fn new() -> Self {
        Self {
            oxygen: 1.0,
            power: 1.0,
        }
    }

    /// Update suit based on ship state.
    /// - Ship O2 available: recharge suit O2
    /// - Ship O2 unavailable: drain suit O2
    /// - Ship power available: recharge suit power
    /// - Ship power unavailable: drain suit power
    pub fn update(&mut self, dt: f32, ship_o2_available: bool, ship_power_available: bool) {
        if ship_o2_available {
            self.oxygen = (self.oxygen + SUIT_RECHARGE * dt).min(1.0);
        } else {
            self.oxygen = (self.oxygen - SUIT_O2_DRAIN * dt).max(0.0);
        }

        if ship_power_available {
            self.power = (self.power + SUIT_RECHARGE * dt).min(1.0);
        } else {
            self.power = (self.power - SUIT_POWER_DRAIN * dt).max(0.0);
        }
    }
}

impl Default for SuitResources {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_full() {
        let s = SuitResources::new();
        assert_eq!(s.oxygen, 1.0);
        assert_eq!(s.power, 1.0);
    }

    #[test]
    fn drains_when_ship_unavailable() {
        let mut s = SuitResources::new();
        s.update(1.0, false, false);
        assert!(s.oxygen < 1.0);
        assert!(s.power < 1.0);
    }

    #[test]
    fn recharges_when_ship_available() {
        let mut s = SuitResources::new();
        s.oxygen = 0.5;
        s.power = 0.5;
        s.update(1.0, true, true);
        assert!(s.oxygen > 0.5);
        assert!(s.power > 0.5);
    }

    #[test]
    fn o2_lasts_about_8_minutes() {
        let mut s = SuitResources::new();
        s.update(480.0, false, true); // 8 minutes
        assert!(
            s.oxygen < 0.05,
            "O2 should be nearly empty after 8min: {}",
            s.oxygen
        );
    }

    #[test]
    fn power_lasts_about_16_minutes() {
        let mut s = SuitResources::new();
        s.update(960.0, true, false); // 16 minutes
        assert!(
            s.power < 0.05,
            "Power should be nearly empty after 16min: {}",
            s.power
        );
    }
}
