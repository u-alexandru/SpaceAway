# Landing System Design — Phase 3

**Date:** 2026-03-29
**Status:** Approved
**Depends on:** CDLOD Terrain (Phase 1+2 complete)
**Scope:** Flight-to-ground-to-flight cycle from the helm. No ship exit (Phase 4).

---

## Overview

The player flies their ship from orbit to a planet's surface, lands, and takes off again — all under manual control. Landing is a pilot skill: approach angle, speed management, and timing the landing lock are the player's responsibility. No autopilot, no auto-deceleration, no hand-holding.

---

## 1. Ship Modifications

### 1.1 Thrust Increase

Max thrust increased from 500,000 N to **750,000 N** for 50,000 kg ship (15 m/s² max acceleration).

| Planet gravity | TWR | Landing viability |
|---|---|---|
| < 1.2g (< 11.8 m/s²) | > 1.27 | Comfortable round-trip |
| 1.2–1.5g (11.8–14.7 m/s²) | 1.02–1.27 | Risky, tight margins |
| > 1.5g (> 14.7 m/s²) | < 1.02 | One-way trip until engine upgrades |

Change `DEFAULT_MAX_THRUST` in `sa_ship/src/ship.rs` from 500,000 to 750,000.

### 1.2 Landing Skid Colliders

Add **4 solid sphere colliders** to the ship body at landing skid positions:

- **Positions (ship-local):** fore (0, -1.5, -12), aft (0, -1.5, 12), port (-2, -1.5, 0), starboard (2, -1.5, 0)
- **Shape:** Small spheres, radius 0.3m
- **Friction:** 0.6 (metal on rock)
- **Restitution:** 0.0 (no bounce)
- **Collision group:** `SHIP_EXTERIOR` (new group) — interacts with `TERRAIN` only
- **Not sensor:** These generate real contact forces via rapier

The ship hull remains a sensor (mass provider only). The skid colliders are separate — they don't affect the player walking inside the ship because `SHIP_EXTERIOR` doesn't interact with `PLAYER`.

### 1.3 Landing Gear Lock Button

New cockpit interactable: a toggle button on the helm console.

- **LOCK:** Only activates if ship speed < 5 m/s. Enters LANDED state.
- **UNLOCK:** Resumes physics immediately. Player should throttle up first.
- **Visual:** Button illuminates green when locked, red/off when unlocked.
- **Audio:** Click sound on press.

---

## 2. Terrain Collision Improvements

### 2.1 HeightField Friction

Set `friction(0.8)` on all HeightField terrain colliders (currently uses rapier default of 0.0). Combined with skid friction 0.6, the ship decelerates naturally on ground contact. A good pilot touching down at < 10 m/s stops within a few ship lengths.

### 2.2 Sphere Barrier Update

Update the sphere barrier's collision filter to include `SHIP_EXTERIOR` so the solid skid colliders collide with it. This prevents flythrough at distances where HeightField colliders don't exist.

### 2.3 Collider Range

`COLLIDER_RANGE_M` stays at 500m. At terminal velocity (~270 m/s on heavy planets), this gives ~1.8 seconds of lead time. The sphere barrier covers larger distances. If testing reveals gaps during steep dives, increase to 1000m.

### 2.4 Minimum Collider LOD

Stays at LOD 10 (~10km chunks). Fine enough for approach. Finer LODs stream in as altitude decreases, replacing coarse colliders with detailed terrain.

---

## 3. Ground Contact Detection

### 3.1 Raycast System

**4-point raycasting** runs each frame when ship altitude < 100m above terrain:

- Cast from the same 4 landing skid positions along `-gravity_direction`
- Each ray: origin = `ship_transform * skid_local_pos`, direction = `-gravity_dir`, max distance = 100m
- Records `min_clearance` = minimum hit distance across all 4 rays
- Misses (no hit) treated as clearance = 100m (still above terrain)

This is for **state detection only**, not collision. The solid skid colliders handle physics.

### 3.2 Altitude HUD

When raycasts are active (< 100m), display altitude on the helm screen. The altitude warning audio (Section 5) also triggers from this data.

---

## 4. Landing State Machine

### 4.1 States

```
FLYING ──(skids contact terrain)──> SLIDING
SLIDING ──(player clicks LOCK, speed < 5 m/s)──> LANDED
LANDED ──(player clicks UNLOCK)──> SLIDING
SLIDING ──(altitude > 10m, thrust active)──> FLYING
```

### 4.2 FLYING

Normal helm physics. Raycasts activate below 100m altitude. No special behavior.

### 4.3 SLIDING

Ship is on the ground, skid colliders provide friction against terrain. Physics running normally — rapier resolves contacts. Player can:
- Steer with RCS to adjust position
- Apply thrust to slow down or lift off
- Click LOCK button when speed < 5 m/s

Transition to FLYING when altitude > 10m and thrust is active (engine on + throttle > 0). The altitude + thrust check prevents brief bounces from triggering FLYING.

### 4.4 LANDED

- Ship body: velocity zeroed each frame, `gravity_scale = 0`, position locked
- Engine/throttle state preserved (player can adjust throttle while locked)
- Player can look around from helm, interact with cockpit
- UNLOCK button resumes physics immediately: `gravity_scale` restored, velocity = 0, forces resume

### 4.5 Impact Detection

On FLYING → SLIDING transition, record vertical speed (component along gravity direction):

| Impact speed | Category | Immediate effect |
|---|---|---|
| < 10 m/s | Clean | Soft thud audio |
| 10–30 m/s | Minor | Camera shake (small), heavy thud + metallic stress audio |
| 30–80 m/s | Major | Camera shake (large), crash crunch + alarm audio |
| > 80 m/s | Destroyed | Explosion audio, ship destroyed, crew dead |

**Damage event:** `LandingImpactEvent` emitted via `EventBus` containing:
- `impact_speed_ms: f32` — vertical speed at contact
- `per_skid_speeds: [f32; 4]` — individual skid contact velocities
- `planet_gravity: f32` — surface gravity in m/s²
- `category: ImpactCategory` — Clean / Minor / Major / Destroyed

Phase 3 consumes this event for camera shake and audio only. The future damage system subscribes to the same event.

---

## 5. Audio Cues

All sounds from the existing 3100 WAV library in `assets/sounds/`.

### 5.1 Descent (altitude < 100m)

**Altitude warning beep:** Repeating beep, frequency increases as altitude decreases.
- 100m: 1 beep/sec
- 50m: 2 beeps/sec
- 20m: 4 beeps/sec
- 10m: 8 beeps/sec
- 5m: continuous tone

### 5.2 Contact

| Category | Sound |
|---|---|
| Clean (< 10 m/s) | Soft metallic thud |
| Minor (10–30 m/s) | Heavy thud + metallic stress creak |
| Major (30–80 m/s) | Crash crunch + alarm klaxon |
| Destroyed (> 80 m/s) | Explosion |

### 5.3 Landed State

- Lock button click
- Engine idle hum (existing, continues)

### 5.4 Takeoff

- Unlock button click
- Engine power-up ramp (existing throttle-reactive audio)

---

## 6. Collision Group Update

### 6.1 New Group

Add `SHIP_EXTERIOR` to `ship_colliders.rs`:

```
SHIP_EXTERIOR = Group::GROUP_6  // 0x0020 (PROJECTILE not yet implemented; reassign if needed)
```

### 6.2 Updated Interaction Table

| Group | Collides with |
|---|---|
| SHIP_HULL (sensor) | NONE (mass provider only) |
| SHIP_INTERIOR | PLAYER |
| PLAYER | SHIP_INTERIOR, TERRAIN |
| SHIP_EXTERIOR | TERRAIN |
| TERRAIN | PLAYER, SHIP_EXTERIOR |
| INTERACTABLE | raycast only |

---

## 7. Code Cleanup Prerequisite

Before adding Phase 3 code, split `main.rs` (2000+ lines) into focused modules:

- `terrain_system.rs` — terrain activation, deactivation, streaming orchestration
- `landing.rs` — landing state machine, raycast detection, impact events
- `drive_system.rs` — cruise/warp galactic position updates, drive integration

This keeps files under the 300-line convention and gives landing its own module.

---

## 8. Future System Hooks

### 8.1 Ship Damage System (future spec)

Phase 3 emits `LandingImpactEvent` but does not implement damage. Future damage system will:

- Subscribe to `LandingImpactEvent`
- Apply hull integrity loss based on impact category
- Trigger system failures (engines, O2, lights) for major impacts
- Destroy ship for catastrophic impacts
- Hull integrity displayed on cockpit status screen

**Prepared by Phase 3:** Event type, impact categories, per-skid velocity data.

### 8.2 Navigation Console (future spec)

Planet gravity and landing viability display:

- Planet gravity (m/s²) and equivalent g-force
- Ship thrust-to-weight ratio (TWR) at that gravity
- Status indicator: "CAN LAND" / "CAN LAND — NO RETURN" / "CANNOT LAND"
- TWR calculation: `ship_max_thrust / (ship_mass * planet_gravity)`

**Prepared by Phase 3:** Surface gravity already in `TerrainConfig`. TWR computable from `Ship::max_thrust` and `Ship::MASS`.

### 8.3 Ship Upgrades (future spec)

Engine thrust upgrades unlock heavier planets:

- Base thrust: 750,000 N (TWR 1.53 at 1g)
- Tier 2: 1,200,000 N (TWR 2.44 at 1g, round-trip at 2g)
- Tier 3: 2,000,000 N (TWR 4.08 at 1g, round-trip at 3.5g)

**Prepared by Phase 3:** `Ship::max_thrust` is already a configurable field.

### 8.4 Ship Exit — Airlock (Phase 4)

Player exits the ship via the airlock (not instant teleport):

1. Walk to airlock door, interact
2. Doors open with animation
3. Step out onto terrain
4. Doors close behind
5. Player controller switches to `PlanetSurface` mode
6. Planet-relative gravity, terrain collision, ship interior remains for re-entry

**Prepared by Phase 3:** Landed state, terrain colliders active, TERRAIN collision group defined.
