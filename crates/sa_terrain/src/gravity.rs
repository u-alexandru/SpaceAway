/// Gravity blending state for the transition between ship gravity and planet gravity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GravityState {
    /// Normalized direction gravity pulls (unit vector pointing "down").
    pub direction: [f32; 3],
    /// Magnitude of gravity in m/s².
    pub magnitude: f32,
    /// Blend factor: 0.0 = fully ship gravity, 1.0 = fully planet gravity.
    pub blend: f32,
}

/// Compute the blended gravity for a ship/player near a planet surface.
///
/// # Parameters
/// - `ship_pos_planet_relative`: position of the ship relative to the planet center, in meters (f64 for precision).
/// - `ship_down`: current "down" direction in ship/inertial frame (unit vector).
/// - `planet_radius_m`: planet radius in meters.
/// - `surface_gravity_ms2`: planet surface gravity in m/s².
/// - `ship_gravity_ms2`: ship artificial gravity magnitude in m/s².
///
/// # Transition zone
/// The blend starts when altitude < `planet_radius * 0.2` (top of atmosphere),
/// reaching full planet gravity at the surface (altitude = 0).
pub fn compute_gravity(
    ship_pos_planet_relative: [f64; 3],
    ship_down: [f32; 3],
    planet_radius_m: f64,
    surface_gravity_ms2: f32,
    ship_gravity_ms2: f32,
) -> GravityState {
    let dist = length_f64(ship_pos_planet_relative);
    let altitude = dist - planet_radius_m;
    let atmosphere_top = planet_radius_m * 0.2;

    // Above the transition zone: pure ship gravity.
    if altitude >= atmosphere_top {
        return GravityState {
            direction: ship_down,
            magnitude: ship_gravity_ms2,
            blend: 0.0,
        };
    }

    // Blend factor: 0 at atmosphere top, 1 at surface (altitude <= 0).
    let t = if atmosphere_top > 0.0 {
        (1.0 - (altitude / atmosphere_top).max(0.0) as f32).clamp(0.0, 1.0)
    } else {
        1.0
    };

    // Planet "down" = toward center = -normalize(position).
    let planet_down = negate(normalize_f64_to_f32(ship_pos_planet_relative));

    // Antiparallel guard: if ship_down and planet_down are nearly opposite and
    // we are past the halfway point, snap to planet_down to avoid degenerate lerp.
    let dot = dot3(ship_down, planet_down);
    let direction = if dot < -0.99 && t > 0.5 {
        planet_down
    } else {
        normalize_f32(lerp3(ship_down, planet_down, t))
    };

    let magnitude = lerp_f32(ship_gravity_ms2, surface_gravity_ms2, t);

    GravityState {
        direction,
        magnitude,
        blend: t,
    }
}

/// Compute surface gravity relative to Earth given mass and radius ratios.
///
/// `mass_ratio = planet_mass / earth_mass`, `radius_ratio = planet_radius / earth_radius`.
/// Returns surface gravity in m/s².
pub fn surface_gravity(mass_ratio: f32, radius_ratio: f32) -> f32 {
    9.81 * mass_ratio / (radius_ratio * radius_ratio)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn length_f64(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize_f64_to_f32(v: [f64; 3]) -> [f32; 3] {
    let len = length_f64(v);
    if len < 1e-30 {
        return [0.0, -1.0, 0.0];
    }
    [(v[0] / len) as f32, (v[1] / len) as f32, (v[2] / len) as f32]
}

fn negate(v: [f32; 3]) -> [f32; 3] {
    [-v[0], -v[1], -v[2]]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn normalize_f32(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-15 {
        return [0.0, -1.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    const EARTH_GRAVITY: f32 = 9.81;
    const SHIP_GRAVITY: f32 = 9.81; // 1 g ship gravity

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    fn vec_approx_eq(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        approx_eq(a[0], b[0], eps) && approx_eq(a[1], b[1], eps) && approx_eq(a[2], b[2], eps)
    }

    /// 1. Well above the atmosphere — should return pure ship gravity unchanged.
    #[test]
    fn above_atmosphere_returns_ship_gravity() {
        // 50% above the atmosphere top (radius * 1.2 + a bit)
        let pos = [0.0_f64, EARTH_RADIUS_M * 1.5, 0.0]; // altitude = 0.5 * radius > 0.2 * radius
        let ship_down = [0.0_f32, -1.0, 0.0];
        let state = compute_gravity(pos, ship_down, EARTH_RADIUS_M, EARTH_GRAVITY, SHIP_GRAVITY);

        assert!(approx_eq(state.blend, 0.0, 1e-6), "blend should be 0, got {}", state.blend);
        assert!(
            approx_eq(state.magnitude, SHIP_GRAVITY, 1e-4),
            "magnitude should be ship gravity, got {}",
            state.magnitude
        );
        assert!(vec_approx_eq(state.direction, ship_down, 1e-6));
    }

    /// 2. On the surface (altitude ≈ 0) — should return pure planet gravity.
    #[test]
    fn on_surface_returns_planet_gravity() {
        // Exactly on the surface: distance = radius, altitude = 0.
        let pos = [EARTH_RADIUS_M, 0.0_f64, 0.0];
        let ship_down = [0.0_f32, -1.0, 0.0]; // ship gravity pointing "down" (arbitrary)
        let state = compute_gravity(pos, ship_down, EARTH_RADIUS_M, EARTH_GRAVITY, SHIP_GRAVITY);

        assert!(approx_eq(state.blend, 1.0, 1e-5), "blend should be 1, got {}", state.blend);
        assert!(
            approx_eq(state.magnitude, EARTH_GRAVITY, 1e-4),
            "magnitude should be planet surface gravity, got {}",
            state.magnitude
        );
        // Direction should point toward center: -normalize([1,0,0]) = [-1,0,0]
        assert!(
            vec_approx_eq(state.direction, [-1.0, 0.0, 0.0], 1e-5),
            "direction should be toward planet center, got {:?}",
            state.direction
        );
    }

    /// 3. Mid-transition — blend should be between 0 and 1, direction and magnitude interpolated.
    #[test]
    fn mid_transition_blends() {
        // altitude = 0.1 * radius => t = 0.5
        let altitude = EARTH_RADIUS_M * 0.1;
        let pos = [0.0_f64, EARTH_RADIUS_M + altitude, 0.0];
        let ship_down = [0.0_f32, -1.0, 0.0]; // same as planet down in this case
        let state = compute_gravity(pos, ship_down, EARTH_RADIUS_M, EARTH_GRAVITY, SHIP_GRAVITY);

        assert!(
            state.blend > 0.0 && state.blend < 1.0,
            "blend should be between 0 and 1, got {}",
            state.blend
        );
        assert!(
            approx_eq(state.blend, 0.5, 1e-4),
            "blend should be ~0.5, got {}",
            state.blend
        );
        // When ship_down == planet_down, magnitude is the same at any blend anyway
        // since ship and planet gravity are equal here, but direction should still be valid unit vector
        let len = (state.direction[0].powi(2)
            + state.direction[1].powi(2)
            + state.direction[2].powi(2))
        .sqrt();
        assert!(approx_eq(len, 1.0, 1e-5), "direction must be unit vector, len={}", len);
    }

    /// 4. Earth gravity from surface_gravity() — mass_ratio=1, radius_ratio=1 => 9.81 m/s².
    #[test]
    fn earth_surface_gravity() {
        let g = surface_gravity(1.0, 1.0);
        assert!(approx_eq(g, 9.81, 1e-4), "Earth gravity should be 9.81, got {}", g);
    }

    /// 5. Super-Earth gravity — 2x mass, 1.2x radius.
    #[test]
    fn super_earth_surface_gravity() {
        let g = surface_gravity(2.0, 1.2);
        let expected = 9.81 * 2.0 / (1.2 * 1.2);
        assert!(
            approx_eq(g, expected, 1e-4),
            "super-Earth gravity should be ~{:.3}, got {:.3}",
            expected,
            g
        );
        // Should be higher than Earth's
        assert!(g > 9.81, "super-Earth gravity should exceed Earth's");
    }

    /// 6. Moon gravity — 0.0123 mass ratio, 0.2727 radius ratio.
    #[test]
    fn moon_surface_gravity() {
        let g = surface_gravity(0.0123, 0.2727);
        // Moon is ~1.62 m/s²
        assert!(
            approx_eq(g, 1.62, 0.05),
            "Moon gravity should be ~1.62 m/s², got {:.3}",
            g
        );
    }

    /// 7. Antiparallel guard — ship down is exactly opposite planet down, t > 0.5.
    #[test]
    fn antiparallel_guard_snaps_to_planet_down() {
        // Ship is near the surface, pointing "up" (away from planet center),
        // which is the opposite of planet's down direction.
        // pos along +Y => planet_down = [0, -1, 0]
        // ship_down = [0, 1, 0] (pointing away, i.e., antiparallel)
        let altitude = EARTH_RADIUS_M * 0.01; // very close to surface, t ~= 0.95
        let pos = [0.0_f64, EARTH_RADIUS_M + altitude, 0.0];
        let ship_down = [0.0_f32, 1.0, 0.0]; // antiparallel to planet_down

        let state = compute_gravity(pos, ship_down, EARTH_RADIUS_M, EARTH_GRAVITY, SHIP_GRAVITY);

        // Should snap to planet down = [0, -1, 0]
        assert!(
            vec_approx_eq(state.direction, [0.0, -1.0, 0.0], 1e-5),
            "antiparallel guard should snap to planet_down, got {:?}",
            state.direction
        );
        // t should be close to 1 (near surface)
        assert!(state.blend > 0.5, "blend should be > 0.5, got {}", state.blend);
    }
}
