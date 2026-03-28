# Phase 5b-slim: The Gameplay Loop — Design Spec

## Overview

Build the minimal complete gameplay loop: fuel/power/O2 survival + resource nodes in space + sensors to find them + gathering mechanic + consequences. The player has a reason to explore and pressure to keep moving.

## The Loop

```
Start with limited fuel
    → Check sensor monitor for nearby resources
    → Fly to asteroid/derelict
    → Gather fuel/supplies
    → Continue exploring deeper into space
    → Manage power along the way
    → Repeat
```

## Scope

### What we build:
- sa_survival crate: Fuel, Oxygen, Power as simple drain-rate resources
- Resource nodes in sa_universe
- Sensors monitor (egui)
- Gathering mechanic (proximity interaction)
- Consequences (visual feedback for low resources)
- HUD warning indicators

### What we DON'T build:
- Complex power routing/distribution panels
- Hull damage, breaches, temperature
- Player hunger/thirst
- Multiplayer sync of resources
- Audio warnings (Phase 7)
- Death screen / restart (just fade to black for now)

## Architecture

### New Crate: sa_survival

Game Logic layer. Depends on: sa_core, sa_math.

```rust
pub struct ShipResources {
    /// Fuel remaining (0.0 to 1.0). Burns slowly from reactor, faster from engines.
    pub fuel: f32,
    /// Oxygen remaining (0.0 to 1.0). Depletes when life support has no power.
    pub oxygen: f32,
    /// Power output (0.0 to 1.0). Proportional to fuel availability.
    pub power: f32,
}
```

**Drain rates (per second):**
- Reactor idle fuel burn: 0.0005 (lasts ~33 minutes at idle)
- Engine fuel burn: 0.002 × throttle (full throttle lasts ~8 minutes)
- Total fuel drain: `0.0005 + 0.002 * throttle`
- O2 drain when no power: 0.005 (lasts ~3.3 minutes without life support)
- O2 regen when powered: +0.002 (slowly recovers when life support has power)
- Power = fuel > 0 ? 1.0 : 0.0 (simple on/off for now)

**Update function:**
```rust
pub fn update(&mut self, dt: f32, throttle: f32, engine_on: bool) {
    // Fuel drain
    let engine_drain = if engine_on { 0.002 * throttle } else { 0.0 };
    self.fuel = (self.fuel - (0.0005 + engine_drain) * dt).max(0.0);

    // Power from fuel
    self.power = if self.fuel > 0.0 { 1.0 } else { 0.0 };

    // O2: drains without power, regenerates with power
    if self.power > 0.0 {
        self.oxygen = (self.oxygen + 0.002 * dt).min(1.0);
    } else {
        self.oxygen = (self.oxygen - 0.005 * dt).max(0.0);
    }
}
```

### Resource Nodes in sa_universe

Add to `sa_universe/src/galaxy.rs` or new `sa_universe/src/resources.rs`:

```rust
pub struct ResourceDeposit {
    pub position: WorldPos,
    pub kind: ResourceKind,
    pub amount: f32,      // 0.0 to 1.0 (how much fuel/supplies)
    pub id: ObjectId,
}

pub enum ResourceKind {
    FuelAsteroid,         // refuels the ship
    SupplyCache,          // restores O2 supplies
    Derelict,             // both fuel and supplies
}
```

Generated deterministically from sector seed, placed near star systems. ~2-5 per sector. Each has a position offset from the nearest star.

### Sensors Monitor

New egui monitor at the sensors station. Shows:
- List of nearby resource deposits (within query radius)
- Each entry: icon (asteroid/derelict), distance, direction arrow, type
- Sorted by distance (closest first)
- Station accent color: purple

Layout:
```
┌─────────────────────────┐
│  ◆ SENSORS              │
│                         │
│  ▸ Fuel Asteroid  12 ly │
│  ▸ Supply Cache   28 ly │
│  ▸ Derelict       41 ly │
│                         │
│  [3 contacts detected]  │
└─────────────────────────┘
```

### Gathering Mechanic

When the ship is within 100m of a resource deposit:
- A new interactable appears on the HUD: "Gather Resources"
- The crosshair changes to a gather icon
- Click to start gathering (instant for now — fill resource by deposit amount)
- The deposit is consumed (marked as depleted in a HashSet of gathered IDs)

For Phase 5b-slim, gathering is instant. Future: progress bar, EVA to the asteroid, mining mini-game.

### Consequences

**Low fuel (< 20%):**
- Engine thrust reduced proportionally
- HUD fuel bar appears (amber color)
- Sensors monitor shows "LOW FUEL" warning

**No fuel (0%):**
- No engine thrust — ship drifts on momentum
- No power — life support stops
- O2 starts draining

**Low O2 (< 30%):**
- Screen edges darken (vignette shader effect)
- HUD O2 bar appears (red color)

**No O2 (0%):**
- Screen fades to black
- Game pauses with "LIFE SUPPORT FAILURE" text
- For now, just stops — no restart/respawn yet

### Vignette Shader Effect

Add a post-processing pass: a fullscreen quad that darkens the screen edges based on O2 level. When O2 = 1.0, no effect. When O2 = 0.3, subtle darkening. When O2 = 0.0, screen nearly black.

```wgsl
// In post-processing or as part of the HUD render:
let dist_from_center = length(uv - 0.5) * 2.0;
let vignette = smoothstep(0.3, 1.2, dist_from_center);
let darkness = 1.0 - (1.0 - oxygen) * vignette;
color *= darkness;
```

### HUD Warning Indicators

Extend the existing HUD system:

**Fuel bar:** Horizontal bar, bottom-left corner. Amber when < 50%, red when < 20%. Only visible when < 50%.

**O2 bar:** Horizontal bar, bottom-right corner. Blue when > 50%, red when < 30%. Only visible when < 80%.

Both bars: thin, clean, minimal. Fade in/out smoothly.

### Monitors

Two monitors total (one already exists):
1. **Helm monitor** (exists) — speed, throttle, engine. Add fuel gauge.
2. **Sensors monitor** (new) — nearby resources, distance, type.

Future monitors: engineering (power distribution), navigation (star map).

## File Structure

```
crates/sa_survival/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── resources.rs      # ShipResources, update logic, drain rates

crates/sa_universe/src/
└── resources.rs          # ResourceDeposit, ResourceKind, generation

crates/spaceaway/src/
└── ui/
    ├── hud.rs            # Add fuel/O2 bars, vignette
    └── sensors_screen.rs # New sensors monitor layout
```

## Testing

- ShipResources: fuel drains at correct rate, O2 recovers with power, dies without
- Resource generation: deterministic, reasonable count per sector
- Gathering: deposit consumed, resources restored
- Consequences: thrust reduction at low fuel, vignette at low O2
- HUD: bars appear/disappear at correct thresholds
