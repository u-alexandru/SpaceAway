# SpaceAway — Game Engine Design Spec

## Overview

SpaceAway is a cooperative first-person space exploration game with a custom engine built in Rust. 1-4 players crew a single ship, exploring a procedurally generated infinite universe. The game emphasizes mystery, wonder, and the existential awe of deep space. The project doubles as an experiment in AI-agent-driven development — architecture decisions optimize for Claude Code productivity.

## Core Pillars

- **Mystery & wonder** — the universe rewards curiosity but never demands it
- **Cooperative tension** — the ship requires teamwork, solo play requires multitasking
- **Seamless infinity** — no loading screens, no boundaries, no transitions
- **The void as a character** — deep space feels vast, lonely, beautiful, and dangerous
- **AI-agent-first development** — every architectural choice considers Claude Code's ability to work independently

## Tech Stack

- **Language:** Rust
- **Graphics:** wgpu (Vulkan/Metal/DX12)
- **Math:** glam (with double-precision wrapper types)
- **Physics:** rapier3d
- **Windowing:** winit
- **ECS:** hecs
- **Serialization:** serde + RON (config/data), bincode (network)
- **Networking:** quinn (QUIC — provides both reliable and unreliable channels)
- **Audio:** kira (spatial audio, streaming, game-oriented)
- **Error handling:** thiserror per crate
- **CI:** GitHub Actions

## Target Platforms

- **Primary:** macOS (development and testing)
- **Secondary:** Windows (cross-compile checked in CI, manual testing on major versions)
- **Tertiary:** Linux (if it works, great — wgpu makes this likely)

## Architecture

### Crate Workspace

The project is a Cargo workspace with independent crates organized into four layers. Each layer only depends on layers below it. No upward or lateral cross-crate dependencies. Cross-crate communication happens through a typed event bus.

```
Application Layer
  spaceaway          — game binary, main loop, game states

Game Logic Layer
  sa_ship             — ship systems, stations, subsystems, damage
  sa_survival         — power, food, oxygen, energy management
  sa_universe         — procedural generation, chunk streaming, anomalies
  sa_player           — first-person controller, interaction, inventory

Engine Layer
  sa_render           — wgpu renderer, shaders, low-poly pipeline
  sa_physics          — Newtonian simulation, rapier integration, collision
  sa_net              — P2P networking, host/client, state sync
  sa_audio            — spatial audio, ambience, sound propagation
  sa_input            — input mapping, keybinds, gamepad

Core Layer
  sa_ecs              — hecs wrapper, system scheduling
  sa_math             — glam wrapper, double-precision coordinates, unit types
  sa_core             — shared types, event bus, resource handles, time
```

All crates use the `sa_` prefix to avoid name collisions and clarify imports.

### ECS (Entity Component System)

Based on hecs — lightweight, no macros, no proc-macro compilation overhead. Components are plain structs. Systems are plain functions that query the ECS world. This pattern is ideal for AI agents: each system is a small, testable function with explicit inputs and outputs.

### Event Bus

Crates communicate by emitting and consuming strongly-typed events through `sa_core::events`. Example: `sa_ship` emits `PowerLevelChanged { system: EngineId, watts: Watts }`, and `sa_render` consumes it to update lighting. No crate calls another crate's functions directly.

### Strong Typing

Bare numeric types are avoided. Instead: `Meters(f64)`, `Watts(f32)`, `Kilograms(f32)`, `Seconds(f64)`, etc. The compiler catches unit mismatches. This is critical for AI-generated code — a wrong unit is a compile error, not a runtime bug.

## Procedural Universe

### Spatial Hierarchy — Octree Sectors

The universe is divided into an **8-layer octree of sectors** (inspired by Elite: Dangerous's Stellar Forge). Each layer provides a different level of detail:

- **Layer 0** (largest, ~1000 ly³) — galaxy-scale density and age distribution. Determines how many stars exist in a region and their approximate metallicity.
- **Layers 1-3** — intermediate sectors. Used for far-distance star field rendering (stars as points scaled by luminosity).
- **Layers 4-5** — individual star systems visible as distinct objects. Generated on approach.
- **Layers 6-7** (smallest, ~1 ly³) — full system detail: orbits, planets, moons, asteroid belts, anomalies. Physics active, geometry loaded, interaction enabled.

Only the layer needed for the current distance is generated. This gives natural LOD without loading screens.

### Packed u64 Object IDs

Every object in the universe is addressed by a **single 64-bit integer** encoding:
- Sector X, Y, Z coordinates (16 bits each = 48 bits)
- Octree layer (3 bits)
- System index within sector (8 bits)
- Body index within system (5 bits)

This gives every star, planet, moon, and anomaly a unique, deterministic, reconstructable 8-byte address. Hash the ID to get the object's generation seed. Client and server always agree on identity. Perfect for networking — send object IDs, not data.

### What's in the sky is real

Every visible point of light is an actual object in the procedural universe at its true f64 position. Stars, pulsars, black holes, rogue planets, nebulae — all reachable. The "skybox" is a real-time projection of the universe database, with parallax as you travel. Fly toward any point of light and you will arrive at whatever it actually is.

### Coordinate Hashing (not Sequential RNG)

Generation uses **hash functions (xxHash64)** instead of sequential PRNGs. `hash(master_seed, sector_x, sector_y, sector_z)` → sector seed. Benefits:
- O(1) access — generate any sector instantly without generating all previous ones
- Perfectly deterministic from coordinates
- No correlation between adjacent sectors (hash decorrelates)
- Parallelizable — generate multiple sectors on different threads simultaneously

Player modifications (mined asteroids, placed structures, discovered anomalies) are stored as diffs against the seed.

### H-R Diagram Star Generation

Stars are generated by **sampling the Hertzsprung-Russell diagram** (inspired by SpaceEngine). Given a seeded random value:

1. **Sample mass** from the Initial Mass Function (IMF) — most stars are small red dwarfs, massive blue giants are rare
2. **Derive temperature** from mass via main-sequence relationships
3. **Derive luminosity** from mass-luminosity relation (L ∝ M^3.5 for main sequence)
4. **Derive color** from temperature (black-body radiation → RGB)
5. **Derive radius** from luminosity and temperature (Stefan-Boltzmann)
6. **Classify** — main sequence, red giant, white dwarf, neutron star, black hole based on mass and age

This means every star has physically correct properties. Players learn real astronomy by exploring.

### Stellar Forge-Inspired Planetary Formation

Star systems are generated by **simplified simulation of planetary formation** (inspired by Elite: Dangerous):

1. Seed determines total available mass, metallicity, and angular momentum
2. Central body forms (star type from H-R generation above)
3. Remaining mass distributed into a protoplanetary disc
4. Disc accretes into bodies: rocky inner planets (high temperature zone), gas giants (frost line and beyond), ice bodies (outer system)
5. Moons captured or formed from disc remnants
6. Orbital parameters (semi-major axis, eccentricity, inclination) derived from formation physics

This produces systems that feel scientifically plausible — not random scatter.

### Star Distribution — Poisson Disk Sampling

Stars within sectors are placed using **Bridson's Poisson disk sampling** (blue noise) instead of uniform random. Stars maintain a minimum spacing while appearing natural. This prevents clustering artifacts and empty voids that pure random creates. The density gradient follows the sector's position in the galaxy (denser toward galactic center, sparser in the halo). Deterministic from the sector seed.

### Double-Precision Origin Rebasing

World coordinates use f64 (accurate to ~1mm at solar system distances). The rendering origin is periodically rebased to the camera position, converting to f32 camera-relative coordinates for the GPU. This eliminates floating-point jitter at large distances.

### LOD & Streaming

- Far sectors: point rendering (single pixel scaled by luminosity from H-R data)
- Medium distance: billboard sprites, simplified orbital data
- Near: full geometry, physics, interaction
- Background thread generates/unloads chunks based on distance
- No loading screens — seamless transitions via LOD blending

### Volumetric Nebulae (Future Phase)

Nebulae will be rendered as **raymarched volumetric SDFs** in a compute shader. Key techniques:
- Adaptive step sizes (large strides through empty space, small steps in dense regions)
- Early ray termination (stop when sufficient opacity accumulates)
- Temporal reprojection (reuse samples from previous frames for quality at lower cost)
- Procedural density from 3D noise seeded by nebula position

Target: 60fps on modern GPUs. Nebulae become real 3D volumes you fly through, not flat sprites.

### GPU Terrain Generation (Future Phase — Planet Landing)

When landable planets are added, terrain will be generated on the **GPU via wgpu compute shaders**:
- Cube-to-sphere projection with quadtree subdivision
- Fractional Brownian Motion + Hybrid Multifractal noise for heightmaps
- 5-10ms generation time per heightmap (vs 100ms+ on CPU)
- Seamless planet approach without loading screens

### Mystery Layer

Anomalies, derelicts, signals, and ruins are seeded into the universe at generation time with rarity tiers. Some are purely procedural, others are hand-crafted templates placed by the generator. Discovery is organic — no markers, no quest log. Unusual sensor readings, visual cues, or radio signals are the only hints.

## Ship Systems

### The Ship Is a Place

The ship interior and exterior space are the same continuous scene. Players walk around inside the ship in first person. Walking to an airlock, opening it, and EVA-ing into space is seamless — look back and see your ship. No loading screens, no level transitions. The ship is geometry in the world like everything else.

### Stations

Five physical stations inside the ship, each a location you walk to and interact with:

**Helm** — bow of the ship
- Direct ship rotation and thrust (Newtonian — no space brakes)
- Retrograde/prograde HUD markers
- Auto-hold maintains current thrust vector when unmanned
- Can request power levels from Engineering

**Navigation** — upper mid-ship
- Star charts, system maps
- Plot fuel-efficient routes
- Calculate intercept trajectories
- Mark points of interest
- Feeds waypoints to Helm's HUD

**Sensors** — lower mid-ship
- Long-range scanning
- Anomaly detection and analysis
- Identify resource deposits
- Detect signals and transmissions
- The discovery station — finds the mysteries

**Engineering** — aft mid-ship
- Power distribution across all systems
- Life support management (O2, temperature)
- Damage control and repair
- Resource monitoring (fuel, food, water)
- The survival station — keeps everyone alive

**Engine Room** — stern
- Fuel management, reactor controls
- Manual overrides when automation fails
- Physical access for hands-on repairs

### Cooperation Model

**Solo (1 player):** Run between stations. Tension from prioritization — can't steer and reroute power simultaneously. Auto-hold on helm enables brief absences.

**Duo (2 players):** Typically Helm+Navigation and Engineering+Sensors. Each player covers two adjacent stations.

**Full Crew (3-4 players):** Each player owns a station. 4th player can be a roving engineer/EVA specialist. Most efficient but requires voice coordination.

### Physics

Ship movement is fully Newtonian:
- Thrust applies force in the thrust direction. No drag, no speed limit.
- Rotation via reaction control thrusters.
- Momentum persists — must thrust retrograde to slow down.
- Mass changes with fuel consumption and cargo.
- Interior physics: players walk on decks using magnetic boots (simple, no rotating ship sections needed), transitioning to zero-g during EVA.

Reference frame transitions are seamless: ship interior gravity → airlock → zero-g EVA → planetary surface gravity.

## Survival Systems

Managed primarily from Engineering. Slow-burn tension, not frantic — resources deplete gradually, giving time to plan. Cascading failures create genuine emergencies.

**Power Grid**
- Reactor generates power, distributed across systems
- Engineering decides allocation tradeoffs
- Reactor consumes fuel — depletion causes cascading shutdowns

**Life Support**
- Oxygen generation and CO2 scrubbing (requires power)
- Temperature regulation (space is cold, reactors are hot)
- Food and water stores (consumed over time, replenished by scavenging)

**Ship Integrity**
- Hull damage from collisions, debris, thermal stress
- Breaches cause atmosphere loss in affected sections
- Repairs require materials and physically visiting the damage site

**Player Vitals**
- Oxygen (suit supply for EVA, ship supply when aboard)
- Hunger/thirst (long-term pressure, not frantic)
- Suit power (EVA, lights, tools)

## Networking

### P2P Host Model

- Host runs the authoritative simulation (physics, universe generation, resources)
- Clients send inputs, receive state updates
- Host is "truth" — clients predict locally and reconcile

### State Sync

- Ship state: reliable, ~10Hz (power levels, damage, systems)
- Player positions/actions: fast with client-side prediction, ~30Hz
- Physics objects: host-authoritative, interpolated on clients
- Universe chunks: host shares seeds, clients regenerate deterministically (minimal bandwidth)

### Session Flow

- Host starts game, provides session code or direct IP
- Clients join, spawn aboard the ship
- Host disconnect ends the game
- Save state on host's machine

### Scope

Intentionally simple. No relay servers, no NAT traversal initially. Players use direct connect or tools like Tailscale. Hole-punching can be added later.

## Rendering

### Visual Philosophy

Geometry is simple. Lighting does all the emotional work. The void is the dominant visual element.

### Style

- Flat-shaded low poly — visible triangles, no normal smoothing
- Vertex colors or flat material colors — no texture UVs for most geometry
- No normal maps, no PBR textures
- Extremely simple asset pipeline — meshes can be generated programmatically by AI agents

### Lighting

- Single harsh directional light from nearest star — deep shadows, bright faces
- Point lights inside ship (overhead, emergency red, station screens)
- Volumetric light shafts through windows and hull breaches
- Bioluminescent anomalies as rare color in the void
- Helmet lamp during EVA — tiny cone in infinite dark
- True black in deep space — no ambient light, no horizon
- Auto-exposure — eyes adjust looking at stars vs. into shadow

### Pipeline

Forward rendering with targeted post-processing:
1. Depth pre-pass (early Z)
2. Geometry pass (flat-shaded low poly)
3. Lighting pass (shadow maps + point lights)
4. Post-processing (bloom, film grain, auto-exposure, subtle chromatic aberration)
5. UI overlay (HUD, station interfaces)

### The Sky

Not a skybox. Real-time projection of the procedural universe. Every point is a real object at its true position. Parallax shifts as you travel. Stars rendered as points scaled by luminosity and distance.

## Audio

### Philosophy

Sound design reinforces the isolation and physicality of space.

**Exterior (EVA/space):** Near-silence. Only suit-transmitted sounds — breathing, heartbeat, servos, radio comms. Muffled thuds through physical contact (boots on hull). No sound in vacuum.

**Interior (ship):** The ship hums, creaks, groans. Engine vibration through the deck. Systems have distinct audio signatures — you learn to hear problems before alarms fire. Life support drones subtly.

**Transition:** Walking from interior through airlock to EVA, sound fades from full to muffled to near-silence. Seamless, no cut.

### Spatial Audio

- Full 3D positional audio inside the ship
- Sound travels through corridors, muffled through walls
- Radio comms between players have slight processing (crackle, compression)

### Music

Minimal. Ambient drones that emerge and fade based on context (approaching anomaly, deep void, danger). Not a soundtrack — the universe humming.

## AI Agent Development Conventions

### File & Module Rules

- Every Rust file stays under 300 lines. Split when it grows past.
- One concept per file. `thrust.rs` does thrust. `power_grid.rs` does power routing.
- Public API at the top of each file, private internals below.
- All config in RON/TOML. No binary formats in the repo.
- Shaders in WGSL, versioned in git.
- Meshes defined in code or RON (low vertex counts, no modeling tools needed).

### Testing

- Each crate testable independently: `cargo test -p sa_physics`
- Unit tests inline (Rust `#[cfg(test)]` convention)
- Integration tests in each crate's `tests/` directory
- Physics, universe generation, survival are all headless-testable (no GPU)
- Shader compilation testable offline

### Workflow

- `cargo check` for fast error detection (seconds, no full compile)
- `cargo clippy` for idiomatic Rust (always run before committing)
- `cargo test` per-crate during development, full workspace in CI
- Strong types catch unit/domain mismatches at compile time
- Typed error enums per crate with `thiserror` — no string errors, no panics in library code

### CI (GitHub Actions)

- `cargo check` on every push
- `cargo clippy` on every push
- `cargo test --workspace` on every push
- Cross-compile check for Windows target on macOS CI
- No manual steps
