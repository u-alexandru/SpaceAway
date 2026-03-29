# Phase 1: Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Set up the Cargo workspace with core crates (sa_core, sa_math, sa_ecs) and a basic game loop that opens a window, runs the ECS, and processes events.

**Architecture:** Cargo workspace with layered crate dependencies. sa_core provides the event bus and shared types. sa_math wraps glam with double-precision unit types. sa_ecs wraps hecs with system scheduling. The spaceaway binary ties them together in a game loop using winit for windowing and wgpu for a cleared screen.

**Tech Stack:** Rust, Cargo workspace, glam, hecs, winit, wgpu, thiserror, serde, RON

---

## File Structure

```
SpaceAway/
├── Cargo.toml                          # Workspace root
├── CLAUDE.md                           # AI agent instructions
├── .github/
│   └── workflows/
│       └── ci.yml                      # GitHub Actions CI
├── crates/
│   ├── sa_core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Re-exports
│   │       ├── events.rs               # Typed event bus
│   │       ├── time.rs                 # Frame time, delta, fixed timestep
│   │       └── resource.rs             # Resource handle type
│   ├── sa_math/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Re-exports
│   │       ├── units.rs                # Meters, Watts, Kilograms, Seconds, etc.
│   │       ├── coords.rs              # WorldPos (f64), LocalPos (f32), origin rebasing
│   │       └── conversions.rs         # WorldPos <-> LocalPos conversion
│   ├── sa_ecs/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Re-exports
│   │       ├── world.rs               # hecs::World wrapper
│   │       └── schedule.rs            # System scheduling (ordered function list)
│   └── spaceaway/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                 # Game loop, winit window, wgpu cleared screen
```

---

### Task 1: Workspace and Cargo Configuration

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/sa_core/Cargo.toml`
- Create: `crates/sa_math/Cargo.toml`
- Create: `crates/sa_ecs/Cargo.toml`
- Create: `crates/spaceaway/Cargo.toml`
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/sa_core",
    "crates/sa_math",
    "crates/sa_ecs",
    "crates/spaceaway",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
# Core
glam = { version = "0.29", features = ["serde"] }
hecs = "0.10"
thiserror = "2"
serde = { version = "1", features = ["derive"] }
ron = "0.8"

# Windowing & Graphics
winit = "0.30"
wgpu = "24"

# Logging
log = "0.4"
env_logger = "0.11"

# Internal crates
sa_core = { path = "crates/sa_core" }
sa_math = { path = "crates/sa_math" }
sa_ecs = { path = "crates/sa_ecs" }
```

- [ ] **Step 2: Create rust-toolchain.toml**

```toml
# rust-toolchain.toml
[toolchain]
channel = "stable"
```

- [ ] **Step 3: Create sa_core/Cargo.toml**

```toml
# crates/sa_core/Cargo.toml
[package]
name = "sa_core"
version.workspace = true
edition.workspace = true

[dependencies]
thiserror.workspace = true
log.workspace = true
```

- [ ] **Step 4: Create sa_math/Cargo.toml**

```toml
# crates/sa_math/Cargo.toml
[package]
name = "sa_math"
version.workspace = true
edition.workspace = true

[dependencies]
glam.workspace = true
serde.workspace = true

[dev-dependencies]
approx = "0.5"
```

- [ ] **Step 5: Create sa_ecs/Cargo.toml**

```toml
# crates/sa_ecs/Cargo.toml
[package]
name = "sa_ecs"
version.workspace = true
edition.workspace = true

[dependencies]
hecs.workspace = true
sa_core.workspace = true
log.workspace = true
```

- [ ] **Step 6: Create spaceaway/Cargo.toml**

```toml
# crates/spaceaway/Cargo.toml
[package]
name = "spaceaway"
version.workspace = true
edition.workspace = true

[dependencies]
sa_core.workspace = true
sa_math.workspace = true
sa_ecs.workspace = true
winit.workspace = true
wgpu.workspace = true
log.workspace = true
env_logger.workspace = true
```

- [ ] **Step 7: Create stub lib.rs for each library crate**

Create minimal `src/lib.rs` files so the workspace compiles:

```rust
// crates/sa_core/src/lib.rs
pub mod events;
pub mod time;
pub mod resource;
```

```rust
// crates/sa_math/src/lib.rs
pub mod units;
pub mod coords;
pub mod conversions;
```

```rust
// crates/sa_ecs/src/lib.rs
pub mod world;
pub mod schedule;
```

Create matching empty module files for each (just `// TODO: implement` as a single line placeholder so `cargo check` passes):

- `crates/sa_core/src/events.rs`
- `crates/sa_core/src/time.rs`
- `crates/sa_core/src/resource.rs`
- `crates/sa_math/src/units.rs`
- `crates/sa_math/src/coords.rs`
- `crates/sa_math/src/conversions.rs`
- `crates/sa_ecs/src/world.rs`
- `crates/sa_ecs/src/schedule.rs`

Create a minimal main.rs:

```rust
// crates/spaceaway/src/main.rs
fn main() {
    println!("SpaceAway");
}
```

- [ ] **Step 8: Verify workspace compiles**

Run: `cargo check`
Expected: Compiles with no errors.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates/
git commit -m "feat: initialize Cargo workspace with core crate stubs"
```

---

### Task 2: sa_math — Unit Types

**Files:**
- Create: `crates/sa_math/src/units.rs`

- [ ] **Step 1: Write failing tests for unit types**

```rust
// crates/sa_math/src/units.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_arithmetic() {
        let a = Meters(10.0);
        let b = Meters(3.0);
        let sum = a + b;
        assert_eq!(sum.0, 13.0);
    }

    #[test]
    fn meters_display() {
        let m = Meters(42.5);
        assert_eq!(format!("{m}"), "42.5 m");
    }

    #[test]
    fn seconds_arithmetic() {
        let a = Seconds(1.0);
        let b = Seconds(0.016);
        let sum = a + b;
        assert!((sum.0 - 1.016).abs() < 1e-10);
    }

    #[test]
    fn watts_arithmetic() {
        let a = Watts(100.0);
        let b = Watts(50.0);
        assert_eq!((a + b).0, 150.0);
    }

    #[test]
    fn kilograms_arithmetic() {
        let a = Kilograms(1000.0);
        let b = Kilograms(250.0);
        assert_eq!((a - b).0, 750.0);
    }

    #[test]
    fn meters_per_second_arithmetic() {
        let a = MetersPerSecond(100.0);
        let b = MetersPerSecond(30.0);
        assert_eq!((a + b).0, 130.0);
    }

    #[test]
    fn newtons_arithmetic() {
        let a = Newtons(500.0);
        let b = Newtons(200.0);
        assert_eq!((a + b).0, 700.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_math -- units`
Expected: FAIL — types not defined yet.

- [ ] **Step 3: Implement unit types**

```rust
// crates/sa_math/src/units.rs
use std::fmt;
use std::ops::{Add, Sub, Mul, Neg};
use serde::{Serialize, Deserialize};

macro_rules! unit_type {
    ($name:ident, $suffix:expr, $inner:ty) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub struct $name(pub $inner);

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self { Self(self.0 - rhs.0) }
        }

        impl Mul<$inner> for $name {
            type Output = Self;
            fn mul(self, rhs: $inner) -> Self { Self(self.0 * rhs) }
        }

        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self { Self(-self.0) }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{} {}", self.0, $suffix)
            }
        }

        impl $name {
            pub const ZERO: Self = Self(0.0);
        }
    };
}

// f64 units (world-scale precision)
unit_type!(Meters, "m", f64);
unit_type!(Seconds, "s", f64);
unit_type!(MetersPerSecond, "m/s", f64);

// f32 units (local-scale / subsystem values)
unit_type!(Watts, "W", f32);
unit_type!(Kilograms, "kg", f32);
unit_type!(Newtons, "N", f32);
unit_type!(Kelvin, "K", f32);
unit_type!(Liters, "L", f32);

#[cfg(test)]
mod tests {
    // ... (same tests as Step 1)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_math -- units`
Expected: All 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_math/src/units.rs
git commit -m "feat(sa_math): add strongly-typed unit types with arithmetic"
```

---

### Task 3: sa_math — World Coordinates and Origin Rebasing

**Files:**
- Create: `crates/sa_math/src/coords.rs`
- Create: `crates/sa_math/src/conversions.rs`

- [ ] **Step 1: Write failing tests for WorldPos and LocalPos**

```rust
// crates/sa_math/src/coords.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_pos_creation() {
        let pos = WorldPos::new(1.0, 2.0, 3.0);
        assert_eq!(pos.x, 1.0);
        assert_eq!(pos.y, 2.0);
        assert_eq!(pos.z, 3.0);
    }

    #[test]
    fn world_pos_addition() {
        let a = WorldPos::new(1.0, 2.0, 3.0);
        let b = WorldPos::new(4.0, 5.0, 6.0);
        let c = a + b;
        assert_eq!(c.x, 5.0);
        assert_eq!(c.y, 7.0);
        assert_eq!(c.z, 9.0);
    }

    #[test]
    fn world_pos_distance() {
        let a = WorldPos::new(0.0, 0.0, 0.0);
        let b = WorldPos::new(3.0, 4.0, 0.0);
        assert!((a.distance_to(b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn world_pos_origin() {
        assert_eq!(WorldPos::ORIGIN.x, 0.0);
        assert_eq!(WorldPos::ORIGIN.y, 0.0);
        assert_eq!(WorldPos::ORIGIN.z, 0.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_math -- coords`
Expected: FAIL — types not defined yet.

- [ ] **Step 3: Implement WorldPos and LocalPos**

```rust
// crates/sa_math/src/coords.rs
use std::ops::{Add, Sub};
use serde::{Serialize, Deserialize};

/// Double-precision world position. Used for all simulation coordinates.
/// Accurate to ~1mm at solar system distances.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl WorldPos {
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0, z: 0.0 };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn distance_to(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl Add for WorldPos {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for WorldPos {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

/// Single-precision position relative to the camera/render origin.
/// Used for GPU rendering only — never for simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalPos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl LocalPos {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 1)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_math -- coords`
Expected: All 4 tests PASS.

- [ ] **Step 5: Write failing tests for origin rebasing conversions**

```rust
// crates/sa_math/src/conversions.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::{WorldPos, LocalPos};

    #[test]
    fn rebase_at_origin() {
        let world = WorldPos::new(10.0, 20.0, 30.0);
        let origin = WorldPos::ORIGIN;
        let local = world_to_local(world, origin);
        assert!((local.x - 10.0).abs() < 1e-5);
        assert!((local.y - 20.0).abs() < 1e-5);
        assert!((local.z - 30.0).abs() < 1e-5);
    }

    #[test]
    fn rebase_cancels_origin() {
        let world = WorldPos::new(1000.0, 2000.0, 3000.0);
        let origin = WorldPos::new(1000.0, 2000.0, 3000.0);
        let local = world_to_local(world, origin);
        assert!(local.x.abs() < 1e-5);
        assert!(local.y.abs() < 1e-5);
        assert!(local.z.abs() < 1e-5);
    }

    #[test]
    fn rebase_large_coordinates() {
        // At 1 AU (~1.5e11 meters), verify precision is maintained
        let world = WorldPos::new(1.5e11, 0.0, 100.0);
        let origin = WorldPos::new(1.5e11, 0.0, 0.0);
        let local = world_to_local(world, origin);
        assert!((local.x).abs() < 1e-3);
        assert!((local.z - 100.0).abs() < 1e-3);
    }
}
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test -p sa_math -- conversions`
Expected: FAIL — function not defined yet.

- [ ] **Step 7: Implement origin rebasing**

```rust
// crates/sa_math/src/conversions.rs
use crate::coords::{WorldPos, LocalPos};

/// Convert a world-space position to camera-relative local-space.
/// This is the core of the origin-rebasing technique:
/// 1. Subtract in f64 (preserving precision at large distances)
/// 2. Cast to f32 (safe because the result is small/camera-relative)
pub fn world_to_local(world: WorldPos, origin: WorldPos) -> LocalPos {
    let dx = world.x - origin.x;
    let dy = world.y - origin.y;
    let dz = world.z - origin.z;
    LocalPos::new(dx as f32, dy as f32, dz as f32)
}

/// Convert local-space back to world-space (for physics/simulation).
pub fn local_to_world(local: LocalPos, origin: WorldPos) -> WorldPos {
    WorldPos::new(
        origin.x + local.x as f64,
        origin.y + local.y as f64,
        origin.z + local.z as f64,
    )
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 5)
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p sa_math -- conversions`
Expected: All 3 tests PASS.

- [ ] **Step 9: Update sa_math lib.rs re-exports**

```rust
// crates/sa_math/src/lib.rs
pub mod units;
pub mod coords;
pub mod conversions;

// Convenient re-exports
pub use coords::{WorldPos, LocalPos};
pub use conversions::{world_to_local, local_to_world};
pub use units::*;
```

- [ ] **Step 10: Run all sa_math tests**

Run: `cargo test -p sa_math`
Expected: All tests PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/sa_math/
git commit -m "feat(sa_math): add world coordinates and origin rebasing"
```

---

### Task 4: sa_core — Event Bus

**Files:**
- Create: `crates/sa_core/src/events.rs`

- [ ] **Step 1: Write failing tests for event bus**

```rust
// crates/sa_core/src/events.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct DamageEvent {
        amount: f32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct HealEvent {
        amount: f32,
    }

    #[test]
    fn emit_and_read_events() {
        let mut bus = EventBus::new();
        bus.emit(DamageEvent { amount: 10.0 });
        bus.emit(DamageEvent { amount: 5.0 });

        let events: Vec<&DamageEvent> = bus.read::<DamageEvent>().collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].amount, 10.0);
        assert_eq!(events[1].amount, 5.0);
    }

    #[test]
    fn different_event_types_independent() {
        let mut bus = EventBus::new();
        bus.emit(DamageEvent { amount: 10.0 });
        bus.emit(HealEvent { amount: 20.0 });

        let damage: Vec<&DamageEvent> = bus.read::<DamageEvent>().collect();
        let heal: Vec<&HealEvent> = bus.read::<HealEvent>().collect();
        assert_eq!(damage.len(), 1);
        assert_eq!(heal.len(), 1);
    }

    #[test]
    fn flush_clears_events() {
        let mut bus = EventBus::new();
        bus.emit(DamageEvent { amount: 10.0 });
        bus.flush();

        let events: Vec<&DamageEvent> = bus.read::<DamageEvent>().collect();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn read_empty_returns_none() {
        let bus = EventBus::new();
        let events: Vec<&DamageEvent> = bus.read::<DamageEvent>().collect();
        assert_eq!(events.len(), 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_core -- events`
Expected: FAIL — EventBus not defined yet.

- [ ] **Step 3: Implement EventBus**

```rust
// crates/sa_core/src/events.rs
use std::any::{Any, TypeId};
use std::collections::HashMap;

/// A typed event bus for cross-system communication.
/// Systems emit events of any type; other systems read them.
/// Events are cleared each frame via `flush()`.
pub struct EventBus {
    channels: HashMap<TypeId, Box<dyn Any>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Emit an event. Any system can read it this frame.
    pub fn emit<T: 'static>(&mut self, event: T) {
        let type_id = TypeId::of::<T>();
        let channel = self
            .channels
            .entry(type_id)
            .or_insert_with(|| Box::new(Vec::<T>::new()));
        channel.downcast_mut::<Vec<T>>().unwrap().push(event);
    }

    /// Read all events of a given type this frame.
    pub fn read<T: 'static>(&self) -> impl Iterator<Item = &T> {
        self.channels
            .get(&TypeId::of::<T>())
            .and_then(|channel| channel.downcast_ref::<Vec<T>>())
            .map(|vec| vec.iter())
            .unwrap_or_else(|| [].iter())
    }

    /// Clear all events. Call once per frame after all systems have run.
    pub fn flush(&mut self) {
        self.channels.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 1)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_core -- events`
Expected: All 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_core/src/events.rs
git commit -m "feat(sa_core): add typed event bus for cross-system communication"
```

---

### Task 5: sa_core — Frame Time

**Files:**
- Create: `crates/sa_core/src/time.rs`

- [ ] **Step 1: Write failing tests for frame time**

```rust
// crates/sa_core/src/time.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn initial_state() {
        let time = FrameTime::new();
        assert_eq!(time.delta_seconds(), 0.0);
        assert_eq!(time.frame_count(), 0);
    }

    #[test]
    fn advance_updates_delta() {
        let mut time = FrameTime::new();
        time.advance(Duration::from_millis(16));
        assert!((time.delta_seconds() - 0.016).abs() < 1e-5);
        assert_eq!(time.frame_count(), 1);
    }

    #[test]
    fn advance_accumulates_total() {
        let mut time = FrameTime::new();
        time.advance(Duration::from_millis(16));
        time.advance(Duration::from_millis(16));
        assert!((time.total_seconds() - 0.032).abs() < 1e-5);
        assert_eq!(time.frame_count(), 2);
    }

    #[test]
    fn delta_clamped_to_max() {
        let mut time = FrameTime::new();
        // Simulate a huge frame (1 second) — should be clamped
        time.advance(Duration::from_secs(1));
        assert!(time.delta_seconds() <= 0.1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_core -- time`
Expected: FAIL — FrameTime not defined yet.

- [ ] **Step 3: Implement FrameTime**

```rust
// crates/sa_core/src/time.rs
use std::time::Duration;

/// Maximum delta time (100ms). Prevents physics explosions on lag spikes.
const MAX_DELTA: Duration = Duration::from_millis(100);

/// Tracks frame timing for the game loop.
pub struct FrameTime {
    delta: Duration,
    total: Duration,
    frame_count: u64,
}

impl FrameTime {
    pub fn new() -> Self {
        Self {
            delta: Duration::ZERO,
            total: Duration::ZERO,
            frame_count: 0,
        }
    }

    /// Call once per frame with the elapsed time since last frame.
    pub fn advance(&mut self, elapsed: Duration) {
        self.delta = elapsed.min(MAX_DELTA);
        self.total += self.delta;
        self.frame_count += 1;
    }

    pub fn delta(&self) -> Duration {
        self.delta
    }

    pub fn delta_seconds(&self) -> f64 {
        self.delta.as_secs_f64()
    }

    pub fn total_seconds(&self) -> f64 {
        self.total.as_secs_f64()
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Default for FrameTime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 1)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_core -- time`
Expected: All 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_core/src/time.rs
git commit -m "feat(sa_core): add frame time tracking with delta clamping"
```

---

### Task 6: sa_core — Resource Handle

**Files:**
- Create: `crates/sa_core/src/resource.rs`

- [ ] **Step 1: Write failing tests for resource handles**

```rust
// crates/sa_core/src/resource.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_unique() {
        let mut gen = HandleGenerator::new();
        let a: Handle<()> = gen.next();
        let b: Handle<()> = gen.next();
        assert_ne!(a, b);
    }

    #[test]
    fn handles_are_typed() {
        // This test verifies that Handle<A> and Handle<B> are different types
        // at compile time. If it compiles, the test passes.
        struct MeshMarker;
        struct TextureMarker;
        let mut gen = HandleGenerator::new();
        let _mesh: Handle<MeshMarker> = gen.next();
        let _tex: Handle<TextureMarker> = gen.next();
        // Can't accidentally assign mesh handle to texture variable — compiler enforces this.
    }

    #[test]
    fn handle_display() {
        let mut gen = HandleGenerator::new();
        let h: Handle<()> = gen.next();
        assert_eq!(format!("{h}"), "Handle(0)");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_core -- resource`
Expected: FAIL — types not defined yet.

- [ ] **Step 3: Implement Handle and HandleGenerator**

```rust
// crates/sa_core/src/resource.rs
use std::fmt;
use std::marker::PhantomData;

/// A strongly-typed handle to a resource. The type parameter T is a marker
/// that prevents mixing handles of different resource types.
#[derive(Debug)]
pub struct Handle<T> {
    id: u64,
    _marker: PhantomData<T>,
}

impl<T> Handle<T> {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> std::hash::Hash for Handle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Display for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle({})", self.id)
    }
}

/// Generates unique handles. One per resource type manager.
pub struct HandleGenerator {
    next_id: u64,
}

impl HandleGenerator {
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    pub fn next<T>(&mut self) -> Handle<T> {
        let id = self.next_id;
        self.next_id += 1;
        Handle {
            id,
            _marker: PhantomData,
        }
    }
}

impl Default for HandleGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 1)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_core -- resource`
Expected: All 3 tests PASS.

- [ ] **Step 5: Update sa_core lib.rs with re-exports**

```rust
// crates/sa_core/src/lib.rs
pub mod events;
pub mod time;
pub mod resource;

pub use events::EventBus;
pub use time::FrameTime;
pub use resource::{Handle, HandleGenerator};
```

- [ ] **Step 6: Run all sa_core tests**

Run: `cargo test -p sa_core`
Expected: All tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/sa_core/
git commit -m "feat(sa_core): add typed resource handles"
```

---

### Task 7: sa_ecs — World Wrapper and System Scheduling

**Files:**
- Create: `crates/sa_ecs/src/world.rs`
- Create: `crates/sa_ecs/src/schedule.rs`

- [ ] **Step 1: Write failing tests for GameWorld**

```rust
// crates/sa_ecs/src/world.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;

    struct Position(f32, f32, f32);
    struct Velocity(f32, f32, f32);

    #[test]
    fn spawn_and_query() {
        let mut world = GameWorld::new();
        let entity = world.spawn((Position(1.0, 2.0, 3.0), Velocity(0.1, 0.0, 0.0)));

        let pos = world.inner().get::<&Position>(entity).unwrap();
        assert_eq!(pos.0, 1.0);
    }

    #[test]
    fn spawn_multiple_and_count() {
        let mut world = GameWorld::new();
        world.spawn((Position(0.0, 0.0, 0.0),));
        world.spawn((Position(1.0, 1.0, 1.0),));
        world.spawn((Position(2.0, 2.0, 2.0),));

        let count = world.inner().query::<&Position>().iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    fn despawn_entity() {
        let mut world = GameWorld::new();
        let entity = world.spawn((Position(1.0, 2.0, 3.0),));
        world.despawn(entity);

        assert!(world.inner().get::<&Position>(entity).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_ecs -- world`
Expected: FAIL — GameWorld not defined yet.

- [ ] **Step 3: Implement GameWorld**

```rust
// crates/sa_ecs/src/world.rs
use hecs::{Entity, World};

/// Thin wrapper around hecs::World.
/// Provides a stable API surface so game code doesn't depend on hecs directly.
pub struct GameWorld {
    world: World,
}

impl GameWorld {
    pub fn new() -> Self {
        Self {
            world: World::new(),
        }
    }

    /// Spawn an entity with the given components.
    pub fn spawn(&mut self, components: impl hecs::DynamicBundle) -> Entity {
        self.world.spawn(components)
    }

    /// Despawn an entity.
    pub fn despawn(&mut self, entity: Entity) {
        let _ = self.world.despawn(entity);
    }

    /// Access the inner hecs::World for queries.
    pub fn inner(&self) -> &World {
        &self.world
    }

    /// Mutable access to the inner hecs::World.
    pub fn inner_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

impl Default for GameWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 1)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_ecs -- world`
Expected: All 3 tests PASS.

- [ ] **Step 5: Write failing tests for Schedule**

```rust
// crates/sa_ecs/src/schedule.rs

// ... (implementation will go here)

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn systems_run_in_order() {
        let order = Arc::new(AtomicU32::new(0));
        let mut schedule = Schedule::new();

        let o1 = Arc::clone(&order);
        schedule.add_system("first", move |_world, _events, _time| {
            assert_eq!(o1.fetch_add(1, Ordering::SeqCst), 0);
        });

        let o2 = Arc::clone(&order);
        schedule.add_system("second", move |_world, _events, _time| {
            assert_eq!(o2.fetch_add(1, Ordering::SeqCst), 1);
        });

        let mut world = crate::world::GameWorld::new();
        let mut events = sa_core::EventBus::new();
        let time = sa_core::FrameTime::new();

        schedule.run(&mut world, &mut events, &time);
        assert_eq!(order.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn empty_schedule_runs() {
        let schedule = Schedule::new();
        let mut world = crate::world::GameWorld::new();
        let mut events = sa_core::EventBus::new();
        let time = sa_core::FrameTime::new();

        schedule.run(&mut world, &mut events, &time);
        // Should not panic
    }
}
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test -p sa_ecs -- schedule`
Expected: FAIL — Schedule not defined yet.

- [ ] **Step 7: Implement Schedule**

```rust
// crates/sa_ecs/src/schedule.rs
use crate::world::GameWorld;
use sa_core::{EventBus, FrameTime};

/// A system function that operates on the world, events, and time.
type SystemFn = Box<dyn FnMut(&mut GameWorld, &mut EventBus, &FrameTime)>;

struct System {
    name: String,
    run: SystemFn,
}

/// An ordered list of systems that run each frame.
/// Systems run sequentially in the order they were added.
pub struct Schedule {
    systems: Vec<System>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    /// Add a system to the end of the schedule.
    pub fn add_system(
        &mut self,
        name: &str,
        system: impl FnMut(&mut GameWorld, &mut EventBus, &FrameTime) + 'static,
    ) {
        self.systems.push(System {
            name: name.to_string(),
            run: Box::new(system),
        });
    }

    /// Run all systems in order.
    pub fn run(&mut self, world: &mut GameWorld, events: &mut EventBus, time: &FrameTime) {
        for system in &mut self.systems {
            log::trace!("Running system: {}", system.name);
            (system.run)(world, events, time);
        }
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // ... (same tests as Step 5)
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p sa_ecs -- schedule`
Expected: All 2 tests PASS.

- [ ] **Step 9: Update sa_ecs lib.rs with re-exports**

```rust
// crates/sa_ecs/src/lib.rs
pub mod world;
pub mod schedule;

pub use world::GameWorld;
pub use schedule::Schedule;
```

- [ ] **Step 10: Run all sa_ecs tests**

Run: `cargo test -p sa_ecs`
Expected: All tests PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/sa_ecs/
git commit -m "feat(sa_ecs): add GameWorld wrapper and system scheduling"
```

---

### Task 8: Game Binary — Window and Game Loop

**Files:**
- Create: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Implement the game binary with winit + wgpu**

This is the integration point — no unit tests for the binary itself (it's a thin orchestration layer). We verify it works by running it.

```rust
// crates/spaceaway/src/main.rs
use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

struct GpuContext {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    world: GameWorld,
    events: EventBus,
    time: FrameTime,
    schedule: Schedule,
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            world: GameWorld::new(),
            events: EventBus::new(),
            time: FrameTime::new(),
            schedule: Schedule::new(),
            last_frame: Instant::now(),
        }
    }

    fn init_gpu(&mut self, window: Arc<Window>) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("Failed to find a suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SpaceAway Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
            None,
        ))
        .expect("Failed to create GPU device");

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Surface not supported by adapter");
        surface.configure(&device, &config);

        self.gpu = Some(GpuContext {
            surface,
            device,
            queue,
            config,
        });
    }

    fn render(&mut self) {
        let Some(gpu) = &self.gpu else { return };

        let frame = match gpu.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return;
            }
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Frame Encoder"),
        });

        // Clear to near-black (the void)
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.005,
                            g: 0.005,
                            b: 0.015,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("SpaceAway")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            self.init_gpu(window.clone());
            self.window = Some(window);
            self.last_frame = Instant::now();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.config.width = new_size.width.max(1);
                    gpu.config.height = new_size.height.max(1);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed = now - self.last_frame;
                self.last_frame = now;
                self.time.advance(elapsed);

                // Run all systems
                self.schedule.run(&mut self.world, &mut self.events, &self.time);

                // Render
                self.render();

                // Flush events for next frame
                self.events.flush();

                // Request next frame
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    log::info!("SpaceAway starting...");

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
```

- [ ] **Step 2: Add pollster dependency for blocking on async GPU init**

Add to `crates/spaceaway/Cargo.toml` under `[dependencies]`:

```toml
pollster = "0.4"
```

And add to workspace root `Cargo.toml` under `[workspace.dependencies]`:

```toml
pollster = "0.4"
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p spaceaway`
Expected: Compiles with no errors.

- [ ] **Step 4: Run it manually to verify window opens**

Run: `cargo run -p spaceaway`
Expected: A 1280x720 window opens titled "SpaceAway" with a near-black background. Close it with the X button.

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/ Cargo.toml
git commit -m "feat(spaceaway): add game binary with winit window and wgpu clear screen"
```

---

### Task 9: CI Configuration

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create GitHub Actions CI workflow**

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check & Lint
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - name: cargo check
        run: cargo check --workspace
      - name: cargo clippy
        run: cargo clippy --workspace -- -D warnings
      - name: cargo test
        run: cargo test --workspace

  check-windows:
    name: Windows Cross-Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc
      - uses: Swatinem/rust-cache@v2
      - name: cargo check (Windows target)
        run: cargo check --workspace --target x86_64-pc-windows-msvc
```

- [ ] **Step 2: Commit**

```bash
git add .github/
git commit -m "ci: add GitHub Actions workflow for check, clippy, test, and Windows cross-check"
```

---

### Task 10: CLAUDE.md

**Files:**
- Create: `CLAUDE.md`

- [ ] **Step 1: Create CLAUDE.md for AI agent instructions**

```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

SpaceAway is a cooperative first-person space exploration game with a custom Rust engine. 1-4 players crew a ship exploring a procedurally generated infinite universe. The engine is designed for AI-agent-driven development.

See `docs/superpowers/specs/2026-03-27-spaceaway-engine-design.md` for the full design spec.

## Build Commands

```bash
cargo check                    # Fast type checking (use first, always)
cargo build -p spaceaway       # Build the game binary
cargo run -p spaceaway         # Run the game
cargo test --workspace         # Run all tests
cargo test -p sa_physics       # Run tests for a single crate
cargo clippy --workspace -- -D warnings  # Lint (run before committing)
```

## Architecture

Cargo workspace with layered crates. Dependencies flow downward only.

```
Application:  spaceaway (game binary)
Game Logic:   sa_ship, sa_survival, sa_universe, sa_player
Engine:       sa_render, sa_physics, sa_net, sa_audio, sa_input
Core:         sa_ecs, sa_math, sa_core
```

All crates live in `crates/`. The `sa_` prefix is mandatory for all crate names.

### Cross-Crate Communication

Crates never call each other directly. They communicate through `sa_core::EventBus` — emit strongly-typed events, consume them in other systems. This keeps dependencies clean and one-directional.

### Coordinate System

- **Simulation:** `WorldPos` (f64) — double-precision, used everywhere in game logic
- **Rendering:** `LocalPos` (f32) — camera-relative, converted from WorldPos via origin rebasing
- Never use bare f32/f64 for positions. Always use the typed wrappers.

### Unit Types

Use strong types from `sa_math::units`: `Meters`, `Seconds`, `Watts`, `Kilograms`, `Newtons`, `Kelvin`, `Liters`, `MetersPerSecond`. Never use bare numbers for physical quantities.

### ECS

Based on hecs. Components are plain structs. Systems are functions that take `(&mut GameWorld, &mut EventBus, &FrameTime)`. Register systems in the `Schedule`.

## Conventions

- Every file stays under 300 lines. Split when it grows.
- One concept per file.
- Public API at the top of each file, private internals below.
- All config in RON/TOML. No binary formats in the repo.
- Shaders in WGSL.
- `thiserror` for error types per crate. No string errors, no panics in library code.
- Unit tests inline with `#[cfg(test)]`. Integration tests in `tests/`.
- Run `cargo clippy` before every commit.

## Platform

Primary target: macOS (Metal via wgpu). Secondary: Windows (DX12/Vulkan via wgpu). The same code runs on both — wgpu abstracts the graphics backend.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add CLAUDE.md for AI agent development guidance"
```

---

### Task 11: Run Full Workspace Checks

- [ ] **Step 1: Run cargo check**

Run: `cargo check --workspace`
Expected: No errors.

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 4: Run the game binary**

Run: `cargo run -p spaceaway`
Expected: Window opens with near-black background. Close it.

- [ ] **Step 5: Final commit if any fixes were needed**

Only if previous steps required fixes:

```bash
git add -A
git commit -m "fix: address clippy warnings and test failures"
```
