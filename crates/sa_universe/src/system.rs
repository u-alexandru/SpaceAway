use crate::seed::Rng64;
use crate::star::{SpectralClass, Star};
use serde::{Deserialize, Serialize};

/// Classification of a planet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetType {
    Rocky,
    GasGiant,
    IceGiant,
}

/// Detailed sub-type describing a planet's surface/atmosphere conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetSubType {
    Molten,
    Desert,
    Temperate,
    Ocean,
    Frozen,
    Barren,
    HotGiant,
    WarmGiant,
    ColdGiant,
    CyanIce,
    TealIce,
}

/// Parameters describing a planet's atmosphere for rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AtmosphereParams {
    pub color: [f32; 3],
    pub opacity: f32,
    pub scattering_power: f32,
}

/// Parameters describing a planetary ring system.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RingParams {
    pub inner_ratio: f32,
    pub outer_ratio: f32,
    pub color: [f32; 3],
}

/// A moon orbiting a planet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Moon {
    pub orbital_radius_km: f32,
    pub radius_km: f32,
    pub sub_type: PlanetSubType,
    pub orbital_period_hours: f32,
    pub initial_phase: f32,
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
    /// Detailed sub-type.
    pub sub_type: PlanetSubType,
    /// Atmosphere parameters (None for airless bodies).
    pub atmosphere: Option<AtmosphereParams>,
    /// Whether this planet has rings.
    pub has_rings: bool,
    /// Ring parameters if rings are present.
    pub ring_params: Option<RingParams>,
    /// Axial tilt in degrees.
    pub axial_tilt_deg: f32,
    /// Rotation period in hours.
    pub rotation_period_hours: f32,
    /// Surface/effective temperature in Kelvin.
    pub surface_temperature_k: f32,
    /// Seed for procedural surface color generation.
    pub color_seed: u64,
    /// Initial orbital phase in radians [0, 2*PI).
    pub initial_phase: f32,
    /// Moons orbiting this planet.
    pub moons: Vec<Moon>,
}

/// A star with its planetary system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetarySystem {
    pub planets: Vec<Planet>,
}

impl PlanetarySystem {
    pub fn total_moon_count(&self) -> usize {
        self.planets.iter().map(|p| p.moons.len()).sum()
    }
}

/// Compute the frost line distance in AU for a star.
/// Frost line ~ 3 AU * sqrt(L/L_sun).
fn frost_line_au(luminosity: f32) -> f32 {
    3.0 * luminosity.sqrt()
}

/// Assign a sub-type to a rocky planet based on orbital position and mass.
fn rocky_sub_type(
    orbital_radius_au: f32,
    mass_earth: f32,
    hz_inner: f32,
    hz_outer: f32,
    rng: &mut Rng64,
) -> PlanetSubType {
    // Very small bodies are always barren
    if mass_earth < 0.1 {
        return PlanetSubType::Barren;
    }

    if orbital_radius_au < 0.3 * hz_inner {
        PlanetSubType::Molten
    } else if orbital_radius_au < hz_inner && mass_earth < 0.3 {
        PlanetSubType::Barren
    } else if orbital_radius_au >= hz_inner
        && orbital_radius_au <= hz_outer
        && mass_earth > 0.8
        && rng.next_f32() > 0.5
    {
        PlanetSubType::Ocean
    } else if orbital_radius_au >= hz_inner
        && orbital_radius_au <= hz_outer
        && mass_earth > 0.5
    {
        PlanetSubType::Temperate
    } else if orbital_radius_au < hz_outer {
        PlanetSubType::Desert
    } else {
        PlanetSubType::Frozen
    }
}

/// Assign atmosphere parameters based on planet sub-type.
fn atmosphere_for_sub_type(sub_type: PlanetSubType) -> Option<AtmosphereParams> {
    match sub_type {
        PlanetSubType::Temperate => Some(AtmosphereParams {
            color: [0.4, 0.6, 1.0],
            opacity: 0.6,
            scattering_power: 3.0,
        }),
        PlanetSubType::Ocean => Some(AtmosphereParams {
            color: [0.3, 0.5, 0.9],
            opacity: 0.7,
            scattering_power: 2.5,
        }),
        PlanetSubType::Desert => Some(AtmosphereParams {
            color: [0.8, 0.5, 0.3],
            opacity: 0.3,
            scattering_power: 4.0,
        }),
        PlanetSubType::Frozen => Some(AtmosphereParams {
            color: [0.5, 0.6, 0.8],
            opacity: 0.2,
            scattering_power: 4.0,
        }),
        PlanetSubType::Molten => Some(AtmosphereParams {
            color: [0.9, 0.4, 0.2],
            opacity: 0.5,
            scattering_power: 2.0,
        }),
        PlanetSubType::HotGiant
        | PlanetSubType::WarmGiant
        | PlanetSubType::ColdGiant => Some(AtmosphereParams {
            color: [0.6, 0.5, 0.4],
            opacity: 0.4,
            scattering_power: 2.0,
        }),
        PlanetSubType::CyanIce | PlanetSubType::TealIce => Some(AtmosphereParams {
            color: [0.3, 0.6, 0.8],
            opacity: 0.5,
            scattering_power: 2.5,
        }),
        PlanetSubType::Barren => None,
    }
}

/// Generate moons for a planet.
fn generate_moons(
    planet_type: PlanetType,
    mass_earth: f32,
    radius_earth: f32,
    rng: &mut Rng64,
) -> Vec<Moon> {
    let (min_moons, max_moons) = match planet_type {
        PlanetType::Rocky => {
            if mass_earth < 1.0 {
                // 70% chance of 0
                if rng.next_f32() < 0.7 {
                    return Vec::new();
                }
                (1u32, 1u32)
            } else if mass_earth < 5.0 {
                // 50% chance of 0
                if rng.next_f32() < 0.5 {
                    return Vec::new();
                }
                (1, 2)
            } else {
                (0, 3)
            }
        }
        PlanetType::GasGiant => {
            if mass_earth < 100.0 {
                (2, 8)
            } else {
                (4, 15)
            }
        }
        PlanetType::IceGiant => (1, 6),
    };

    let count = min_moons
        + (rng.range_f32(0.0, 1.0) * (max_moons - min_moons + 1) as f32) as u32;
    let count = count.min(max_moons);

    let planet_radius_km = radius_earth * 6371.0; // Earth radius in km

    let mut moons = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let radius_km = rng.range_f32(200.0, 5000.0);
        let orbit_planet_radii = rng.range_f32(5.0, 60.0);
        let orbital_radius_km = orbit_planet_radii * planet_radius_km;

        // Kepler's third law for moon orbital period (simplified):
        // T = 2*pi * sqrt(a^3 / (G*M_planet))
        // Using Earth mass units: period in hours
        let a_m = (orbital_radius_km as f64) * 1000.0;
        let m_kg = (mass_earth as f64) * 5.972e24;
        let g = 6.674e-11_f64;
        let period_s = 2.0
            * std::f64::consts::PI
            * (a_m.powi(3) / (g * m_kg)).sqrt();
        let orbital_period_hours = (period_s / 3600.0) as f32;

        let initial_phase =
            rng.range_f32(0.0, 2.0 * std::f32::consts::PI);
        let sub_type = if rng.next_f32() < 0.5 {
            PlanetSubType::Barren
        } else {
            PlanetSubType::Frozen
        };

        moons.push(Moon {
            orbital_radius_km,
            radius_km,
            sub_type,
            orbital_period_hours,
            initial_phase,
        });
    }

    moons
}

/// Generate a planetary system from a star and a seed.
pub fn generate_system(star: &Star, seed: u64) -> PlanetarySystem {
    let mut rng = Rng64::new(seed);

    // Planet count varies by spectral class
    let (min, max): (u32, u32) = match star.spectral_class {
        SpectralClass::O => (0, 1),
        SpectralClass::B => (0, 3),
        SpectralClass::A => (1, 5),
        SpectralClass::F => (2, 8),
        SpectralClass::G => (2, 8),
        SpectralClass::K => (2, 7),
        SpectralClass::M => (1, 5),
    };
    let planet_count = {
        let raw = rng.range_f32(0.0, 1.0) * (max - min + 1) as f32;
        (min + raw as u32).min(max)
    };

    let frost_line = frost_line_au(star.luminosity);
    let hz_inner = 0.95 * star.luminosity.sqrt();
    let hz_outer = 1.37 * star.luminosity.sqrt();

    // Starting radius scales with star luminosity
    let lum_sqrt = star.luminosity.sqrt();
    let mut current_radius = (0.1 * lum_sqrt
        + rng.range_f32(0.0, 0.3) * lum_sqrt)
        .max(0.05);

    let mut planets = Vec::with_capacity(planet_count as usize);

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
                3.0 + (mass_earth / 318.0).powf(0.1) * 8.0
            }
            PlanetType::IceGiant => {
                2.0 + (mass_earth / 17.0).powf(0.3) * 2.0
            }
        };

        // Kepler's 3rd law: P^2 = a^3 / M_star
        let orbital_period_years =
            (orbital_radius_au.powf(3.0) / star.mass).sqrt();

        // Sub-type assignment
        let sub_type = match planet_type {
            PlanetType::Rocky => rocky_sub_type(
                orbital_radius_au,
                mass_earth,
                hz_inner,
                hz_outer,
                &mut rng,
            ),
            PlanetType::GasGiant => {
                if orbital_radius_au < 0.5 {
                    PlanetSubType::HotGiant
                } else if orbital_radius_au < frost_line {
                    PlanetSubType::WarmGiant
                } else {
                    PlanetSubType::ColdGiant
                }
            }
            PlanetType::IceGiant => {
                if rng.next_f32() < 0.5 {
                    PlanetSubType::CyanIce
                } else {
                    PlanetSubType::TealIce
                }
            }
        };

        // Surface temperature
        let surface_temperature_k = 278.0
            * star.luminosity.powf(0.25)
            / orbital_radius_au.sqrt();

        // Atmosphere
        let atmosphere = atmosphere_for_sub_type(sub_type);

        // Rings
        let ring_chance = match planet_type {
            PlanetType::GasGiant => 0.3,
            PlanetType::IceGiant => 0.15,
            PlanetType::Rocky => 0.0,
        };
        let has_rings = rng.next_f32() < ring_chance;
        let ring_params = if has_rings {
            Some(RingParams {
                inner_ratio: rng.range_f32(1.3, 1.5),
                outer_ratio: rng.range_f32(2.0, 2.5),
                color: [0.76, 0.70, 0.50], // tan/gold
            })
        } else {
            None
        };

        // Axial tilt, rotation, color seed, phase
        let axial_tilt_deg = rng.range_f32(0.0, 30.0);
        let rotation_period_hours = rng.range_f32(10.0, 40.0);
        let color_seed = rng.next_u64();
        let initial_phase =
            rng.range_f32(0.0, 2.0 * std::f32::consts::PI);

        // Moons
        let moons = generate_moons(
            planet_type,
            mass_earth,
            radius_earth,
            &mut rng,
        );

        planets.push(Planet {
            orbital_radius_au,
            mass_earth,
            radius_earth,
            orbital_period_years,
            planet_type,
            sub_type,
            atmosphere,
            has_rings,
            ring_params,
            axial_tilt_deg,
            rotation_period_hours,
            surface_temperature_k,
            color_seed,
            initial_phase,
            moons,
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
        assert!(
            (fl - 3.0).abs() < 0.5,
            "Frost line for Sun should be ~3 AU, got {fl}"
        );
    }

    #[test]
    fn frost_line_increases_with_luminosity() {
        let fl_low = frost_line_au(0.1);
        let fl_high = frost_line_au(100.0);
        assert!(
            fl_high > fl_low,
            "Frost line should increase with luminosity"
        );
    }

    #[test]
    fn generate_system_deterministic() {
        let star = generate_star(42);
        let a = generate_system(&star, 100);
        let b = generate_system(&star, 100);
        assert_eq!(a.planets.len(), b.planets.len());
        for (pa, pb) in a.planets.iter().zip(b.planets.iter()) {
            assert_eq!(
                pa.orbital_radius_au.to_bits(),
                pb.orbital_radius_au.to_bits()
            );
            assert_eq!(pa.mass_earth.to_bits(), pb.mass_earth.to_bits());
            assert_eq!(pa.planet_type, pb.planet_type);
            assert_eq!(pa.sub_type, pb.sub_type);
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
                    p.planet_type,
                    PlanetType::Rocky,
                    "Inner planet at {} AU should be rocky (frost line at {fl} AU)",
                    p.orbital_radius_au
                );
            }
        }
    }

    #[test]
    fn generate_system_reasonable_planet_count() {
        for i in 0..100 {
            let star = generate_star(i * 13 + 7);
            let sys = generate_system(&star, i * 17 + 3);
            assert!(
                sys.planets.len() <= 12,
                "Too many planets: {} for seed {}",
                sys.planets.len(),
                i
            );
        }
    }

    #[test]
    fn generate_system_orbital_periods_physical() {
        let star = generate_star(42);
        let sys = generate_system(&star, 500);
        for p in &sys.planets {
            if p.orbital_radius_au > 0.0 {
                assert!(
                    p.orbital_period_years > 0.0,
                    "Period must be positive"
                );
            }
        }
    }

    #[test]
    fn generate_system_planet_mass_positive() {
        let star = generate_star(42);
        let sys = generate_system(&star, 600);
        for p in &sys.planets {
            assert!(p.mass_earth > 0.0, "Planet mass must be positive");
            assert!(
                p.radius_earth > 0.0,
                "Planet radius must be positive"
            );
        }
    }

    #[test]
    fn planet_has_sub_type() {
        let star = generate_star(42);
        let sys = generate_system(&star, 100);
        for p in &sys.planets {
            match p.planet_type {
                PlanetType::Rocky => assert!(matches!(
                    p.sub_type,
                    PlanetSubType::Molten
                        | PlanetSubType::Desert
                        | PlanetSubType::Temperate
                        | PlanetSubType::Ocean
                        | PlanetSubType::Frozen
                        | PlanetSubType::Barren
                )),
                PlanetType::GasGiant => assert!(matches!(
                    p.sub_type,
                    PlanetSubType::HotGiant
                        | PlanetSubType::WarmGiant
                        | PlanetSubType::ColdGiant
                )),
                PlanetType::IceGiant => assert!(matches!(
                    p.sub_type,
                    PlanetSubType::CyanIce | PlanetSubType::TealIce
                )),
            }
        }
    }

    #[test]
    fn gas_giants_may_have_rings() {
        let mut found = false;
        for i in 0..200 {
            let star = generate_star(i * 7 + 1);
            let sys = generate_system(&star, i * 13 + 5);
            for p in &sys.planets {
                if p.has_rings {
                    found = true;
                }
            }
        }
        assert!(found);
    }

    #[test]
    fn system_has_moons() {
        let mut found = false;
        for i in 0..100 {
            let star = generate_star(i * 11 + 3);
            let sys = generate_system(&star, i * 17 + 7);
            if sys.total_moon_count() > 0 {
                found = true;
            }
        }
        assert!(found);
    }

    #[test]
    fn planet_count_varies_by_star_type() {
        let mut m_total = 0u32;
        let mut g_total = 0u32;
        let mut m_count = 0u32;
        let mut g_count = 0u32;
        for i in 0..500 {
            let star = generate_star(i * 7);
            let sys = generate_system(&star, i * 13);
            match star.spectral_class {
                SpectralClass::M => {
                    m_total += sys.planets.len() as u32;
                    m_count += 1;
                }
                SpectralClass::G => {
                    g_total += sys.planets.len() as u32;
                    g_count += 1;
                }
                _ => {}
            }
        }
        if m_count > 10 && g_count > 10 {
            let m_avg = m_total as f32 / m_count as f32;
            let g_avg = g_total as f32 / g_count as f32;
            assert!(
                g_avg > m_avg,
                "G-stars ({g_avg}) should average more planets than M-dwarfs ({m_avg})"
            );
        }
    }
}
