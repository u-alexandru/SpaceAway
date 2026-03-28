# Solar Systems from Space — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Warp to any star and see a unique procedural solar system with planets, moons, gas giants, atmospheres, rings, and a glowing star — all rendered from space with the flat-shaded low-poly aesthetic.

**Architecture:** Extend `sa_universe/system.rs` with sub-types, moons, atmospheres. Add `sa_render/src/icosphere.rs` for LOD-switched procedural planet mesh generation. Add `spaceaway/src/solar_system.rs` for active system management. Add `spaceaway/src/navigation.rs` for window markers and lock-on targeting. Planets use existing `DrawCommand` pipeline with origin rebasing.

**Tech Stack:** Rust, sa_universe, sa_render (wgpu), sa_math (WorldPos), existing MeshStore + DrawCommand pipeline.

**Spec:** `docs/superpowers/specs/2026-03-28-solar-system-from-space-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sa_universe/src/system.rs` | Modify | Add PlanetSubType, AtmosphereParams, Moon, ring data, star-type planet counts |
| `crates/sa_universe/src/lib.rs` | Modify | Export new types |
| `crates/sa_render/src/icosphere.rs` | Create | Icosphere generation at variable subdivision, vertex displacement, biome coloring |
| `crates/sa_render/src/lib.rs` | Modify | Export icosphere module |
| `crates/spaceaway/src/solar_system.rs` | Create | Active system management, LOD, orbital positions, draw commands |
| `crates/spaceaway/src/navigation.rs` | Create | Window markers, lock-on, gravity well auto-drop, bookmarks |
| `crates/spaceaway/src/main.rs` | Modify | Wire solar system + navigation into game loop |
| `crates/spaceaway/src/ui/helm_screen.rs` | Modify | System overview on arrival, target info |

---

### Task 1: Extend planet generation with sub-types and moons

**Files:**
- Modify: `crates/sa_universe/src/system.rs`
- Modify: `crates/sa_universe/src/lib.rs`

- [ ] **Step 1: Write failing tests for new planet fields**

Add these tests to `system.rs`:

```rust
#[test]
fn planet_has_sub_type() {
    let star = generate_star(42);
    let sys = generate_system(&star, 100);
    for p in &sys.planets {
        // Sub-type should be set for all planets
        assert!(matches!(p.sub_type,
            PlanetSubType::Molten | PlanetSubType::Desert | PlanetSubType::Temperate |
            PlanetSubType::Ocean | PlanetSubType::Frozen | PlanetSubType::Barren |
            PlanetSubType::HotGiant | PlanetSubType::WarmGiant | PlanetSubType::ColdGiant |
            PlanetSubType::CyanIce | PlanetSubType::TealIce
        ));
    }
}

#[test]
fn gas_giants_may_have_rings() {
    // Over many seeds, at least one gas giant should have rings
    let mut found_rings = false;
    for i in 0..200 {
        let star = generate_star(i * 7 + 1);
        let sys = generate_system(&star, i * 13 + 5);
        for p in &sys.planets {
            if p.has_rings { found_rings = true; }
        }
    }
    assert!(found_rings, "At least one gas giant should have rings across 200 systems");
}

#[test]
fn system_has_moons() {
    let mut found_moons = false;
    for i in 0..100 {
        let star = generate_star(i * 11 + 3);
        let sys = generate_system(&star, i * 17 + 7);
        if sys.total_moon_count() > 0 { found_moons = true; }
    }
    assert!(found_moons, "At least some systems should have moons");
}

#[test]
fn planet_count_varies_by_star_type() {
    // M-dwarfs should average fewer planets than G-stars
    let mut m_total = 0u32;
    let mut g_total = 0u32;
    let mut m_count = 0u32;
    let mut g_count = 0u32;
    for i in 0..500 {
        let star = generate_star(i * 7);
        let sys = generate_system(&star, i * 13);
        match star.spectral_class {
            SpectralClass::M => { m_total += sys.planets.len() as u32; m_count += 1; }
            SpectralClass::G => { g_total += sys.planets.len() as u32; g_count += 1; }
            _ => {}
        }
    }
    if m_count > 10 && g_count > 10 {
        let m_avg = m_total as f32 / m_count as f32;
        let g_avg = g_total as f32 / g_count as f32;
        assert!(g_avg > m_avg, "G-stars should average more planets ({g_avg}) than M-dwarfs ({m_avg})");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_universe system`
Expected: FAIL — `sub_type`, `has_rings`, `total_moon_count`, `PlanetSubType` not found

- [ ] **Step 3: Implement extended planet generation**

Add new types and extend `Planet` in `system.rs`:

```rust
/// Planet sub-type classification based on distance, mass, and atmosphere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanetSubType {
    // Rocky
    Molten, Desert, Temperate, Ocean, Frozen, Barren,
    // Gas giant
    HotGiant, WarmGiant, ColdGiant,
    // Ice giant
    CyanIce, TealIce,
}

/// Atmosphere visual parameters for rendering from space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AtmosphereParams {
    pub color: [f32; 3],
    pub opacity: f32,
    pub scattering_power: f32,
}

/// Ring system parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RingParams {
    /// Inner ring radius as multiple of planet radius.
    pub inner_ratio: f32,
    /// Outer ring radius as multiple of planet radius.
    pub outer_ratio: f32,
    /// Primary ring color.
    pub color: [f32; 3],
}

/// A moon orbiting a planet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Moon {
    /// Orbital radius in km from parent planet center.
    pub orbital_radius_km: f32,
    /// Radius in km.
    pub radius_km: f32,
    /// Sub-type (always a rocky variant).
    pub sub_type: PlanetSubType,
    /// Orbital period in hours.
    pub orbital_period_hours: f32,
    /// Initial orbital phase (radians, from seed).
    pub initial_phase: f32,
}
```

Extend `Planet` struct with new fields:
```rust
pub struct Planet {
    pub orbital_radius_au: f32,
    pub mass_earth: f32,
    pub radius_earth: f32,
    pub orbital_period_years: f32,
    pub planet_type: PlanetType,
    // New fields:
    pub sub_type: PlanetSubType,
    pub atmosphere: Option<AtmosphereParams>,
    pub has_rings: bool,
    pub ring_params: Option<RingParams>,
    pub axial_tilt_deg: f32,
    pub rotation_period_hours: f32,
    pub surface_temperature_k: f32,
    pub color_seed: u64,
    pub initial_phase: f32,
    pub moons: Vec<Moon>,
}
```

Add `total_moon_count()` to `PlanetarySystem`:
```rust
impl PlanetarySystem {
    pub fn total_moon_count(&self) -> usize {
        self.planets.iter().map(|p| p.moons.len()).sum()
    }
}
```

Extend `generate_system()` to:
1. Use star-type-dependent planet counts (spec section 4.1)
2. Assign sub-types based on HZ distance + mass
3. Assign atmosphere params from sub-type (spec section 5.4)
4. Generate rings for gas/ice giants (~30% / ~15% chance)
5. Generate moons per planet (spec section 4.2)
6. Compute surface temperature: `278.0 * star.luminosity.powf(0.25) / orbital_radius_au.sqrt()`
7. Set initial_phase from seed
8. Set rotation_period_hours from seed (10–40 hours range)
9. Set axial_tilt_deg from seed (0–30 degrees)
10. Set color_seed from seed

All values deterministic from the planet's seed.

- [ ] **Step 4: Export new types from lib.rs**

Add to `crates/sa_universe/src/lib.rs`:
```rust
pub use system::{Planet, PlanetType, PlanetSubType, PlanetarySystem, AtmosphereParams, RingParams, Moon, generate_system};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p sa_universe`
Expected: all old + new tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/sa_universe/src/system.rs crates/sa_universe/src/lib.rs
git commit -m "feat(universe): planet sub-types, moons, atmospheres, rings, star-type counts"
```

---

### Task 2: Icosphere mesh generation with LOD

**Files:**
- Create: `crates/sa_render/src/icosphere.rs`
- Modify: `crates/sa_render/src/lib.rs`

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icosphere_lod0_has_20_faces() {
        let mesh = generate_icosphere(0);
        assert_eq!(mesh.indices.len() / 3, 20, "LOD 0 icosphere should have 20 triangles");
    }

    #[test]
    fn icosphere_lod1_has_80_faces() {
        let mesh = generate_icosphere(1);
        assert_eq!(mesh.indices.len() / 3, 80);
    }

    #[test]
    fn icosphere_lod4_has_5120_faces() {
        let mesh = generate_icosphere(4);
        assert_eq!(mesh.indices.len() / 3, 5120);
    }

    #[test]
    fn icosphere_vertices_on_unit_sphere() {
        let mesh = generate_icosphere(2);
        for v in &mesh.positions {
            let len = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "vertex should be on unit sphere, len={len}");
        }
    }

    #[test]
    fn icosphere_no_degenerate_triangles() {
        let mesh = generate_icosphere(3);
        for tri in mesh.indices.chunks_exact(3) {
            let a = glam::Vec3::from(mesh.positions[tri[0] as usize]);
            let b = glam::Vec3::from(mesh.positions[tri[1] as usize]);
            let c = glam::Vec3::from(mesh.positions[tri[2] as usize]);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(area > 1e-8, "degenerate triangle found");
        }
    }
}
```

- [ ] **Step 2: Implement icosphere generation**

Create `crates/sa_render/src/icosphere.rs`:

```rust
//! Icosphere mesh generation with subdivision levels for LOD.
//!
//! LOD 0 = 20 faces (icosahedron), each subdivision multiplies by 4.
//! LOD 4 = 5,120 faces, LOD 5 = 20,480 faces.
//! All vertices lie on the unit sphere. Caller scales to planet radius.

use std::collections::HashMap;

/// Raw icosphere data (positions on unit sphere + triangle indices).
/// Not yet a renderable mesh — caller adds displacement, colors, normals.
pub struct IcosphereData {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

/// Generate a unit icosphere with the given subdivision level.
/// LOD 0 = 20 triangles (icosahedron base).
/// Each level multiplies face count by 4.
pub fn generate_icosphere(subdivisions: u32) -> IcosphereData {
    // Start with icosahedron vertices
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let mut positions: Vec<[f32; 3]> = vec![
        normalize([-1.0,  t,  0.0]),
        normalize([ 1.0,  t,  0.0]),
        normalize([-1.0, -t,  0.0]),
        normalize([ 1.0, -t,  0.0]),
        normalize([ 0.0, -1.0,  t]),
        normalize([ 0.0,  1.0,  t]),
        normalize([ 0.0, -1.0, -t]),
        normalize([ 0.0,  1.0, -t]),
        normalize([ t,  0.0, -1.0]),
        normalize([ t,  0.0,  1.0]),
        normalize([-t,  0.0, -1.0]),
        normalize([-t,  0.0,  1.0]),
    ];

    let mut indices: Vec<u32> = vec![
        0,11,5,  0,5,1,  0,1,7,  0,7,10, 0,10,11,
        1,5,9,   5,11,4, 11,10,2, 10,7,6, 7,1,8,
        3,9,4,   3,4,2,  3,2,6,  3,6,8,  3,8,9,
        4,9,5,   2,4,11, 6,2,10, 8,6,7,  9,8,1,
    ];

    // Subdivide
    let mut midpoint_cache = HashMap::new();
    for _ in 0..subdivisions {
        let mut new_indices = Vec::with_capacity(indices.len() * 4);
        midpoint_cache.clear();
        for tri in indices.chunks_exact(3) {
            let a = tri[0];
            let b = tri[1];
            let c = tri[2];
            let ab = get_midpoint(a, b, &mut positions, &mut midpoint_cache);
            let bc = get_midpoint(b, c, &mut positions, &mut midpoint_cache);
            let ca = get_midpoint(c, a, &mut positions, &mut midpoint_cache);
            new_indices.extend_from_slice(&[a, ab, ca]);
            new_indices.extend_from_slice(&[b, bc, ab]);
            new_indices.extend_from_slice(&[c, ca, bc]);
            new_indices.extend_from_slice(&[ab, bc, ca]);
        }
        indices = new_indices;
    }

    IcosphereData { positions, indices }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    [v[0]/len, v[1]/len, v[2]/len]
}

fn get_midpoint(
    a: u32, b: u32,
    positions: &mut Vec<[f32; 3]>,
    cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };
    if let Some(&idx) = cache.get(&key) {
        return idx;
    }
    let pa = positions[a as usize];
    let pb = positions[b as usize];
    let mid = normalize([
        (pa[0] + pb[0]) / 2.0,
        (pa[1] + pb[1]) / 2.0,
        (pa[2] + pb[2]) / 2.0,
    ]);
    let idx = positions.len() as u32;
    positions.push(mid);
    cache.insert(key, idx);
    idx
}
```

- [ ] **Step 3: Register module and export**

Add to `crates/sa_render/src/lib.rs`:
```rust
pub mod icosphere;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p sa_render icosphere`
Expected: all 5 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/sa_render/src/icosphere.rs crates/sa_render/src/lib.rs
git commit -m "feat(render): icosphere generation with subdivision LOD"
```

---

### Task 3: Planet mesh builder (displacement + biome colors)

**Files:**
- Create: `crates/sa_render/src/planet_mesh.rs`
- Modify: `crates/sa_render/src/lib.rs`

This task converts an `IcosphereData` + planet parameters into a renderable `MeshData` with noise displacement, biome vertex colors, and flat-shaded face normals.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rocky_planet_mesh_not_empty() {
        let mesh = build_rocky_planet_mesh(3, 6_371_000.0, PlanetSubType::Temperate, 42);
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn gas_giant_mesh_no_displacement() {
        let mesh = build_gas_giant_mesh(3, 69_911_000.0, PlanetSubType::ColdGiant, 42);
        // All vertices should be at the planet radius (no terrain displacement)
        for v in &mesh.vertices {
            let r = (v.position[0]*v.position[0] + v.position[1]*v.position[1] + v.position[2]*v.position[2]).sqrt();
            assert!((r - 69_911_000.0).abs() < 1000.0, "gas giant vertex should be at planet radius, r={r}");
        }
    }

    #[test]
    fn mesh_has_flat_normals() {
        let mesh = build_rocky_planet_mesh(2, 1_000_000.0, PlanetSubType::Barren, 77);
        // Each triangle's 3 vertices should share the same normal (flat shading)
        for tri in mesh.indices.chunks_exact(3) {
            let na = mesh.vertices[tri[0] as usize].normal;
            let nb = mesh.vertices[tri[1] as usize].normal;
            let nc = mesh.vertices[tri[2] as usize].normal;
            assert!((na[0]-nb[0]).abs() < 1e-4 && (na[1]-nb[1]).abs() < 1e-4);
            assert!((na[0]-nc[0]).abs() < 1e-4 && (na[1]-nc[1]).abs() < 1e-4);
        }
    }

    #[test]
    fn different_seeds_produce_different_terrain() {
        let a = build_rocky_planet_mesh(2, 1_000_000.0, PlanetSubType::Barren, 42);
        let b = build_rocky_planet_mesh(2, 1_000_000.0, PlanetSubType::Barren, 99);
        // At least some vertices should differ
        let differs = a.vertices.iter().zip(b.vertices.iter())
            .any(|(va, vb)| (va.position[0] - vb.position[0]).abs() > 0.1);
        assert!(differs, "Different seeds should produce different terrain");
    }
}
```

- [ ] **Step 2: Implement planet mesh builder**

Create `crates/sa_render/src/planet_mesh.rs`:

The module needs:
- `build_rocky_planet_mesh(subdivisions, radius_m, sub_type, seed) -> MeshData`
  - Generate icosphere at given subdivision
  - Displace vertices along radial direction using fBm noise (3-5 octaves of simplex noise sampled at 3D position on unit sphere)
  - Height range: ±2% of radius for gentle terrain, ±5% for mountainous
  - Assign vertex colors from biome lookup based on (height, latitude, sub_type)
  - Compute flat face normals per triangle
  - Return MeshData ready for GPU upload

- `build_gas_giant_mesh(subdivisions, radius_m, sub_type, seed) -> MeshData`
  - Generate icosphere, scale to radius, NO displacement
  - Color from latitude-based band function (8-16 bands)
  - Band edges perturbed by noise for swirling boundaries
  - Flat face normals

- `build_star_mesh(subdivisions, radius_m, temperature_k, seed) -> MeshData`
  - Generate icosphere, scale to radius, NO displacement
  - Emissive colors: Voronoi cellular pattern, bright centers, darker edges
  - Use star blackbody color as base

Use the `noise` crate for simplex noise. Add `noise = "0.9"` to `sa_render/Cargo.toml`.

For biome colors, define palettes per `PlanetSubType`:
```rust
fn biome_color(sub_type: PlanetSubType, height_normalized: f32, latitude: f32, seed: u64) -> [f32; 3] {
    match sub_type {
        PlanetSubType::Barren => mix_grey(height_normalized),
        PlanetSubType::Desert => mix_desert(height_normalized, latitude),
        PlanetSubType::Temperate => mix_temperate(height_normalized, latitude, seed),
        PlanetSubType::Ocean => mix_ocean(height_normalized),
        PlanetSubType::Frozen => mix_frozen(height_normalized, latitude),
        PlanetSubType::Molten => mix_molten(height_normalized),
        _ => [0.5, 0.5, 0.5], // fallback
    }
}
```

- [ ] **Step 3: Export from lib.rs**

Add: `pub mod planet_mesh;`

- [ ] **Step 4: Run tests**

Run: `cargo test -p sa_render planet_mesh`
Expected: all 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/sa_render/src/planet_mesh.rs crates/sa_render/src/lib.rs crates/sa_render/Cargo.toml
git commit -m "feat(render): planet mesh builder with noise terrain + biome colors"
```

---

### Task 4: Active solar system management

**Files:**
- Create: `crates/spaceaway/src/solar_system.rs`
- Modify: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Create the solar system manager**

```rust
//! Active solar system management.
//!
//! Tracks which star system the player is in, manages planet 3D meshes,
//! computes orbital positions, generates DrawCommands for rendering.

use sa_math::WorldPos;
use sa_render::{MeshData, MeshMarker, MeshStore};
use sa_universe::{ObjectId, Star, PlanetarySystem, generate_system, generate_star};
use sa_universe::sector::{SectorCoord, PlacedStar, generate_sector};
use sa_universe::seed::{MasterSeed, sector_hash};
use sa_core::Handle;
use glam::{Mat4, Vec3};

/// Conversion: 1 AU in meters.
const AU_TO_METERS: f64 = 1.496e11;
/// Time acceleration factor (30x real time).
const TIME_SCALE: f64 = 30.0;
/// System entry radius in AU (auto-drop distance from star).
const SYSTEM_RADIUS_AU: f64 = 50.0;

/// A celestial body currently loaded as a 3D mesh.
struct LoadedBody {
    mesh_handle: Handle<MeshMarker>,
    orbital_radius_au: f64,
    orbital_period_years: f64,
    initial_phase: f64,
    radius_m: f64,
    body_type: BodyType,
}

enum BodyType {
    Star,
    Planet { index: usize },
    Moon { planet_index: usize, moon_index: usize },
}

/// Manages the currently active solar system.
pub struct ActiveSystem {
    /// The star this system belongs to.
    pub star: Star,
    pub star_id: ObjectId,
    /// The generated planetary system.
    pub system: PlanetarySystem,
    /// Star's galactic position in light-years.
    pub star_galactic_pos: WorldPos,
    /// Loaded 3D bodies.
    bodies: Vec<LoadedBody>,
    /// Game time accumulator for orbital computation.
    game_time_seconds: f64,
    /// Catalog name.
    pub catalog_name: String,
}
```

Methods:
- `load(star_id, star, star_pos, system, mesh_store, device) -> Self` — generate meshes for star + planets + moons at appropriate LOD, upload to MeshStore
- `update(dt, galactic_pos) -> Vec<DrawCommand>` — advance game_time, compute orbital positions, compute angular sizes for LOD switching, return DrawCommands with model matrices positioned relative to the star
- `unload(mesh_store)` — release mesh handles

- [ ] **Step 2: Integrate into main.rs**

Add `active_system: Option<solar_system::ActiveSystem>` to App struct. Each frame:
1. If in a system: update and collect DrawCommands, append to the render command list
2. If not in a system: check if we've arrived near a star (gravity well detection triggers system load)

The orbital position computation:
```rust
// For each body:
let theta = body.initial_phase + (2.0 * PI * game_time * TIME_SCALE) / (body.orbital_period_years * 365.25 * 24.0 * 3600.0);
let x = body.orbital_radius_au * AU_TO_METERS * theta.cos();
let z = body.orbital_radius_au * AU_TO_METERS * theta.sin();
// Position relative to star, then offset by star's galactic position for rendering
```

- [ ] **Step 3: Build and verify compilation**

Run: `cargo build`

- [ ] **Step 4: Commit**

```bash
git add crates/spaceaway/src/solar_system.rs crates/spaceaway/src/main.rs
git commit -m "feat: active solar system manager with orbital positions"
```

---

### Task 5: Navigation — window markers + lock-on + gravity well

**Files:**
- Create: `crates/spaceaway/src/navigation.rs`
- Modify: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Create navigation module**

```rust
//! Navigation: nearby star markers, lock-on targeting, gravity well detection.

use sa_math::WorldPos;
use sa_universe::{ObjectId, Universe, VisibleStar};
use sa_universe::sector::{SectorCoord, generate_sector};
use sa_universe::seed::MasterSeed;

/// A nearby star visible for navigation.
pub struct NavStar {
    pub id: ObjectId,
    pub galactic_pos: WorldPos,
    pub catalog_name: String,
    pub distance_ly: f64,
    pub color: [f32; 3],
    pub spectral_class: sa_universe::SpectralClass,
}

/// Navigation state.
pub struct Navigation {
    /// Nearby stars for window markers (updated periodically).
    pub nearby_stars: Vec<NavStar>,
    /// Currently locked target (if any).
    pub locked_target: Option<NavStar>,
    /// Bookmarked systems.
    pub bookmarks: Vec<Bookmark>,
    seed: MasterSeed,
}

pub struct Bookmark {
    pub star_id: ObjectId,
    pub galactic_pos: WorldPos,
    pub catalog_name: String,
    pub nickname: Option<String>,
}
```

Methods:
- `update_nearby(galactic_pos, universe)` — query stars within 50 ly, compute distances, build NavStar list sorted by distance, limit to 15
- `lock_target(star_index)` / `clear_target()` — set/clear lock-on
- `check_gravity_well(galactic_pos) -> Option<NavStar>` — during warp, check if within 50 AU of any nearby star. Returns the star to drop at.
- `catalog_name(id: ObjectId) -> String` — format `"SEC {x:04}.{z:04} / S-{sys:03}"`
- `add_bookmark(star)` / `remove_bookmark(index)`

- [ ] **Step 2: Wire into main.rs**

Add `navigation: navigation::Navigation` to App struct.

In the game loop:
1. Update nearby stars every 30 frames (not every frame — it queries sectors)
2. During warp: call `check_gravity_well()`. If hit, auto-drop warp + load system
3. Render window markers as HUD overlay text (using egui or the existing debug overlay approach)

For lock-on: when seated at helm, if cursor is ungrabbed and player clicks near a star marker, lock on to that star. Show bracket marker + distance/ETA on helm monitor.

- [ ] **Step 3: Update helm screen with system info and target**

In `helm_screen.rs`, add:
- When in a system: show system overview (star class, planet count, body list)
- When locked on target: show target name, distance, ETA

- [ ] **Step 4: Build and test**

Run: `cargo build && cargo test --workspace`

- [ ] **Step 5: Commit**

```bash
git add crates/spaceaway/src/navigation.rs crates/spaceaway/src/main.rs crates/spaceaway/src/ui/helm_screen.rs
git commit -m "feat: navigation — window markers, lock-on, gravity well auto-drop"
```

---

### Task 6: Atmosphere shell mesh

**Files:**
- Modify: `crates/sa_render/src/planet_mesh.rs`
- Modify: `crates/spaceaway/src/solar_system.rs`

- [ ] **Step 1: Add atmosphere mesh builder**

In `planet_mesh.rs`, add:

```rust
/// Build an atmosphere shell mesh (slightly larger than planet).
/// Uses back-face rendering with Fresnel rim alpha.
/// Returns MeshData with vertex colors = atmosphere color and
/// vertex alpha encoded in the color's brightness.
pub fn build_atmosphere_mesh(
    subdivisions: u32,
    planet_radius_m: f32,
    atmosphere: &AtmosphereParams,
) -> MeshData {
    // Generate icosphere at atmosphere_radius = planet_radius * 1.03
    // All vertices get the atmosphere color
    // Normal points outward (for Fresnel calculation in shader)
    // This mesh will be rendered with alpha blending, back-faces only
}
```

The atmosphere rendering requires a separate blend pipeline (alpha blending, cull front faces so only back-faces show). This may need a new pipeline in the renderer, or use the existing nebula pipeline (which already does alpha blending).

- [ ] **Step 2: Load atmosphere meshes in ActiveSystem**

For each planet with `atmosphere.is_some()`, generate and upload an atmosphere mesh. Include in DrawCommands with a flag indicating it needs alpha-blend rendering.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat: atmosphere shell mesh with Fresnel rim glow"
```

---

### Task 7: Ring system mesh

**Files:**
- Modify: `crates/sa_render/src/planet_mesh.rs`
- Modify: `crates/spaceaway/src/solar_system.rs`

- [ ] **Step 1: Add ring mesh builder**

```rust
/// Build a ring system mesh (flat annular disc).
pub fn build_ring_mesh(
    planet_radius_m: f32,
    ring_params: &RingParams,
    axial_tilt_deg: f32,
    seed: u64,
) -> MeshData {
    // Annular disc with 64 radial segments, 12 concentric rings
    // Vertex colors from radial distance function (dense = bright, gaps = dark)
    // Tilted by axial_tilt around X axis
    // Alpha-blended rendering
}
```

- [ ] **Step 2: Load ring meshes in ActiveSystem**

For planets with `has_rings`, generate ring mesh and include in DrawCommands.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat: ring system mesh with gaps and radial color variation"
```

---

### Task 8: Star corona billboard

**Files:**
- Modify: `crates/sa_render/src/planet_mesh.rs` or new `star_corona.rs`
- Modify: `crates/spaceaway/src/solar_system.rs`

- [ ] **Step 1: Add corona billboard**

The star's corona is a large additive-blended billboard behind the star mesh:
- Billboard quad, 3–5x star radius
- Radial falloff: `pow(1 - dist, 2.5)` in fragment shader
- Uses the star's blackbody color

This could use the existing nebula billboard pipeline (similar concept — camera-facing quad with alpha/additive blending).

- [ ] **Step 2: Include corona in active system rendering**

When star is close enough to render as 3D, also render the corona billboard.

- [ ] **Step 3: Commit**

```bash
git commit -m "feat: star corona billboard with radial glow"
```

---

### Task 9: Integration testing + visual polish

- [ ] **Step 1: Full test suite**

Run: `cargo test --workspace`

- [ ] **Step 2: Clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3: Manual test plan**

1. Run the game
2. Sit at helm, start engine
3. Look out the window — should see nearby star markers with catalog names and distances
4. Click a star marker to lock on — bracket appears, helm shows distance/ETA
5. Engage warp (press 3) toward locked target
6. When gravity well detected: auto-drop, "SYSTEM ENTERED" on helm
7. See the star as a bright sphere with corona glow
8. See planets at their orbital positions (may need to cruise closer)
9. Gas giants should have colored bands
10. Ringed planets should show ring disc
11. Planets with atmospheres should have blue/orange rim glow
12. Moons should orbit their planets
13. Engage warp again to visit another star

- [ ] **Step 4: Commit and push**

```bash
git add -A
git commit -m "feat: Sub-project 1 complete — solar systems visible from space"
git push
```

---

## Summary

| Task | What it builds | Key files |
|------|---------------|-----------|
| 1 | Extended planet generation (sub-types, moons, rings, atmospheres) | `sa_universe/system.rs` |
| 2 | Icosphere mesh generation with subdivision LOD | `sa_render/icosphere.rs` |
| 3 | Planet mesh builder (noise terrain, biome colors, gas giant bands) | `sa_render/planet_mesh.rs` |
| 4 | Active solar system manager (load, orbital positions, draw commands) | `spaceaway/solar_system.rs` |
| 5 | Navigation (window markers, lock-on, gravity well, bookmarks) | `spaceaway/navigation.rs` |
| 6 | Atmosphere shell mesh (Fresnel rim glow) | `sa_render/planet_mesh.rs` |
| 7 | Ring system mesh | `sa_render/planet_mesh.rs` |
| 8 | Star corona billboard | `sa_render/planet_mesh.rs` |
| 9 | Integration testing + visual polish | All files |

**Tasks 1–3 are pure library code** (no game loop changes). Fully testable in isolation.
**Tasks 4–5 are integration** (wire into game loop). Require judgment.
**Tasks 6–8 are rendering additions** (atmosphere, rings, corona). Build on Task 3.
**Task 9 is verification.**
