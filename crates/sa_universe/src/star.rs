use crate::seed::Rng64;
use sa_math::Kelvin;
use serde::{Deserialize, Serialize};

/// Spectral classification based on surface temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectralClass {
    O, B, A, F, G, K, M,
}

/// A procedurally generated star with physically derived properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Star {
    /// Mass in solar masses.
    pub mass: f32,
    /// Surface temperature.
    pub temperature: Kelvin,
    /// Luminosity in solar luminosities.
    pub luminosity: f32,
    /// Radius in solar radii.
    pub radius: f32,
    /// Spectral class.
    pub spectral_class: SpectralClass,
    /// RGB color [0..1] derived from blackbody temperature.
    pub color: [f32; 3],
    /// Apparent brightness (for rendering, 0..1 range).
    pub brightness: f32,
}

/// Sample a stellar mass from the Kroupa IMF using inverse transform sampling.
/// Kroupa IMF: dN/dM ~ M^(-alpha) where alpha=1.3 for M<0.5, alpha=2.3 for M>=0.5.
/// Returns mass in solar masses, range [0.08, 100].
pub fn sample_mass_kroupa(rng: &mut Rng64) -> f32 {
    let u = rng.next_f64();

    // We split the CDF into two segments at M=0.5.
    // Segment 1: M in [0.08, 0.5], alpha = 1.3, exponent = -0.3
    // Segment 2: M in [0.5, 100], alpha = 2.3, exponent = -1.3
    //
    // Unnormalized integrals:
    // I1 = integral(M^-1.3, 0.08, 0.5) = (M^-0.3 / -0.3) from 0.08 to 0.5
    // I2 = k * integral(M^-2.3, 0.5, 100) where k = 0.5^(-1.3)/0.5^(-2.3) = 0.5
    // k ensures continuity at M=0.5.

    // alpha1 = 1.3 for M in [0.08, 0.5), alpha2 = 2.3 for M in [0.5, 100]
    // Antiderivative exponent: e = -alpha + 1
    let e1: f64 = -0.3;  // -1.3 + 1
    let e2: f64 = -1.3;  // -2.3 + 1
    let m_lo: f64 = 0.08;
    let m_mid: f64 = 0.5;
    let m_hi: f64 = 100.0;

    // Continuity factor: at M=0.5, M^{-1.3} = k * M^{-2.3}, so k = M^{1.0} = 0.5
    let k: f64 = m_mid;

    // Unnormalized integrals of each segment (antiderivative = M^e / e)
    let i1 = (m_mid.powf(e1) - m_lo.powf(e1)) / e1;
    let i2 = k * (m_hi.powf(e2) - m_mid.powf(e2)) / e2;
    let total = i1 + i2;

    let p1 = i1 / total;

    if u < p1 {
        // Invert CDF for segment 1
        let u_seg = u / p1;
        let lo_pow = m_lo.powf(e1);
        let hi_pow = m_mid.powf(e1);
        let m = (lo_pow + u_seg * (hi_pow - lo_pow)).powf(1.0 / e1);
        m as f32
    } else {
        // Invert CDF for segment 2
        let u_seg = (u - p1) / (1.0 - p1);
        let lo_pow = m_mid.powf(e2);
        let hi_pow = m_hi.powf(e2);
        let m = (lo_pow + u_seg * (hi_pow - lo_pow)).powf(1.0 / e2);
        m as f32
    }
}

/// Convert blackbody temperature (Kelvin) to approximate RGB [0..1].
/// Uses Tanner Helland's algorithm (attempt to fit Planckian locus).
#[allow(clippy::excessive_precision)]
pub fn temperature_to_rgb(temp_k: f32) -> [f32; 3] {
    let t = (temp_k / 100.0).clamp(10.0, 400.0);

    let r = if t <= 66.0 {
        1.0
    } else {
        let x = t - 60.0;
        (329.698727446 * x.powf(-0.1332047592) / 255.0).clamp(0.0, 1.0)
    };

    let g = if t <= 66.0 {
        let x = t;
        (99.4708025861 * x.ln() - 161.1195681661).clamp(0.0, 255.0) / 255.0
    } else {
        let x = t - 60.0;
        (288.1221695283 * x.powf(-0.0755148492) / 255.0).clamp(0.0, 1.0)
    };

    let b = if t >= 66.0 {
        1.0
    } else if t <= 19.0 {
        0.0
    } else {
        let x = t - 10.0;
        (138.5177312231 * x.ln() - 305.0447927307).clamp(0.0, 255.0) / 255.0
    };

    [r, g, b]
}

/// Classify a star by temperature into a spectral class.
pub fn classify(temp_k: f32) -> SpectralClass {
    match temp_k as u32 {
        0..3700 => SpectralClass::M,
        3700..5200 => SpectralClass::K,
        5200..6000 => SpectralClass::G,
        6000..7500 => SpectralClass::F,
        7500..10000 => SpectralClass::A,
        10000..30000 => SpectralClass::B,
        _ => SpectralClass::O,
    }
}

/// Generate a complete star from a seed.
pub fn generate_star(seed: u64) -> Star {
    let mut rng = Rng64::new(seed);
    let mass = sample_mass_kroupa(&mut rng);

    // Main sequence relations (approximations)
    let temperature = Kelvin(5778.0 * mass.powf(0.57));
    let luminosity = mass.powf(3.5);
    let radius = mass.powf(0.8);

    let spectral_class = classify(temperature.0);
    let color = temperature_to_rgb(temperature.0);

    // Brightness for rendering: log-scaled luminosity mapped to [0.1, 1.0]
    let brightness = (0.1 + 0.9 * (luminosity.ln().max(0.0) / 15.0_f32.ln())).clamp(0.1, 1.0);

    Star {
        mass,
        temperature,
        luminosity,
        radius,
        spectral_class,
        color,
        brightness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kroupa_imf_mass_range() {
        let mut rng = Rng64::new(42);
        for _ in 0..1000 {
            let m = sample_mass_kroupa(&mut rng);
            assert!(m >= 0.08, "Mass too low: {m}");
            assert!(m <= 150.0, "Mass too high: {m}");
        }
    }

    #[test]
    fn kroupa_imf_mostly_low_mass() {
        let mut rng = Rng64::new(123);
        let masses: Vec<f32> = (0..10000).map(|_| sample_mass_kroupa(&mut rng)).collect();
        let low_mass_count = masses.iter().filter(|&&m| m < 1.0).count();
        let fraction = low_mass_count as f64 / 10000.0;
        assert!(fraction > 0.7, "Expected >70% low-mass stars, got {:.1}%", fraction * 100.0);
    }

    #[test]
    fn kroupa_imf_deterministic() {
        let mut a = Rng64::new(42);
        let mut b = Rng64::new(42);
        for _ in 0..100 {
            assert_eq!(
                sample_mass_kroupa(&mut a).to_bits(),
                sample_mass_kroupa(&mut b).to_bits(),
            );
        }
    }

    #[test]
    fn temperature_to_rgb_hot_is_blue() {
        let [r, _, b] = temperature_to_rgb(30000.0);
        assert!(b > r, "Hot star should be bluer: r={r}, b={b}");
    }

    #[test]
    fn temperature_to_rgb_cool_is_red() {
        let [r, _, b] = temperature_to_rgb(3000.0);
        assert!(r > b, "Cool star should be redder: r={r}, b={b}");
    }

    #[test]
    fn temperature_to_rgb_sun_is_yellowish() {
        let [r, g, b] = temperature_to_rgb(5778.0);
        assert!(r > 0.8, "Sun should have strong red: {r}");
        assert!(g > 0.7, "Sun should have strong green: {g}");
        assert!(b < r, "Sun blue should be less than red: b={b}, r={r}");
    }

    #[test]
    fn temperature_to_rgb_in_range() {
        for temp in [2000.0, 4000.0, 6000.0, 10000.0, 25000.0, 40000.0] {
            let [r, g, b] = temperature_to_rgb(temp);
            assert!((0.0..=1.0).contains(&r), "r out of range at {temp}K: {r}");
            assert!((0.0..=1.0).contains(&g), "g out of range at {temp}K: {g}");
            assert!((0.0..=1.0).contains(&b), "b out of range at {temp}K: {b}");
        }
    }

    #[test]
    fn classify_sun_is_g() {
        assert_eq!(classify(5778.0), SpectralClass::G);
    }

    #[test]
    fn classify_hot_is_o() {
        assert_eq!(classify(35000.0), SpectralClass::O);
    }

    #[test]
    fn classify_cool_is_m() {
        assert_eq!(classify(2800.0), SpectralClass::M);
    }

    #[test]
    fn generate_star_deterministic() {
        let a = generate_star(42);
        let b = generate_star(42);
        assert_eq!(a.mass.to_bits(), b.mass.to_bits());
        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.luminosity.to_bits(), b.luminosity.to_bits());
        assert_eq!(a.spectral_class, b.spectral_class);
    }

    #[test]
    fn generate_star_luminosity_increases_with_mass() {
        let mut pairs: Vec<(f32, f32)> = (0..500)
            .map(|i| {
                let s = generate_star(i * 7 + 13);
                (s.mass, s.luminosity)
            })
            .collect();
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let q = pairs.len() / 4;
        let low_avg: f32 = pairs[..q].iter().map(|p| p.1).sum::<f32>() / q as f32;
        let high_avg: f32 = pairs[3 * q..].iter().map(|p| p.1).sum::<f32>() / q as f32;
        assert!(high_avg > low_avg, "High-mass stars should be more luminous");
    }

    #[test]
    fn generate_star_physical_sanity() {
        let s = generate_star(99);
        assert!(s.mass > 0.0);
        assert!(s.temperature.0 > 0.0);
        assert!(s.luminosity > 0.0);
        assert!(s.radius > 0.0);
        assert!(s.brightness > 0.0 && s.brightness <= 1.0);
    }
}
