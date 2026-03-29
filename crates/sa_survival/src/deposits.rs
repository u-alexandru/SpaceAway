//! Resource deposits in game coordinates (meters from origin).
//!
//! Simple deterministic generation for Phase 5b-slim. Deposits are placed
//! within a few km of the starting point for the player to find and gather.

/// Type of resource a deposit contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// Refuels the ship.
    FuelAsteroid,
    /// Restores O2 supplies.
    SupplyCache,
    /// Both fuel and supplies.
    Derelict,
}

impl ResourceKind {
    /// Human-readable label for the sensors display.
    pub fn label(self) -> &'static str {
        match self {
            ResourceKind::FuelAsteroid => "Fuel Asteroid",
            ResourceKind::SupplyCache => "Supply Cache",
            ResourceKind::Derelict => "Derelict",
        }
    }

    /// Sensor display icon character.
    pub fn icon(self) -> &'static str {
        match self {
            ResourceKind::FuelAsteroid => "\u{25C6}", // diamond
            ResourceKind::SupplyCache => "\u{25CB}",  // circle
            ResourceKind::Derelict => "\u{25A0}",     // square
        }
    }
}

/// A gatherable resource deposit in the game world.
#[derive(Debug, Clone)]
pub struct ResourceDeposit {
    /// Unique identifier for tracking gathered state.
    pub id: u64,
    /// Position in game coordinates (meters from origin).
    pub position: [f32; 3],
    /// What kind of resource this contains.
    pub kind: ResourceKind,
    /// How much resource to grant (0.0 to 1.0).
    pub amount: f32,
}

/// Simple xorshift for deposit generation (self-contained, no dependency).
fn xorshift(state: &mut u64) -> u64 {
    let mut s = *state;
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    *state = s;
    s
}

fn rand_f32(state: &mut u64) -> f32 {
    (xorshift(state) >> 40) as f32 / ((1u64 << 24) as f32)
}

fn rand_range(state: &mut u64, min: f32, max: f32) -> f32 {
    min + rand_f32(state) * (max - min)
}

/// Generate resource deposits deterministically from a seed.
///
/// Places 8-12 deposits within ~5km of the origin in game coordinates.
/// Each deposit has a position in meters, a kind, and an amount.
pub fn generate_deposits(seed: u64) -> Vec<ResourceDeposit> {
    // Ensure nonzero state
    let mut state = seed.wrapping_add(0x9E3779B97F4A7C15);
    if state == 0 {
        state = 1;
    }

    let count = 8 + (xorshift(&mut state) % 5) as usize; // 8-12
    let mut deposits = Vec::with_capacity(count);

    for i in 0..count {
        // Position: scattered within 500-5000m from origin
        let distance = rand_range(&mut state, 500.0, 5000.0);
        let angle = rand_range(&mut state, 0.0, std::f32::consts::TAU);
        let y_offset = rand_range(&mut state, -200.0, 200.0);

        let x = distance * angle.cos();
        let z = distance * angle.sin();
        let y = y_offset;

        // Kind distribution: ~50% fuel, ~30% supply, ~20% derelict
        let kind_roll = xorshift(&mut state) % 10;
        let kind = if kind_roll < 5 {
            ResourceKind::FuelAsteroid
        } else if kind_roll < 8 {
            ResourceKind::SupplyCache
        } else {
            ResourceKind::Derelict
        };

        // Amount: 0.1 to 0.3 for fuel/supply, 0.15 to 0.25 for derelict (gives both)
        let amount = match kind {
            ResourceKind::FuelAsteroid => rand_range(&mut state, 0.1, 0.3),
            ResourceKind::SupplyCache => rand_range(&mut state, 0.1, 0.3),
            ResourceKind::Derelict => rand_range(&mut state, 0.15, 0.25),
        };

        deposits.push(ResourceDeposit {
            id: seed.wrapping_mul(1000).wrapping_add(i as u64),
            position: [x, y, z],
            kind,
            amount,
        });
    }

    deposits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let a = generate_deposits(42);
        let b = generate_deposits(42);
        assert_eq!(a.len(), b.len());
        for (da, db) in a.iter().zip(b.iter()) {
            assert_eq!(da.id, db.id);
            assert_eq!(da.position[0].to_bits(), db.position[0].to_bits());
            assert_eq!(da.kind, db.kind);
        }
    }

    #[test]
    fn count_in_range() {
        let deposits = generate_deposits(42);
        assert!(
            (8..=12).contains(&deposits.len()),
            "Expected 8-12 deposits, got {}",
            deposits.len()
        );
    }

    #[test]
    fn different_seeds_differ() {
        let a = generate_deposits(1);
        let b = generate_deposits(2);
        // At least one position should differ
        let differs = a.iter().zip(b.iter()).any(|(da, db)| {
            da.position[0].to_bits() != db.position[0].to_bits()
        });
        assert!(differs, "Different seeds should produce different deposits");
    }

    #[test]
    fn positions_within_range() {
        let deposits = generate_deposits(42);
        for d in &deposits {
            let dist = (d.position[0] * d.position[0] + d.position[2] * d.position[2]).sqrt();
            assert!(
                (400.0..=6000.0).contains(&dist),
                "Deposit at distance {dist} outside expected range"
            );
            assert!(
                d.position[1].abs() <= 250.0,
                "Y position {} too extreme",
                d.position[1]
            );
        }
    }

    #[test]
    fn amounts_valid() {
        let deposits = generate_deposits(42);
        for d in &deposits {
            assert!(d.amount > 0.0 && d.amount <= 1.0, "Invalid amount: {}", d.amount);
        }
    }

    #[test]
    fn unique_ids() {
        let deposits = generate_deposits(42);
        let ids: std::collections::HashSet<u64> = deposits.iter().map(|d| d.id).collect();
        assert_eq!(ids.len(), deposits.len(), "Deposit IDs must be unique");
    }

    #[test]
    fn has_variety_of_kinds() {
        // With 8-12 deposits, we should have at least 2 different kinds
        let deposits = generate_deposits(42);
        let kinds: std::collections::HashSet<_> = deposits.iter().map(|d| std::mem::discriminant(&d.kind)).collect();
        assert!(kinds.len() >= 2, "Should have variety of resource kinds");
    }
}
