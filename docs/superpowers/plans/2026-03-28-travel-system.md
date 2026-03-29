# Travel System Phase A: Drive Core + Coordinate Mapping

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the three-tier drive system (Impulse/Cruise/Warp) with correct galactic coordinate mapping so the ship moves through the universe at the right speed for each tier.

**Architecture:** New `drive.rs` module in `sa_ship` defines DriveMode/DriveController. New `drive_integration.rs` in `spaceaway` maps drive state to `galactic_position` updates. Helm controller extended with drive selection keys (1/2/3). Star streaming and rendering use `galactic_position` (already working).

**Tech Stack:** Rust, sa_ship crate, sa_survival crate, spaceaway binary, existing physics/rendering pipeline.

**Spec:** `docs/superpowers/specs/2026-03-28-travel-system-design.md` — Sections 3, 5, 6, 7.

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sa_ship/src/drive.rs` | Create | DriveMode enum, DriveStatus, DriveController with speed math |
| `crates/sa_ship/src/lib.rs` | Modify | Export drive module |
| `crates/sa_ship/src/helm.rs` | Modify | Add drive selection keys (1/2/3) while seated |
| `crates/sa_survival/src/resources.rs` | Modify | Add `exotic_fuel` field, drive-aware drain |
| `crates/spaceaway/src/drive_integration.rs` | Create | Map DriveController state → galactic_position delta |
| `crates/spaceaway/src/main.rs` | Modify | Wire drive system into game loop, helm section |
| `crates/spaceaway/src/ui/helm_screen.rs` | Modify | Show drive mode + speed on helm monitor |

---

### Task 1: DriveMode and DriveStatus enums

**Files:**
- Create: `crates/sa_ship/src/drive.rs`
- Modify: `crates/sa_ship/src/lib.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/sa_ship/src/drive.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_mode_default_is_impulse() {
        let dc = DriveController::new();
        assert_eq!(dc.mode(), DriveMode::Impulse);
        assert_eq!(dc.status(), DriveStatus::Idle);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p sa_ship drive_mode_default`
Expected: FAIL — `DriveController` not found

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/sa_ship/src/drive.rs
//! Drive system: three-tier propulsion with speed calculations.
//!
//! - Impulse: Newtonian, 0–1000 m/s
//! - Cruise: velocity-based, 1c–500c (in-system)
//! - Warp: space distortion, 100,000c–5,000,000c (interstellar)

/// Speed of light in m/s.
pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;
/// Meters per light-year.
pub const METERS_PER_LY: f64 = 9.461e15;
/// Light-years per second at 1c.
pub const LY_PER_SECOND_AT_C: f64 = 3.169e-8;

/// Cruise speed range (multiples of c).
pub const CRUISE_MIN_C: f64 = 1.0;
pub const CRUISE_MAX_C: f64 = 500.0;

/// Warp speed range (multiples of c).
pub const WARP_MIN_C: f64 = 100_000.0;
pub const WARP_MAX_C: f64 = 5_000_000.0;

/// Warp spool time in seconds.
pub const WARP_SPOOL_TIME: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveMode {
    Impulse,
    Cruise,
    Warp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DriveStatus {
    /// Drive is off / not engaged.
    Idle,
    /// Warp drive is charging (0.0 to 1.0 progress).
    Spooling(f32),
    /// Drive is active and producing velocity.
    Engaged,
}

pub struct DriveController {
    mode: DriveMode,
    status: DriveStatus,
    /// Throttle within current tier (0.0 = min speed, 1.0 = max speed).
    speed_fraction: f32,
}

impl DriveController {
    pub fn new() -> Self {
        Self {
            mode: DriveMode::Impulse,
            status: DriveStatus::Idle,
            speed_fraction: 0.0,
        }
    }

    pub fn mode(&self) -> DriveMode {
        self.mode
    }

    pub fn status(&self) -> DriveStatus {
        self.status
    }

    pub fn speed_fraction(&self) -> f32 {
        self.speed_fraction
    }

    pub fn set_speed_fraction(&mut self, f: f32) {
        self.speed_fraction = f.clamp(0.0, 1.0);
    }
}
```

- [ ] **Step 4: Register the module**

Add to `crates/sa_ship/src/lib.rs`:

```rust
pub mod drive;
```

And add to the `pub use` section:

```rust
pub use drive::{DriveMode, DriveStatus, DriveController};
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p sa_ship drive_mode_default`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/sa_ship/src/drive.rs crates/sa_ship/src/lib.rs
git commit -m "feat(drive): DriveMode, DriveStatus, DriveController skeleton"
```

---

### Task 2: Drive engagement and disengagement

**Files:**
- Modify: `crates/sa_ship/src/drive.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn engage_cruise_from_impulse() {
    let mut dc = DriveController::new();
    assert!(dc.request_engage(DriveMode::Cruise));
    assert_eq!(dc.mode(), DriveMode::Cruise);
    assert_eq!(dc.status(), DriveStatus::Engaged);
}

#[test]
fn engage_warp_starts_spooling() {
    let mut dc = DriveController::new();
    assert!(dc.request_engage(DriveMode::Warp));
    assert_eq!(dc.mode(), DriveMode::Warp);
    assert_eq!(dc.status(), DriveStatus::Spooling(0.0));
}

#[test]
fn warp_spool_completes() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Warp);
    // Simulate 5 seconds of updates
    for _ in 0..300 {
        dc.update(1.0 / 60.0);
    }
    assert_eq!(dc.status(), DriveStatus::Engaged);
}

#[test]
fn disengage_returns_to_impulse() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Cruise);
    dc.request_disengage();
    assert_eq!(dc.mode(), DriveMode::Impulse);
    assert_eq!(dc.status(), DriveStatus::Idle);
}

#[test]
fn cannot_engage_impulse_explicitly() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Cruise);
    // "Engaging" impulse is just disengaging
    assert!(!dc.request_engage(DriveMode::Impulse));
    assert_eq!(dc.mode(), DriveMode::Cruise);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_ship drive`
Expected: FAIL — methods not found

- [ ] **Step 3: Implement engagement logic**

Add to `DriveController` impl block in `drive.rs`:

```rust
/// Request engagement of a drive mode.
/// Cruise engages instantly. Warp begins spooling.
/// Returns false if the request is invalid (e.g., engaging Impulse).
pub fn request_engage(&mut self, mode: DriveMode) -> bool {
    match mode {
        DriveMode::Impulse => false, // Use request_disengage() instead
        DriveMode::Cruise => {
            self.mode = DriveMode::Cruise;
            self.status = DriveStatus::Engaged;
            true
        }
        DriveMode::Warp => {
            self.mode = DriveMode::Warp;
            self.status = DriveStatus::Spooling(0.0);
            true
        }
    }
}

/// Disengage current drive, return to Impulse.
pub fn request_disengage(&mut self) {
    self.mode = DriveMode::Impulse;
    self.status = DriveStatus::Idle;
}

/// Update per frame. Advances warp spool progress.
pub fn update(&mut self, dt: f32) {
    if let DriveStatus::Spooling(progress) = self.status {
        let new_progress = progress + dt / WARP_SPOOL_TIME;
        if new_progress >= 1.0 {
            self.status = DriveStatus::Engaged;
        } else {
            self.status = DriveStatus::Spooling(new_progress);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_ship drive`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sa_ship/src/drive.rs
git commit -m "feat(drive): engagement, disengagement, warp spool timer"
```

---

### Task 3: Speed calculations (c, ly/s)

**Files:**
- Modify: `crates/sa_ship/src/drive.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn impulse_speed_is_zero_c() {
    let dc = DriveController::new();
    assert_eq!(dc.current_speed_c(), 0.0);
    assert_eq!(dc.current_speed_ly_s(), 0.0);
}

#[test]
fn cruise_speed_at_min_throttle() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Cruise);
    dc.set_speed_fraction(0.0);
    let speed = dc.current_speed_c();
    assert!((speed - CRUISE_MIN_C).abs() < 0.01, "got {speed}");
}

#[test]
fn cruise_speed_at_max_throttle() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Cruise);
    dc.set_speed_fraction(1.0);
    let speed = dc.current_speed_c();
    assert!((speed - CRUISE_MAX_C).abs() < 1.0, "got {speed}");
}

#[test]
fn warp_speed_at_max_throttle() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Warp);
    // Complete spool
    for _ in 0..300 { dc.update(1.0 / 60.0); }
    dc.set_speed_fraction(1.0);
    let speed = dc.current_speed_c();
    assert!((speed - WARP_MAX_C).abs() < 100.0, "got {speed}");
}

#[test]
fn warp_spooling_has_zero_speed() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Warp);
    dc.set_speed_fraction(1.0);
    // Still spooling — speed should be zero
    assert_eq!(dc.current_speed_c(), 0.0);
}

#[test]
fn cruise_ly_s_at_500c() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Cruise);
    dc.set_speed_fraction(1.0);
    let ly_s = dc.current_speed_ly_s();
    // 500c = 500 * 3.169e-8 = 1.585e-5 ly/s
    assert!((ly_s - 1.585e-5).abs() < 1e-7, "got {ly_s}");
}

#[test]
fn warp_ly_s_at_5m_c() {
    let mut dc = DriveController::new();
    dc.request_engage(DriveMode::Warp);
    for _ in 0..300 { dc.update(1.0 / 60.0); }
    dc.set_speed_fraction(1.0);
    let ly_s = dc.current_speed_ly_s();
    // 5,000,000c = 5e6 * 3.169e-8 = 0.1585 ly/s
    assert!((ly_s - 0.1585).abs() < 0.001, "got {ly_s}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_ship drive`
Expected: FAIL — methods not found

- [ ] **Step 3: Implement speed calculations**

Add to `DriveController` impl block:

```rust
/// Current speed in multiples of c.
/// Returns 0.0 for Impulse mode or if drive is not Engaged.
pub fn current_speed_c(&self) -> f64 {
    if self.status != DriveStatus::Engaged {
        return 0.0;
    }
    match self.mode {
        DriveMode::Impulse => 0.0,
        DriveMode::Cruise => {
            // Logarithmic mapping: 1c at 0%, 500c at 100%
            let t = self.speed_fraction as f64;
            CRUISE_MIN_C * (CRUISE_MAX_C / CRUISE_MIN_C).powf(t)
        }
        DriveMode::Warp => {
            let t = self.speed_fraction as f64;
            WARP_MIN_C * (WARP_MAX_C / WARP_MIN_C).powf(t)
        }
    }
}

/// Current speed in light-years per second.
pub fn current_speed_ly_s(&self) -> f64 {
    self.current_speed_c() * LY_PER_SECOND_AT_C
}
```

Also need `PartialEq` on `DriveStatus` — update the enum:

```rust
impl PartialEq for DriveStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Idle, Self::Idle) => true,
            (Self::Engaged, Self::Engaged) => true,
            (Self::Spooling(a), Self::Spooling(b)) => (a - b).abs() < 1e-6,
            _ => false,
        }
    }
}
```

Wait — `DriveStatus` already derives `PartialEq`. But comparing floats with `==` is fine for the exact cases we test (0.0 and specific values). The tests use `assert_eq!` on the status after spool completion which will be `Engaged`, not a float comparison. This is fine as-is.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_ship drive`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/sa_ship/src/drive.rs
git commit -m "feat(drive): speed calculations in c and ly/s with log mapping"
```

---

### Task 4: Drive-aware fuel drain

**Files:**
- Modify: `crates/sa_survival/src/resources.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn exotic_fuel_starts_full() {
    let r = ShipResources::new();
    assert_eq!(r.exotic_fuel, 1.0);
}

#[test]
fn cruise_drains_fuel_faster() {
    let mut r = ShipResources::new();
    r.update_with_drive(1.0, 0.0, false, DriveMode::Cruise, 1.0);
    // Cruise at full: IDLE + CRUISE_MAX = 0.0005 + 0.02 = 0.0205/s
    let expected = 1.0 - 0.0005 - 0.02;
    assert!((r.fuel - expected).abs() < 1e-4, "fuel={}", r.fuel);
}

#[test]
fn warp_drains_exotic_fuel() {
    let mut r = ShipResources::new();
    r.update_with_drive(1.0, 0.0, false, DriveMode::Warp, 1.0);
    // Warp at full: 0.05/s exotic drain
    let expected = 1.0 - 0.05;
    assert!((r.exotic_fuel - expected).abs() < 1e-4, "exotic={}", r.exotic_fuel);
}

#[test]
fn warp_does_not_drain_hydrogen() {
    let mut r = ShipResources::new();
    let fuel_before = r.fuel;
    r.update_with_drive(1.0, 0.0, false, DriveMode::Warp, 1.0);
    // Only idle drain, no engine drain during warp
    let expected = fuel_before - 0.0005;
    assert!((r.fuel - expected).abs() < 1e-4, "fuel={}", r.fuel);
}

#[test]
fn impulse_unchanged_from_original() {
    let mut r1 = ShipResources::new();
    let mut r2 = ShipResources::new();
    r1.update(1.0, 0.5, true);
    r2.update_with_drive(1.0, 0.5, true, DriveMode::Impulse, 0.0);
    assert!((r1.fuel - r2.fuel).abs() < 1e-6);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_survival`
Expected: FAIL — `exotic_fuel` not found, `update_with_drive` not found, `DriveMode` not found

- [ ] **Step 3: Add sa_ship dependency to sa_survival**

Check `crates/sa_survival/Cargo.toml` and add:

```toml
sa_ship = { path = "../sa_ship" }
```

- [ ] **Step 4: Implement exotic fuel and drive-aware drain**

Modify `crates/sa_survival/src/resources.rs`:

Add constants:

```rust
/// Cruise drive hydrogen drain at max speed (per second).
const CRUISE_FUEL_DRAIN: f32 = 0.02;
/// Warp drive exotic fuel drain at min speed (per second).
const WARP_EXOTIC_DRAIN_MIN: f32 = 0.005;
/// Warp drive exotic fuel drain at max speed (per second).
const WARP_EXOTIC_DRAIN_MAX: f32 = 0.05;
```

Add `exotic_fuel` field to `ShipResources`:

```rust
pub struct ShipResources {
    pub fuel: f32,
    pub oxygen: f32,
    pub power: f32,
    /// Exotic matter for warp drive (0.0 to 1.0). Rare resource.
    pub exotic_fuel: f32,
}
```

Update `new()` and `Default`:

```rust
pub fn new() -> Self {
    Self {
        fuel: 1.0,
        oxygen: 1.0,
        power: 1.0,
        exotic_fuel: 1.0,
    }
}
```

Add the new method (keep the old `update` unchanged for backward compat):

```rust
use sa_ship::DriveMode;

/// Update resources for one frame with drive mode awareness.
///
/// - `dt`: delta time in seconds
/// - `throttle`: impulse throttle 0.0–1.0
/// - `engine_on`: whether impulse engine is firing
/// - `drive`: current drive mode
/// - `drive_fraction`: drive throttle 0.0–1.0 (cruise/warp speed setting)
pub fn update_with_drive(
    &mut self,
    dt: f32,
    throttle: f32,
    engine_on: bool,
    drive: DriveMode,
    drive_fraction: f32,
) {
    // Hydrogen drain: idle always, engine only during impulse, cruise adds extra
    let engine_drain = match drive {
        DriveMode::Impulse => {
            if engine_on { ENGINE_FUEL_DRAIN * throttle } else { 0.0 }
        }
        DriveMode::Cruise => {
            // Cruise uses hydrogen at 10x rate, scaled by throttle
            CRUISE_FUEL_DRAIN * drive_fraction
        }
        DriveMode::Warp => 0.0, // Warp uses exotic fuel, not hydrogen
    };
    self.fuel = (self.fuel - (IDLE_FUEL_DRAIN + engine_drain) * dt).max(0.0);

    // Exotic fuel drain (warp only)
    if drive == DriveMode::Warp {
        let exotic_drain = WARP_EXOTIC_DRAIN_MIN
            + (WARP_EXOTIC_DRAIN_MAX - WARP_EXOTIC_DRAIN_MIN) * drive_fraction;
        self.exotic_fuel = (self.exotic_fuel - exotic_drain * dt).max(0.0);
    }

    // Power from fuel
    self.power = if self.fuel > 0.0 { 1.0 } else { 0.0 };

    // O2 logic unchanged
    if self.power > 0.0 {
        self.oxygen = (self.oxygen + O2_REGEN * dt).min(1.0);
    } else {
        self.oxygen = (self.oxygen - O2_DRAIN * dt).max(0.0);
    }
}

/// Add exotic fuel from a gathered resource.
pub fn add_exotic_fuel(&mut self, amount: f32) {
    self.exotic_fuel = (self.exotic_fuel + amount).min(1.0);
}
```

Note: `DriveMode` needs `PartialEq` and `Eq` — it already derives them.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p sa_survival`
Expected: PASS (both old and new tests)

- [ ] **Step 6: Commit**

```bash
git add crates/sa_survival/src/resources.rs crates/sa_survival/Cargo.toml
git commit -m "feat(resources): exotic fuel + drive-aware drain rates"
```

---

### Task 5: Drive integration — galactic position updates

**Files:**
- Create: `crates/spaceaway/src/drive_integration.rs`

- [ ] **Step 1: Create the module with tests**

```rust
// crates/spaceaway/src/drive_integration.rs
//! Maps DriveController state to galactic_position deltas.
//!
//! Each drive tier moves galactic_position at a different rate:
//! - Impulse: effectively frozen (m/s → ly/s ≈ 0)
//! - Cruise: moves at 1c–500c in ly/s
//! - Warp: moves at 100,000c–5,000,000c in ly/s

use sa_math::WorldPos;
use sa_ship::drive::{DriveController, DriveMode, DriveStatus};

/// Compute the galactic position delta for this frame.
///
/// `direction`: normalized travel direction (ship forward in world space).
/// `dt`: frame delta time in seconds.
///
/// Returns the delta to add to `galactic_position`.
pub fn galactic_position_delta(
    drive: &DriveController,
    direction: [f64; 3],
    dt: f64,
) -> [f64; 3] {
    let speed_ly_s = drive.current_speed_ly_s();
    if speed_ly_s < 1e-20 {
        return [0.0, 0.0, 0.0];
    }
    [
        direction[0] * speed_ly_s * dt,
        direction[1] * speed_ly_s * dt,
        direction[2] * speed_ly_s * dt,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_delta_is_zero() {
        let dc = DriveController::new();
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0 / 60.0);
        assert!(delta[0].abs() < 1e-20);
        assert!(delta[1].abs() < 1e-20);
        assert!(delta[2].abs() < 1e-20);
    }

    #[test]
    fn cruise_delta_moves_forward() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Cruise);
        dc.set_speed_fraction(1.0); // 500c
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        // 500c = 1.585e-5 ly/s, direction is -Z
        assert!(delta[2] < -1e-6, "should move in -Z, got {}", delta[2]);
        assert!((delta[2].abs() - 1.585e-5).abs() < 1e-7, "delta={}", delta[2]);
    }

    #[test]
    fn warp_delta_is_large() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        // Complete spool
        for _ in 0..300 { dc.update(1.0 / 60.0); }
        dc.set_speed_fraction(1.0); // 5,000,000c
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        // 5e6c = 0.1585 ly/s
        assert!((delta[2].abs() - 0.1585).abs() < 0.001, "delta={}", delta[2]);
    }

    #[test]
    fn spooling_warp_has_zero_delta() {
        let mut dc = DriveController::new();
        dc.request_engage(DriveMode::Warp);
        // Still spooling
        dc.set_speed_fraction(1.0);
        let delta = galactic_position_delta(&dc, [0.0, 0.0, -1.0], 1.0);
        assert!(delta[2].abs() < 1e-20);
    }
}
```

- [ ] **Step 2: Register the module**

Add to `crates/spaceaway/src/main.rs`:

```rust
mod drive_integration;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p spaceaway drive_integration`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/spaceaway/src/drive_integration.rs crates/spaceaway/src/main.rs
git commit -m "feat: drive_integration maps DriveController to galactic_position deltas"
```

---

### Task 6: Wire drive into the game loop

**Files:**
- Modify: `crates/spaceaway/src/main.rs`

This is the integration task. The DriveController lives on the App struct. The helm section uses it.

- [ ] **Step 1: Add DriveController to App struct**

Find the `struct App` fields (around line 167) and add:

```rust
drive: sa_ship::DriveController,
```

Initialize in the `App` constructor (around line 245):

```rust
drive: sa_ship::DriveController::new(),
```

- [ ] **Step 2: Add drive key bindings to helm seated section**

Find the helm seated section (around line 776 — `else if self.helm.as_ref().map(|h| h.is_seated())`). After the `update_seated` call and before `apply_thrust`, add drive selection:

```rust
// Drive mode selection (1/2/3) while seated
if input.keyboard.just_pressed(KeyCode::Digit1) {
    self.drive.request_disengage();
    log::info!("Drive: IMPULSE");
}
if input.keyboard.just_pressed(KeyCode::Digit2) {
    if self.drive.request_engage(sa_ship::DriveMode::Cruise) {
        log::info!("Drive: CRUISE engaged");
    }
}
if input.keyboard.just_pressed(KeyCode::Digit3) {
    if self.ship_resources.exotic_fuel > 0.0 {
        if self.drive.request_engage(sa_ship::DriveMode::Warp) {
            log::info!("Drive: WARP spooling...");
        }
    } else {
        log::warn!("Cannot engage warp: no exotic fuel");
    }
}
```

Note: these key bindings (1/2/3) conflict with the teleport keys. The teleport keys are only active in the general key handler, not in the helm seated section. The helm seated section has its own input handling inside the `if self.helm.as_ref().map(|h| h.is_seated())` block. So there is no conflict — 1/2/3 select drives when seated, and teleport when not seated.

- [ ] **Step 3: Update drive controller each frame**

In the helm seated section, after the drive key bindings and before physics step:

```rust
// Update drive spool progress
self.drive.update(dt);

// Map throttle lever to drive speed fraction when in cruise/warp
match self.drive.mode() {
    sa_ship::DriveMode::Cruise | sa_ship::DriveMode::Warp => {
        if let Some(ship) = &self.ship {
            self.drive.set_speed_fraction(ship.throttle);
        }
    }
    _ => {}
}
```

- [ ] **Step 4: Update galactic_position from drive**

In the helm seated section, after the physics step, add the galactic position update:

```rust
// Update galactic position based on drive speed
if self.drive.mode() != sa_ship::DriveMode::Impulse {
    // Get ship forward direction for cruise/warp travel direction
    let direction = if let Some(ship) = &self.ship {
        if let Some(body) = self.physics.get_body(ship.body_handle) {
            let rot = body.rotation();
            let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
            [fwd.x as f64, fwd.y as f64, fwd.z as f64]
        } else {
            [0.0, 0.0, -1.0]
        }
    } else {
        [0.0, 0.0, -1.0]
    };
    let delta = drive_integration::galactic_position_delta(
        &self.drive,
        direction,
        dt as f64,
    );
    self.galactic_position.x += delta[0];
    self.galactic_position.y += delta[1];
    self.galactic_position.z += delta[2];
}
```

- [ ] **Step 5: Update resource drain to use drive mode**

Find where `self.ship_resources.update(...)` is called (search for `ship_resources.update`). Replace with:

```rust
self.ship_resources.update_with_drive(
    dt,
    ship.throttle,
    ship.engine_on,
    self.drive.mode(),
    self.drive.speed_fraction(),
);
```

If the original `update()` is called in multiple places, update all of them. The walk mode resource update should use `DriveMode::Impulse` since drives can't be engaged while walking.

- [ ] **Step 6: Auto-disengage drives when standing up from helm**

In the "wants_stand" block (around line 791), add:

```rust
// Disengage any active drive when leaving the helm
self.drive.request_disengage();
```

- [ ] **Step 7: Emergency warp drop on empty exotic fuel**

After the resource update, add:

```rust
// Emergency drop from warp if exotic fuel exhausted
if self.drive.mode() == sa_ship::DriveMode::Warp && self.ship_resources.exotic_fuel <= 0.0 {
    self.drive.request_disengage();
    log::warn!("WARP DRIVE FAILED — exotic fuel exhausted!");
}
```

- [ ] **Step 8: Build and verify compilation**

Run: `cargo build`
Expected: builds with no errors (warnings OK)

- [ ] **Step 9: Run all tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 10: Commit**

```bash
git add crates/spaceaway/src/main.rs
git commit -m "feat: wire drive system into game loop — helm controls, galactic position, fuel"
```

---

### Task 7: Helm screen drive mode display

**Files:**
- Modify: `crates/spaceaway/src/ui/helm_screen.rs`

- [ ] **Step 1: Read current helm_screen.rs**

Read the file to understand the `HelmData` struct and `draw_helm_screen()` function.

- [ ] **Step 2: Add drive fields to HelmData**

Add to the `HelmData` struct:

```rust
pub drive_mode: sa_ship::DriveMode,
pub drive_status: sa_ship::DriveStatus,
pub drive_speed_c: f64,
pub exotic_fuel: f32,
```

- [ ] **Step 3: Add drive display to the helm screen**

In `draw_helm_screen()`, add a section showing:
- Drive mode: "IMPULSE", "CRUISE", "WARP"
- Drive status: "IDLE", "SPOOLING 75%", "ENGAGED"
- Speed in appropriate units (m/s for impulse, c for cruise, kc for warp)
- Exotic fuel bar

The exact UI code depends on the current egui layout. Read the file and add a section below the existing speed/throttle display.

- [ ] **Step 4: Update HelmData construction in main.rs**

Find where `HelmData` is created (search for `HelmData {`). Add the new fields:

```rust
drive_mode: self.drive.mode(),
drive_status: self.drive.status(),
drive_speed_c: self.drive.current_speed_c(),
exotic_fuel: self.ship_resources.exotic_fuel,
```

- [ ] **Step 5: Build and test**

Run: `cargo build`
Expected: compiles

- [ ] **Step 6: Commit**

```bash
git add crates/spaceaway/src/ui/helm_screen.rs crates/spaceaway/src/main.rs
git commit -m "feat(ui): helm screen shows drive mode, speed, exotic fuel"
```

---

### Task 8: Integration testing and cleanup

**Files:**
- Modify: `crates/spaceaway/src/main.rs` (cleanup only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Fix any warnings.

- [ ] **Step 3: Manual test plan**

1. Run the game: `cargo run -p spaceaway`
2. Walk to helm, click helm seat to sit down
3. Start engine (click engine button), push throttle lever up
4. Verify impulse flight works as before
5. Press **2** — should see "Drive: CRUISE engaged" in log
6. Observe: galactic_position in debug JSON should start changing (very slowly at cruise)
7. Press **3** — should see "Drive: WARP spooling..." then after 5s it engages
8. Observe: galactic_position changes rapidly, stars stream past smoothly
9. Press **1** — should return to impulse, galactic position stops changing
10. Press **E** to stand up — drive should auto-disengage

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: Phase A complete — three-tier drive system with coordinate mapping"
```

---

## Summary

| Task | What it builds | Tests |
|------|---------------|-------|
| 1 | DriveMode, DriveStatus, DriveController skeleton | 1 test |
| 2 | Engagement, disengagement, warp spool | 5 tests |
| 3 | Speed calculations (c, ly/s) | 7 tests |
| 4 | Exotic fuel + drive-aware drain | 5 tests |
| 5 | Drive integration (galactic_position delta) | 4 tests |
| 6 | Game loop wiring (helm, fuel, position) | compilation + workspace tests |
| 7 | Helm screen UI | compilation |
| 8 | Integration testing + cleanup | manual test plan |

**Total: 8 tasks, ~22 automated tests, 1 manual test plan.**

After Phase A, the ship has three working drive tiers that move it through the galaxy at correct speeds. Stars stream smoothly at all tiers. Fuel depletes per-tier. The helm screen shows the current state. No visual effects yet (Phase B).
