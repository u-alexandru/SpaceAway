# Phase 4: Procedural Universe --- Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the random star field with a deterministic procedural universe. Every star is generated from a master seed via coordinate hashing, placed with Poisson disk sampling, and rendered with physically correct H-R diagram properties. Planetary systems orbit each star. Same seed always produces the same universe.

**Architecture:** New `sa_universe` crate sits at the Game Logic layer. It generates stars and planets from coordinate hashes (xxHash64), places them via 3D Poisson disk sampling, and exposes spatial queries for the renderer. The renderer's `generate_stars()` is replaced by `sa_universe` queries that produce `StarVertex` data from real procedural stars. No randomness --- everything is deterministic from a master seed.

**Tech Stack:** Rust, xxhash-rust (xxh3), sa_math (WorldPos, Kelvin, Kilograms), sa_core, serde, glam

---

## File Structure

```
crates/sa_universe/
├── Cargo.toml
└── src/
    ├── lib.rs              # Re-exports
    ├── seed.rs             # Coordinate hashing, master seed, seeded RNG helper
    ├── object_id.rs        # Packed u64 ObjectId with pack/unpack
    ├── star.rs             # Star generation from H-R diagram
    ├── sector.rs           # Sector generation, Poisson disk star placement
    ├── system.rs           # Planetary system generation
    └── query.rs            # Spatial queries: nearby sectors, visible stars
```

---

### Task 1: Crate Setup + seed.rs (Coordinate Hashing)

**Files:**
- Modify: `Cargo.toml` (workspace root --- add sa_universe member and xxhash-rust dependency)
- Create: `crates/sa_universe/Cargo.toml`
- Create: `crates/sa_universe/src/lib.rs`
- Create: `crates/sa_universe/src/seed.rs`

- [ ] **Step 1: Add sa_universe to workspace**

Add `"crates/sa_universe"` to the workspace members in root `Cargo.toml`, and add under `[workspace.dependencies]`:

```toml
xxhash-rust = { version = "0.8", features = ["xxh3"] }
sa_universe = { path = "crates/sa_universe" }
```

The members array becomes:
```toml
members = [
    "crates/sa_core",
    "crates/sa_math",
    "crates/sa_ecs",
    "crates/sa_input",
    "crates/sa_render",
    "crates/sa_physics",
    "crates/sa_player",
    "crates/sa_universe",
    "crates/spaceaway",
]
```

- [ ] **Step 2: Create sa_universe Cargo.toml**

Create `crates/sa_universe/Cargo.toml`:
```toml
[package]
name = "sa_universe"
version.workspace = true
edition.workspace = true

[dependencies]
xxhash-rust.workspace = true
glam.workspace = true
log.workspace = true
sa_core.workspace = true
sa_math.workspace = true
serde.workspace = true
```

- [ ] **Step 3: Create lib.rs with module declarations**

Create `crates/sa_universe/src/lib.rs`:
```rust
pub mod seed;

pub use seed::{MasterSeed, Rng64, sector_hash};
```

- [ ] **Step 4: Write failing tests for seed.rs**

Create `crates/sa_universe/src/seed.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Master seed for the entire universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterSeed(pub u64);

/// Hash sector coordinates into a deterministic 64-bit seed.
pub fn sector_hash(_master: MasterSeed, _sx: i32, _sy: i32, _sz: i32) -> u64 {
    todo!()
}

/// Minimal xorshift64 PRNG seeded from a u64.
pub struct Rng64 {
    state: u64,
}

impl Rng64 {
    pub fn new(_seed: u64) -> Self {
        todo!()
    }

    /// Returns a u64.
    pub fn next_u64(&mut self) -> u64 {
        todo!()
    }

    /// Returns a f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        todo!()
    }

    /// Returns a f32 in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        todo!()
    }

    /// Returns a f64 in [min, max).
    pub fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        todo!()
    }

    /// Returns a f32 in [min, max).
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_hash_deterministic() {
        let seed = MasterSeed(12345);
        let h1 = sector_hash(seed, 10, -20, 30);
        let h2 = sector_hash(seed, 10, -20, 30);
        assert_eq!(h1, h2, "Same inputs must produce same hash");
    }

    #[test]
    fn sector_hash_different_coords_differ() {
        let seed = MasterSeed(12345);
        let h1 = sector_hash(seed, 0, 0, 0);
        let h2 = sector_hash(seed, 1, 0, 0);
        assert_ne!(h1, h2, "Different coords must produce different hashes");
    }

    #[test]
    fn sector_hash_different_seeds_differ() {
        let h1 = sector_hash(MasterSeed(1), 0, 0, 0);
        let h2 = sector_hash(MasterSeed(2), 0, 0, 0);
        assert_ne!(h1, h2, "Different master seeds must produce different hashes");
    }

    #[test]
    fn sector_hash_negative_coords() {
        let seed = MasterSeed(42);
        let h1 = sector_hash(seed, -1, -1, -1);
        let h2 = sector_hash(seed, 1, 1, 1);
        assert_ne!(h1, h2, "Negative and positive coords must differ");
    }

    #[test]
    fn rng_deterministic() {
        let mut a = Rng64::new(42);
        let mut b = Rng64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_different_seeds_differ() {
        let mut a = Rng64::new(1);
        let mut b = Rng64::new(2);
        // At least one of the first 10 values should differ
        let differs = (0..10).any(|_| a.next_u64() != b.next_u64());
        assert!(differs);
    }

    #[test]
    fn rng_f64_in_range() {
        let mut rng = Rng64::new(99);
        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "f64 out of [0,1): {v}");
        }
    }

    #[test]
    fn rng_f32_in_range() {
        let mut rng = Rng64::new(99);
        for _ in 0..1000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "f32 out of [0,1): {v}");
        }
    }

    #[test]
    fn rng_range_f64_bounds() {
        let mut rng = Rng64::new(123);
        for _ in 0..1000 {
            let v = rng.range_f64(5.0, 10.0);
            assert!((5.0..10.0).contains(&v), "range_f64 out of [5,10): {v}");
        }
    }

    #[test]
    fn rng_nonzero_state() {
        // Even with seed 0, the RNG should not get stuck
        let mut rng = Rng64::new(0);
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b, "RNG should not get stuck with seed 0");
    }
}
```

Verify tests fail:
```bash
cargo test -p sa_universe
```

- [ ] **Step 5: Implement seed.rs**

Replace the `todo!()` bodies in `crates/sa_universe/src/seed.rs`:

```rust
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

/// Master seed for the entire universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterSeed(pub u64);

/// Hash sector coordinates into a deterministic 64-bit seed.
/// Uses xxh3 (64-bit) for fast, high-quality hashing.
pub fn sector_hash(master: MasterSeed, sx: i32, sy: i32, sz: i32) -> u64 {
    let mut buf = [0u8; 20];
    buf[0..8].copy_from_slice(&master.0.to_le_bytes());
    buf[8..12].copy_from_slice(&sx.to_le_bytes());
    buf[12..16].copy_from_slice(&sy.to_le_bytes());
    buf[16..20].copy_from_slice(&sz.to_le_bytes());
    xxh3_64(&buf)
}

/// Minimal xorshift64 PRNG seeded from a u64.
/// Not cryptographic. Deterministic, fast, good distribution.
pub struct Rng64 {
    state: u64,
}

impl Rng64 {
    pub fn new(seed: u64) -> Self {
        // Ensure state is never zero (xorshift requires nonzero state).
        // Mix the seed through a splitmix step to get a good initial state.
        let mut s = seed.wrapping_add(0x9E3779B97F4A7C15);
        s = (s ^ (s >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        s = (s ^ (s >> 27)).wrapping_mul(0x94D049BB133111EB);
        s ^= s >> 31;
        if s == 0 { s = 1; }
        Self { state: s }
    }

    /// Returns a u64.
    pub fn next_u64(&mut self) -> u64 {
        let mut s = self.state;
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        self.state = s;
        s
    }

    /// Returns a f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Returns a f32 in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / ((1u64 << 24) as f32)
    }

    /// Returns a f64 in [min, max).
    pub fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }

    /// Returns a f32 in [min, max).
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_hash_deterministic() {
        let seed = MasterSeed(12345);
        let h1 = sector_hash(seed, 10, -20, 30);
        let h2 = sector_hash(seed, 10, -20, 30);
        assert_eq!(h1, h2, "Same inputs must produce same hash");
    }

    #[test]
    fn sector_hash_different_coords_differ() {
        let seed = MasterSeed(12345);
        let h1 = sector_hash(seed, 0, 0, 0);
        let h2 = sector_hash(seed, 1, 0, 0);
        assert_ne!(h1, h2, "Different coords must produce different hashes");
    }

    #[test]
    fn sector_hash_different_seeds_differ() {
        let h1 = sector_hash(MasterSeed(1), 0, 0, 0);
        let h2 = sector_hash(MasterSeed(2), 0, 0, 0);
        assert_ne!(h1, h2, "Different master seeds must produce different hashes");
    }

    #[test]
    fn sector_hash_negative_coords() {
        let seed = MasterSeed(42);
        let h1 = sector_hash(seed, -1, -1, -1);
        let h2 = sector_hash(seed, 1, 1, 1);
        assert_ne!(h1, h2, "Negative and positive coords must differ");
    }

    #[test]
    fn rng_deterministic() {
        let mut a = Rng64::new(42);
        let mut b = Rng64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_different_seeds_differ() {
        let mut a = Rng64::new(1);
        let mut b = Rng64::new(2);
        let differs = (0..10).any(|_| a.next_u64() != b.next_u64());
        assert!(differs);
    }

    #[test]
    fn rng_f64_in_range() {
        let mut rng = Rng64::new(99);
        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "f64 out of [0,1): {v}");
        }
    }

    #[test]
    fn rng_f32_in_range() {
        let mut rng = Rng64::new(99);
        for _ in 0..1000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "f32 out of [0,1): {v}");
        }
    }

    #[test]
    fn rng_range_f64_bounds() {
        let mut rng = Rng64::new(123);
        for _ in 0..1000 {
            let v = rng.range_f64(5.0, 10.0);
            assert!((5.0..10.0).contains(&v), "range_f64 out of [5,10): {v}");
        }
    }

    #[test]
    fn rng_nonzero_state() {
        let mut rng = Rng64::new(0);
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b, "RNG should not get stuck with seed 0");
    }
}
```

Verify tests pass:
```bash
cargo test -p sa_universe
```

Then lint:
```bash
cargo clippy -p sa_universe -- -D warnings
```

Commit: `feat(sa_universe): add crate with coordinate hashing and seeded RNG`

---

### Task 2: object_id.rs (Packed u64 Object IDs)

**Files:**
- Create: `crates/sa_universe/src/object_id.rs`
- Modify: `crates/sa_universe/src/lib.rs` (add module)

- [ ] **Step 1: Add module to lib.rs**

Update `crates/sa_universe/src/lib.rs`:
```rust
pub mod object_id;
pub mod seed;

pub use object_id::ObjectId;
pub use seed::{MasterSeed, Rng64, sector_hash};
```

- [ ] **Step 2: Write failing tests for object_id.rs**

Create `crates/sa_universe/src/object_id.rs`:
```rust
use serde::{Deserialize, Serialize};

/// Packed 64-bit identifier for any object in the universe.
///
/// Bit layout (MSB to LSB):
///   [63..48] sector_x  (16 bits, signed as i16)
///   [47..32] sector_y  (16 bits, signed as i16)
///   [31..16] sector_z  (16 bits, signed as i16)
///   [15..13] layer     (3 bits, 0-7)
///   [12..5]  system    (8 bits, 0-255)
///   [4..0]   body      (5 bits, 0-31)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

impl ObjectId {
    pub fn pack(
        _sector_x: i16,
        _sector_y: i16,
        _sector_z: i16,
        _layer: u8,
        _system: u8,
        _body: u8,
    ) -> Self {
        todo!()
    }

    pub fn sector_x(self) -> i16 { todo!() }
    pub fn sector_y(self) -> i16 { todo!() }
    pub fn sector_z(self) -> i16 { todo!() }
    pub fn layer(self) -> u8 { todo!() }
    pub fn system(self) -> u8 { todo!() }
    pub fn body(self) -> u8 { todo!() }

    /// Returns an ObjectId addressing just the sector (layer/system/body = 0).
    pub fn sector_id(sector_x: i16, sector_y: i16, sector_z: i16) -> Self {
        Self::pack(sector_x, sector_y, sector_z, 0, 0, 0)
    }

    /// Returns an ObjectId addressing a star system within a sector.
    pub fn star_id(sector_x: i16, sector_y: i16, sector_z: i16, system: u8) -> Self {
        Self::pack(sector_x, sector_y, sector_z, 0, system, 0)
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({},{},{}):L{}:S{}:B{}",
            self.sector_x(), self.sector_y(), self.sector_z(),
            self.layer(), self.system(), self.body(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let id = ObjectId::pack(100, -50, 32767, 5, 200, 31);
        assert_eq!(id.sector_x(), 100);
        assert_eq!(id.sector_y(), -50);
        assert_eq!(id.sector_z(), 32767);
        assert_eq!(id.layer(), 5);
        assert_eq!(id.system(), 200);
        assert_eq!(id.body(), 31);
    }

    #[test]
    fn pack_unpack_zeros() {
        let id = ObjectId::pack(0, 0, 0, 0, 0, 0);
        assert_eq!(id.0, 0);
        assert_eq!(id.sector_x(), 0);
        assert_eq!(id.sector_y(), 0);
        assert_eq!(id.sector_z(), 0);
        assert_eq!(id.layer(), 0);
        assert_eq!(id.system(), 0);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn pack_unpack_negative_coords() {
        let id = ObjectId::pack(-1, -32768, -100, 7, 255, 0);
        assert_eq!(id.sector_x(), -1);
        assert_eq!(id.sector_y(), -32768);
        assert_eq!(id.sector_z(), -100);
        assert_eq!(id.layer(), 7);
        assert_eq!(id.system(), 255);
    }

    #[test]
    fn pack_unpack_max_values() {
        let id = ObjectId::pack(32767, 32767, 32767, 7, 255, 31);
        assert_eq!(id.sector_x(), 32767);
        assert_eq!(id.sector_y(), 32767);
        assert_eq!(id.sector_z(), 32767);
        assert_eq!(id.layer(), 7);
        assert_eq!(id.system(), 255);
        assert_eq!(id.body(), 31);
    }

    #[test]
    fn sector_id_helper() {
        let id = ObjectId::sector_id(10, 20, 30);
        assert_eq!(id.sector_x(), 10);
        assert_eq!(id.sector_y(), 20);
        assert_eq!(id.sector_z(), 30);
        assert_eq!(id.layer(), 0);
        assert_eq!(id.system(), 0);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn star_id_helper() {
        let id = ObjectId::star_id(5, -5, 0, 42);
        assert_eq!(id.sector_x(), 5);
        assert_eq!(id.sector_y(), -5);
        assert_eq!(id.system(), 42);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn display_format() {
        let id = ObjectId::pack(1, 2, 3, 0, 10, 5);
        let s = format!("{id}");
        assert_eq!(s, "(1,2,3):L0:S10:B5");
    }

    #[test]
    fn different_fields_produce_different_ids() {
        let a = ObjectId::pack(0, 0, 0, 0, 0, 0);
        let b = ObjectId::pack(0, 0, 0, 0, 0, 1);
        let c = ObjectId::pack(0, 0, 0, 0, 1, 0);
        let d = ObjectId::pack(0, 0, 0, 1, 0, 0);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
```

Verify tests fail:
```bash
cargo test -p sa_universe
```

- [ ] **Step 3: Implement object_id.rs**

Replace the `todo!()` bodies in `crates/sa_universe/src/object_id.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Packed 64-bit identifier for any object in the universe.
///
/// Bit layout (MSB to LSB):
///   [63..48] sector_x  (16 bits, signed as i16)
///   [47..32] sector_y  (16 bits, signed as i16)
///   [31..16] sector_z  (16 bits, signed as i16)
///   [15..13] layer     (3 bits, 0-7)
///   [12..5]  system    (8 bits, 0-255)
///   [4..0]   body      (5 bits, 0-31)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

impl ObjectId {
    pub fn pack(
        sector_x: i16,
        sector_y: i16,
        sector_z: i16,
        layer: u8,
        system: u8,
        body: u8,
    ) -> Self {
        let sx = (sector_x as u16) as u64;
        let sy = (sector_y as u16) as u64;
        let sz = (sector_z as u16) as u64;
        let l = (layer & 0x07) as u64;
        let s = system as u64;
        let b = (body & 0x1F) as u64;

        Self((sx << 48) | (sy << 32) | (sz << 16) | (l << 13) | (s << 5) | b)
    }

    pub fn sector_x(self) -> i16 {
        ((self.0 >> 48) & 0xFFFF) as u16 as i16
    }

    pub fn sector_y(self) -> i16 {
        ((self.0 >> 32) & 0xFFFF) as u16 as i16
    }

    pub fn sector_z(self) -> i16 {
        ((self.0 >> 16) & 0xFFFF) as u16 as i16
    }

    pub fn layer(self) -> u8 {
        ((self.0 >> 13) & 0x07) as u8
    }

    pub fn system(self) -> u8 {
        ((self.0 >> 5) & 0xFF) as u8
    }

    pub fn body(self) -> u8 {
        (self.0 & 0x1F) as u8
    }

    /// Returns an ObjectId addressing just the sector (layer/system/body = 0).
    pub fn sector_id(sector_x: i16, sector_y: i16, sector_z: i16) -> Self {
        Self::pack(sector_x, sector_y, sector_z, 0, 0, 0)
    }

    /// Returns an ObjectId addressing a star system within a sector.
    pub fn star_id(sector_x: i16, sector_y: i16, sector_z: i16, system: u8) -> Self {
        Self::pack(sector_x, sector_y, sector_z, 0, system, 0)
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({},{},{}):L{}:S{}:B{}",
            self.sector_x(), self.sector_y(), self.sector_z(),
            self.layer(), self.system(), self.body(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let id = ObjectId::pack(100, -50, 32767, 5, 200, 31);
        assert_eq!(id.sector_x(), 100);
        assert_eq!(id.sector_y(), -50);
        assert_eq!(id.sector_z(), 32767);
        assert_eq!(id.layer(), 5);
        assert_eq!(id.system(), 200);
        assert_eq!(id.body(), 31);
    }

    #[test]
    fn pack_unpack_zeros() {
        let id = ObjectId::pack(0, 0, 0, 0, 0, 0);
        assert_eq!(id.0, 0);
        assert_eq!(id.sector_x(), 0);
        assert_eq!(id.sector_y(), 0);
        assert_eq!(id.sector_z(), 0);
        assert_eq!(id.layer(), 0);
        assert_eq!(id.system(), 0);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn pack_unpack_negative_coords() {
        let id = ObjectId::pack(-1, -32768, -100, 7, 255, 0);
        assert_eq!(id.sector_x(), -1);
        assert_eq!(id.sector_y(), -32768);
        assert_eq!(id.sector_z(), -100);
        assert_eq!(id.layer(), 7);
        assert_eq!(id.system(), 255);
    }

    #[test]
    fn pack_unpack_max_values() {
        let id = ObjectId::pack(32767, 32767, 32767, 7, 255, 31);
        assert_eq!(id.sector_x(), 32767);
        assert_eq!(id.sector_y(), 32767);
        assert_eq!(id.sector_z(), 32767);
        assert_eq!(id.layer(), 7);
        assert_eq!(id.system(), 255);
        assert_eq!(id.body(), 31);
    }

    #[test]
    fn sector_id_helper() {
        let id = ObjectId::sector_id(10, 20, 30);
        assert_eq!(id.sector_x(), 10);
        assert_eq!(id.sector_y(), 20);
        assert_eq!(id.sector_z(), 30);
        assert_eq!(id.layer(), 0);
        assert_eq!(id.system(), 0);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn star_id_helper() {
        let id = ObjectId::star_id(5, -5, 0, 42);
        assert_eq!(id.sector_x(), 5);
        assert_eq!(id.sector_y(), -5);
        assert_eq!(id.system(), 42);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn display_format() {
        let id = ObjectId::pack(1, 2, 3, 0, 10, 5);
        let s = format!("{id}");
        assert_eq!(s, "(1,2,3):L0:S10:B5");
    }

    #[test]
    fn different_fields_produce_different_ids() {
        let a = ObjectId::pack(0, 0, 0, 0, 0, 0);
        let b = ObjectId::pack(0, 0, 0, 0, 0, 1);
        let c = ObjectId::pack(0, 0, 0, 0, 1, 0);
        let d = ObjectId::pack(0, 0, 0, 1, 0, 0);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
```

Verify tests pass:
```bash
cargo test -p sa_universe
```

Then lint:
```bash
cargo clippy -p sa_universe -- -D warnings
```

Commit: `feat(sa_universe): add packed u64 ObjectId with bit-field packing`

---

### Task 3: star.rs (H-R Diagram Star Generation)

**Files:**
- Create: `crates/sa_universe/src/star.rs`
- Modify: `crates/sa_universe/src/lib.rs` (add module)

- [ ] **Step 1: Add module to lib.rs**

Update `crates/sa_universe/src/lib.rs`:
```rust
pub mod object_id;
pub mod seed;
pub mod star;

pub use object_id::ObjectId;
pub use seed::{MasterSeed, Rng64, sector_hash};
pub use star::{SpectralClass, Star, generate_star};
```

- [ ] **Step 2: Write failing tests for star.rs**

Create `crates/sa_universe/src/star.rs`:
```rust
use crate::seed::Rng64;
use sa_math::Kelvin;
use serde::{Deserialize, Serialize};

/// Spectral classification based on surface temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectralClass {
    O, B, A, F, G, K, M,
}

/// A procedurally generated star with physically derived properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Star {
    /// Mass in solar masses.
    pub mass: f32,
    /// Surface temperature.
    pub temperature: Kelvin,
    /// Luminosity in solar luminosities.
    pub luminosity: f32,
    /// Radius in solar radii.
    pub radius: f32,
    /// Spectral class.
    pub spectral_class: SpectralClass,
    /// RGB color [0..1] derived from blackbody temperature.
    pub color: [f32; 3],
    /// Apparent brightness (for rendering, 0..1 range).
    pub brightness: f32,
}

/// Sample a stellar mass from the Kroupa IMF using inverse transform sampling.
/// Returns mass in solar masses, range roughly [0.08, 100].
pub fn sample_mass_kroupa(_rng: &mut Rng64) -> f32 {
    todo!()
}

/// Convert blackbody temperature (Kelvin) to approximate RGB [0..1].
pub fn temperature_to_rgb(_temp_k: f32) -> [f32; 3] {
    todo!()
}

/// Classify a star by temperature into a spectral class.
pub fn classify(_temp_k: f32) -> SpectralClass {
    todo!()
}

/// Generate a complete star from a seed.
pub fn generate_star(_seed: u64) -> Star {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kroupa_imf_mass_range() {
        let mut rng = Rng64::new(42);
        for _ in 0..1000 {
            let m = sample_mass_kroupa(&mut rng);
            assert!(m >= 0.08, "Mass too low: {m}");
            assert!(m <= 150.0, "Mass too high: {m}");
        }
    }

    #[test]
    fn kroupa_imf_mostly_low_mass() {
        // The Kroupa IMF is bottom-heavy: most stars should be < 1 solar mass
        let mut rng = Rng64::new(123);
        let masses: Vec<f32> = (0..10000).map(|_| sample_mass_kroupa(&mut rng)).collect();
        let low_mass_count = masses.iter().filter(|&&m| m < 1.0).count();
        let fraction = low_mass_count as f64 / 10000.0;
        assert!(fraction > 0.7, "Expected >70% low-mass stars, got {fraction:.1%}");
    }

    #[test]
    fn kroupa_imf_deterministic() {
        let mut a = Rng64::new(42);
        let mut b = Rng64::new(42);
        for _ in 0..100 {
            assert_eq!(
                sample_mass_kroupa(&mut a).to_bits(),
                sample_mass_kroupa(&mut b).to_bits(),
            );
        }
    }

    #[test]
    fn temperature_to_rgb_hot_is_blue() {
        let [r, g, b] = temperature_to_rgb(30000.0);
        assert!(b > r, "Hot star should be bluer: r={r}, b={b}");
    }

    #[test]
    fn temperature_to_rgb_cool_is_red() {
        let [r, g, b] = temperature_to_rgb(3000.0);
        assert!(r > b, "Cool star should be redder: r={r}, b={b}");
    }

    #[test]
    fn temperature_to_rgb_sun_is_yellowish() {
        let [r, g, b] = temperature_to_rgb(5778.0);
        assert!(r > 0.8, "Sun should have strong red: {r}");
        assert!(g > 0.7, "Sun should have strong green: {g}");
        assert!(b < r, "Sun blue should be less than red: b={b}, r={r}");
    }

    #[test]
    fn temperature_to_rgb_in_range() {
        for temp in [2000.0, 4000.0, 6000.0, 10000.0, 25000.0, 40000.0] {
            let [r, g, b] = temperature_to_rgb(temp);
            assert!((0.0..=1.0).contains(&r), "r out of range at {temp}K: {r}");
            assert!((0.0..=1.0).contains(&g), "g out of range at {temp}K: {g}");
            assert!((0.0..=1.0).contains(&b), "b out of range at {temp}K: {b}");
        }
    }

    #[test]
    fn classify_sun_is_g() {
        assert_eq!(classify(5778.0), SpectralClass::G);
    }

    #[test]
    fn classify_hot_is_o() {
        assert_eq!(classify(35000.0), SpectralClass::O);
    }

    #[test]
    fn classify_cool_is_m() {
        assert_eq!(classify(2800.0), SpectralClass::M);
    }

    #[test]
    fn generate_star_deterministic() {
        let a = generate_star(42);
        let b = generate_star(42);
        assert_eq!(a.mass.to_bits(), b.mass.to_bits());
        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.luminosity.to_bits(), b.luminosity.to_bits());
        assert_eq!(a.spectral_class, b.spectral_class);
    }

    #[test]
    fn generate_star_luminosity_increases_with_mass() {
        // Generate many stars, check that higher mass generally means higher luminosity
        let mut pairs: Vec<(f32, f32)> = (0..500)
            .map(|i| {
                let s = generate_star(i * 7 + 13);
                (s.mass, s.luminosity)
            })
            .collect();
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        // Check that the top-quartile mass has higher average luminosity than bottom-quartile
        let q = pairs.len() / 4;
        let low_avg: f32 = pairs[..q].iter().map(|p| p.1).sum::<f32>() / q as f32;
        let high_avg: f32 = pairs[3 * q..].iter().map(|p| p.1).sum::<f32>() / q as f32;
        assert!(high_avg > low_avg, "High-mass stars should be more luminous");
    }

    #[test]
    fn generate_star_physical_sanity() {
        let s = generate_star(99);
        assert!(s.mass > 0.0);
        assert!(s.temperature.0 > 0.0);
        assert!(s.luminosity > 0.0);
        assert!(s.radius > 0.0);
        assert!(s.brightness > 0.0 && s.brightness <= 1.0);
    }
}
```

Verify tests fail:
```bash
cargo test -p sa_universe
```

- [ ] **Step 3: Implement star.rs**

Replace the `todo!()` bodies in `crates/sa_universe/src/star.rs`:

```rust
use crate::seed::Rng64;
use sa_math::Kelvin;
use serde::{Deserialize, Serialize};

/// Spectral classification based on surface temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectralClass {
    O, B, A, F, G, K, M,
}

/// A procedurally generated star with physically derived properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Star {
    /// Mass in solar masses.
    pub mass: f32,
    /// Surface temperature.
    pub temperature: Kelvin,
    /// Luminosity in solar luminosities.
    pub luminosity: f32,
    /// Radius in solar radii.
    pub radius: f32,
    /// Spectral class.
    pub spectral_class: SpectralClass,
    /// RGB color [0..1] derived from blackbody temperature.
    pub color: [f32; 3],
    /// Apparent brightness (for rendering, 0..1 range).
    pub brightness: f32,
}

/// Sample a stellar mass from the Kroupa IMF using inverse transform sampling.
/// Kroupa IMF: dN/dM ~ M^(-alpha) where alpha=1.3 for M<0.5, alpha=2.3 for M>=0.5.
/// Returns mass in solar masses, range [0.08, 100].
pub fn sample_mass_kroupa(rng: &mut Rng64) -> f32 {
    let u = rng.next_f64();

    // We split the CDF into two segments at M=0.5.
    // Segment 1: M in [0.08, 0.5], alpha = 1.3, exponent = -0.3
    // Segment 2: M in [0.5, 100], alpha = 2.3, exponent = -1.3
    //
    // Unnormalized integrals:
    // I1 = integral(M^-1.3, 0.08, 0.5) = (M^-0.3 / -0.3) from 0.08 to 0.5
    // I2 = k * integral(M^-2.3, 0.5, 100) where k = 0.5^(-1.3)/0.5^(-2.3) = 0.5
    // k ensures continuity at M=0.5.

    let a1: f64 = -0.3;
    let a2: f64 = -1.3;
    let m_lo: f64 = 0.08;
    let m_mid: f64 = 0.5;
    let m_hi: f64 = 100.0;

    // Continuity factor: at M=0.5, the two power laws must match.
    // k = m_mid^(a2+1) / m_mid^(a1+1) is handled by using m_mid as pivot.
    let k: f64 = m_mid.powf(-1.3) / m_mid.powf(-2.3); // = m_mid^(1.0) = 0.5

    let i1 = (m_mid.powf(a1 + 1.0) - m_lo.powf(a1 + 1.0)) / (a1 + 1.0);
    let i2 = k * (m_hi.powf(a2 + 1.0) - m_mid.powf(a2 + 1.0)) / (a2 + 1.0);
    let total = i1 + i2;

    let p1 = i1 / total;

    if u < p1 {
        // Invert CDF for segment 1: M^(a1+1) = M_lo^(a1+1) + u_scaled * (a1+1) * total / 1.0
        let u_seg = u / p1;
        let lo_pow = m_lo.powf(a1 + 1.0);
        let hi_pow = m_mid.powf(a1 + 1.0);
        let m = (lo_pow + u_seg * (hi_pow - lo_pow)).powf(1.0 / (a1 + 1.0));
        m as f32
    } else {
        // Invert CDF for segment 2
        let u_seg = (u - p1) / (1.0 - p1);
        let lo_pow = m_mid.powf(a2 + 1.0);
        let hi_pow = m_hi.powf(a2 + 1.0);
        let m = (lo_pow + u_seg * (hi_pow - lo_pow)).powf(1.0 / (a2 + 1.0));
        m as f32
    }
}

/// Convert blackbody temperature (Kelvin) to approximate RGB [0..1].
/// Uses Tanner Helland's algorithm (attempt to fit Planckian locus).
pub fn temperature_to_rgb(temp_k: f32) -> [f32; 3] {
    let t = (temp_k / 100.0).clamp(10.0, 400.0);

    let r = if t <= 66.0 {
        1.0
    } else {
        let x = t - 60.0;
        (329.698727446 * x.powf(-0.1332047592) / 255.0).clamp(0.0, 1.0)
    };

    let g = if t <= 66.0 {
        let x = t;
        (99.4708025861 * x.ln() - 161.1195681661).clamp(0.0, 255.0) / 255.0
    } else {
        let x = t - 60.0;
        (288.1221695283 * x.powf(-0.0755148492) / 255.0).clamp(0.0, 1.0)
    };

    let b = if t >= 66.0 {
        1.0
    } else if t <= 19.0 {
        0.0
    } else {
        let x = t - 10.0;
        (138.5177312231 * x.ln() - 305.0447927307).clamp(0.0, 255.0) / 255.0
    };

    [r, g, b]
}

/// Classify a star by temperature into a spectral class.
pub fn classify(temp_k: f32) -> SpectralClass {
    match temp_k as u32 {
        0..3700 => SpectralClass::M,
        3700..5200 => SpectralClass::K,
        5200..6000 => SpectralClass::G,
        6000..7500 => SpectralClass::F,
        7500..10000 => SpectralClass::A,
        10000..30000 => SpectralClass::B,
        _ => SpectralClass::O,
    }
}

/// Generate a complete star from a seed.
pub fn generate_star(seed: u64) -> Star {
    let mut rng = Rng64::new(seed);
    let mass = sample_mass_kroupa(&mut rng);

    // Main sequence relations (approximations)
    let temperature = Kelvin(5778.0 * mass.powf(0.57));
    let luminosity = mass.powf(3.5);
    let radius = mass.powf(0.8);

    let spectral_class = classify(temperature.0);
    let color = temperature_to_rgb(temperature.0);

    // Brightness for rendering: log-scaled luminosity mapped to [0.1, 1.0]
    let brightness = (0.1 + 0.9 * (luminosity.ln().max(0.0) / 15.0_f32.ln())).clamp(0.1, 1.0);

    Star {
        mass,
        temperature,
        luminosity,
        radius,
        spectral_class,
        color,
        brightness,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kroupa_imf_mass_range() {
        let mut rng = Rng64::new(42);
        for _ in 0..1000 {
            let m = sample_mass_kroupa(&mut rng);
            assert!(m >= 0.08, "Mass too low: {m}");
            assert!(m <= 150.0, "Mass too high: {m}");
        }
    }

    #[test]
    fn kroupa_imf_mostly_low_mass() {
        let mut rng = Rng64::new(123);
        let masses: Vec<f32> = (0..10000).map(|_| sample_mass_kroupa(&mut rng)).collect();
        let low_mass_count = masses.iter().filter(|&&m| m < 1.0).count();
        let fraction = low_mass_count as f64 / 10000.0;
        assert!(fraction > 0.7, "Expected >70% low-mass stars, got {fraction:.1%}");
    }

    #[test]
    fn kroupa_imf_deterministic() {
        let mut a = Rng64::new(42);
        let mut b = Rng64::new(42);
        for _ in 0..100 {
            assert_eq!(
                sample_mass_kroupa(&mut a).to_bits(),
                sample_mass_kroupa(&mut b).to_bits(),
            );
        }
    }

    #[test]
    fn temperature_to_rgb_hot_is_blue() {
        let [r, _, b] = temperature_to_rgb(30000.0);
        assert!(b > r, "Hot star should be bluer: r={r}, b={b}");
    }

    #[test]
    fn temperature_to_rgb_cool_is_red() {
        let [r, _, b] = temperature_to_rgb(3000.0);
        assert!(r > b, "Cool star should be redder: r={r}, b={b}");
    }

    #[test]
    fn temperature_to_rgb_sun_is_yellowish() {
        let [r, g, b] = temperature_to_rgb(5778.0);
        assert!(r > 0.8, "Sun should have strong red: {r}");
        assert!(g > 0.7, "Sun should have strong green: {g}");
        assert!(b < r, "Sun blue should be less than red: b={b}, r={r}");
    }

    #[test]
    fn temperature_to_rgb_in_range() {
        for temp in [2000.0, 4000.0, 6000.0, 10000.0, 25000.0, 40000.0] {
            let [r, g, b] = temperature_to_rgb(temp);
            assert!((0.0..=1.0).contains(&r), "r out of range at {temp}K: {r}");
            assert!((0.0..=1.0).contains(&g), "g out of range at {temp}K: {g}");
            assert!((0.0..=1.0).contains(&b), "b out of range at {temp}K: {b}");
        }
    }

    #[test]
    fn classify_sun_is_g() {
        assert_eq!(classify(5778.0), SpectralClass::G);
    }

    #[test]
    fn classify_hot_is_o() {
        assert_eq!(classify(35000.0), SpectralClass::O);
    }

    #[test]
    fn classify_cool_is_m() {
        assert_eq!(classify(2800.0), SpectralClass::M);
    }

    #[test]
    fn generate_star_deterministic() {
        let a = generate_star(42);
        let b = generate_star(42);
        assert_eq!(a.mass.to_bits(), b.mass.to_bits());
        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.luminosity.to_bits(), b.luminosity.to_bits());
        assert_eq!(a.spectral_class, b.spectral_class);
    }

    #[test]
    fn generate_star_luminosity_increases_with_mass() {
        let mut pairs: Vec<(f32, f32)> = (0..500)
            .map(|i| {
                let s = generate_star(i * 7 + 13);
                (s.mass, s.luminosity)
            })
            .collect();
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let q = pairs.len() / 4;
        let low_avg: f32 = pairs[..q].iter().map(|p| p.1).sum::<f32>() / q as f32;
        let high_avg: f32 = pairs[3 * q..].iter().map(|p| p.1).sum::<f32>() / q as f32;
        assert!(high_avg > low_avg, "High-mass stars should be more luminous");
    }

    #[test]
    fn generate_star_physical_sanity() {
        let s = generate_star(99);
        assert!(s.mass > 0.0);
        assert!(s.temperature.0 > 0.0);
        assert!(s.luminosity > 0.0);
        assert!(s.radius > 0.0);
        assert!(s.brightness > 0.0 && s.brightness <= 1.0);
    }
}
```

Verify tests pass:
```bash
cargo test -p sa_universe
```

Then lint:
```bash
cargo clippy -p sa_universe -- -D warnings
```

Commit: `feat(sa_universe): add H-R diagram star generation with Kroupa IMF`

---

### Task 4: sector.rs (Poisson Disk + Star Placement)

**Files:**
- Create: `crates/sa_universe/src/sector.rs`
- Modify: `crates/sa_universe/src/lib.rs` (add module)

- [ ] **Step 1: Add module to lib.rs**

Update `crates/sa_universe/src/lib.rs`:
```rust
pub mod object_id;
pub mod sector;
pub mod seed;
pub mod star;

pub use object_id::ObjectId;
pub use sector::{Sector, SectorCoord, SECTOR_SIZE_LY};
pub use seed::{MasterSeed, Rng64, sector_hash};
pub use star::{SpectralClass, Star, generate_star};
```

- [ ] **Step 2: Write failing tests for sector.rs**

Create `crates/sa_universe/src/sector.rs`:
```rust
use crate::object_id::ObjectId;
use crate::seed::{MasterSeed, Rng64, sector_hash};
use crate::star::{Star, generate_star};
use sa_math::WorldPos;
use serde::{Deserialize, Serialize};

/// Sector size in light-years per side.
pub const SECTOR_SIZE_LY: f64 = 10.0;

/// Integer sector coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectorCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectorCoord {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Convert a WorldPos (in light-years) to the sector it falls in.
    pub fn from_world_pos(pos: WorldPos) -> Self {
        Self {
            x: (pos.x / SECTOR_SIZE_LY).floor() as i32,
            y: (pos.y / SECTOR_SIZE_LY).floor() as i32,
            z: (pos.z / SECTOR_SIZE_LY).floor() as i32,
        }
    }

    /// World-space origin (minimum corner) of this sector.
    pub fn world_origin(self) -> WorldPos {
        WorldPos::new(
            self.x as f64 * SECTOR_SIZE_LY,
            self.y as f64 * SECTOR_SIZE_LY,
            self.z as f64 * SECTOR_SIZE_LY,
        )
    }
}

/// A star placed within a sector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedStar {
    pub id: ObjectId,
    pub position: WorldPos,
    pub star: Star,
}

/// A generated sector containing placed stars.
#[derive(Debug, Clone)]
pub struct Sector {
    pub coord: SectorCoord,
    pub stars: Vec<PlacedStar>,
}

/// Compute star density for a sector based on distance from galactic center.
/// Returns approximate number of stars per sector.
fn sector_density(_coord: SectorCoord) -> u32 {
    todo!()
}

/// 3D Poisson disk sampling (Bridson's algorithm) within a unit cube,
/// then scaled to sector size. Returns positions in [0, SECTOR_SIZE_LY]^3.
fn poisson_disk_3d(_rng: &mut Rng64, _count: u32, _min_distance: f64) -> Vec<[f64; 3]> {
    todo!()
}

/// Generate all stars in a sector, deterministically from the master seed.
pub fn generate_sector(_master: MasterSeed, _coord: SectorCoord) -> Sector {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_coord_from_world_pos() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(15.0, -5.0, 25.0));
        assert_eq!(coord, SectorCoord::new(1, -1, 2));
    }

    #[test]
    fn sector_coord_from_world_pos_exact_boundary() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(10.0, 0.0, 0.0));
        assert_eq!(coord, SectorCoord::new(1, 0, 0));
    }

    #[test]
    fn sector_coord_from_world_pos_negative() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(-0.1, -10.1, 0.0));
        assert_eq!(coord, SectorCoord::new(-1, -2, 0));
    }

    #[test]
    fn sector_world_origin() {
        let origin = SectorCoord::new(1, -1, 2).world_origin();
        assert!((origin.x - 10.0).abs() < 1e-10);
        assert!((origin.y - (-10.0)).abs() < 1e-10);
        assert!((origin.z - 20.0).abs() < 1e-10);
    }

    #[test]
    fn sector_density_center_higher_than_edge() {
        let center = sector_density(SectorCoord::new(0, 0, 0));
        let edge = sector_density(SectorCoord::new(1000, 0, 0));
        assert!(
            center >= edge,
            "Center density ({center}) should be >= edge density ({edge})"
        );
    }

    #[test]
    fn poisson_disk_returns_points() {
        let mut rng = Rng64::new(42);
        let pts = poisson_disk_3d(&mut rng, 20, 1.0);
        assert!(!pts.is_empty(), "Should return at least some points");
    }

    #[test]
    fn poisson_disk_minimum_distance() {
        let mut rng = Rng64::new(42);
        let min_dist = 1.5;
        let pts = poisson_disk_3d(&mut rng, 15, min_dist);
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let dx = pts[i][0] - pts[j][0];
                let dy = pts[i][1] - pts[j][1];
                let dz = pts[i][2] - pts[j][2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!(
                    dist >= min_dist * 0.99,
                    "Points {i} and {j} too close: {dist} < {min_dist}"
                );
            }
        }
    }

    #[test]
    fn poisson_disk_points_in_bounds() {
        let mut rng = Rng64::new(99);
        let pts = poisson_disk_3d(&mut rng, 30, 1.0);
        for (i, p) in pts.iter().enumerate() {
            for d in 0..3 {
                assert!(
                    (0.0..=SECTOR_SIZE_LY).contains(&p[d]),
                    "Point {i} dim {d} out of bounds: {}",
                    p[d]
                );
            }
        }
    }

    #[test]
    fn generate_sector_deterministic() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(5, -3, 1);
        let a = generate_sector(seed, coord);
        let b = generate_sector(seed, coord);
        assert_eq!(a.stars.len(), b.stars.len());
        for (sa, sb) in a.stars.iter().zip(b.stars.iter()) {
            assert_eq!(sa.id, sb.id);
            assert_eq!(sa.position, sb.position);
            assert_eq!(sa.star.mass.to_bits(), sb.star.mass.to_bits());
        }
    }

    #[test]
    fn generate_sector_stars_inside_sector() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(2, -1, 0);
        let sector = generate_sector(seed, coord);
        let origin = coord.world_origin();
        for ps in &sector.stars {
            assert!(ps.position.x >= origin.x && ps.position.x <= origin.x + SECTOR_SIZE_LY);
            assert!(ps.position.y >= origin.y && ps.position.y <= origin.y + SECTOR_SIZE_LY);
            assert!(ps.position.z >= origin.z && ps.position.z <= origin.z + SECTOR_SIZE_LY);
        }
    }

    #[test]
    fn generate_sector_has_valid_object_ids() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(3, 4, 5);
        let sector = generate_sector(seed, coord);
        for (i, ps) in sector.stars.iter().enumerate() {
            assert_eq!(ps.id.sector_x(), coord.x as i16);
            assert_eq!(ps.id.sector_y(), coord.y as i16);
            assert_eq!(ps.id.sector_z(), coord.z as i16);
            assert_eq!(ps.id.system(), i as u8);
            assert_eq!(ps.id.body(), 0);
        }
    }
}
```

Verify tests fail:
```bash
cargo test -p sa_universe
```

- [ ] **Step 3: Implement sector.rs**

Replace the `todo!()` bodies in `crates/sa_universe/src/sector.rs`:

```rust
use crate::object_id::ObjectId;
use crate::seed::{MasterSeed, Rng64, sector_hash};
use crate::star::{Star, generate_star};
use sa_math::WorldPos;
use serde::{Deserialize, Serialize};

/// Sector size in light-years per side.
pub const SECTOR_SIZE_LY: f64 = 10.0;

/// Integer sector coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectorCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SectorCoord {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Convert a WorldPos (in light-years) to the sector it falls in.
    pub fn from_world_pos(pos: WorldPos) -> Self {
        Self {
            x: (pos.x / SECTOR_SIZE_LY).floor() as i32,
            y: (pos.y / SECTOR_SIZE_LY).floor() as i32,
            z: (pos.z / SECTOR_SIZE_LY).floor() as i32,
        }
    }

    /// World-space origin (minimum corner) of this sector.
    pub fn world_origin(self) -> WorldPos {
        WorldPos::new(
            self.x as f64 * SECTOR_SIZE_LY,
            self.y as f64 * SECTOR_SIZE_LY,
            self.z as f64 * SECTOR_SIZE_LY,
        )
    }
}

/// A star placed within a sector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedStar {
    pub id: ObjectId,
    pub position: WorldPos,
    pub star: Star,
}

/// A generated sector containing placed stars.
#[derive(Debug, Clone)]
pub struct Sector {
    pub coord: SectorCoord,
    pub stars: Vec<PlacedStar>,
}

/// Compute star density for a sector based on distance from galactic center.
/// The galaxy layer: exponential falloff from center.
/// Returns approximate number of stars per sector.
fn sector_density(coord: SectorCoord) -> u32 {
    let dx = coord.x as f64;
    let dy = coord.y as f64;
    let dz = coord.z as f64;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();

    // Base density near center ~20 stars per sector, decaying with scale radius ~200 sectors.
    let base = 20.0;
    let scale_radius = 200.0;
    let density = base * (-dist / scale_radius).exp();

    // Minimum 1 star per sector to avoid empty voids everywhere
    (density as u32).max(1)
}

/// 3D Poisson disk sampling (Bridson's algorithm) within the sector cube.
/// Returns positions in [0, SECTOR_SIZE_LY]^3.
fn poisson_disk_3d(rng: &mut Rng64, count: u32, min_distance: f64) -> Vec<[f64; 3]> {
    let size = SECTOR_SIZE_LY;
    let max_attempts = 30;
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(count as usize);
    let mut active: Vec<usize> = Vec::new();

    // First point: random in the cube
    let p0 = [
        rng.next_f64() * size,
        rng.next_f64() * size,
        rng.next_f64() * size,
    ];
    points.push(p0);
    active.push(0);

    while !active.is_empty() && points.len() < count as usize {
        // Pick a random active point
        let active_idx = (rng.next_u64() % active.len() as u64) as usize;
        let parent = points[active[active_idx]];
        let mut found = false;

        for _ in 0..max_attempts {
            // Random point in spherical shell [min_distance, 2*min_distance]
            let r = min_distance * (1.0 + rng.next_f64());
            let theta = rng.next_f64() * std::f64::consts::TAU;
            let phi = (rng.next_f64() * 2.0 - 1.0).acos();

            let dx = r * phi.sin() * theta.cos();
            let dy = r * phi.sin() * theta.sin();
            let dz = r * phi.cos();

            let candidate = [parent[0] + dx, parent[1] + dy, parent[2] + dz];

            // Check bounds
            if candidate[0] < 0.0 || candidate[0] > size
                || candidate[1] < 0.0 || candidate[1] > size
                || candidate[2] < 0.0 || candidate[2] > size
            {
                continue;
            }

            // Check distance to all existing points (brute force, fine for <256 stars)
            let too_close = points.iter().any(|p| {
                let d0 = p[0] - candidate[0];
                let d1 = p[1] - candidate[1];
                let d2 = p[2] - candidate[2];
                (d0 * d0 + d1 * d1 + d2 * d2) < min_distance * min_distance
            });

            if !too_close {
                active.push(points.len());
                points.push(candidate);
                found = true;
                break;
            }
        }

        if !found {
            active.swap_remove(active_idx);
        }
    }

    points
}

/// Generate all stars in a sector, deterministically from the master seed.
pub fn generate_sector(master: MasterSeed, coord: SectorCoord) -> Sector {
    let hash = sector_hash(master, coord.x, coord.y, coord.z);
    let mut rng = Rng64::new(hash);
    let density = sector_density(coord);

    // Minimum distance between stars scales inversely with density
    let min_dist = SECTOR_SIZE_LY / (density as f64 + 1.0).sqrt();

    let positions = poisson_disk_3d(&mut rng, density, min_dist);
    let origin = coord.world_origin();

    let stars = positions
        .iter()
        .enumerate()
        .map(|(i, local_pos)| {
            let id = ObjectId::star_id(
                coord.x as i16,
                coord.y as i16,
                coord.z as i16,
                i as u8,
            );
            // Each star gets a unique seed derived from the sector hash and its index
            let star_seed = hash.wrapping_add(i as u64).wrapping_mul(0x517CC1B727220A95);
            let star = generate_star(star_seed);
            let position = WorldPos::new(
                origin.x + local_pos[0],
                origin.y + local_pos[1],
                origin.z + local_pos[2],
            );
            PlacedStar { id, position, star }
        })
        .collect();

    Sector { coord, stars }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_coord_from_world_pos() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(15.0, -5.0, 25.0));
        assert_eq!(coord, SectorCoord::new(1, -1, 2));
    }

    #[test]
    fn sector_coord_from_world_pos_exact_boundary() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(10.0, 0.0, 0.0));
        assert_eq!(coord, SectorCoord::new(1, 0, 0));
    }

    #[test]
    fn sector_coord_from_world_pos_negative() {
        let coord = SectorCoord::from_world_pos(WorldPos::new(-0.1, -10.1, 0.0));
        assert_eq!(coord, SectorCoord::new(-1, -2, 0));
    }

    #[test]
    fn sector_world_origin() {
        let origin = SectorCoord::new(1, -1, 2).world_origin();
        assert!((origin.x - 10.0).abs() < 1e-10);
        assert!((origin.y - (-10.0)).abs() < 1e-10);
        assert!((origin.z - 20.0).abs() < 1e-10);
    }

    #[test]
    fn sector_density_center_higher_than_edge() {
        let center = sector_density(SectorCoord::new(0, 0, 0));
        let edge = sector_density(SectorCoord::new(1000, 0, 0));
        assert!(
            center >= edge,
            "Center density ({center}) should be >= edge density ({edge})"
        );
    }

    #[test]
    fn poisson_disk_returns_points() {
        let mut rng = Rng64::new(42);
        let pts = poisson_disk_3d(&mut rng, 20, 1.0);
        assert!(!pts.is_empty(), "Should return at least some points");
    }

    #[test]
    fn poisson_disk_minimum_distance() {
        let mut rng = Rng64::new(42);
        let min_dist = 1.5;
        let pts = poisson_disk_3d(&mut rng, 15, min_dist);
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let dx = pts[i][0] - pts[j][0];
                let dy = pts[i][1] - pts[j][1];
                let dz = pts[i][2] - pts[j][2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!(
                    dist >= min_dist * 0.99,
                    "Points {i} and {j} too close: {dist} < {min_dist}"
                );
            }
        }
    }

    #[test]
    fn poisson_disk_points_in_bounds() {
        let mut rng = Rng64::new(99);
        let pts = poisson_disk_3d(&mut rng, 30, 1.0);
        for (i, p) in pts.iter().enumerate() {
            for d in 0..3 {
                assert!(
                    (0.0..=SECTOR_SIZE_LY).contains(&p[d]),
                    "Point {i} dim {d} out of bounds: {}",
                    p[d]
                );
            }
        }
    }

    #[test]
    fn generate_sector_deterministic() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(5, -3, 1);
        let a = generate_sector(seed, coord);
        let b = generate_sector(seed, coord);
        assert_eq!(a.stars.len(), b.stars.len());
        for (sa, sb) in a.stars.iter().zip(b.stars.iter()) {
            assert_eq!(sa.id, sb.id);
            assert_eq!(sa.position, sb.position);
            assert_eq!(sa.star.mass.to_bits(), sb.star.mass.to_bits());
        }
    }

    #[test]
    fn generate_sector_stars_inside_sector() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(2, -1, 0);
        let sector = generate_sector(seed, coord);
        let origin = coord.world_origin();
        for ps in &sector.stars {
            assert!(ps.position.x >= origin.x && ps.position.x <= origin.x + SECTOR_SIZE_LY);
            assert!(ps.position.y >= origin.y && ps.position.y <= origin.y + SECTOR_SIZE_LY);
            assert!(ps.position.z >= origin.z && ps.position.z <= origin.z + SECTOR_SIZE_LY);
        }
    }

    #[test]
    fn generate_sector_has_valid_object_ids() {
        let seed = MasterSeed(42);
        let coord = SectorCoord::new(3, 4, 5);
        let sector = generate_sector(seed, coord);
        for (i, ps) in sector.stars.iter().enumerate() {
            assert_eq!(ps.id.sector_x(), coord.x as i16);
            assert_eq!(ps.id.sector_y(), coord.y as i16);
            assert_eq!(ps.id.sector_z(), coord.z as i16);
            assert_eq!(ps.id.system(), i as u8);
            assert_eq!(ps.id.body(), 0);
        }
    }
}
```

Verify tests pass:
```bash
cargo test -p sa_universe
```

Then lint:
```bash
cargo clippy -p sa_universe -- -D warnings
```

Commit: `feat(sa_universe): add sector generation with Poisson disk star placement`

---

### Task 5: system.rs (Planetary Formation)

**Files:**
- Create: `crates/sa_universe/src/system.rs`
- Modify: `crates/sa_universe/src/lib.rs` (add module)

- [ ] **Step 1: Add module to lib.rs**

Update `crates/sa_universe/src/lib.rs`:
```rust
pub mod object_id;
pub mod sector;
pub mod seed;
pub mod star;
pub mod system;

pub use object_id::ObjectId;
pub use sector::{Sector, SectorCoord, SECTOR_SIZE_LY};
pub use seed::{MasterSeed, Rng64, sector_hash};
pub use star::{SpectralClass, Star, generate_star};
pub use system::{Planet, PlanetType, PlanetarySystem, generate_system};
```

- [ ] **Step 2: Write failing tests for system.rs**

Create `crates/sa_universe/src/system.rs`:
```rust
use crate::seed::Rng64;
use crate::star::Star;
use serde::{Deserialize, Serialize};

/// Classification of a planet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetType {
    Rocky,
    GasGiant,
    IceGiant,
}

/// A procedurally generated planet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    /// Orbital radius in AU.
    pub orbital_radius_au: f32,
    /// Mass in Earth masses.
    pub mass_earth: f32,
    /// Radius in Earth radii.
    pub radius_earth: f32,
    /// Orbital period in Earth years.
    pub orbital_period_years: f32,
    /// Planet classification.
    pub planet_type: PlanetType,
}

/// A star with its planetary system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetarySystem {
    pub planets: Vec<Planet>,
}

/// Compute the frost line distance in AU for a star.
/// Frost line ~ 3 AU * sqrt(L/L_sun).
fn frost_line_au(_luminosity: f32) -> f32 {
    todo!()
}

/// Generate a planetary system from a star and a seed.
pub fn generate_system(_star: &Star, _seed: u64) -> PlanetarySystem {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::star::generate_star;

    #[test]
    fn frost_line_sun_approximately_3au() {
        let fl = frost_line_au(1.0);
        assert!((fl - 3.0).abs() < 0.5, "Frost line for Sun should be ~3 AU, got {fl}");
    }

    #[test]
    fn frost_line_increases_with_luminosity() {
        let fl_low = frost_line_au(0.1);
        let fl_high = frost_line_au(100.0);
        assert!(fl_high > fl_low, "Frost line should increase with luminosity");
    }

    #[test]
    fn generate_system_deterministic() {
        let star = generate_star(42);
        let a = generate_system(&star, 100);
        let b = generate_system(&star, 100);
        assert_eq!(a.planets.len(), b.planets.len());
        for (pa, pb) in a.planets.iter().zip(b.planets.iter()) {
            assert_eq!(pa.orbital_radius_au.to_bits(), pb.orbital_radius_au.to_bits());
            assert_eq!(pa.mass_earth.to_bits(), pb.mass_earth.to_bits());
            assert_eq!(pa.planet_type, pb.planet_type);
        }
    }

    #[test]
    fn generate_system_planets_ordered_by_radius() {
        let star = generate_star(77);
        let sys = generate_system(&star, 200);
        for w in sys.planets.windows(2) {
            assert!(
                w[1].orbital_radius_au >= w[0].orbital_radius_au,
                "Planets should be ordered by orbital radius"
            );
        }
    }

    #[test]
    fn generate_system_inner_planets_rocky() {
        let star = generate_star(42);
        let fl = frost_line_au(star.luminosity);
        let sys = generate_system(&star, 300);
        for p in &sys.planets {
            if p.orbital_radius_au < fl {
                assert_eq!(
                    p.planet_type, PlanetType::Rocky,
                    "Inner planet at {} AU should be rocky (frost line at {fl} AU)",
                    p.orbital_radius_au
                );
            }
        }
    }

    #[test]
    fn generate_system_reasonable_planet_count() {
        // Generate many systems, check planet count is reasonable (0-12)
        for i in 0..100 {
            let star = generate_star(i * 13 + 7);
            let sys = generate_system(&star, i * 17 + 3);
            assert!(
                sys.planets.len() <= 12,
                "Too many planets: {} for seed {}",
                sys.planets.len(), i
            );
        }
    }

    #[test]
    fn generate_system_orbital_periods_physical() {
        // Kepler's 3rd law: P^2 ~ a^3 (in AU and years for solar mass)
        let star = generate_star(42);
        let sys = generate_system(&star, 500);
        for p in &sys.planets {
            if p.orbital_radius_au > 0.0 {
                assert!(p.orbital_period_years > 0.0, "Period must be positive");
            }
        }
    }

    #[test]
    fn generate_system_planet_mass_positive() {
        let star = generate_star(42);
        let sys = generate_system(&star, 600);
        for p in &sys.planets {
            assert!(p.mass_earth > 0.0, "Planet mass must be positive");
            assert!(p.radius_earth > 0.0, "Planet radius must be positive");
        }
    }
}
```

Verify tests fail:
```bash
cargo test -p sa_universe
```

- [ ] **Step 3: Implement system.rs**

Replace the `todo!()` bodies in `crates/sa_universe/src/system.rs`:

```rust
use crate::seed::Rng64;
use crate::star::Star;
use serde::{Deserialize, Serialize};

/// Classification of a planet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetType {
    Rocky,
    GasGiant,
    IceGiant,
}

/// A procedurally generated planet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Planet {
    /// Orbital radius in AU.
    pub orbital_radius_au: f32,
    /// Mass in Earth masses.
    pub mass_earth: f32,
    /// Radius in Earth radii.
    pub radius_earth: f32,
    /// Orbital period in Earth years.
    pub orbital_period_years: f32,
    /// Planet classification.
    pub planet_type: PlanetType,
}

/// A star with its planetary system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetarySystem {
    pub planets: Vec<Planet>,
}

/// Compute the frost line distance in AU for a star.
/// Frost line ~ 3 AU * sqrt(L/L_sun).
fn frost_line_au(luminosity: f32) -> f32 {
    3.0 * luminosity.sqrt()
}

/// Generate a planetary system from a star and a seed.
pub fn generate_system(star: &Star, seed: u64) -> PlanetarySystem {
    let mut rng = Rng64::new(seed);

    // Number of planets: 0-10, biased toward 3-6
    let planet_count = {
        let raw = rng.range_f32(0.0, 1.0);
        // Use a triangular-ish distribution peaking around 4
        let n = (raw * 10.0).round() as u32;
        n.min(10)
    };

    let frost_line = frost_line_au(star.luminosity);

    // Place planets at increasing orbital radii using Titius-Bode-like spacing.
    // Start between 0.2 and 0.6 AU, each next planet ~1.4-2.2x further.
    let mut planets = Vec::with_capacity(planet_count as usize);
    let mut current_radius = rng.range_f32(0.2, 0.6);

    for _ in 0..planet_count {
        let orbital_radius_au = current_radius;

        let planet_type = if orbital_radius_au < frost_line {
            PlanetType::Rocky
        } else {
            // Outer planets: 60% gas giant, 40% ice giant
            if rng.next_f32() < 0.6 {
                PlanetType::GasGiant
            } else {
                PlanetType::IceGiant
            }
        };

        let mass_earth = match planet_type {
            PlanetType::Rocky => rng.range_f32(0.05, 5.0),
            PlanetType::GasGiant => rng.range_f32(10.0, 4000.0),
            PlanetType::IceGiant => rng.range_f32(5.0, 50.0),
        };

        // Radius approximation from mass
        let radius_earth = match planet_type {
            PlanetType::Rocky => mass_earth.powf(0.27),
            PlanetType::GasGiant => {
                // Gas giants: radius grows slowly with mass (Jupiter paradox)
                3.0 + (mass_earth / 318.0).powf(0.1) * 8.0
            }
            PlanetType::IceGiant => 2.0 + (mass_earth / 17.0).powf(0.3) * 2.0,
        };

        // Kepler's 3rd law: P^2 = a^3 / M_star (years, AU, solar masses)
        let orbital_period_years = (orbital_radius_au.powf(3.0) / star.mass).sqrt();

        planets.push(Planet {
            orbital_radius_au,
            mass_earth,
            radius_earth,
            orbital_period_years,
            planet_type,
        });

        // Next planet: spacing factor 1.4 to 2.2x
        current_radius *= rng.range_f32(1.4, 2.2);
    }

    PlanetarySystem { planets }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::star::generate_star;

    #[test]
    fn frost_line_sun_approximately_3au() {
        let fl = frost_line_au(1.0);
        assert!((fl - 3.0).abs() < 0.5, "Frost line for Sun should be ~3 AU, got {fl}");
    }

    #[test]
    fn frost_line_increases_with_luminosity() {
        let fl_low = frost_line_au(0.1);
        let fl_high = frost_line_au(100.0);
        assert!(fl_high > fl_low, "Frost line should increase with luminosity");
    }

    #[test]
    fn generate_system_deterministic() {
        let star = generate_star(42);
        let a = generate_system(&star, 100);
        let b = generate_system(&star, 100);
        assert_eq!(a.planets.len(), b.planets.len());
        for (pa, pb) in a.planets.iter().zip(b.planets.iter()) {
            assert_eq!(pa.orbital_radius_au.to_bits(), pb.orbital_radius_au.to_bits());
            assert_eq!(pa.mass_earth.to_bits(), pb.mass_earth.to_bits());
            assert_eq!(pa.planet_type, pb.planet_type);
        }
    }

    #[test]
    fn generate_system_planets_ordered_by_radius() {
        let star = generate_star(77);
        let sys = generate_system(&star, 200);
        for w in sys.planets.windows(2) {
            assert!(
                w[1].orbital_radius_au >= w[0].orbital_radius_au,
                "Planets should be ordered by orbital radius"
            );
        }
    }

    #[test]
    fn generate_system_inner_planets_rocky() {
        let star = generate_star(42);
        let fl = frost_line_au(star.luminosity);
        let sys = generate_system(&star, 300);
        for p in &sys.planets {
            if p.orbital_radius_au < fl {
                assert_eq!(
                    p.planet_type, PlanetType::Rocky,
                    "Inner planet at {} AU should be rocky (frost line at {fl} AU)",
                    p.orbital_radius_au
                );
            }
        }
    }

    #[test]
    fn generate_system_reasonable_planet_count() {
        for i in 0..100 {
            let star = generate_star(i * 13 + 7);
            let sys = generate_system(&star, i * 17 + 3);
            assert!(
                sys.planets.len() <= 12,
                "Too many planets: {} for seed {}",
                sys.planets.len(), i
            );
        }
    }

    #[test]
    fn generate_system_orbital_periods_physical() {
        let star = generate_star(42);
        let sys = generate_system(&star, 500);
        for p in &sys.planets {
            if p.orbital_radius_au > 0.0 {
                assert!(p.orbital_period_years > 0.0, "Period must be positive");
            }
        }
    }

    #[test]
    fn generate_system_planet_mass_positive() {
        let star = generate_star(42);
        let sys = generate_system(&star, 600);
        for p in &sys.planets {
            assert!(p.mass_earth > 0.0, "Planet mass must be positive");
            assert!(p.radius_earth > 0.0, "Planet radius must be positive");
        }
    }
}
```

Verify tests pass:
```bash
cargo test -p sa_universe
```

Then lint:
```bash
cargo clippy -p sa_universe -- -D warnings
```

Commit: `feat(sa_universe): add planetary system generation with frost line model`

---

### Task 6: query.rs (Spatial Queries for Visible Stars)

**Files:**
- Create: `crates/sa_universe/src/query.rs`
- Modify: `crates/sa_universe/src/lib.rs` (add module)

- [ ] **Step 1: Add module to lib.rs**

Update `crates/sa_universe/src/lib.rs`:
```rust
pub mod object_id;
pub mod query;
pub mod sector;
pub mod seed;
pub mod star;
pub mod system;

pub use object_id::ObjectId;
pub use query::{Universe, VisibleStar};
pub use sector::{Sector, SectorCoord, SECTOR_SIZE_LY};
pub use seed::{MasterSeed, Rng64, sector_hash};
pub use star::{SpectralClass, Star, generate_star};
pub use system::{Planet, PlanetType, PlanetarySystem, generate_system};
```

- [ ] **Step 2: Write failing tests for query.rs**

Create `crates/sa_universe/src/query.rs`:
```rust
use crate::object_id::ObjectId;
use crate::sector::{SectorCoord, generate_sector, SECTOR_SIZE_LY};
use crate::seed::MasterSeed;
use sa_math::WorldPos;

/// A star ready for rendering: position relative to observer, plus visual data.
#[derive(Debug, Clone)]
pub struct VisibleStar {
    pub id: ObjectId,
    /// Position relative to the observer (camera-space, in light-years).
    pub relative_pos: [f32; 3],
    pub brightness: f32,
    pub color: [f32; 3],
}

/// Top-level universe handle. Holds the master seed and provides queries.
pub struct Universe {
    pub seed: MasterSeed,
}

impl Universe {
    pub fn new(seed: MasterSeed) -> Self {
        Self { seed }
    }

    /// Return all sectors within `radius` sectors of the given position.
    pub fn nearby_sectors(&self, _pos: WorldPos, _radius: i32) -> Vec<SectorCoord> {
        todo!()
    }

    /// Query all visible stars within `radius` sectors of `observer_pos`.
    /// Returns stars with positions relative to the observer for rendering.
    pub fn visible_stars(&self, _observer_pos: WorldPos, _radius: i32) -> Vec<VisibleStar> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_sectors_at_origin() {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 1);
        // Radius 1 around sector (0,0,0): should be 3x3x3 = 27 sectors
        assert_eq!(sectors.len(), 27);
    }

    #[test]
    fn nearby_sectors_contains_current() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(5.0, 5.0, 5.0); // sector (0,0,0)
        let sectors = uni.nearby_sectors(pos, 1);
        assert!(
            sectors.contains(&SectorCoord::new(0, 0, 0)),
            "Should contain the sector the observer is in"
        );
    }

    #[test]
    fn nearby_sectors_radius_zero() {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 0);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0], SectorCoord::new(0, 0, 0));
    }

    #[test]
    fn nearby_sectors_offset_position() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(15.0, 0.0, 0.0); // sector (1,0,0)
        let sectors = uni.nearby_sectors(pos, 0);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0], SectorCoord::new(1, 0, 0));
    }

    #[test]
    fn visible_stars_deterministic() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(5.0, 5.0, 5.0);
        let a = uni.visible_stars(pos, 1);
        let b = uni.visible_stars(pos, 1);
        assert_eq!(a.len(), b.len());
        for (sa, sb) in a.iter().zip(b.iter()) {
            assert_eq!(sa.id, sb.id);
            assert_eq!(sa.relative_pos[0].to_bits(), sb.relative_pos[0].to_bits());
        }
    }

    #[test]
    fn visible_stars_have_valid_color() {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, 1);
        for s in &stars {
            for c in &s.color {
                assert!((0.0..=1.0).contains(c), "Color out of range: {c}");
            }
            assert!(s.brightness > 0.0 && s.brightness <= 1.0);
        }
    }

    #[test]
    fn visible_stars_returns_nonempty() {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, 2);
        assert!(!stars.is_empty(), "Should find at least some stars");
    }

    #[test]
    fn visible_stars_relative_positions() {
        let uni = Universe::new(MasterSeed(42));
        let observer = WorldPos::new(50.0, 50.0, 50.0);
        let stars = uni.visible_stars(observer, 1);
        // Relative positions should be small (within a few sector sizes)
        let max_dist = (3.0 * SECTOR_SIZE_LY * SECTOR_SIZE_LY * 4.0).sqrt() as f32;
        for s in &stars {
            let d = (s.relative_pos[0].powi(2)
                + s.relative_pos[1].powi(2)
                + s.relative_pos[2].powi(2))
            .sqrt();
            assert!(
                d < max_dist * 2.0,
                "Star too far from observer: {d} ly"
            );
        }
    }
}
```

Verify tests fail:
```bash
cargo test -p sa_universe
```

- [ ] **Step 3: Implement query.rs**

Replace the `todo!()` bodies in `crates/sa_universe/src/query.rs`:

```rust
use crate::object_id::ObjectId;
use crate::sector::{SectorCoord, generate_sector, SECTOR_SIZE_LY};
use crate::seed::MasterSeed;
use sa_math::WorldPos;

/// A star ready for rendering: position relative to observer, plus visual data.
#[derive(Debug, Clone)]
pub struct VisibleStar {
    pub id: ObjectId,
    /// Position relative to the observer (camera-space, in light-years).
    pub relative_pos: [f32; 3],
    pub brightness: f32,
    pub color: [f32; 3],
}

/// Top-level universe handle. Holds the master seed and provides queries.
pub struct Universe {
    pub seed: MasterSeed,
}

impl Universe {
    pub fn new(seed: MasterSeed) -> Self {
        Self { seed }
    }

    /// Return all sectors within `radius` sectors of the given position.
    pub fn nearby_sectors(&self, pos: WorldPos, radius: i32) -> Vec<SectorCoord> {
        let center = SectorCoord::from_world_pos(pos);
        let mut sectors = Vec::new();
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    sectors.push(SectorCoord::new(
                        center.x + dx,
                        center.y + dy,
                        center.z + dz,
                    ));
                }
            }
        }
        sectors
    }

    /// Query all visible stars within `radius` sectors of `observer_pos`.
    /// Returns stars with positions relative to the observer for rendering.
    pub fn visible_stars(&self, observer_pos: WorldPos, radius: i32) -> Vec<VisibleStar> {
        let sectors = self.nearby_sectors(observer_pos, radius);
        let mut visible = Vec::new();

        for coord in sectors {
            let sector = generate_sector(self.seed, coord);
            for placed in &sector.stars {
                let dx = (placed.position.x - observer_pos.x) as f32;
                let dy = (placed.position.y - observer_pos.y) as f32;
                let dz = (placed.position.z - observer_pos.z) as f32;

                // Distance-based brightness attenuation
                let dist_sq = dx * dx + dy * dy + dz * dz;
                let attenuation = if dist_sq > 0.01 {
                    1.0 / (1.0 + dist_sq * 0.001)
                } else {
                    1.0
                };
                let brightness = (placed.star.brightness * attenuation).clamp(0.01, 1.0);

                visible.push(VisibleStar {
                    id: placed.id,
                    relative_pos: [dx, dy, dz],
                    brightness,
                    color: placed.star.color,
                });
            }
        }

        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_sectors_at_origin() {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 1);
        assert_eq!(sectors.len(), 27);
    }

    #[test]
    fn nearby_sectors_contains_current() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(5.0, 5.0, 5.0);
        let sectors = uni.nearby_sectors(pos, 1);
        assert!(
            sectors.contains(&SectorCoord::new(0, 0, 0)),
            "Should contain the sector the observer is in"
        );
    }

    #[test]
    fn nearby_sectors_radius_zero() {
        let uni = Universe::new(MasterSeed(42));
        let sectors = uni.nearby_sectors(WorldPos::ORIGIN, 0);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0], SectorCoord::new(0, 0, 0));
    }

    #[test]
    fn nearby_sectors_offset_position() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(15.0, 0.0, 0.0);
        let sectors = uni.nearby_sectors(pos, 0);
        assert_eq!(sectors.len(), 1);
        assert_eq!(sectors[0], SectorCoord::new(1, 0, 0));
    }

    #[test]
    fn visible_stars_deterministic() {
        let uni = Universe::new(MasterSeed(42));
        let pos = WorldPos::new(5.0, 5.0, 5.0);
        let a = uni.visible_stars(pos, 1);
        let b = uni.visible_stars(pos, 1);
        assert_eq!(a.len(), b.len());
        for (sa, sb) in a.iter().zip(b.iter()) {
            assert_eq!(sa.id, sb.id);
            assert_eq!(sa.relative_pos[0].to_bits(), sb.relative_pos[0].to_bits());
        }
    }

    #[test]
    fn visible_stars_have_valid_color() {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, 1);
        for s in &stars {
            for c in &s.color {
                assert!((0.0..=1.0).contains(c), "Color out of range: {c}");
            }
            assert!(s.brightness > 0.0 && s.brightness <= 1.0);
        }
    }

    #[test]
    fn visible_stars_returns_nonempty() {
        let uni = Universe::new(MasterSeed(42));
        let stars = uni.visible_stars(WorldPos::ORIGIN, 2);
        assert!(!stars.is_empty(), "Should find at least some stars");
    }

    #[test]
    fn visible_stars_relative_positions() {
        let uni = Universe::new(MasterSeed(42));
        let observer = WorldPos::new(50.0, 50.0, 50.0);
        let stars = uni.visible_stars(observer, 1);
        let max_dist = (3.0 * SECTOR_SIZE_LY * SECTOR_SIZE_LY * 4.0).sqrt() as f32;
        for s in &stars {
            let d = (s.relative_pos[0].powi(2)
                + s.relative_pos[1].powi(2)
                + s.relative_pos[2].powi(2))
            .sqrt();
            assert!(
                d < max_dist * 2.0,
                "Star too far from observer: {d} ly"
            );
        }
    }
}
```

Verify tests pass:
```bash
cargo test -p sa_universe
```

Then lint:
```bash
cargo clippy -p sa_universe -- -D warnings
```

Commit: `feat(sa_universe): add spatial queries for visible stars`

---

### Task 7: Integration with sa_render (Replace Random Stars)

**Files:**
- Modify: `crates/sa_render/src/star_field.rs` (add `from_universe` conversion)
- Modify: `crates/sa_render/src/renderer.rs` (accept universe stars)
- Modify: `crates/sa_render/src/lib.rs` (update re-exports)
- Modify: `crates/sa_render/Cargo.toml` (add sa_universe dependency)

- [ ] **Step 1: Add sa_universe dependency to sa_render**

In `crates/sa_render/Cargo.toml`, add to `[dependencies]`:
```toml
sa_universe.workspace = true
```

- [ ] **Step 2: Add conversion function to star_field.rs**

Add to the bottom of `crates/sa_render/src/star_field.rs` (before the closing, or after `generate_stars`):

```rust
/// Convert universe visible stars into StarVertex data for the GPU.
pub fn stars_from_universe(visible: &[sa_universe::VisibleStar]) -> Vec<StarVertex> {
    visible
        .iter()
        .map(|vs| {
            // Normalize relative position to unit sphere for sky rendering.
            // Stars are rendered on a unit sphere (directional, not positional).
            let dx = vs.relative_pos[0];
            let dy = vs.relative_pos[1];
            let dz = vs.relative_pos[2];
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            let (nx, ny, nz) = if len > 0.0001 {
                (dx / len, dy / len, dz / len)
            } else {
                (0.0, 1.0, 0.0) // degenerate: put straight up
            };

            StarVertex {
                position: [nx, ny, nz],
                brightness: vs.brightness,
                color: vs.color,
                _pad: 0.0,
            }
        })
        .collect()
}
```

- [ ] **Step 3: Update lib.rs re-exports**

In `crates/sa_render/src/lib.rs`, update the star_field re-export line:

Replace:
```rust
pub use star_field::{generate_stars, StarField, StarVertex};
```
With:
```rust
pub use star_field::{generate_stars, stars_from_universe, StarField, StarVertex};
```

- [ ] **Step 4: Add method to Renderer for updating star buffer**

In `crates/sa_render/src/renderer.rs`, add the following method to `impl Renderer` (after the `resize` method, before `render_frame`):

```rust
    /// Rebuild the star vertex buffer from new star data.
    pub fn update_stars(&mut self, gpu: &GpuContext, stars: &[crate::star_field::StarVertex]) {
        self.star_field.vertex_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Star Vertices"),
                    contents: bytemuck::cast_slice(stars),
                    usage: wgpu::BufferUsages::VERTEX,
                });
        self.star_field.star_count = stars.len() as u32;
    }
```

This will also need the import at the top of `renderer.rs`:
```rust
use wgpu::util::DeviceExt;
```
(Note: `DeviceExt` is already imported in renderer.rs --- verify and skip if present.)

Verify the crate compiles:
```bash
cargo check -p sa_render
```

Then lint:
```bash
cargo clippy -p sa_render -- -D warnings
```

Commit: `feat(sa_render): add universe star conversion and dynamic star buffer updates`

---

### Task 8: Update Game Binary + Final Verification

**Files:**
- Modify: `crates/spaceaway/Cargo.toml` (add sa_universe dependency)
- Modify: `crates/spaceaway/src/main.rs` (wire up universe)

- [ ] **Step 1: Add sa_universe dependency to spaceaway**

In `crates/spaceaway/Cargo.toml`, add to `[dependencies]`:
```toml
sa_universe.workspace = true
```

- [ ] **Step 2: Update main.rs to use procedural universe**

In `crates/spaceaway/src/main.rs`:

Add import at the top:
```rust
use sa_universe::{MasterSeed, Universe};
```

Add `universe` field to the `App` struct:
```rust
    universe: Universe,
```

Initialize it in `App::new()`, before the `Self { ... }` return:
```rust
        let universe = Universe::new(MasterSeed(0xDEAD_BEEF_CAFE_BABE));
```

And add to the `Self { ... }` fields:
```rust
            universe,
```

- [ ] **Step 3: Replace random stars with universe stars in setup_scene**

Replace the `setup_scene` method body. The current method only uploads the cube mesh. Add star generation after it:

```rust
    fn setup_scene(&mut self) {
        let renderer = self.renderer.as_mut().unwrap();
        let gpu = self.gpu.as_ref().unwrap();
        let handle = renderer.mesh_store.upload(&gpu.device, &make_cube());
        self.cube_mesh = Some(handle);

        // Generate initial star field from the procedural universe
        let stars = self.universe.visible_stars(self.camera.position, 3);
        let star_verts = sa_render::stars_from_universe(&stars);
        renderer.update_stars(gpu, &star_verts);
        log::info!("Loaded {} procedural stars", star_verts.len());
    }
```

- [ ] **Step 4: Remove the random star generation from Renderer::new**

In `crates/sa_render/src/renderer.rs`, the `Renderer::new` method currently calls `generate_stars(4000, 42)`. Replace it with an empty star buffer so the initial star field is empty (the game binary will populate it in `setup_scene`):

Replace this line:
```rust
        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);
```

With:
```rust
        let star_field = StarField::new(&gpu.device, gpu.config.format, &[]);
```

- [ ] **Step 5: Full build and test**

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo build -p spaceaway
```

Verify all tests pass and the game binary compiles.

- [ ] **Step 6: Optional --- add star refresh on movement (future polish)**

For now, the stars are loaded once at startup. In a future step, the `update` method in `App` can periodically check if the player has crossed a sector boundary and call `update_stars` with new data. This is noted here as a follow-up but not implemented in Phase 4:

```rust
// Future: in App::update(), check if player moved to new sector:
// let new_coord = SectorCoord::from_world_pos(self.camera.position);
// if new_coord != self.last_sector {
//     let stars = self.universe.visible_stars(self.camera.position, 3);
//     let star_verts = sa_render::stars_from_universe(&stars);
//     self.renderer.as_mut().unwrap().update_stars(self.gpu.as_ref().unwrap(), &star_verts);
//     self.last_sector = new_coord;
// }
```

Commit: `feat(spaceaway): wire procedural universe into game binary`

Final verification:
```bash
cargo run -p spaceaway
```

The game should launch with the star field now showing physically-colored procedural stars placed deterministically via the universe seed, instead of the old random star field.
