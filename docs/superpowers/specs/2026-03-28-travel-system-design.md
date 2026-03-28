# Multi-Tier Travel System Design

Seamless multi-scale travel across a continuous procedural universe with no loading screens. Three drive tiers with distinct physics, visuals, and fuel costs create a gameplay loop that encourages exploration and resource gathering.

---

## 1. Design Goals

- **Seamless**: no loading screens, no teleportation cheats. The universe is continuous at all scales.
- **Physical**: each tier has grounded physics (Newtonian, then increasingly speculative but internally consistent).
- **Beautiful**: looking out the cockpit window should be awe-inspiring at every speed. The visual effects tell you how fast you're going.
- **Economical**: fuel scarcity drives the exploration loop. Different fuels for different tiers create meaningful resource decisions.
- **Real universe**: players CAN theoretically travel anywhere at any speed. The game doesn't prevent it — fuel economy and time make certain choices impractical.

---

## 2. Scale Reference

| Scale | Distance | Example |
|-------|----------|---------|
| Ship interior | 40 m | Walking to the engine room |
| Orbital | 1,000 km | Low orbit maneuvers |
| Planetary system | 1–100 AU | Earth to Jupiter: 4.2 AU |
| Interstellar | 1–50 ly | Sun to Alpha Centauri: 4.37 ly |
| Galactic | 100,000 ly | Milky Way diameter |
| Intergalactic | 2,500,000 ly | To Andromeda |

Conversion constants:
- 1 AU = 1.496 × 10¹¹ m = 1.581 × 10⁻⁵ ly
- 1 ly = 9.461 × 10¹⁵ m
- c = 299,792,458 m/s = 3.169 × 10⁻⁸ ly/s

---

## 3. Drive Tiers

### 3.1 Tier 1: Impulse Drive (Newtonian)

**Speed range**: 0 to 1,000 m/s (gameplay max)

**Physics**: Current Newtonian engine. Force-based acceleration via rapier3d. F=ma with thrust applied through the ship's forward vector.

**Acceleration**: 500 kN thrust on 50,000 kg ship = 10 m/s² (≈1g). Reach max speed in ~100 seconds. Feels responsive — not sluggish, not instant.

**Used for**: Docking with stations. Asteroid mining approach. Combat maneuvering. Exploring a planet's moons. Any local operation within a few thousand km.

**Fuel**: Hydrogen/deuterium.
- Sources: gas giant scooping, ice asteroid extraction, nebula collection, station purchase.
- Abundant — rarely a concern with basic management.
- Drain rate: current values (0.0005/s idle, 0.002/s at full throttle).

**Galactic position**: Frozen. At 1,000 m/s, movement is 1.057 × 10⁻¹³ ly/s. A 10 ly sector takes ~3 million years to cross. Star field is completely static.

**Engagement**: Always available. Default mode. No charge-up.

### 3.2 Tier 2: Cruise Drive

**Speed range**: 1c to 500c (299,792,458 m/s to 1.499 × 10¹¹ m/s)

**Physics**: Space compression / time acceleration bubble around the ship. Not true FTL — think Alcubierre-lite. The ship doesn't experience acceleration; it sets a target velocity and the drive maintains it. Velocity-based, not force-based.

**Used for**: Planet-to-planet travel within a solar system. Reaching asteroid belts, distant moons, nearby stations.

**Key travel times**:

| Distance | At 1c | At 10c | At 100c | At 500c |
|----------|-------|--------|---------|---------|
| 1 AU | 8.3 min | 50 sec | 5 sec | 1 sec |
| 5 AU | 41.5 min | 4.2 min | 25 sec | 5 sec |
| 100 AU (solar system) | 13.9 hr | 83 min | 8.3 min | 1.7 min |

**Fuel**: Same hydrogen as impulse, but 10× burn rate (0.02/s at full cruise throttle). A full tank gives ~50 seconds of max cruise. Encourages efficient route planning — don't cruise everywhere, plan your trips.

**Galactic position**: Moves at 3.169 × 10⁻⁸ to 1.585 × 10⁻⁵ ly/s. At max cruise (500c), crosses one 10 ly sector every 7.3 days. Star streaming is unaffected — sectors don't change during normal in-system flight.

**Engagement rules**:
- Available when seated at helm.
- Throttle lever controls cruise speed (1c at bottom, 500c at top).
- Auto-drops near gravity wells: large bodies (planets, stars) force exit from cruise. Drop distance proportional to body mass. Prevents flying through planets.
- Smooth engage/disengage — no charge-up needed.

### 3.3 Tier 3: Warp Drive

**Speed range**: 100,000c to 5,000,000c (3.169 × 10⁻³ to 1.585 × 10⁻¹ ly/s)

**Physics**: Full space-time distortion bubble. The ship doesn't move — space moves around it. Internally consistent with the game's physics model even though it's speculative.

**Used for**: Star-to-star travel. Crossing galactic regions. Exploration voyages.

**Key travel times**:

| Distance | At 100,000c | At 1,000,000c | At 5,000,000c |
|----------|-------------|---------------|---------------|
| 5 ly (nearest star) | 26 min | 2.6 min | **31.5 sec** |
| 50 ly (local region) | 4.4 hr | 26 min | **5.25 min** |
| 1,000 ly (galactic arm) | 3.7 days | 8.8 hr | **1.75 hr** |
| 100,000 ly (galaxy) | 7.3 months | 36.5 days | **7.3 days** |
| 2,500,000 ly (Andromeda) | 15 years | 1.5 years | **182 days** |

**Fuel**: Exotic matter.
- Sources: dense nebula cores, neutron star debris fields, ancient alien ruins, rare asteroid deposits.
- **Rare** — this is the bottleneck resource that drives the exploration loop.
- Drain rate: 0.005/s at min warp, 0.05/s at max warp. A full exotic tank gives ~20 seconds at max warp (enough for one short hop to a nearby star). Lower warp speeds are more fuel-efficient.
- Separate resource from hydrogen. New field: `exotic_fuel: f32` (0.0–1.0).

**Galactic position**: Moves at 0.003169 to 0.1585 ly/s. At max warp, crosses one 10 ly sector every 63 seconds. Star streaming handles this — sectors fade in/out smoothly.

**Engagement rules**:
- Must be seated at helm.
- 5-second charge-up (spool time). Crew can see/hear the drive charging. Creates tension and anticipation.
- Must be in open space — no gravity wells nearby. If too close to a star/planet, engagement fails with a warning.
- Throttle controls warp speed (100,000c at bottom, 5,000,000c at top).
- Emergency drop-out if exotic fuel runs out. Ship is stranded — must find exotic matter or wait for rescue (multiplayer). **This is where fear lives.**
- Automatic drop-out when approaching a destination star's gravity well.

### 3.4 No Tier 4 (Intergalactic)

The Milky Way has 400 billion star systems. Content is effectively infinite. Reaching Andromeda at max warp takes 182 real-time days — intentionally prohibitive. The game doesn't prevent it; fuel economy makes it an expedition, not a commute. Players who attempt intergalactic travel are making a dramatic, risky choice.

---

## 4. Visual Effects

All visual effects are driven by continuous float parameters computed by game logic. The shaders have no knowledge of "drive modes" — they just respond to numbers. This keeps the rendering pipeline clean and makes transitions smooth.

### 4.1 Uniform Parameters

**Star shader uniforms** (added to existing `StarUniforms`):

| Field | Type | Range | Purpose |
|-------|------|-------|---------|
| `velocity_dir` | `vec3<f32>` | unit vector | Normalized travel direction |
| `beta` | `f32` | 0.0–0.99 | Speed as fraction of c (for aberration) |
| `streak_factor` | `f32` | 0.0–300 | Star streak length in pixels |
| `flash_intensity` | `f32` | 0.0–1.0 | Additive white flash for transitions |

**Sky shader uniforms** (added to existing `SkyUniforms`):

| Field | Type | Range | Purpose |
|-------|------|-------|---------|
| `warp_dir` | `vec3<f32>` | unit vector | Warp travel direction |
| `warp_intensity` | `f32` | 0.0–1.0 | Tunnel/vignette strength |

### 4.2 Speed-to-Parameter Mapping

| Speed | beta | streak_factor | warp_intensity | Visual description |
|-------|------|---------------|----------------|--------------------|
| 0–100 m/s | 0.0 | 0.0 | 0.0 | Static stars (current) |
| 0.01c | 0.01 | 0.0 | 0.0 | Imperceptible aberration |
| 0.5c | 0.5 | 0.0 | 0.0 | Visible aberration + mild Doppler |
| Cruise 10c | 0.99 | 5.0 | 0.0 | Short streaks, maxed aberration |
| Cruise 100c | 0.99 | 30.0 | 0.0 | Long streaks |
| Cruise 500c | 0.99 | 80.0 | 0.0 | Very long streaks |
| Warp 100,000c | 0.99 | 150.0 | 0.3 | Extreme streaks, tunnel begins |
| Warp 1,000,000c | 0.99 | 250.0 | 0.7 | Full tunnel forming |
| Warp 5,000,000c | 0.99 | 300.0 | 1.0 | Complete warp tunnel |

### 4.3 Star Shader Effects

**Relativistic aberration** (vertex shader):
- Transform each star's unit-sphere direction using: `cos(θ') = (cos(θ) - β) / (1 - β·cos(θ))`
- Stars crowd toward the velocity vector. At β=0.9, stars originally at 90° appear at ~26° from forward.
- Perpendicular component preserved, angle compressed.

**Doppler color shift** (vertex → fragment):
- Doppler factor: `f = sqrt((1 + β·cos(θ)) / (1 - β·cos(θ)))`
- Forward stars: blue-shifted (multiply by `vec3(0.7, 0.85, 1.3)`)
- Rear stars: red-shifted (multiply by `vec3(1.3, 0.85, 0.7)`)
- Relativistic beaming: brightness × `doppler³` (stars ahead dramatically brighter)

**Star streaking** (vertex shader):
- Project velocity direction to screen space to get streak axis.
- Stretch billboard quad along that axis by `streak_factor` pixels.
- Streak length modulated by `sin(θ)` — stars near the velocity axis streak less, stars at 90° streak most.
- Width stays at 1–2 pixels for line-like appearance.

**Fragment shader changes**:
- When `streak_factor > 0`: switch from circular falloff (`length(uv)`) to elongated shape.
- Bright core at head, fading tail. Color shifts blue→white at head, warmer at tail.

### 4.4 Sky Shader Effects (Warp Tunnel)

When `warp_intensity > 0`:

**Rear-hemisphere darkening**:
- `darkening = smoothstep(-0.5, 0.0, dot(view_dir, warp_dir))`
- Galaxy brightness multiplied by darkening. Rear goes nearly black.

**Procedural radial tunnel**:
- Compute radial streaks from screen center using angle-hashing.
- Animate with a time uniform. Streaks rush outward from center.
- Strength controlled by `warp_intensity`.
- Blue-white center, dark edges.

**Radial color tinting** (cheap chromatic aberration substitute):
- Blue tint toward center, red-warm tint toward edges.
- No triple-evaluation needed — just a radial color ramp.

### 4.5 Transition Effects

**Engaging Cruise** (1–2 seconds):
- `beta` ramps from 0 to 0.99 over 1 second (stars crowd forward).
- `streak_factor` begins increasing from 0 once past 1c.
- Smooth, no flash.

**Engaging Warp** (0.5–1.0 seconds after spool completes):
- Brief flash: `flash_intensity` spikes to 1.0, decays exponentially over 0.3s.
- `streak_factor` jumps from cruise-length to warp-length over 0.2 seconds.
- `warp_intensity` ramps from 0 to target over 0.3 seconds.
- The "moment" — slow build-up, then sudden dramatic jump.

**Dropping out of Warp** (0.5 seconds):
- `streak_factor` decays exponentially (fast).
- `warp_intensity` drops immediately (0.1s).
- Brief smaller flash.
- `beta` ramps back down over 0.5 seconds — stars settle back to normal positions.

---

## 5. Coordinate System Integration

### 5.1 galactic_position (light-years)

The existing `galactic_position: WorldPos` field tracks the ship's position in the galaxy.

| Mode | Update rule | Rate |
|------|------------|------|
| Impulse | `galactic_position += ship_velocity_m_s * dt * METERS_TO_LY` | ~10⁻¹³ ly/s (frozen) |
| Cruise | `galactic_position += cruise_dir * cruise_speed_ly_s * dt` | 3×10⁻⁸ to 1.6×10⁻⁵ ly/s |
| Warp | `galactic_position += warp_dir * warp_speed_ly_s * dt` | 0.003 to 0.159 ly/s |

Constant: `METERS_TO_LY: f64 = 1.0 / 9.461e15`

### 5.2 Star Streaming at High Speed

The `StarStreaming` system checks for sector changes each frame. At max warp (0.159 ly/s), the observer crosses one 10 ly sector every ~63 seconds. The current 0.5s fade duration handles this smoothly — sectors have plenty of time to load and fade in.

At max warp, ~81 new sectors enter the query radius per sector crossing (one shell of the 9×9×9 cube). Generating 81 sectors at ~40 stars each = ~3,240 stars. Generation is deterministic O(1) per sector — this takes microseconds.

No changes needed to star streaming for any drive tier.

### 5.3 Physics Interaction

- **Impulse**: Full rapier3d physics. Ship body moves, collisions active, player walks around.
- **Cruise**: Ship body frozen in physics. Position updated via `galactic_position`. Player can walk around (ship interior physics still active). Ship doesn't collide with anything — cruise auto-drops before collision with gravity wells.
- **Warp**: Same as cruise. Ship body frozen. `galactic_position` updated at warp speed. Interior physics active — crew walks around, monitors systems.

---

## 6. Fuel System

### 6.1 Resources

| Resource | Storage | Sources | Used by |
|----------|---------|---------|---------|
| Hydrogen | `fuel: f32` (0.0–1.0) | Gas giants, ice asteroids, nebulae, stations | Impulse (low drain), Cruise (10× drain) |
| Exotic Matter | `exotic_fuel: f32` (0.0–1.0) | Nebula cores, neutron star debris, ancient ruins | Warp only |

### 6.2 Drain Rates

| Drive | Throttle | Drain/s | Full tank duration |
|-------|----------|---------|-------------------|
| Impulse idle | 0% | 0.0005 | 33 min |
| Impulse full | 100% | 0.002 | 8.3 min |
| Cruise min (1c) | 0% | 0.005 | 3.3 min |
| Cruise max (500c) | 100% | 0.02 | 50 sec |
| Warp min (100,000c) | 0% | 0.005 | 3.3 min |
| Warp max (5,000,000c) | 100% | 0.05 | 20 sec |

### 6.3 Gameplay Loop

```
Explore local system (Impulse) → mine hydrogen, find points of interest
  → Cruise to distant planet/asteroid belt → discover exotic matter deposit
  → Gather exotic matter
  → Warp to new star system (nearest star ~5 ly, costs ~5-10% exotic at mid-warp)
  → Explore new system → repeat
```

A full exotic tank at efficient warp (1,000,000c) lasts 3.3 minutes = enough for multiple short hops or one longer journey. Resource tension comes from planning: do you make several short hops to explore nearby stars, or one long jump to a distant target?

Running out of exotic fuel between stars = stranded. Must find exotic matter (unlikely in deep space) or wait for rescue (multiplayer). This creates the game's signature emotion: **the cold fear of deep space**.

---

## 7. Controls

### 7.1 Drive Selection (Seated at Helm)

- **Key 1**: Select Impulse mode (instant)
- **Key 2**: Select Cruise mode (instant engage if in open space)
- **Key 3**: Select Warp mode (begins 5-second spool if in open space + has fuel)
- **Escape**: Disengage to Impulse (instant)

### 7.2 Throttle Mapping

The existing thrust lever serves double duty:
- **Impulse**: lever controls thrust (0–100% of max impulse thrust). Current behavior.
- **Cruise**: lever controls cruise speed (1c at 0%, 500c at 100%). Logarithmic mapping.
- **Warp**: lever controls warp speed (100,000c at 0%, 5,000,000c at 100%). Logarithmic mapping.

### 7.3 Cockpit Additions

No new physical interactables needed for Phase A–B. The drive mode is selected via keyboard (1/2/3) while seated. The speed display monitor shows current drive mode, speed, and fuel. Future phases can add physical console buttons.

---

## 8. Architecture

### 8.1 New Module: `sa_ship/src/drive.rs`

```
DriveMode { Impulse, Cruise, Warp }
DriveStatus { Idle, Spooling(f32), Engaged, Disengaging }
DriveController {
    mode: DriveMode,
    status: DriveStatus,
    speed_fraction: f32,     // 0.0–1.0 throttle within current tier
    spool_progress: f32,     // 0.0–1.0 for warp charge-up
}
```

Methods: `request_engage(mode)`, `request_disengage()`, `update(dt)`, `current_speed_c() -> f64`, `current_speed_ly_s() -> f64`.

### 8.2 Modified: `sa_survival/src/resources.rs`

Add `exotic_fuel: f32` field to `ShipResources`. Add `drain_exotic(rate, dt)` method. Modify `update()` to accept `DriveMode` parameter.

### 8.3 New: `spaceaway/src/drive_integration.rs`

Maps `DriveController` state to:
- `galactic_position` updates (coordinate system)
- Shader uniform parameters (beta, streak_factor, warp_intensity, flash_intensity)
- Star streaming observer position

Single function: `compute_drive_effects(drive: &DriveController, dt: f32) -> DriveEffects`

Where `DriveEffects` contains the galactic_position delta and all shader parameters.

### 8.4 Modified Shaders

- `stars.wgsl`: Add aberration, Doppler, streaking to vertex/fragment shaders
- `sky.wgsl`: Add warp tunnel overlay and rear-hemisphere darkening

### 8.5 Modified UI

- `helm_screen.rs`: Show drive mode, drive-specific speed, exotic fuel gauge
- `hud.rs`: Drive mode indicator (IMPULSE / CRUISE / WARP with status)

---

## 9. Implementation Phases

**Phase A — Drive System Core + Coordinate Mapping**
- `drive.rs` module with DriveMode, DriveController, speed calculations
- `drive_integration.rs` for galactic_position updates
- Helm key bindings (1/2/3) for drive selection
- Testable: engage cruise/warp from helm, ship moves through universe at correct speed, star streaming works

**Phase B — Visual Effects**
- Star shader: aberration + Doppler + streaking
- Sky shader: warp tunnel overlay + rear darkening
- Transition effects (flash, smooth parameter ramps)
- Testable: visuals match speed tier, transitions look smooth

**Phase C — Fuel System + Engagement Rules**
- Exotic fuel resource
- Per-tier drain rates
- Gravity well detection + auto-drop
- Warp spool timer
- Emergency drop-out on fuel empty
- Testable: fuel depletes correctly, drives disengage when empty, gravity wells block engagement

**Phase D — UI Updates**
- Helm screen: drive mode, speed, exotic fuel gauge
- HUD: drive mode indicator
- Testable: player sees current mode and fuel state at all times

---

## 10. Testing Strategy

Each phase includes unit tests for the new module and integration tests via gameplay:

- **Drive math**: speed_c(), speed_ly_s() return correct values for all tiers
- **Coordinate mapping**: galactic_position moves at correct rate for each tier
- **Fuel drain**: rates match spec for each tier × throttle combination
- **Spool timer**: warp takes exactly 5 seconds to engage
- **Parameter mapping**: shader uniforms produce correct values at speed boundaries
- **Star streaming**: no popping at any speed tier (visual test)
- **Transition smoothness**: no jarring visual discontinuities (visual test)
