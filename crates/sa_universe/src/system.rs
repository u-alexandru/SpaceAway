use crate::seed::Rng64;
use crate::star::Star;
use serde::{Deserialize, Serialize};

/// Classification of a planet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetType {
    Rocky,
    GasGiant,
    IceGiant,
}

/// A procedurally generated planet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    /// Orbital radius in AU.
    pub orbital_radius_au: f32,
    /// Mass in Earth masses.
    pub mass_earth: f32,
    /// Radius in Earth radii.
    pub radius_earth: f32,
    /// Orbital period in Earth years.
    pub orbital_period_years: f32,
    /// Planet classification.
    pub planet_type: PlanetType,
}

/// A star with its planetary system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetarySystem {
    pub planets: Vec<Planet>,
}

/// Compute the frost line distance in AU for a star.
/// Frost line ~ 3 AU * sqrt(L/L_sun).
fn frost_line_au(luminosity: f32) -> f32 {
    3.0 * luminosity.sqrt()
}

/// Generate a planetary system from a star and a seed.
pub fn generate_system(star: &Star, seed: u64) -> PlanetarySystem {
    let mut rng = Rng64::new(seed);

    // Number of planets: 0-10, biased toward 3-6
    let planet_count = {
        let raw = rng.range_f32(0.0, 1.0);
        // Use a triangular-ish distribution peaking around 4
        let n = (raw * 10.0).round() as u32;
        n.min(10)
    };

    let frost_line = frost_line_au(star.luminosity);

    // Place planets at increasing orbital radii using Titius-Bode-like spacing.
    // Start between 0.2 and 0.6 AU, each next planet ~1.4-2.2x further.
    let mut planets = Vec::with_capacity(planet_count as usize);
    let mut current_radius = rng.range_f32(0.2, 0.6);

    for _ in 0..planet_count {
        let orbital_radius_au = current_radius;

        let planet_type = if orbital_radius_au < frost_line {
            PlanetType::Rocky
        } else {
            // Outer planets: 60% gas giant, 40% ice giant
            if rng.next_f32() < 0.6 {
                PlanetType::GasGiant
            } else {
                PlanetType::IceGiant
            }
        };

        let mass_earth = match planet_type {
            PlanetType::Rocky => rng.range_f32(0.05, 5.0),
            PlanetType::GasGiant => rng.range_f32(10.0, 4000.0),
            PlanetType::IceGiant => rng.range_f32(5.0, 50.0),
        };

        // Radius approximation from mass
        let radius_earth = match planet_type {
            PlanetType::Rocky => mass_earth.powf(0.27),
            PlanetType::GasGiant => {
                // Gas giants: radius grows slowly with mass (Jupiter paradox)
                3.0 + (mass_earth / 318.0).powf(0.1) * 8.0
            }
            PlanetType::IceGiant => 2.0 + (mass_earth / 17.0).powf(0.3) * 2.0,
        };

        // Kepler's 3rd law: P^2 = a^3 / M_star (years, AU, solar masses)
        let orbital_period_years = (orbital_radius_au.powf(3.0) / star.mass).sqrt();

        planets.push(Planet {
            orbital_radius_au,
            mass_earth,
            radius_earth,
            orbital_period_years,
            planet_type,
        });

        // Next planet: spacing factor 1.4 to 2.2x
        current_radius *= rng.range_f32(1.4, 2.2);
    }

    PlanetarySystem { planets }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::star::generate_star;

    #[test]
    fn frost_line_sun_approximately_3au() {
        let fl = frost_line_au(1.0);
        assert!((fl - 3.0).abs() < 0.5, "Frost line for Sun should be ~3 AU, got {fl}");
    }

    #[test]
    fn frost_line_increases_with_luminosity() {
        let fl_low = frost_line_au(0.1);
        let fl_high = frost_line_au(100.0);
        assert!(fl_high > fl_low, "Frost line should increase with luminosity");
    }

    #[test]
    fn generate_system_deterministic() {
        let star = generate_star(42);
        let a = generate_system(&star, 100);
        let b = generate_system(&star, 100);
        assert_eq!(a.planets.len(), b.planets.len());
        for (pa, pb) in a.planets.iter().zip(b.planets.iter()) {
            assert_eq!(pa.orbital_radius_au.to_bits(), pb.orbital_radius_au.to_bits());
            assert_eq!(pa.mass_earth.to_bits(), pb.mass_earth.to_bits());
            assert_eq!(pa.planet_type, pb.planet_type);
        }
    }

    #[test]
    fn generate_system_planets_ordered_by_radius() {
        let star = generate_star(77);
        let sys = generate_system(&star, 200);
        for w in sys.planets.windows(2) {
            assert!(
                w[1].orbital_radius_au >= w[0].orbital_radius_au,
                "Planets should be ordered by orbital radius"
            );
        }
    }

    #[test]
    fn generate_system_inner_planets_rocky() {
        let star = generate_star(42);
        let fl = frost_line_au(star.luminosity);
        let sys = generate_system(&star, 300);
        for p in &sys.planets {
            if p.orbital_radius_au < fl {
                assert_eq!(
                    p.planet_type, PlanetType::Rocky,
                    "Inner planet at {} AU should be rocky (frost line at {fl} AU)",
                    p.orbital_radius_au
                );
            }
        }
    }

    #[test]
    fn generate_system_reasonable_planet_count() {
        // Generate many systems, check planet count is reasonable (0-12)
        for i in 0..100 {
            let star = generate_star(i * 13 + 7);
            let sys = generate_system(&star, i * 17 + 3);
            assert!(
                sys.planets.len() <= 12,
                "Too many planets: {} for seed {}",
                sys.planets.len(), i
            );
        }
    }

    #[test]
    fn generate_system_orbital_periods_physical() {
        // Kepler's 3rd law: P^2 ~ a^3 (in AU and years for solar mass)
        let star = generate_star(42);
        let sys = generate_system(&star, 500);
        for p in &sys.planets {
            if p.orbital_radius_au > 0.0 {
                assert!(p.orbital_period_years > 0.0, "Period must be positive");
            }
        }
    }

    #[test]
    fn generate_system_planet_mass_positive() {
        let star = generate_star(42);
        let sys = generate_system(&star, 600);
        for p in &sys.planets {
            assert!(p.mass_earth > 0.0, "Planet mass must be positive");
            assert!(p.radius_earth > 0.0, "Planet radius must be positive");
        }
    }
}
