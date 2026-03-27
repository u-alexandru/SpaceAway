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
Application:  spaceaway          — game binary, main loop, winit event handling

Game Logic:   sa_universe        — procedural generation, galaxy, star systems, nebulae
              sa_player          — first-person controller, interaction, inventory
              sa_meshgen         — modular ship mesh generation (hulls, parts, assembly)

Engine:       sa_render          — wgpu renderer, flat-shaded low-poly pipeline, shaders
              sa_physics         — Newtonian simulation, rapier3d integration, collision
              sa_input           — input mapping, keyboard/mouse state

Core:         sa_ecs             — hecs wrapper, system scheduling, Schedule
              sa_math            — glam wrapper, double-precision WorldPos/LocalPos, unit types
              sa_core            — shared types, event bus, resource handles, FrameTime
```

All crates live in `crates/`. The `sa_` prefix is mandatory for all crate names.

Planned crates (not yet created): `sa_net`, `sa_audio`, `sa_ship`, `sa_survival`.

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

| Key | Action |
|-----|--------|
| `0` | Return to normal scene view |
| `1` | Teleport — mid galactic disc |
| `2` | Teleport — above galaxy |
| `3` | Teleport — galaxy edge |
| `4` | Teleport — near galactic center |
| `5` | Teleport — near a nebula |
| `6` | Cycle through individual ship parts (mesh inspection) |
| `7` | Load full assembled ship mesh |
| `F` | Toggle fly mode (free-camera, galaxy-scale movement) |
| `+` / `=` | Double fly speed |
| `-` | Halve fly speed (min 1 ly/s) |
| `W/A/S/D` | Move in fly mode |
| `Space` | Move up in fly mode |
| `Shift` | Move down in fly mode |
| `Escape` | Quit |

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
