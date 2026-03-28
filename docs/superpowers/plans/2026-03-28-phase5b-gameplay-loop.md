# Phase 5b-slim: The Gameplay Loop -- Implementation Plan

**Date:** 2026-03-28
**Spec:** `docs/superpowers/specs/2026-03-28-phase5b-gameplay-loop-design.md`
**Status:** Implementing

## Overview

Build the minimal complete gameplay loop: fuel/power/O2 survival resources, resource deposits in game coordinates, a sensors monitor to find them, gathering mechanic, visual consequences, and HUD warning bars.

## Architecture Decisions

- **sa_survival** is a pure-math crate (no physics, no rendering). Depends only on sa_core, sa_math.
- **Resource deposits** are placed in GAME coordinates (meters from origin), not universe light-years. This avoids the physics-vs-universe coordinate mismatch. Deposits are within a few km of the starting point for Phase 5b-slim.
- **Sensors monitor** follows the helm_screen.rs pattern: separate egui context, separate offscreen texture, second ScreenQuad in the cockpit.
- **Gathering** uses proximity check in physics coordinates (ship position vs deposit position). Threshold: 500 meters.
- **Vignette** is drawn via egui Painter as semi-transparent rects, not a shader pass.
- **Gathered deposits** tracked in a `HashSet<u64>` keyed by deposit ID.

## Tasks

### Task 1: sa_survival crate -- ShipResources

New crate `crates/sa_survival/`.

**Files:**
- `Cargo.toml` -- depends on sa_core, sa_math (workspace deps)
- `src/lib.rs` -- re-exports
- `src/resources.rs` -- ShipResources struct + update logic + tests

**ShipResources:**
- `fuel: f32` (0.0-1.0), `oxygen: f32` (0.0-1.0), `power: f32` (0.0-1.0)
- `update(dt, throttle, engine_on)` per spec drain rates
- `add_fuel(amount)`, `add_oxygen(amount)` for gathering
- Start: fuel=1.0, oxygen=1.0, power=1.0

**Tests:** drain rates, O2 recovery, fuel zeroes out, power follows fuel.

**Wiring:**
- Add `sa_survival = { path = "crates/sa_survival" }` to workspace Cargo.toml
- Add `"crates/sa_survival"` to workspace members
- Add `sa_survival.workspace = true` to spaceaway's Cargo.toml

---

### Task 2: Resource deposits (sa_survival, not sa_universe)

Since deposits are in game coordinates (meters), they belong in sa_survival, not sa_universe.

**File:** `crates/sa_survival/src/deposits.rs`

**Types:**
- `ResourceKind { FuelAsteroid, SupplyCache, Derelict }`
- `ResourceDeposit { id: u64, position: [f32; 3], kind: ResourceKind, amount: f32 }`

**Functions:**
- `generate_deposits(seed: u64) -> Vec<ResourceDeposit>` -- 8-12 deposits within 5km of origin
- Deterministic from seed

**No universe dependency.** Simple positions in meters.

**Tests:** deterministic generation, count range, valid positions.

---

### Task 3: Sensors monitor (egui)

**File:** `crates/spaceaway/src/ui/sensors_screen.rs`

Follow helm_screen.rs pattern exactly:
- `SensorsData { deposits: Vec<SensorContact>, ship_fuel: f32, ship_oxygen: f32 }`
- `SensorContact { kind: &str, distance: f32, direction: [f32; 3], gathered: bool }`
- `draw_sensors_screen(ctx, data)` -- purple accent, list of contacts sorted by distance

**UiSystem changes (mod.rs):**
- Add second offscreen texture + context + renderer for sensors monitor
- `render_sensors_monitor(...)` method
- `sensors_texture_view()` accessor

---

### Task 4: Gathering mechanic

**Changes in main.rs:**
- Add `deposits: Vec<ResourceDeposit>` and `gathered: HashSet<u64>` to App
- Generate deposits in `App::new()` using seed
- Each frame: check ship position vs each ungathered deposit (distance < 500m)
- If within range and left-click: transfer amount to ShipResources, add to gathered set
- Pass `nearest_gatherable` info to HudState for crosshair icon

**HUD changes (hud.rs):**
- Add `gather_available: bool` to HudState
- New crosshair icon: diamond/pickup shape when gather is available

---

### Task 5: Consequences -- visual feedback

**Low fuel (<20%):** reduce `ship.max_thrust` to `fuel * 5.0 * Ship::DEFAULT_MAX_THRUST` (linear reduction)

**Low O2 (<30%):** vignette darkening via egui Painter
- Draw 4 semi-transparent black rects at screen edges
- Intensity = `(0.3 - oxygen) / 0.3` clamped to [0, 1]

**No O2 (0%):** full screen dark overlay with "LIFE SUPPORT FAILURE" text

**Helm monitor:** add fuel gauge bar below engine status

---

### Task 6: HUD warning bars

**File:** `crates/spaceaway/src/ui/hud.rs`

- Fuel bar: bottom-left, amber->red, visible when fuel < 50%
- O2 bar: bottom-right, blue->red, visible when O2 < 80%
- Thin horizontal bars (200px wide, 6px tall)
- Drawn via egui Painter in the HUD overlay

**HudState additions:** `fuel: f32`, `oxygen: f32`

---

## Execution Order

1. Task 1: sa_survival crate + cargo check
2. Task 2: Resource deposits + tests
3. Task 3: Sensors monitor + UiSystem wiring
4. Task 4: Gathering mechanic in main.rs
5. Task 5: Consequences (thrust reduction, vignette, helm fuel gauge)
6. Task 6: HUD warning bars

After all tasks: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, commit + push.
