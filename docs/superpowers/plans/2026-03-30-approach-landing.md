# Planet Approach, Landing & Departure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unified approach state machine that manages the complete planet approach pipeline — from "dot in the distance" to "walking on the surface" — with flythrough prevention, smooth deceleration, and seamless terrain streaming.

**Architecture:** A new `approach.rs` module owns all phase decisions (distance-based state machine). Other systems (helm_mode, render_frame, terrain_integration) become consumers that query `ApproachState` instead of doing their own proximity checks. Ray-sphere intersection prevents flythrough at any cruise speed. Speed cap proportional to altitude replaces piecewise deceleration.

**Tech Stack:** Rust, wgpu, rapier3d, sa_terrain, sa_ship

**Spec:** `docs/superpowers/specs/2026-03-30-approach-landing-design.md`

---

## File Map

### New Files
- `crates/spaceaway/src/approach.rs` — ApproachManager, ApproachPhase, ApproachState, ray-sphere test, speed cap

### Modified Files
- `crates/spaceaway/src/main.rs:79-198` — add `approach: ApproachManager` field to App
- `crates/spaceaway/src/helm_mode.rs` — remove scattered planet checks, use ApproachState
- `crates/spaceaway/src/drive_integration.rs` — remove `cruise_deceleration()`, `CRUISE_DISENGAGE_LY`
- `crates/spaceaway/src/render_frame.rs:65-132` — terrain activation uses ApproachState
- `crates/spaceaway/src/terrain_integration.rs:25-30,312-326,433-492` — remove find_terrain_planet, simplify should_deactivate

---

## Task 1: Create ApproachManager Core

**Files:**
- Create: `crates/spaceaway/src/approach.rs`

- [ ] **Step 1: Write tests for phase transitions**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn earth_like() -> (f64, f64) {
        // (planet_radius_m, planet_distance_m_from_camera)
        (6_371_000.0, 6_371_000.0) // radius in meters
    }

    #[test]
    fn distant_phase_far_from_planet() {
        let mut mgr = ApproachManager::new();
        let radius = 6_371_000.0;
        let altitude = radius * 100.0; // 100× radius
        let phase = mgr.compute_phase(altitude, radius);
        assert_eq!(phase, ApproachPhase::Distant);
    }

    #[test]
    fn approaching_phase_at_30x_radius() {
        let mut mgr = ApproachManager::new();
        let radius = 6_371_000.0;
        let altitude = radius * 30.0;
        let phase = mgr.compute_phase(altitude, radius);
        assert_eq!(phase, ApproachPhase::Approaching);
    }

    #[test]
    fn orbit_phase_at_3x_radius() {
        let mut mgr = ApproachManager::new();
        let radius = 6_371_000.0;
        let altitude = radius * 3.0;
        let phase = mgr.compute_phase(altitude, radius);
        assert_eq!(phase, ApproachPhase::Orbit);
    }

    #[test]
    fn upper_atmosphere_at_1x_radius() {
        let mut mgr = ApproachManager::new();
        let radius = 6_371_000.0;
        let altitude = radius * 1.0;
        let phase = mgr.compute_phase(altitude, radius);
        assert_eq!(phase, ApproachPhase::UpperAtmosphere);
    }

    #[test]
    fn lower_atmosphere_at_1km() {
        let mut mgr = ApproachManager::new();
        let radius = 6_371_000.0;
        let altitude = 1_000.0; // 1km
        let phase = mgr.compute_phase(altitude, radius);
        assert_eq!(phase, ApproachPhase::LowerAtmosphere);
    }

    #[test]
    fn landing_phase_at_200m() {
        let mut mgr = ApproachManager::new();
        let radius = 6_371_000.0;
        let altitude = 200.0;
        let phase = mgr.compute_phase(altitude, radius);
        assert_eq!(phase, ApproachPhase::Landing);
    }

    #[test]
    fn cruise_speed_cap_proportional_to_altitude() {
        let cap_high = cruise_speed_cap_ms(1_000_000.0); // 1000km
        let cap_low = cruise_speed_cap_ms(100_000.0); // 100km
        assert!(cap_high > cap_low);
        // speed = altitude / 8.0
        assert!((cap_high - 1_000_000.0 / 8.0).abs() < 1.0);
        assert!((cap_low - 100_000.0 / 8.0).abs() < 1.0);
    }

    #[test]
    fn cruise_speed_cap_zero_at_disengage_altitude() {
        let cap = cruise_speed_cap_ms(50_000.0); // 50km, below 100km disengage
        assert_eq!(cap, 0.0);
    }

    #[test]
    fn ray_sphere_detects_crossing() {
        // Ship at 200km, moving toward planet center, sphere at 100km
        let origin = [0.0, 0.0, 200_000.0]; // 200km from center on Z
        let delta = [0.0, 0.0, -300_000.0]; // moving 300km toward center
        let sphere_center = [0.0, 0.0, 0.0];
        let sphere_radius = 100_000.0; // 100km

        let t = ray_sphere_intersect(origin, delta, sphere_center, sphere_radius);
        assert!(t.is_some());
        let t = t.unwrap();
        // Should hit at ~100km from center = at t where z = 100km
        // origin.z + delta.z * t = 100km → 200k + (-300k)*t = 100k → t = 1/3
        assert!((t - 1.0/3.0).abs() < 0.01);
    }

    #[test]
    fn ray_sphere_misses_when_parallel() {
        let origin = [200_000.0, 0.0, 0.0]; // 200km on X
        let delta = [0.0, 0.0, -300_000.0]; // moving along Z, misses sphere
        let sphere_center = [0.0, 0.0, 0.0];
        let sphere_radius = 100_000.0;

        let t = ray_sphere_intersect(origin, delta, sphere_center, sphere_radius);
        assert!(t.is_none());
    }

    #[test]
    fn ray_sphere_no_hit_when_moving_away() {
        let origin = [0.0, 0.0, 200_000.0];
        let delta = [0.0, 0.0, 100_000.0]; // moving AWAY from planet
        let sphere_center = [0.0, 0.0, 0.0];
        let sphere_radius = 100_000.0;

        let t = ray_sphere_intersect(origin, delta, sphere_center, sphere_radius);
        assert!(t.is_none()); // t would be negative = behind us
    }
}
```

- [ ] **Step 2: Implement ApproachManager**

```rust
//! Unified planet approach state machine.
//!
//! Single source of truth for planet proximity. All systems (helm_mode,
//! render_frame, terrain_integration) query ApproachState instead of
//! doing their own distance checks.

use sa_math::WorldPos;

/// Light-years to meters.
const LY_TO_M: f64 = 9.461e15;

// -- Phase thresholds (multiples of planet radius) --
const PHASE_APPROACHING: f64 = 50.0;
const PHASE_ORBIT: f64 = 5.0;
const PHASE_UPPER_ATMO: f64 = 2.0;
const PHASE_LOWER_ATMO: f64 = 0.2;
const PHASE_LANDING_M: f64 = 500.0; // meters, not radius multiple

// Hysteresis (departure thresholds — must be > approach thresholds)
const DEPART_APPROACHING: f64 = 60.0;
const DEPART_ORBIT: f64 = 6.0;

// Cruise deceleration
const APPROACH_TIME_SECONDS: f64 = 8.0;
const CRUISE_DISENGAGE_ALT_M: f64 = 100_000.0; // 100km
const EXCLUSION_RADIUS_M: f64 = 100_000.0; // 100km above surface

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

/// Read-only snapshot of approach state for other systems.
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

pub struct ApproachManager {
    phase: ApproachPhase,
    planet_pos_ly: Option<WorldPos>,
    planet_radius_m: Option<f64>,
    altitude_m: f64,
    body_index: Option<usize>,
    ascending: bool, // true during departure
}

impl ApproachManager {
    pub fn new() -> Self {
        Self {
            phase: ApproachPhase::Distant,
            planet_pos_ly: None,
            planet_radius_m: None,
            altitude_m: f64::MAX,
            body_index: None,
            ascending: false,
        }
    }

    /// Update approach state for the current frame.
    /// `find_planet`: closure that returns (body_index, pos_ly, radius_m, surface_gravity)
    /// for the nearest landable planet, or None.
    pub fn update(
        &mut self,
        camera_ly: WorldPos,
        find_planet: Option<(usize, WorldPos, f64)>,
        landing_state_landed: bool,
    ) -> ApproachState {
        // Update planet info
        if let Some((idx, pos, radius)) = find_planet {
            self.body_index = Some(idx);
            self.planet_pos_ly = Some(pos);
            self.planet_radius_m = Some(radius);

            let dx = (camera_ly.x - pos.x) * LY_TO_M;
            let dy = (camera_ly.y - pos.y) * LY_TO_M;
            let dz = (camera_ly.z - pos.z) * LY_TO_M;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            self.altitude_m = dist - radius;
        } else {
            self.body_index = None;
            self.planet_pos_ly = None;
            self.planet_radius_m = None;
            self.altitude_m = f64::MAX;
        }

        let radius = self.planet_radius_m.unwrap_or(1.0);

        // Track ascent/descent
        if landing_state_landed {
            self.phase = ApproachPhase::Surface;
            self.ascending = false;
        } else if self.phase == ApproachPhase::Surface {
            self.phase = ApproachPhase::Departing;
            self.ascending = true;
        }

        // Phase transitions
        if self.planet_radius_m.is_some() {
            let new_phase = if self.ascending {
                self.compute_phase_ascending(self.altitude_m, radius)
            } else {
                self.compute_phase(self.altitude_m, radius)
            };
            // Departing transitions to LowerAtmosphere once airborne
            if self.phase == ApproachPhase::Departing
                && new_phase != ApproachPhase::Surface
            {
                self.phase = new_phase;
                if matches!(self.phase,
                    ApproachPhase::Distant | ApproachPhase::Approaching)
                {
                    self.ascending = false; // done departing
                }
            } else if self.phase != ApproachPhase::Surface
                && self.phase != ApproachPhase::Departing
            {
                self.phase = new_phase;
            }
        } else {
            self.phase = ApproachPhase::Distant;
            self.ascending = false;
        }

        // Compute derived state
        let terrain_active = matches!(self.phase,
            ApproachPhase::Orbit
            | ApproachPhase::UpperAtmosphere
            | ApproachPhase::LowerAtmosphere
            | ApproachPhase::Landing
            | ApproachPhase::Surface
            | ApproachPhase::Departing
        );
        let collision_active = matches!(self.phase,
            ApproachPhase::LowerAtmosphere
            | ApproachPhase::Landing
            | ApproachPhase::Surface
            | ApproachPhase::Departing
        );
        let disengage_cruise = self.planet_radius_m.is_some()
            && self.altitude_m <= CRUISE_DISENGAGE_ALT_M;
        let cascade_warp_to_cruise = self.planet_radius_m.is_some()
            && matches!(self.phase, ApproachPhase::Approaching
                | ApproachPhase::Orbit
                | ApproachPhase::UpperAtmosphere);

        let can_engage_cruise = matches!(self.phase,
            ApproachPhase::Distant
            | ApproachPhase::Approaching
            | ApproachPhase::Orbit
            | ApproachPhase::UpperAtmosphere
        );
        let can_engage_warp = self.phase == ApproachPhase::Distant;

        let cruise_speed_cap_ms = if self.planet_radius_m.is_some() {
            Some(cruise_speed_cap_ms(self.altitude_m))
        } else {
            None
        };

        ApproachState {
            phase: self.phase,
            altitude_m: self.altitude_m,
            planet_pos_ly: self.planet_pos_ly,
            planet_radius_m: radius,
            body_index: self.body_index,
            terrain_active,
            collision_active,
            disengage_cruise,
            cascade_warp_to_cruise,
            can_engage_cruise,
            can_engage_warp,
            cruise_speed_cap_ms,
        }
    }

    /// Compute phase for descent (approach).
    pub fn compute_phase(&mut self, altitude_m: f64, radius_m: f64) -> ApproachPhase {
        if altitude_m > radius_m * PHASE_APPROACHING {
            ApproachPhase::Distant
        } else if altitude_m > radius_m * PHASE_ORBIT {
            ApproachPhase::Approaching
        } else if altitude_m > radius_m * PHASE_UPPER_ATMO {
            ApproachPhase::Orbit
        } else if altitude_m > radius_m * PHASE_LOWER_ATMO {
            ApproachPhase::UpperAtmosphere
        } else if altitude_m > PHASE_LANDING_M {
            ApproachPhase::LowerAtmosphere
        } else {
            ApproachPhase::Landing
        }
    }

    /// Compute phase for ascent (departure) with hysteresis.
    fn compute_phase_ascending(&self, altitude_m: f64, radius_m: f64) -> ApproachPhase {
        if altitude_m > radius_m * DEPART_APPROACHING {
            ApproachPhase::Distant
        } else if altitude_m > radius_m * DEPART_ORBIT {
            ApproachPhase::Approaching
        } else if altitude_m > radius_m * PHASE_UPPER_ATMO {
            ApproachPhase::Orbit
        } else if altitude_m > radius_m * PHASE_LOWER_ATMO {
            ApproachPhase::UpperAtmosphere
        } else if altitude_m > PHASE_LANDING_M {
            ApproachPhase::LowerAtmosphere
        } else {
            ApproachPhase::Landing
        }
    }

    pub fn phase(&self) -> ApproachPhase {
        self.phase
    }
}

/// Cruise speed cap: speed proportional to altitude.
/// Returns 0.0 below the disengage altitude (100km).
pub fn cruise_speed_cap_ms(altitude_m: f64) -> f64 {
    if altitude_m <= CRUISE_DISENGAGE_ALT_M {
        return 0.0;
    }
    altitude_m / APPROACH_TIME_SECONDS
}

/// Analytical ray-sphere intersection.
/// Returns the t parameter (0..1) where the ray enters the sphere,
/// or None if no intersection in the forward direction.
///
/// `origin`: ray start position
/// `delta`: ray direction (unnormalized, full frame movement)
/// `center`: sphere center
/// `radius`: sphere radius
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
    let a = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
    if a < 1e-30 {
        return None; // zero-length ray
    }
    let b = 2.0 * (oc[0] * delta[0] + oc[1] * delta[1] + oc[2] * delta[2]);
    let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None; // no intersection
    }
    let sqrt_disc = discriminant.sqrt();
    let t1 = (-b - sqrt_disc) / (2.0 * a);
    // t1 is the entry point. Only valid if 0 < t1 < 1 (crosses during this frame).
    if t1 > 0.0 && t1 < 1.0 {
        Some(t1)
    } else {
        None
    }
}

/// Test all planets in the system and clamp cruise delta if it would cross
/// any planet's exclusion sphere.
/// Returns the clamped delta and whether cruise should disengage.
pub fn clamp_cruise_delta(
    origin_ly: WorldPos,
    delta_ly: [f64; 3],
    planets: &[(WorldPos, f64)], // (pos_ly, radius_m) for each planet
) -> ([f64; 3], bool) {
    let mut best_t: f64 = 1.0;
    let mut should_disengage = false;

    for &(planet_pos, planet_radius_m) in planets {
        let exclusion_ly = (planet_radius_m + EXCLUSION_RADIUS_M) / LY_TO_M;
        let origin = [origin_ly.x, origin_ly.y, origin_ly.z];
        let center = [planet_pos.x, planet_pos.y, planet_pos.z];

        // Check if already inside exclusion sphere
        let dx = origin[0] - center[0];
        let dy = origin[1] - center[1];
        let dz = origin[2] - center[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        if dist < exclusion_ly {
            return ([0.0, 0.0, 0.0], true); // already inside, stop
        }

        if let Some(t) = ray_sphere_intersect(origin, delta_ly, center, exclusion_ly) {
            if t < best_t {
                best_t = t;
                should_disengage = true;
            }
        }
    }

    if should_disengage {
        let clamped_t = best_t * 0.99; // stop just outside
        let clamped = [
            delta_ly[0] * clamped_t,
            delta_ly[1] * clamped_t,
            delta_ly[2] * clamped_t,
        ];
        (clamped, true)
    } else {
        (delta_ly, false)
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p spaceaway --lib approach`
Expected: All tests pass.

- [ ] **Step 4: Add module to main.rs**

Add `pub mod approach;` to `crates/spaceaway/src/main.rs` module declarations.
Add `approach: crate::approach::ApproachManager` field to the App struct.
Initialize with `approach: crate::approach::ApproachManager::new()` in App::new().

- [ ] **Step 5: Run check and commit**

```bash
cargo check -p spaceaway
cargo clippy -p spaceaway -- -D warnings
git add crates/spaceaway/src/approach.rs crates/spaceaway/src/main.rs
git commit -m "feat: add ApproachManager with phase state machine and ray-sphere flythrough prevention"
```

---

## Task 2: Wire ApproachManager into the Frame Loop

**Files:**
- Modify: `crates/spaceaway/src/render_frame.rs:65-132`
- Modify: `crates/spaceaway/src/terrain_integration.rs:25-30,312-326,433-492`

- [ ] **Step 1: Call approach.update() each frame in render_frame.rs**

At the top of the render/update function (before terrain checks), add:

```rust
// Compute approach state for this frame
let find_planet = self.active_system.as_ref().and_then(|sys| {
    let positions = sys.compute_positions_ly_pub();
    let ly_to_m = 9.461e15_f64;
    let mut best: Option<(usize, WorldPos, f64, f64)> = None;
    for (i, pos) in positions.iter().enumerate() {
        let r = sys.body_radius_m(i)?;
        if sys.planet_data(i).is_none() { continue; }
        let dx = (self.galactic_position.x - pos.x) * ly_to_m;
        let dy = (self.galactic_position.y - pos.y) * ly_to_m;
        let dz = (self.galactic_position.z - pos.z) * ly_to_m;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        if best.is_none_or(|b| dist < b.3) {
            best = Some((i, *pos, r, dist));
        }
    }
    best.map(|(i, pos, r, _)| (i, pos, r))
});
let landed = self.landing.as_ref().map_or(false, |l| l.state() == landing::LandingState::Landed);
let approach_state = self.approach.update(self.galactic_position, find_planet, landed);
```

- [ ] **Step 2: Replace terrain activation with approach_state**

Replace the `find_terrain_planet()` call and terrain activation block (lines 65-132) with:

```rust
// Terrain activation driven by approach state
if approach_state.terrain_active && self.terrain.is_none() {
    if let Some(planet_pos) = approach_state.planet_pos_ly {
        if let Some(body_idx) = approach_state.body_index {
            if let Some(sys) = &self.active_system {
                if let Some((color_seed, sub_type, disp_frac, mass_e, radius_e)) =
                    sys.planet_data(body_idx)
                {
                    let config = sa_terrain::TerrainConfig {
                        radius_m: approach_state.planet_radius_m,
                        noise_seed: color_seed,
                        sub_type,
                        displacement_fraction: disp_frac,
                    };
                    let surface_grav = sa_terrain::gravity::surface_gravity(mass_e, radius_e);
                    // ... existing TerrainManager creation, seed_base_chunks, rebase logic
                }
            }
        }
    }
}
```

- [ ] **Step 3: Replace terrain deactivation**

Replace `should_deactivate()` check with:

```rust
if !approach_state.terrain_active && self.terrain.is_some() {
    // deactivate terrain (existing cleanup logic)
}
```

- [ ] **Step 4: Remove find_terrain_planet() from terrain_integration.rs**

Delete the `find_terrain_planet()` function (lines 433-492) and the `ACTIVATE_RADIUS_MULT` / `DEACTIVATE_RADIUS_MULT` constants (lines 25, 30). Simplify `should_deactivate()` to just return false (approach manager owns deactivation now), or remove it entirely and let render_frame check `approach_state.terrain_active`.

Also remove the `is_landable()` helper if nothing else uses it.

- [ ] **Step 5: Remove drive auto-disengage from render_frame.rs**

Remove the warp auto-disengage on terrain activation block (lines 125-131). The approach manager handles drive cascade.

- [ ] **Step 6: Build and test**

```bash
cargo check -p spaceaway
cargo clippy -p spaceaway -- -D warnings
cargo test -p spaceaway --lib
git add -u
git commit -m "feat: wire ApproachManager into render_frame, replace terrain activation logic"
```

---

## Task 3: Wire Cruise Deceleration and Flythrough Prevention

**Files:**
- Modify: `crates/spaceaway/src/helm_mode.rs:396-553`
- Modify: `crates/spaceaway/src/drive_integration.rs:184-209`

- [ ] **Step 1: Replace target_dist and cruise movement in helm_mode.rs**

Replace the entire cruise/warp movement block (lines 396-553) with approach-manager-driven logic. The key changes:

1. For cruise mode, apply speed cap from approach_state:
```rust
let (mut delta, effective_speed) = drive_integration::galactic_position_delta_decel(
    &self.drive, direction, dt as f64, target_dist,
);

// Apply approach-manager cruise speed cap
if self.drive.mode() == sa_ship::DriveMode::Cruise {
    if let Some(cap_ms) = approach_state.cruise_speed_cap_ms {
        let cap_ly_s = cap_ms / 9.461e15;
        let max_delta = cap_ly_s * dt as f64;
        let delta_len = (delta[0]*delta[0] + delta[1]*delta[1] + delta[2]*delta[2]).sqrt();
        if delta_len > max_delta && delta_len > 1e-30 {
            let scale = max_delta / delta_len;
            delta[0] *= scale;
            delta[1] *= scale;
            delta[2] *= scale;
        }
    }
}
```

2. For cruise mode, apply ray-sphere flythrough prevention:
```rust
if self.drive.mode() == sa_ship::DriveMode::Cruise {
    let planets: Vec<(sa_math::WorldPos, f64)> = approach_state.planet_pos_ly
        .iter()
        .map(|pos| (*pos, approach_state.planet_radius_m))
        .collect();
    if !planets.is_empty() {
        let (clamped, should_disengage) = crate::approach::clamp_cruise_delta(
            self.galactic_position, delta, &planets,
        );
        delta = clamped;
        if should_disengage {
            self.drive.request_disengage();
        }
    }
}
```

3. Apply delta:
```rust
self.galactic_position.x += delta[0];
self.galactic_position.y += delta[1];
self.galactic_position.z += delta[2];
```

4. Check approach_state for cruise disengage:
```rust
if approach_state.disengage_cruise && self.drive.mode() == sa_ship::DriveMode::Cruise {
    self.drive.request_disengage();
    log::info!("Cruise auto-disengage: {:.0}km above surface",
        approach_state.altitude_m / 1000.0);
}
```

- [ ] **Step 2: Remove old planet detection functions from helm_mode.rs**

Delete `nearest_planet_info()` (lines 783-803) and `nearest_planet_altitude_ly()` (lines 771-780).

Delete the old flythrough binary search block, planet boundary clamp, and cascade cruise disengage logic that was replaced in Step 1.

- [ ] **Step 3: Add warp → cruise cascade**

In the warp auto-disengage section, after `self.drive.request_disengage()`:

```rust
if approach_state.cascade_warp_to_cruise {
    self.drive.request_engage(sa_ship::DriveMode::Cruise);
    self.audio.announce(sa_audio::VoiceId::AllSystemsReady);
    log::info!("Warp → cruise cascade: approaching planet");
}
```

- [ ] **Step 4: Wire drive engagement to approach_state**

Replace the cruise engagement check (around line 30) with:

```rust
KeyCode::Digit2 => {
    if approach_state.can_engage_cruise {
        // existing engagement logic
    }
}
```

Replace the warp engagement check (around line 171) with:

```rust
if approach_state.can_engage_warp {
    // existing warp spool logic
}
```

- [ ] **Step 5: Remove cruise_deceleration from drive_integration.rs**

Delete `cruise_deceleration()` (lines 189-209) and `CRUISE_DISENGAGE_LY` (line 184). Keep `warp_deceleration()`, `WARP_DISENGAGE_LY`, and `galactic_position_delta_decel()`.

In `galactic_position_delta_decel()`, remove the `DriveMode::Cruise => cruise_deceleration(d)` branch. Cruise deceleration is now handled by the speed cap in helm_mode. Replace with `DriveMode::Cruise => 1.0` (no deceleration at the delta level — speed cap handles it).

- [ ] **Step 6: Pass approach_state to helm_update**

The `approach_state` needs to be accessible in `helm_update()`. Either:
- Compute it in render_frame and pass it as a parameter to helm_update
- Or store it on App and update it before helm_update runs

Choose whichever matches the existing pattern (check how other per-frame state is passed).

- [ ] **Step 7: Build, test, and commit**

```bash
cargo check -p spaceaway
cargo clippy -p spaceaway -- -D warnings
cargo test --workspace
git add -u
git commit -m "feat: wire cruise deceleration and flythrough prevention via ApproachManager"
```

---

## Task 4: Wire Collision Grid Activation

**Files:**
- Modify: `crates/spaceaway/src/terrain_integration.rs`
- Modify: `crates/spaceaway/src/render_frame.rs`

- [ ] **Step 1: Pass collision_active to terrain update**

In the terrain manager update call, use `approach_state.collision_active` to control collision grid activation instead of the current altitude check.

In `terrain_integration.rs`, the collision grid activation (around line 210) should use a flag passed from render_frame:

```rust
// In TerrainManager::update(), add collision_active parameter
if collision_active {
    self.col.update_collision_grid(cam_rel_m, &self.config, physics, rebase_bodies);
}
```

- [ ] **Step 2: Force synchronous collision on LowerAtmosphere entry**

When approach transitions from UpperAtmosphere to LowerAtmosphere (first frame of collision_active), force the collision grid's first update to be synchronous. Add a `first_collision_frame` flag:

```rust
let first_collision = approach_state.collision_active
    && self.terrain.as_ref().map_or(true, |t| !t.collision_was_active());
```

Pass this to the terrain manager so it can call the collision grid synchronously on the first frame.

- [ ] **Step 3: Force anchor rebase on LowerAtmosphere entry**

When collision first activates, force an immediate anchor rebase:

```rust
if first_collision {
    // Force rebase to current ship position
    self.col.force_rebase(physics, rebase_bodies);
}
```

- [ ] **Step 4: Build and commit**

```bash
cargo check -p spaceaway
cargo clippy -p spaceaway -- -D warnings
git add -u
git commit -m "feat: collision grid activation driven by ApproachManager phases"
```

---

## Task 5: Cleanup and Final Integration

**Files:**
- Modify: `crates/spaceaway/src/helm_mode.rs` — remove dead code
- Modify: `crates/spaceaway/src/drive_integration.rs` — remove dead code
- Modify: `crates/spaceaway/src/terrain_integration.rs` — remove dead code

- [ ] **Step 1: Remove all dead code**

- Delete any remaining `nearest_planet_info`, `nearest_planet_altitude_ly` references
- Delete unused imports
- Delete `ACTIVATE_RADIUS_MULT`, `DEACTIVATE_RADIUS_MULT` if still present
- Delete `cruise_deceleration()` if still present
- Delete `CRUISE_DISENGAGE_LY` if still present
- Delete `find_terrain_planet()` if still present
- Delete any unused test functions that tested removed code

- [ ] **Step 2: Update cruise deceleration test**

In drive_integration.rs tests, remove or update the cruise_deceleration test since the function no longer exists. Replace with a test for the approach manager's speed cap if not already covered.

- [ ] **Step 3: Run full test suite**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Visual testing**

Run `cargo run -p spaceaway` and test:
1. **Warp to star system** → warp auto-cascades to cruise (voice announces)
2. **Cruise toward planet** → smooth deceleration, planet grows steadily over ~1 minute
3. **At 100km** → cruise auto-disengages, ship at zero velocity
4. **Impulse descent** → gravity pulls ship, terrain streams, collision grid active
5. **Landing** → skids touch, FLYING → SLIDING → LANDED
6. **Departure** → throttle up, ascend, re-engage cruise at 2× radius, warp at 50× radius
7. **At 5000c cruise** → ray-sphere prevents flythrough, ship stops at 100km
8. **At 20c cruise** → smooth approach, no early disengage at 87K km

- [ ] **Step 5: Commit cleanup**

```bash
git add -u
git commit -m "chore: remove dead approach logic from helm_mode, drive_integration, terrain_integration"
```
