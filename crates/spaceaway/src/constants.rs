//! Centralized gameplay constants for the spaceaway binary.
//!
//! All distance thresholds, speed limits, and gameplay-tuning values live here.
//! Crate-internal constants (terrain LOD, mesh gen) stay in sa_terrain::config.
//! Drive physics constants (speed of light, c) stay in sa_ship::drive.
//!
//! When debugging distance/threshold issues, check HERE first.

// ═══════════════════════════════════════════════════════════════════════
// UNIT CONVERSIONS
// ═══════════════════════════════════════════════════════════════════════

/// Light-year in meters.
pub const LY_TO_M: f64 = 9.461e15;

/// Meters per light-year (inverse).
pub const M_TO_LY: f64 = 1.0 / LY_TO_M;

/// Astronomical unit in light-years.
pub const AU_IN_LY: f64 = 1.581e-5;

/// Astronomical unit in meters.
pub const AU_IN_M: f64 = 1.496e11;

// ═══════════════════════════════════════════════════════════════════════
// WARP DRIVE
// ═══════════════════════════════════════════════════════════════════════

/// Distance at which warp auto-disengages near locked target (~630 AU).
pub const WARP_DISENGAGE_LY: f64 = 0.01;

/// Gravity well detection radius around stars.
pub const GRAVITY_WELL_AU: f64 = 50.0;

/// Warp spool time in seconds (from sa_ship::drive::WARP_SPOOL_TIME).
pub const WARP_SPOOL_SECONDS: f64 = 5.0;

// ═══════════════════════════════════════════════════════════════════════
// CRUISE DRIVE
// ═══════════════════════════════════════════════════════════════════════

/// Cruise deceleration time constant. Ship speed = altitude / this value.
/// Lower = faster approach, higher = slower/more cinematic.
pub const APPROACH_TIME_SECONDS: f64 = 8.0;

/// Minimum altitude where cruise auto-disengages (meters). 100 km for planets.
/// For larger bodies (stars), use `safe_standoff_m()` which scales with radius.
pub const CRUISE_DISENGAGE_ALT_M: f64 = 100_000.0;

/// Minimum exclusion sphere margin above surface for flythrough prevention (meters).
/// For larger bodies (stars), use `safe_standoff_m()` which scales with radius.
pub const EXCLUSION_RADIUS_M: f64 = 100_000.0;

/// Fraction of body radius used as safe standoff distance for large bodies.
/// Stars: 0.5 × 427,000 km = 213,500 km (outside photosphere).
/// Planets: 0.5 × 7,700 km = 3,850 km (but floored at 100 km).
pub const SAFE_STANDOFF_FRACTION: f64 = 0.5;

/// Compute safe standoff distance for a body (meters).
/// Returns max(100 km, 0.5 × radius) — safe for both planets and stars.
pub fn safe_standoff_m(body_radius_m: f64) -> f64 {
    (body_radius_m * SAFE_STANDOFF_FRACTION).max(EXCLUSION_RADIUS_M)
}

/// Maximum ship speed (m/s) to allow cruise/warp engagement.
pub const DRIVE_ENGAGE_MAX_SPEED_MS: f32 = 100.0;

// ═══════════════════════════════════════════════════════════════════════
// APPROACH PHASES (multiples of planet radius unless noted)
// ═══════════════════════════════════════════════════════════════════════

/// Distant → Approaching transition.
pub const PHASE_APPROACHING: f64 = 50.0;

/// Approaching → Orbit transition. Terrain activates here.
pub const PHASE_ORBIT: f64 = 5.0;

/// Orbit → UpperAtmosphere transition. Gravity blending begins.
pub const PHASE_UPPER_ATMO: f64 = 2.0;

/// UpperAtmosphere → LowerAtmosphere. Collision grid activates. Impulse only.
pub const PHASE_LOWER_ATMO: f64 = 0.2;

/// LowerAtmosphere → Landing. Skid raycasts begin. In METERS (not radius mult).
pub const PHASE_LANDING_M: f64 = 500.0;

/// Departure hysteresis: Orbit → Approaching (must be > PHASE_ORBIT).
pub const DEPART_ORBIT: f64 = 6.0;

/// Departure hysteresis: Approaching → Distant (must be > PHASE_APPROACHING).
pub const DEPART_APPROACHING: f64 = 60.0;

// ═══════════════════════════════════════════════════════════════════════
// SOLAR SYSTEM
// ═══════════════════════════════════════════════════════════════════════

/// System unloads when ship is this far from the star (in AU).
/// Must be > WARP_DISENGAGE_LY / AU_IN_LY (~630 AU) to prevent
/// load-then-immediately-unload on warp arrival.
pub const SYSTEM_BOUNDARY_AU: f64 = 1000.0;

/// Orbital time scale (how fast planets orbit — 30× real time).
pub const TIME_SCALE: f64 = 30.0;

// ═══════════════════════════════════════════════════════════════════════
// LANDING
// ═══════════════════════════════════════════════════════════════════════

/// Maximum raycast distance for skid ground detection (meters).
pub const LANDING_RAY_DIST_M: f32 = 100.0;

/// Flying → Sliding when any skid is closer than this (meters).
pub const SLIDING_THRESHOLD_M: f32 = 1.0;

/// Sliding → Flying when ALL skids are farther than this (meters).
pub const FLYING_THRESHOLD_M: f32 = 10.0;

/// Sliding → Landed requires speed below this (m/s).
pub const LOCK_SPEED_THRESHOLD_MS: f32 = 5.0;

/// Impact categories (m/s vertical speed).
pub const IMPACT_CLEAN_MS: f32 = 10.0;
pub const IMPACT_MINOR_MS: f32 = 30.0;
pub const IMPACT_MAJOR_MS: f32 = 80.0;

// ═══════════════════════════════════════════════════════════════════════
// NAVIGATION
// ═══════════════════════════════════════════════════════════════════════

/// Maximum distance for nearby star search (light-years).
pub const NEARBY_STAR_RANGE_LY: f64 = 50.0;

/// Maximum nearby stars in the navigation list.
pub const MAX_NEARBY_STARS: usize = 15;
