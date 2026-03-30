# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

SpaceAway is a cooperative first-person space exploration game with a custom Rust engine. 1-4 players crew a ship exploring a procedurally generated infinite universe. The engine is designed for AI-agent-driven development.

See `docs/completed/specs/2026-03-27-spaceaway-engine-design.md` for the full design spec.

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
Application:  spaceaway          — game binary, main loop, winit event handling

Game Logic:   sa_universe        — procedural generation, galaxy, star systems, nebulae
              sa_player          — first-person controller, interaction, inventory
              sa_meshgen         — modular ship mesh generation (hulls, parts, assembly)

Engine:       sa_render          — wgpu renderer, flat-shaded low-poly pipeline, terrain pipeline, slab allocator, shaders
              sa_terrain         — CDLOD quadtree, config, collision grid, chunk streaming
              sa_physics         — Newtonian simulation, rapier3d integration, collision
              sa_input           — input mapping, keyboard/mouse state

Core:         sa_ecs             — hecs wrapper, system scheduling, Schedule
              sa_math            — glam wrapper, double-precision WorldPos/LocalPos, unit types
              sa_core            — shared types, event bus, resource handles, FrameTime
```

All crates live in `crates/`. The `sa_` prefix is mandatory for all crate names.

Planned crates (not yet created): `sa_net`, `sa_audio`, `sa_ship`, `sa_survival`.

### Terrain System (sa_terrain + sa_render)

CDLOD terrain with CPU mesh generation. See `docs/superpowers/specs/2026-03-30-terrain-redesign.md` for the full design spec.

- **Config** (`sa_terrain/config.rs`): centralized terrain constants (grid size, LOD levels, budgets)
- **Collision grid** (`sa_terrain/collision_grid.rs`): fixed-LOD 7x7 collision grid (pure math, no rapier). Never changes LOD under resting bodies to prevent energy injection.
- **Terrain vertex** (`sa_render/terrain_vertex.rs`): GPU TerrainVertex with morph_target field (48 bytes)
- **Slab allocator** (`sa_render/slab_allocator.rs`): budget-driven vertex buffer pool — heightmap tier (30MB, fixed 58KB slots) + volumetric tier (20MB, reserved for caves)
- **Terrain pipeline** (`sa_render/terrain_pipeline.rs`): separate render pipeline for terrain (TerrainVertex + TerrainInstanceRaw with morph_factor), does not pollute the shared geometry pipeline
- **Terrain shader** (`sa_render/shaders/terrain.wgsl`): CDLOD vertex morphing — odd vertices lerp toward parent-LOD position via morph_factor
- **Shared index buffer**: all heightmap chunks use one static index buffer (same 33x33 topology)
- **Icosphere coexistence**: depth-test icosphere at 0.999x planet radius, terrain occludes it naturally

CPU mesh generation was chosen over GPU displacement to support both heightmap AND future volumetric terrain (caves/overhangs). Volumetric extension points are in place: `ChunkType` enum, volumetric slab tier, TriMesh collider path.

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

## Galaxy / Universe System

`sa_universe` implements the full procedural universe:

- **Galaxy structure** (`galaxy.rs`): density functions, dust lanes, nebulae placement, distant galaxy generation. Galaxy is an 8-layer octree of sectors; density follows a disc + bulge model.
- **Sectors** (`sector.rs`): `SectorCoord` + `SECTOR_SIZE_LY`. Stars are placed inside sectors via Poisson-disk sampling (blue noise).
- **Star generation** (`star.rs`): H-R diagram sampling — mass → temperature → luminosity → color → radius → spectral class. Physically correct properties.
- **Planetary systems** (`system.rs`): simplified formation simulation — rocky inner, gas giants at frost line, ice bodies outer.
- **Object IDs** (`object_id.rs`): every object has a packed `u64` (sector XYZ 16 bits each, layer 3 bits, system index 8 bits, body index 5 bits). Deterministic and reconstructable from coordinates.
- **Seeding** (`seed.rs`): xxHash64-based coordinate hashing — O(1) access, perfectly deterministic, no sequential RNG.
- **Universe query** (`query.rs`): `Universe` type + `VisibleStar` for real-time star field queries.

All visible stars are real objects at their true f64 positions. The "skybox" is a live projection of the universe database with parallax.

## Mesh Generation (sa_meshgen)

All ship mesh generation lives in `sa_meshgen`. See `docs/modular-ship-standards.md` for the full engineering spec and `docs/interior-standards.md` for interior details.

### Key conventions

- All parts use a **hexagonal cross-section** (6-sided), height always 3.0m.
- Axes: +X = starboard, -X = port, +Y = dorsal, -Z = fore/bow, +Z = aft/stern.
- Part origin is at the **center of the fore hex face** (x=0, y=0, z=0).
- Hull spans z=0 (fore) to z=length (aft) in local coordinates.
- Standard widths: `STD_WIDTH=4.0`, `ROOM_WIDTH=5.0`.
- Width changes between sections are handled **exclusively by standalone `hull_transition()` pieces** — no part may embed transitions for its neighbors (Rule 4.4).
- Connection points carry `width` and `height` metadata for validation.
- Every fore/aft connection pair must have identical hex ring vertices at the shared face (within ε=1e-4m).

### Two-Sided Rendering Rules (R-TS1 through R-TS8)

Backface culling is **disabled** (`cull_mode: None`). The fragment shader uses `@builtin(front_facing)` to flip normals for back-faces.

| Rule  | Summary |
|-------|---------|
| R-TS1 | Backface culling DISABLED — all triangles visible from both sides |
| R-TS2 | Shader flips normals on back-faces via `front_facing` builtin |
| R-TS3 | Produce normals pointing toward the PRIMARY viewing direction |
| R-TS4 | Single-sided panels (floors, ceilings, bulkheads) are fine — R-TS2 handles lighting |
| R-TS5 | Hull panels MUST be double-sided (exterior + 0.05m inset interior) because exterior/interior have different colors |
| R-TS6 | Same-color surfaces: ONE face only — do NOT duplicate with flipped normals |
| R-TS7 | Unavoidably overlapping faces need a 0.02–0.05m offset to prevent Z-fighting |
| R-TS8 | Ambient term prevents fully-black back-faces even before the front_facing fix |

See `docs/modular-ship-standards.md` Section 8 and `docs/interior-standards.md` Section 8 for full rationale.

## Key Bindings (current game)

### General (standing, not seated)

| Key | Action |
|-----|--------|
| `0` | Return to normal scene view |
| `1-5` | Teleport to galaxy viewpoints (mid-disc, above, edge, center, nebula) |
| `6` | Cycle through individual ship parts (mesh inspection) |
| `7` | Load full assembled ship mesh |
| `8` | Debug: jump to nearest star system (teleport next to first planet) |
| `Tab` | Lock nearest star for navigation |
| `F` | Toggle fly mode (free-camera, galaxy-scale movement) |
| `V` | Toggle VSync (benchmark mode — uncapped FPS) |
| `+` / `=` | Double fly speed |
| `-` | Halve fly speed (min 1 ly/s) |
| `W/A/S/D` | Move in fly mode |
| `Space` | Move up in fly mode |
| `Shift` | Move down in fly mode |
| `Escape` | Release cursor / quit |

### Seated at Helm

| Key | Action |
|-----|--------|
| `W/S` | Pitch (nose up/down) |
| `A/D` | Yaw (turn left/right) |
| `Q/E` | Roll (left/right) |
| `F` | Stand up (exit seat) |
| `1` | Select Impulse drive |
| `2` | Select Cruise drive (1c–5,000c) |
| `3` | Select Warp drive (100,000c–5,000,000c, 5s spool) |
| `Tab` | Lock nearest star for navigation |

Throttle and engine controlled by clicking cockpit lever/button.

### Drive System

Three tiers of travel with increasing speed and fuel cost:

- **Impulse** (default): Newtonian physics, 0–1000 m/s. Uses hydrogen fuel.
- **Cruise** (key 2): 1c–5,000c, for planet-to-planet within a system. Uses hydrogen at 10x rate.
- **Warp** (key 3): 100,000c–5,000,000c, for star-to-star travel. Uses exotic fuel. 5-second spool time.

Throttle lever controls speed within each tier (logarithmic mapping).

### Rendering: Reversed-Z Infinite Depth Buffer

The renderer uses reversed-Z with infinite far plane for all depth testing:
- Near plane: 0.1m, far plane: infinity
- Depth clear: 0.0 (infinity), depth compare: GreaterEqual
- Provides sub-millimeter precision near camera AND renders planets at millions of km
- All new pipelines MUST use `CompareFunction::GreaterEqual` and clear depth to `0.0`
- Sky/background shaders output depth near 0.0 (far), not near 1.0

Fly mode bypasses physics and moves the camera directly in light-years per second.

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

## Standards Docs

- `docs/modular-ship-standards.md` — complete hex hull construction rules, connection point spec, validation functions, two-sided rendering rules
- `docs/interior-standards.md` — interior dimensions, bulkhead system, color palette, validation rules
- `docs/ship-design-guide.md` — ship layout reference (bow→stern sections), color palette, part assembly guide
- `docs/performance-techniques.md` — all performance optimizations with rationale, files, and benchmarks
- `docs/collision-system-standards.md` — 3-tier collision system, collision groups

## Design Specs (in docs/superpowers/specs/)

- `2026-03-28-travel-system-design.md` — three-tier drive system (Impulse/Cruise/Warp), fuel economy, visual effects
- `2026-03-28-solar-system-from-space-design.md` — procedural solar systems, planet rendering, navigation, 1:1 scale
- `2026-03-28-main-menu-design.md` — main menu with random celestial background scenes
- `2026-03-28-visor-hud-design.md` — suit visor HUD with Orbitron font, degradation system
- `2026-03-28-audio-system-design.md` — 5-channel spatial audio, computer voice, context music
- `2026-03-30-terrain-redesign.md` — CDLOD terrain redesign: CPU meshgen, slab allocator, collision grid, volumetric extension points

## Upcoming Features (prioritized)

1. **CDLOD terrain — landing + surface walking** — terrain rendering and collision grid complete; next: ship landing sequence, player walkable surfaces (`sa_terrain` crate)
2. **Resource gathering** — asteroid mining, gas giant scooping, exotic matter deposits
3. **Navigation console** — full 3D star map, route planning, ship database with bookmarks
4. **Alpha-blend render pass** — proper transparent atmosphere shells and planetary rings
5. **FBX/GLTF asset import** — load purchased 3D models (buttons, levers) via `gltf` crate
6. **Save/load system** — persist universe state, ship position, discoveries

## Planned Library Adoptions

Libraries evaluated for future integration, in priority order:

| Library | Crate | Purpose | When to adopt |
|---------|-------|---------|---------------|
| **fastnoise-lite** | `fastnoise-lite` | SIMD noise (5-10x faster than `noise` crate) | Before CDLOD terrain — noise is the bottleneck |
| **gltf** | `gltf` | Load 3D models (GLTF/GLB format) | When integrating purchased mesh assets |
| **kira** | `kira` | Advanced audio engine (replaces rodio) | When adding reverb, filters, advanced spatial audio |
| **tracy-client** | `tracy-client` | Frame profiler with timeline visualizer | When optimizing terrain/rendering performance |
| **rkyv** or **bincode** | `rkyv` / `bincode` | Fast serialization for save/load | When building save system |
| **wgpu-profiler** | `wgpu-profiler` | GPU render pass timing | When optimizing GPU-heavy features |
| **quinn** | `quinn` | QUIC networking for P2P coop | Phase 6 (multiplayer) |
| **naga_oil** | `naga_oil` | WGSL shader includes/composition | When shaders get large and duplicate code |
