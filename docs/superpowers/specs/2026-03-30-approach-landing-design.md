# Planet Approach, Landing & Departure System

**Date:** 2026-03-30
**Status:** Approved
**Scope:** `spaceaway` (approach.rs new, helm_mode.rs, drive_integration.rs,
render_frame.rs, terrain_integration.rs refactored)

## Overview

A unified approach state machine that manages the complete pipeline from "dot
in the distance" to "walking on the surface" and back. Replaces the current
scattered proximity checks across 4+ files with a single source of truth.

### Problems Solved

1. **Cruise flythrough** → analytical ray-sphere intersection prevents crossing
   any planet's boundary at any speed
2. **Fragmented logic** → single ApproachManager owns all phase decisions
3. **Jarring transitions** → warp auto-cascades to cruise, smooth deceleration
4. **Terrain pop-in** → terrain activates at 5× radius (was 2×), streams before
   visual range
5. **Collision gaps** → synchronous collision grid generation before ship can
   reach surface

---

## 1. Approach State Machine

### ApproachPhase

```rust
pub enum ApproachPhase {
    Distant,          // > 50× radius — drive unrestricted
    Approaching,      // 50× → 5× — cruise decelerating
    Orbit,            // 5× → 2× — terrain activating, chunks streaming
    UpperAtmosphere,  // 2× → 0.2× — cruise crawl → disengage, gravity blending
    LowerAtmosphere,  // 0.2× → 500m — impulse only, collision grid active
    Landing,          // < 500m — skid raycasts, landing state machine
    Surface,          // landed — ship locked
    Departing,        // reverse of approach
}
```

### Phase Transitions

All driven by `altitude_m` (distance from nearest planet surface), computed
once per frame by the approach manager.

**Descent:**

```
Distant ──────→ Approaching       altitude < 50× radius
Approaching ──→ Orbit             altitude < 5× radius
Orbit ────────→ UpperAtmosphere   altitude < 2× radius
UpperAtmosphere → LowerAtmosphere altitude < 0.2× radius
LowerAtmosphere → Landing         altitude < 500m
Landing ──────→ Surface           LandingState == Landed
```

**Ascent:**

```
Surface ──────→ Departing         LandingState leaves Landed
Departing ────→ LowerAtmosphere   altitude > 500m
LowerAtmosphere → UpperAtmosphere altitude > 0.2× radius
UpperAtmosphere → Orbit           altitude > 2× radius
Orbit ────────→ Approaching       altitude > 6× radius  (hysteresis)
Approaching ──→ Distant           altitude > 60× radius (hysteresis)
```

No planet in range → `Distant` regardless of altitude.

### ApproachManager

```rust
pub struct ApproachManager {
    phase: ApproachPhase,
    planet_pos_ly: Option<WorldPos>,
    planet_radius_m: Option<f64>,
    altitude_m: f64,
    body_index: Option<usize>,
}
```

Each frame, `update()`:
1. Scan `active_system` for nearest landable planet
2. Compute `altitude_m = distance_to_center - planet_radius`
3. Transition phase if threshold crossed
4. Return `ApproachState` (read-only snapshot)

### ApproachState

```rust
pub struct ApproachState {
    pub phase: ApproachPhase,
    pub altitude_m: f64,
    pub planet_pos_ly: Option<WorldPos>,
    pub planet_radius_m: f64,
    pub body_index: Option<usize>,
    /// Speed cap in m/s for cruise (None = unlimited).
    pub cruise_speed_cap_ms: Option<f64>,
    /// Whether terrain should be active.
    pub terrain_active: bool,
    /// Whether collision grid should be active.
    pub collision_active: bool,
    /// Whether to auto-disengage cruise this frame.
    pub disengage_cruise: bool,
    /// Whether to auto-cascade warp → cruise.
    pub cascade_warp_to_cruise: bool,
    /// Whether cruise can be engaged.
    pub can_engage_cruise: bool,
    /// Whether warp can be engaged.
    pub can_engage_warp: bool,
}
```

Other systems query `ApproachState` instead of doing their own distance checks.

---

## 2. Flythrough Prevention

### Analytical Ray-Sphere Intersection

Before applying any cruise delta, test the proposed movement ray against every
planet's exclusion sphere (radius + 100km).

```
Ray: origin = galactic_position, direction = delta (unnormalized)
Sphere: center = planet_pos_ly, radius = (planet_radius_m + 100_000) / LY_TO_M

oc = origin - center
a = dot(delta, delta)
b = 2 × dot(oc, delta)
c = dot(oc, oc) - radius²
discriminant = b² - 4ac

If discriminant < 0: no intersection → safe, apply full delta
If discriminant ≥ 0:
  t = (-b - sqrt(discriminant)) / (2a)
  If 0 < t < 1: ray crosses sphere this frame
    Truncate delta to t × 0.99 (stop just outside)
    Set disengage_cruise = true
  If t < 0 and t_exit > 0: already inside sphere
    Set disengage_cruise = true, don't apply delta
```

O(n) where n = planets (1-8). Exact, handles any approach angle, works at any
speed. No binary search, no iteration.

### Integration

The approach manager computes the ray-sphere test and stores the result in
`ApproachState`. helm_mode reads it:

```rust
let (mut delta, _) = galactic_position_delta_decel(...);
// Approach manager truncates delta if needed
let delta = approach_state.clamp_cruise_delta(delta);
galactic_position += delta;
if approach_state.disengage_cruise {
    drive.request_disengage();
}
```

### What This Replaces

- Binary search flythrough check in helm_mode.rs
- Post-hoc position clamp
- `nearest_planet_info()` distance checks in the movement path

---

## 3. Cruise Deceleration

### Speed Proportional to Altitude

Replace piecewise deceleration curve with absolute speed cap:

```
cruise_speed_cap_ms = altitude_m / APPROACH_TIME_SECONDS
```

Where `APPROACH_TIME_SECONDS = 8.0`. This gives a natural exponential decay —
the ship always takes ~8 seconds to halve its distance to the planet.

| Altitude | Speed Cap | Per-Frame (60fps) |
|----------|-----------|-------------------|
| 385,000 km (50×) | 48,000 km/s | 800 km |
| 38,000 km (5×) | 4,750 km/s | 79 km |
| 10,000 km | 1,250 km/s | 21 km |
| 1,000 km | 125 km/s | 2 km |
| 100 km | 12.5 km/s | 208 m |
| ≤ 100 km | disengage | — |

Total approach time from 50× radius to 100km:
`8 × ln(385,300,000 / 100,000) ≈ 66 seconds ≈ 1 minute`.

The planet grows smoothly in the viewport throughout. No knee points, no abrupt
speed changes.

### How It Works

The approach manager sets `cruise_speed_cap_ms`. The drive integration applies
it as a clamp to the cruise speed:

```rust
let base_speed = drive.current_speed_ly_s();
let base_speed_ms = base_speed * LY_TO_M;
let capped_ms = base_speed_ms.min(cruise_speed_cap_ms);
let effective_speed = capped_ms / LY_TO_M;
```

At high altitude the cap exceeds cruise max → no effect (full speed).
As altitude drops, the cap reduces speed smoothly.

### Constant: APPROACH_TIME_SECONDS

`8.0` seconds gives a ~1 minute approach from the deceleration zone to 100km.
This is configurable in the approach manager. Lower values = faster approach,
higher = slower/more cinematic.

### No-Planet Fallback

When no planet is in range, `cruise_speed_cap_ms = None` (unlimited). The
existing star-target-based `warp_deceleration()` and `WARP_DISENGAGE_LY`
continue to work for warp. For cruise without a planet, the existing cascade
disengage at `CRUISE_DISENGAGE_LY` from the locked star target is preserved.

### What This Replaces

- `cruise_deceleration()` function in drive_integration.rs
- All piecewise threshold constants (10M, 1M, 100K, 1K, 100 km)
- `target_dist` computation in helm_mode.rs

---

## 4. Drive Cascade

### Warp → Cruise Auto-Cascade

When warp auto-disengages (at `WARP_DISENGAGE_LY` or gravity well), the
approach manager sets `cascade_warp_to_cruise = true`. helm_mode reads this
and engages cruise instead of dropping to impulse:

```rust
if approach_state.cascade_warp_to_cruise {
    drive.request_disengage();  // warp → impulse
    drive.request_engage(DriveMode::Cruise);  // impulse → cruise
    audio.announce(VoiceId::CruiseDriveEngaged);  // "Switching to cruise drive"
}
```

The cascade only fires when there's an active system. In deep space (no system),
warp drops to impulse as before.

### Drive Engagement Rules

| Drive | Can Engage When |
|-------|----------------|
| Cruise | `phase ≥ UpperAtmosphere` on departure (altitude > 2× radius), speed < 100 m/s |
| Warp | `phase == Distant` (altitude > 50× radius), no terrain active, speed < 100 m/s, exotic fuel > 0 |

These replace the scattered checks across helm_mode.rs and drive.rs. The
approach manager exposes `can_engage_cruise` and `can_engage_warp` booleans.

### Velocity on Disengage

All drive disengages zero the ship's rapier velocity. FTL drives move
`galactic_position` directly — they're not Newtonian. Dropping out to a dead
stop is physically consistent.

---

## 5. Terrain Streaming Strategy

### Early Activation at Orbit Phase (5× radius)

| Phase | Altitude | Terrain Action |
|-------|----------|----------------|
| Approaching (50× → 5×) | Far | Nothing. Icosphere only. |
| Orbit (5× → 2×) | Enter | **Activate terrain.** Seed LOD 0-1 (30 base chunks). Start streaming. Burst mode (64 chunks/frame). |
| UpperAtmosphere (2× → 0.2×) | Close | Steady streaming (8 chunks/frame). Full quadtree subdivision. Vertex morphing active. |
| LowerAtmosphere (0.2× → 500m) | Near | Fine LODs streaming. Collision grid active. |

By the time the player reaches 2× radius (where terrain detail matters), the
slab has had the entire 5× → 2× travel time to fill. At deceleration speeds,
this is 10-30 seconds of streaming.

### Deactivation at 6× Radius

Hysteresis: activate at 5×, deactivate at 6×. The 1× radius gap prevents
terrain toggling when orbiting near the boundary.

### Icosphere Coexistence (unchanged)

Icosphere renders at 0.999× radius. Terrain chunks at true radius occlude it
via depth testing. No show/hide logic. Already implemented in terrain redesign.

### What Changes

- `find_terrain_planet()` activation: 2.0× → 5.0× radius
- Deactivation: 2.5× → 6.0× radius
- Activation triggered by approach manager's `terrain_active` flag
- `render_frame.rs` reads `approach_state.terrain_active` instead of calling
  `find_terrain_planet()`

---

## 6. Collision Readiness

### Synchronous Generation at LowerAtmosphere (0.2× radius)

When approach transitions to `LowerAtmosphere`, the collision grid generates
its 7×7 chunks synchronously on that frame. Cost: ~7ms (49 × 0.15ms per chunk,
heights only — no mesh).

The ship is in impulse at this point. At max impulse (1,000 m/s), it takes
120+ seconds to descend from 0.2× radius (~1,500 km for a 7,706 km planet) to
the surface. Collision is guaranteed ready long before landing.

### Forced Anchor Rebase

On `LowerAtmosphere` entry, force an immediate physics anchor rebase to the
ship's current position. This ensures the rapier origin is fresh at the start
of the collision phase.

### What Changes

- Collision grid activation driven by `approach_state.collision_active`
- First update synchronous (approach manager flags `first_collision_frame`)
- Forced anchor rebase on phase transition

---

## 7. Landing, Surface, and Departure

### Landing (unchanged)

The existing landing state machine is correct:

```
FLYING → SLIDING:  min skid clearance < 1.0m → impact event
SLIDING → LANDED:  lock request + speed < 5 m/s
LANDED → SLIDING:  unlock request
SLIDING → FLYING:  all clearances > 10m + engine + throttle > 0
```

The approach manager reads the landing state to determine `phase = Surface`.

### Departure

Phase transitions on ascent use the same thresholds:

```
Departing:          impulse thrust lifts ship
altitude > 500m:    → LowerAtmosphere (collision grid stays)
altitude > 0.2×:    → UpperAtmosphere (collision grid destroyed)
altitude > 2×:      → Orbit (cruise engagement allowed)
altitude > 6×:      → Approaching (terrain deactivates)
altitude > 60×:     → Distant (warp allowed)
```

### What Stays the Same

- Landing state machine (landing.rs)
- Skid raycasts and impact events
- Airlock exit/entry (walk_mode.rs)
- Gravity blending (sa_terrain::gravity)
- Physics anchor rebase (terrain_colliders.rs)

---

## 8. Refactoring Map

### New File

| File | Purpose | Size |
|------|---------|------|
| `crates/spaceaway/src/approach.rs` | ApproachManager, ApproachPhase, ApproachState, ray-sphere intersection, speed cap computation | ~250 lines |

### Modified Files

| File | Remove | Keep |
|------|--------|------|
| **helm_mode.rs** | `nearest_planet_info()`, `nearest_planet_altitude_ly()`, flythrough binary search, planet boundary clamp, cascade cruise disengage, `target_dist` planet computation | Seated controls, throttle, drive engagement (uses approach_state), warp spool, rapier body sync, impact handling |
| **drive_integration.rs** | `cruise_deceleration()`, `CRUISE_DISENGAGE_LY` | `warp_deceleration()`, `WARP_DISENGAGE_LY`, `galactic_position_delta_decel()` (used for warp) |
| **render_frame.rs** | `find_terrain_planet()` call, terrain activation block, drive auto-disengage | Reads `approach_state.terrain_active` to activate/deactivate terrain |
| **terrain_integration.rs** | `find_terrain_planet()`, `ACTIVATE_RADIUS_MULT`, `DEACTIVATE_RADIUS_MULT` | TerrainManager, streaming, slab, build_draw_commands, `should_deactivate()` simplified to query approach_state |

### Unchanged Files

| File | Why |
|------|-----|
| landing.rs | State machine is correct, approach reads its state |
| terrain_colliders.rs | Collision wiring correct, approach triggers activation |
| sa_ship/drive.rs | DriveController fine, approach calls its methods |
| sa_terrain/config.rs | Terrain constants stay, approach thresholds separate |
| navigation.rs | Star gravity wells unrelated to planet approach |
| walk_mode.rs | Surface walking independent |

### Constants (in approach.rs)

```rust
// Phase transition thresholds (multiples of planet radius)
pub const PHASE_APPROACHING: f64 = 50.0;
pub const PHASE_ORBIT: f64 = 5.0;
pub const PHASE_UPPER_ATMO: f64 = 2.0;
pub const PHASE_LOWER_ATMO: f64 = 0.2;
pub const PHASE_LANDING: f64 = 500.0;  // meters, not radius multiple

// Hysteresis (departure thresholds)
pub const DEPART_ORBIT: f64 = 6.0;
pub const DEPART_APPROACHING: f64 = 60.0;

// Cruise deceleration
pub const APPROACH_TIME_SECONDS: f64 = 8.0;
pub const CRUISE_DISENGAGE_ALT_M: f64 = 100_000.0;  // 100km

// Flythrough prevention
pub const EXCLUSION_RADIUS_M: f64 = 100_000.0;  // 100km above surface
```
