//! Unified planet approach state machine.
//!
//! Centralizes all proximity checks, phase transitions, and speed
//! limiting into a single authoritative module. Other systems query
//! `ApproachState` instead of doing their own distance math.

use sa_math::WorldPos;

use crate::constants::{
    APPROACH_TIME_SECONDS, DEPART_APPROACHING, DEPART_ORBIT,
    LY_TO_M, PHASE_APPROACHING, PHASE_LANDING_M, PHASE_LOWER_ATMO,
    PHASE_ORBIT, PHASE_UPPER_ATMO, safe_standoff_m,
};

// ── Phase ───────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApproachPhase {
    Distant,
    Approaching,
    Orbit,
    UpperAtmosphere,
    LowerAtmosphere,
    Landing,
    Surface,
    Departing,
}

// ── Read-only snapshot ──────────────────────────────────────────────
#[derive(Clone)]
pub struct ApproachState {
    pub phase: ApproachPhase,
    pub altitude_m: f64,
    pub planet_pos_ly: Option<WorldPos>,
    pub planet_radius_m: f64,
    pub body_index: Option<usize>,
    pub terrain_active: bool,
    pub collision_active: bool,
    pub disengage_cruise: bool,
    pub cascade_warp_to_cruise: bool,
    pub can_engage_cruise: bool,
    pub can_engage_warp: bool,
    pub cruise_speed_cap_ms: Option<f64>,
}

impl ApproachState {
    /// Default state when no approach computation has run yet.
    pub fn default_distant() -> Self {
        Self {
            phase: ApproachPhase::Distant,
            altitude_m: f64::MAX,
            planet_pos_ly: None,
            planet_radius_m: 0.0,
            body_index: None,
            terrain_active: false,
            collision_active: false,
            disengage_cruise: false,
            cascade_warp_to_cruise: false,
            can_engage_cruise: true,
            can_engage_warp: true,
            cruise_speed_cap_ms: None,
        }
    }
}

// ── Manager ─────────────────────────────────────────────────────────
pub struct ApproachManager {
    phase: ApproachPhase,
    ascending: bool,
}

impl Default for ApproachManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ApproachManager {
    pub fn new() -> Self {
        Self {
            phase: ApproachPhase::Distant,
            ascending: false,
        }
    }

    pub fn phase(&self) -> ApproachPhase {
        self.phase
    }

    /// Tick the state machine. `find_planet` is the nearest planet as
    /// `(body_index, position_ly, radius_m)`.
    pub fn update(
        &mut self,
        camera_ly: WorldPos,
        find_planet: Option<(usize, WorldPos, f64)>,
        landing_state_landed: bool,
    ) -> ApproachState {
        let (body_index, planet_pos, radius_m, altitude_m) =
            if let Some((idx, pos, r)) = find_planet {
                let dist_ly = camera_ly.distance_to(pos);
                let dist_m = dist_ly * LY_TO_M;
                let alt = (dist_m - r).max(0.0);
                (Some(idx), Some(pos), r, alt)
            } else {
                self.phase = ApproachPhase::Distant;
                self.ascending = false;
                return self.build_state(
                    ApproachPhase::Distant, f64::MAX, None, 0.0, None,
                );
            };

        // Surface override
        if landing_state_landed {
            self.phase = ApproachPhase::Surface;
            self.ascending = false;
            return self.build_state(
                self.phase, altitude_m, planet_pos, radius_m, body_index,
            );
        }

        // Detect departure from surface
        if self.phase == ApproachPhase::Surface {
            self.ascending = true;
            self.phase = ApproachPhase::Departing;
        }

        // Phase computation
        let new_phase = if self.ascending {
            self.compute_phase_ascending(altitude_m, radius_m)
        } else {
            compute_phase(altitude_m, radius_m)
        };

        // Clear ascending flag once we reach Distant
        if new_phase == ApproachPhase::Distant {
            self.ascending = false;
        }

        self.phase = new_phase;
        self.build_state(self.phase, altitude_m, planet_pos, radius_m, body_index)
    }

    fn compute_phase_ascending(&self, altitude_m: f64, radius_m: f64) -> ApproachPhase {
        let ratio = altitude_m / radius_m;
        if ratio >= DEPART_APPROACHING {
            ApproachPhase::Distant
        } else if ratio >= DEPART_ORBIT {
            ApproachPhase::Approaching
        } else if ratio >= PHASE_UPPER_ATMO {
            ApproachPhase::Orbit
        } else if ratio >= PHASE_LOWER_ATMO {
            ApproachPhase::UpperAtmosphere
        } else if altitude_m >= PHASE_LANDING_M {
            ApproachPhase::LowerAtmosphere
        } else {
            ApproachPhase::Departing
        }
    }

    fn build_state(
        &self,
        phase: ApproachPhase,
        altitude_m: f64,
        planet_pos_ly: Option<WorldPos>,
        planet_radius_m: f64,
        body_index: Option<usize>,
    ) -> ApproachState {
        let has_planet = planet_pos_ly.is_some();
        use ApproachPhase::*;
        ApproachState {
            phase,
            altitude_m,
            planet_pos_ly,
            planet_radius_m,
            body_index,
            terrain_active: matches!(
                phase,
                Orbit | UpperAtmosphere | LowerAtmosphere
                    | Landing | Surface | Departing
            ),
            collision_active: matches!(
                phase,
                LowerAtmosphere | Landing | Surface | Departing
            ),
            disengage_cruise: has_planet && altitude_m <= safe_standoff_m(planet_radius_m),
            cascade_warp_to_cruise: has_planet
                && matches!(phase, Approaching | Orbit | UpperAtmosphere),
            can_engage_cruise: matches!(
                phase,
                Distant | Approaching | Orbit | UpperAtmosphere
            ),
            can_engage_warp: phase == Distant,
            cruise_speed_cap_ms: if has_planet {
                Some(cruise_speed_cap_ms(altitude_m, planet_radius_m))
            } else {
                None
            },
        }
    }
}

fn compute_phase(altitude_m: f64, radius_m: f64) -> ApproachPhase {
    let ratio = altitude_m / radius_m;
    if ratio > PHASE_APPROACHING {
        ApproachPhase::Distant
    } else if ratio > PHASE_ORBIT {
        ApproachPhase::Approaching
    } else if ratio > PHASE_UPPER_ATMO {
        ApproachPhase::Orbit
    } else if ratio > PHASE_LOWER_ATMO {
        ApproachPhase::UpperAtmosphere
    } else if altitude_m > PHASE_LANDING_M {
        ApproachPhase::LowerAtmosphere
    } else {
        ApproachPhase::Landing
    }
}

// ── Public helpers ──────────────────────────────────────────────────

/// Speed cap proportional to altitude. Returns 0 below safe standoff.
pub fn cruise_speed_cap_ms(altitude_m: f64, body_radius_m: f64) -> f64 {
    if altitude_m <= safe_standoff_m(body_radius_m) {
        return 0.0;
    }
    altitude_m / APPROACH_TIME_SECONDS
}

/// Analytical ray-sphere intersection. Returns `Some(t)` when the ray
/// from `origin` along `delta` first enters the sphere at parameter
/// `0 < t < 1`.
pub fn ray_sphere_intersect(
    origin: [f64; 3],
    delta: [f64; 3],
    center: [f64; 3],
    radius: f64,
) -> Option<f64> {
    let oc = [
        origin[0] - center[0],
        origin[1] - center[1],
        origin[2] - center[2],
    ];
    let a = dot(delta, delta);
    let b = 2.0 * dot(oc, delta);
    let c = dot(oc, oc) - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    if t > 0.0 && t < 1.0 {
        Some(t)
    } else {
        None
    }
}

/// Clamp a cruise movement delta so it never crosses a planet
/// exclusion sphere. Returns the (possibly shortened) delta and
/// whether clamping occurred.
pub fn clamp_cruise_delta(
    origin_ly: [f64; 3],
    delta_ly: [f64; 3],
    planets: &[(WorldPos, f64)],
) -> ([f64; 3], bool) {
    // Already inside any exclusion sphere → stop.
    for &(pos, radius_m) in planets {
        let excl = (radius_m + safe_standoff_m(radius_m)) / LY_TO_M;
        let oc = [
            origin_ly[0] - pos.x,
            origin_ly[1] - pos.y,
            origin_ly[2] - pos.z,
        ];
        if dot(oc, oc) < excl * excl {
            return ([0.0, 0.0, 0.0], true);
        }
    }

    let mut best_t: Option<f64> = None;
    for &(pos, radius_m) in planets {
        let excl = (radius_m + safe_standoff_m(radius_m)) / LY_TO_M;
        let center = [pos.x, pos.y, pos.z];
        if let Some(t) = ray_sphere_intersect(origin_ly, delta_ly, center, excl) {
            best_t = Some(match best_t {
                Some(prev) => prev.min(t),
                None => t,
            });
        }
    }

    if let Some(t) = best_t {
        let s = t * 0.99;
        (
            [delta_ly[0] * s, delta_ly[1] * s, delta_ly[2] * s],
            true,
        )
    } else {
        (delta_ly, false)
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ── Tests ───────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn earth_radius() -> f64 {
        6_371_000.0
    }

    #[test]
    fn phase_distant() {
        let p = compute_phase(earth_radius() * 60.0, earth_radius());
        assert_eq!(p, ApproachPhase::Distant);
    }

    #[test]
    fn phase_approaching() {
        let p = compute_phase(earth_radius() * 30.0, earth_radius());
        assert_eq!(p, ApproachPhase::Approaching);
    }

    #[test]
    fn phase_orbit() {
        let p = compute_phase(earth_radius() * 3.0, earth_radius());
        assert_eq!(p, ApproachPhase::Orbit);
    }

    #[test]
    fn phase_upper_atmo() {
        let p = compute_phase(earth_radius() * 1.0, earth_radius());
        assert_eq!(p, ApproachPhase::UpperAtmosphere);
    }

    #[test]
    fn phase_lower_atmo() {
        let p = compute_phase(earth_radius() * 0.1, earth_radius());
        assert_eq!(p, ApproachPhase::LowerAtmosphere);
    }

    #[test]
    fn phase_landing() {
        let p = compute_phase(300.0, earth_radius());
        assert_eq!(p, ApproachPhase::Landing);
    }

    #[test]
    fn hysteresis_orbit_to_approaching() {
        // At 5.5× radius, descent says Orbit but ascent also says Orbit
        // because ascent threshold for Approaching is 6×.
        let mut mgr = ApproachManager::new();
        mgr.phase = ApproachPhase::Departing;
        mgr.ascending = true;
        let ratio = 5.5;
        let p = mgr.compute_phase_ascending(earth_radius() * ratio, earth_radius());
        assert_eq!(p, ApproachPhase::Orbit);

        // At 6.5× ascent gives Approaching
        let p2 = mgr.compute_phase_ascending(earth_radius() * 6.5, earth_radius());
        assert_eq!(p2, ApproachPhase::Approaching);
    }

    #[test]
    fn cruise_cap_proportional() {
        let alt = 800_000.0;
        // Use a small planet radius so 100km standoff applies
        let cap = cruise_speed_cap_ms(alt, 6_371_000.0);
        assert!((cap - alt / APPROACH_TIME_SECONDS).abs() < 1.0);
    }

    #[test]
    fn cruise_cap_zero_below_standoff() {
        // Planet (< 100,000 km radius): standoff = 100 km
        let r = 6_371_000.0;
        assert_eq!(cruise_speed_cap_ms(50_000.0, r), 0.0);
        assert_eq!(cruise_speed_cap_ms(100_000.0, r), 0.0);
        // Above 100km: should have speed
        assert!(cruise_speed_cap_ms(200_000.0, r) > 0.0);
        // Star (≥ 100,000 km radius): standoff = 0.5 × radius
        let star_r = 427_926_000.0;
        assert_eq!(cruise_speed_cap_ms(200_000_000.0, star_r), 0.0);
    }

    #[test]
    fn ray_sphere_hit() {
        let origin = [0.0, 0.0, -10.0];
        let delta = [0.0, 0.0, 20.0];
        let center = [0.0, 0.0, 5.0];
        let radius = 2.0;
        let t = ray_sphere_intersect(origin, delta, center, radius);
        assert!(t.is_some());
        let t = t.unwrap();
        assert!(t > 0.0 && t < 1.0);
    }

    #[test]
    fn ray_sphere_miss_parallel() {
        let origin = [5.0, 0.0, 0.0];
        let delta = [0.0, 0.0, 10.0];
        let center = [0.0, 0.0, 5.0];
        let radius = 2.0;
        assert!(ray_sphere_intersect(origin, delta, center, radius).is_none());
    }

    #[test]
    fn ray_sphere_miss_moving_away() {
        let origin = [0.0, 0.0, -10.0];
        let delta = [0.0, 0.0, -5.0]; // moving away
        let center = [0.0, 0.0, 5.0];
        let radius = 2.0;
        assert!(ray_sphere_intersect(origin, delta, center, radius).is_none());
    }

    #[test]
    fn clamp_delta_truncates() {
        let origin = [0.0, 0.0, 0.0];
        let delta = [10.0, 0.0, 0.0];
        let planet_pos = WorldPos::new(5.0, 0.0, 0.0);
        let radius_m = 0.0; // exclusion = EXCLUSION_RADIUS_M / LY_TO_M
        let (clamped, hit) =
            clamp_cruise_delta(origin, delta, &[(planet_pos, radius_m)]);
        assert!(hit);
        // Clamped delta should be shorter than original
        let len = (clamped[0] * clamped[0]).sqrt();
        assert!(len < 10.0);
    }

    #[test]
    fn clamp_delta_inside_sphere() {
        let planet_pos = WorldPos::new(0.0, 0.0, 0.0);
        let radius_m = LY_TO_M * 10.0; // huge sphere
        let (d, hit) =
            clamp_cruise_delta([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], &[(planet_pos, radius_m)]);
        assert!(hit);
        assert_eq!(d, [0.0, 0.0, 0.0]);
    }
}
