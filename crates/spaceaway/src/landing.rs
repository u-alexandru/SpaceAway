//! Landing state machine for planet surface operations.
//!
//! Tracks FLYING / SLIDING / LANDED states using 4-point raycasting from
//! landing skid positions against TERRAIN colliders (GROUP_5).
//!
//! # State transitions
//! - FLYING  → SLIDING : any skid clearance < 1.0 m
//! - SLIDING → LANDED  : player requests lock AND speed < 5 m/s
//! - LANDED  → SLIDING : player requests unlock
//! - SLIDING → FLYING  : all clearances > 10 m AND engine on AND throttle > 0
//! - Any     → FLYING  : terrain_active becomes false

use nalgebra::{Isometry3, Point3, Unit, Vector3};
use rapier3d::prelude::{Group, InteractionGroups, QueryFilter};
use sa_physics::PhysicsWorld;
use sa_ship::Ship;

use spaceaway::ship_colliders::TERRAIN;

use crate::constants::{
    FLYING_THRESHOLD_M, IMPACT_CLEAN_MS, IMPACT_MAJOR_MS, IMPACT_MINOR_MS,
    LANDING_RAY_DIST_M, LOCK_SPEED_THRESHOLD_MS, SLIDING_THRESHOLD_M,
};

// ---------------------------------------------------------------------------
// Public types — kept at top of file per convention
// ---------------------------------------------------------------------------

/// Classification of impact speed at the moment of touchdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactCategory {
    /// < 10 m/s — gentle landing, no damage.
    Clean,
    /// 10–30 m/s — bumpy, minor structural stress.
    Minor,
    /// 30–80 m/s — hard landing, possible damage.
    Major,
    /// > 80 m/s — catastrophic.
    Destroyed,
}

impl ImpactCategory {
    /// Classify a combined impact speed (m/s).
    pub fn from_speed(speed_ms: f32) -> Self {
        if speed_ms < IMPACT_CLEAN_MS {
            ImpactCategory::Clean
        } else if speed_ms < IMPACT_MINOR_MS {
            ImpactCategory::Minor
        } else if speed_ms < IMPACT_MAJOR_MS {
            ImpactCategory::Major
        } else {
            ImpactCategory::Destroyed
        }
    }
}

/// Event emitted the moment the ship transitions FLYING → SLIDING (first ground contact).
#[derive(Debug, Clone)]
pub struct LandingImpactEvent {
    /// Vertical speed (component along gravity direction) at the moment of touchdown (m/s).
    pub impact_speed_ms: f32,
    /// Speed contribution measured at each individual skid (m/s).
    // Stored for future use by audio/damage systems (Task 8+).
    #[allow(dead_code)]
    pub per_skid_speeds: [f32; 4],
    /// Planet surface gravity magnitude (m/s²).
    // Stored for future use by audio/damage systems (Task 8+).
    #[allow(dead_code)]
    pub planet_gravity: f32,
    /// Severity classification of the impact.
    pub category: ImpactCategory,
}

/// Current landing state of the ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingState {
    /// Airborne — no skid is near the terrain.
    Flying,
    /// At least one skid is within `SLIDING_THRESHOLD_M` of terrain.
    /// The ship may be rolling/sliding on the surface.
    Sliding,
    /// Player has locked the ship down; ship is stationary on terrain.
    Landed,
}

/// Per-frame output from `LandingSystem::update`.
#[derive(Debug, Clone)]
pub struct LandingUpdate {
    /// Current state after this update.
    pub state: LandingState,
    /// State before this update (for detecting transitions).
    pub previous_state: LandingState,
    /// Minimum clearance across all skids, or `None` when no terrain is detected.
    pub min_clearance: Option<f32>,
    /// Whether any skid raycast detected terrain contact (clearance < SLIDING_THRESHOLD_M).
    pub skid_contact: bool,
    /// Impact event if the state transitioned to `Sliding` (first ground contact) this frame, else `None`.
    pub impact: Option<LandingImpactEvent>,
}

/// Input bundle passed to `LandingSystem::update` each tick.
pub struct LandingParams<'a> {
    /// Physics world for raycasting.
    pub physics: &'a PhysicsWorld,
    /// Ship world transform (position + rotation).
    pub ship_iso: &'a Isometry3<f32>,
    /// Scalar ship speed in m/s.
    pub ship_speed_ms: f32,
    /// Ship velocity vector in world space (m/s).
    pub ship_velocity: Vector3<f32>,
    /// Unit vector pointing toward the planet centre (gravity direction).
    pub gravity_dir: Unit<Vector3<f32>>,
    /// Surface gravity magnitude (m/s²) of the current planet.
    pub planet_gravity: f32,
    /// Whether terrain colliders are currently loaded.
    pub terrain_active: bool,
    /// Whether the engine is running.
    pub engine_on: bool,
    /// Throttle in [0, 1]; > 0 means the pilot is commanding thrust.
    pub throttle: f32,
}

/// Landing state machine.
///
/// Create one instance at ship spawn and call `update()` every game tick.
pub struct LandingSystem {
    state: LandingState,
    /// Pending lock request from the player (set by `request_lock_toggle`).
    lock_requested: bool,
}

impl LandingSystem {
    /// Creates a new system in the `Flying` state.
    pub fn new() -> Self {
        Self {
            state: LandingState::Flying,
            lock_requested: false,
        }
    }

    /// Toggle the landing-lock request.  Call when the player presses the
    /// land/unlock button.  The request is consumed on the next `update()`.
    pub fn request_lock_toggle(&mut self) {
        self.lock_requested = true;
    }

    /// Current state (read-only).
    pub fn state(&self) -> LandingState {
        self.state
    }

    /// Advance the state machine one tick.
    pub fn update(&mut self, p: LandingParams<'_>) -> LandingUpdate {
        let previous_state = self.state;

        // If terrain is no longer active, immediately reset to Flying.
        if !p.terrain_active {
            self.state = LandingState::Flying;
            self.lock_requested = false;
            return LandingUpdate {
                state: self.state,
                previous_state,
                min_clearance: None,
                skid_contact: false,
                impact: None,
            };
        }

        // Cast downward rays from each skid position.
        let clearances = self.cast_skid_rays(p.physics, p.ship_iso, &p.gravity_dir);
        let min_clearance = clearances.iter().copied().reduce(f32::min);
        let skid_contact = min_clearance.is_some_and(|c| c < SLIDING_THRESHOLD_M);

        let mut impact_event: Option<LandingImpactEvent> = None;
        let consume_lock = self.lock_requested;
        self.lock_requested = false;

        match self.state {
            LandingState::Flying => {
                if min_clearance.is_some_and(|c| c < SLIDING_THRESHOLD_M) {
                    // FLYING → SLIDING: actual ground contact — emit impact event now.
                    // Use the vertical speed component (along gravity) for impact severity,
                    // so a horizontal graze at high speed doesn't register as a hard landing.
                    let vertical_speed = p.ship_velocity.dot(&p.gravity_dir).abs();
                    let per_skid = per_skid_speeds_from_clearance(&clearances, vertical_speed);
                    let category = ImpactCategory::from_speed(vertical_speed);
                    impact_event = Some(LandingImpactEvent {
                        impact_speed_ms: vertical_speed,
                        per_skid_speeds: per_skid,
                        planet_gravity: p.planet_gravity,
                        category,
                    });
                    self.state = LandingState::Sliding;
                }
            }

            LandingState::Sliding => {
                // Allow unlock request to be a no-op here (already sliding).
                if consume_lock && p.ship_speed_ms < LOCK_SPEED_THRESHOLD_MS {
                    // Transition to Landed — lock the ship in place.
                    self.state = LandingState::Landed;
                } else {
                    // Check if we can return to Flying.
                    let all_clear = min_clearance.map(|c| c > FLYING_THRESHOLD_M).unwrap_or(true);
                    if all_clear && p.engine_on && p.throttle > 0.0 {
                        self.state = LandingState::Flying;
                    }
                }
            }

            LandingState::Landed => {
                if consume_lock {
                    // Player requested unlock.
                    self.state = LandingState::Sliding;
                }
                // While Landed, the ship is stationary — no terrain check needed.
            }
        }

        LandingUpdate {
            state: self.state,
            previous_state,
            min_clearance,
            skid_contact,
            impact: impact_event,
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Cast one ray per skid along the gravity direction and return clearances.
    ///
    /// Returns an array of 4 floats.  If a skid ray misses terrain the
    /// corresponding value is `LANDING_RAY_DIST_M` (treated as "no terrain nearby").
    fn cast_skid_rays(
        &self,
        physics: &PhysicsWorld,
        ship_iso: &Isometry3<f32>,
        gravity_dir: &Unit<Vector3<f32>>,
    ) -> [f32; 4] {
        let filter =
            QueryFilter::default().groups(InteractionGroups::new(Group::ALL, TERRAIN));

        let ray_dir = gravity_dir.into_inner();
        let mut clearances = [LANDING_RAY_DIST_M; 4];

        for (i, local) in Ship::skid_positions().iter().enumerate() {
            let local_pt = Point3::new(local[0], local[1], local[2]);
            let world_pt = ship_iso.transform_point(&local_pt);

            if let Some((_handle, toi)) =
                physics.cast_ray(world_pt, ray_dir, LANDING_RAY_DIST_M, true, filter)
            {
                clearances[i] = toi;
            }
        }

        clearances
    }
}

impl Default for LandingSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private utilities
// ---------------------------------------------------------------------------

/// Distribute the combined ship speed proportionally across skids.
///
/// Skids closer to terrain (smaller clearance) absorb a larger share of the
/// impact.  If no skid has detected terrain all speeds are equal.
fn per_skid_speeds_from_clearance(clearances: &[f32; 4], ship_speed_ms: f32) -> [f32; 4] {
    // Weight each skid by (1 / clearance).  Skids with max clearance get 0.
    let mut weights = [0.0f32; 4];
    for (i, &c) in clearances.iter().enumerate() {
        if c < LANDING_RAY_DIST_M {
            weights[i] = 1.0 / c.max(0.01);
        }
    }

    let total: f32 = weights.iter().sum();
    if total < f32::EPSILON {
        // Uniform distribution if no clearance info.
        return [ship_speed_ms / 4.0; 4];
    }

    let mut result = [0.0f32; 4];
    for i in 0..4 {
        result[i] = ship_speed_ms * weights[i] / total;
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Convenience builder for LandingParams with defaults.
    fn default_params<'a>(physics: &'a PhysicsWorld, iso: &'a Isometry3<f32>) -> LandingParams<'a> {
        LandingParams {
            physics,
            ship_iso: iso,
            ship_speed_ms: 0.0,
            ship_velocity: Vector3::zeros(),
            gravity_dir: Unit::new_normalize(Vector3::new(0.0, -1.0, 0.0)),
            planet_gravity: 9.81,
            terrain_active: true,
            engine_on: false,
            throttle: 0.0,
        }
    }

    // ---- ImpactCategory tests ----

    #[test]
    fn impact_category_clean() {
        assert_eq!(ImpactCategory::from_speed(0.0), ImpactCategory::Clean);
        assert_eq!(ImpactCategory::from_speed(9.9), ImpactCategory::Clean);
    }

    #[test]
    fn impact_category_minor() {
        assert_eq!(ImpactCategory::from_speed(10.0), ImpactCategory::Minor);
        assert_eq!(ImpactCategory::from_speed(29.9), ImpactCategory::Minor);
    }

    #[test]
    fn impact_category_major() {
        assert_eq!(ImpactCategory::from_speed(30.0), ImpactCategory::Major);
        assert_eq!(ImpactCategory::from_speed(79.9), ImpactCategory::Major);
    }

    #[test]
    fn impact_category_destroyed() {
        assert_eq!(ImpactCategory::from_speed(80.0), ImpactCategory::Destroyed);
        assert_eq!(ImpactCategory::from_speed(200.0), ImpactCategory::Destroyed);
    }

    // ---- Initial state ----

    #[test]
    fn initial_state_is_flying() {
        let sys = LandingSystem::new();
        assert_eq!(sys.state, LandingState::Flying);
    }

    // ---- Lock / unlock logic ----

    #[test]
    fn terrain_inactive_resets_to_flying() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Sliding;

        let physics = sa_physics::PhysicsWorld::new();
        let iso = Isometry3::identity();
        let update = sys.update(LandingParams {
            terrain_active: false,
            ship_iso: &iso,
            ..default_params(&physics, &iso)
        });
        assert_eq!(update.state, LandingState::Flying);
    }

    #[test]
    fn lock_request_consumed_after_update() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Landed;
        sys.request_lock_toggle();

        let physics = sa_physics::PhysicsWorld::new();
        let iso = Isometry3::identity();

        // Landed + unlock request → Sliding.
        let update = sys.update(LandingParams {
            ship_iso: &iso,
            ..default_params(&physics, &iso)
        });
        assert_eq!(update.state, LandingState::Sliding);

        // Lock flag consumed — second update stays Sliding (no terrain colliders).
        let update2 = sys.update(LandingParams {
            ship_iso: &iso,
            ..default_params(&physics, &iso)
        });
        assert_eq!(update2.state, LandingState::Sliding);
    }

    #[test]
    fn sliding_to_flying_requires_engine_and_throttle() {
        let mut sys = LandingSystem::new();
        sys.state = LandingState::Sliding;

        let physics = sa_physics::PhysicsWorld::new();
        let iso = Isometry3::identity();

        // No engine → stays Sliding (rays miss → all_clear=true but engine_on=false).
        let upd = sys.update(LandingParams {
            ship_iso: &iso,
            ..default_params(&physics, &iso)
        });
        assert_eq!(upd.state, LandingState::Sliding);

        // Engine on, zero throttle → stays Sliding.
        let upd2 = sys.update(LandingParams {
            ship_iso: &iso,
            engine_on: true,
            throttle: 0.0,
            ..default_params(&physics, &iso)
        });
        assert_eq!(upd2.state, LandingState::Sliding);

        // Engine on + throttle > 0, rays miss → all_clear → Flying.
        let upd3 = sys.update(LandingParams {
            ship_iso: &iso,
            engine_on: true,
            throttle: 0.5,
            ..default_params(&physics, &iso)
        });
        assert_eq!(upd3.state, LandingState::Flying);
    }

    #[test]
    fn per_skid_speeds_uniform_when_no_terrain() {
        let clearances = [LANDING_RAY_DIST_M; 4];
        let speeds = per_skid_speeds_from_clearance(&clearances, 8.0);
        for s in &speeds {
            assert!((*s - 2.0).abs() < 1e-4, "expected 2.0, got {s}");
        }
    }

    #[test]
    fn per_skid_speeds_sum_to_total() {
        let clearances = [0.5, 0.8, 2.0, LANDING_RAY_DIST_M];
        let speed = 20.0;
        let speeds = per_skid_speeds_from_clearance(&clearances, speed);
        let total: f32 = speeds.iter().sum();
        assert!(
            (total - speed).abs() < 1e-3,
            "speeds should sum to {speed}, got {total}"
        );
    }
}
