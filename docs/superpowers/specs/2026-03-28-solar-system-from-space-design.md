# Sub-project 1: Solar Systems Visible from Space

Procedural solar systems with planets, moons, rings, atmospheres, and stars — all viewable from the cockpit. Navigation with window markers and lock-on warp targeting. Arrival at system edge with helm monitor overview.

**Scope:** See solar systems from space. Not landable yet — that's Sub-project 2.

---

## 1. Design Goals

- Warp to any star and find a unique procedural solar system
- Planets, moons, gas giants, rings, atmospheres rendered beautifully from space
- Flat-shaded low-poly icosphere rendering matching the game's aesthetic
- Star as an emissive 3D object with corona glow
- Window markers for nearby stars with lock-on targeting
- Gravity well auto-drop on arrival
- Helm monitor shows system overview on arrival
- 1:1 planet scale — real sizes, real distances, real vastness
- 30x time scale — ~50 minute day/night cycle, planets orbit visibly over sessions

---

## 2. Planet Scale (1:1)

All sizes are real-world scale. No reduction.

| Body type | Radius range | Example |
|-----------|-------------|---------|
| Small rocky | 1,000–4,000 km | Mercury (2,440 km), Moon (1,737 km) |
| Earth-like | 4,000–10,000 km | Earth (6,371 km), Venus (6,052 km) |
| Super-Earth | 10,000–16,000 km | Larger rocky worlds |
| Ice giant | 15,000–55,000 km | Uranus (25,362 km), Neptune (24,622 km) |
| Gas giant | 25,000–160,000 km | Jupiter (69,911 km), Saturn (58,232 km) |
| Star | 100,000–2,000,000 km | Sun (696,000 km) |

Orbital distances are real AU scale. System outer edge ~40–50 AU from star.

---

## 3. Time Scale

**30x acceleration.** 1 real second = 30 game seconds.

| Event | Real time |
|-------|-----------|
| Day/night cycle (planet rotation) | ~48 minutes |
| Mercury orbit (88 days) | 2.9 days of play |
| Earth orbit (365 days) | 12.2 days of play |
| Jupiter orbit (12 years) | 146 days of play |

Planet positions computed as: `theta = initial_phase + (2 * PI * game_time * 30) / (orbital_period_seconds)`. Circular orbits. Initial phase from seed.

---

## 4. Solar System Generation

### 4.1 Extend existing `sa_universe/system.rs`

The current `generate_system()` already produces planets with orbital radius, mass, radius, type (Rocky/GasGiant/IceGiant), and period. Extend with:

**New fields on Planet:**
- `sub_type: PlanetSubType` — Molten, Desert, Temperate, Ocean, Frozen, Barren (rocky); HotJupiter, WarmGiant, ColdGiant (gas); Cyan, Teal (ice)
- `atmosphere: AtmosphereParams` — color, opacity, scattering_power. None for barren/small bodies.
- `has_rings: bool` — gas giants have ~30% chance, ice giants ~15%
- `ring_params: Option<RingParams>` — inner/outer radius, color, gap positions
- `axial_tilt: f32` — degrees, affects ring angle and seasons
- `rotation_period_hours: f32` — for day/night cycle (10–40 hours typical)
- `surface_temperature_k: f32` — from `278 * L^0.25 / sqrt(d_AU) * greenhouse`
- `color_seed: u64` — for biome/color generation
- `moons: Vec<Moon>` — generated per planet

**Planet sub-type assignment** from distance + mass:
- `d < 0.3 * HZ_inner` → Molten
- `d < HZ_inner, mass < 0.3` → Barren
- `d in HZ, mass > 0.8, high water roll` → Ocean
- `d in HZ, mass > 0.5` → Temperate
- `d < HZ_outer, low moisture` → Desert
- `d > HZ_outer` → Frozen
- `mass < 0.1` → Barren (override)

**Planet count by star type** (refined from research):

| Spectral | Min | Max | Mean |
|----------|-----|-----|------|
| O | 0 | 1 | 0.3 |
| B | 0 | 3 | 1.2 |
| A | 1 | 5 | 2.5 |
| F | 2 | 8 | 5.0 |
| G | 2 | 8 | 5.5 |
| K | 2 | 7 | 4.0 |
| M | 1 | 5 | 2.5 |

### 4.2 Moon generation

Per planet:
- Rocky < 1 M_earth: 0–1 moons (70% chance of 0)
- Rocky 1–5 M_earth: 0–2 moons
- Rocky 5–10 M_earth: 0–3 moons
- Gas giant < 100 M_earth: 2–8 moons
- Gas giant > 100 M_earth: 4–15 moons
- Ice giant: 1–6 moons

Moon properties: radius 200–5,000 km, orbital radius 5–60 planet radii, tidally locked.

### 4.3 ObjectId allocation

Body index (5 bits, 0–31): body 0 = star, bodies 1–N = planets in orbital order, bodies N+1 onward = moons grouped by parent planet. Asteroid belts stored as system properties (no body ID).

---

## 5. Rendering: Planets from Space

### 5.1 LOD chain (angular-size-based)

| Phase | Angular size (pixels) | Rendering |
|-------|----------------------|-----------|
| Point | < 2 px | Colored dot in star field (existing system) |
| Disk | 2–20 px | Billboard quad with planet color + atmosphere glow ring |
| Low-poly sphere | 20–200 px | Icosphere 2–3 subdivisions (80–320 faces) |
| Medium sphere | 200–1000 px | Icosphere 4–5 subdivisions (1,280–5,120 faces) |
| High-detail sphere | > 1000 px | Icosphere 5–6 subdivisions (5,120–20,480 faces) |

Angular size computed as: `2.0 * atan(planet_radius / distance)` × `screen_height / fov`.
Hysteresis: switch up at threshold, switch down at threshold × 0.8.

### 5.2 Rocky planet rendering

- Icosphere with vertex displacement from simplex noise (3D Cartesian sampling for seamless coverage)
- 3–5 octaves fBm, ridged multifractal for mountains
- Vertex colors from biome lookup: `(height, latitude, slope) → color`
- Per-face flat normals (no interpolation)
- Biome palettes per sub-type:

| Sub-type | Color palette |
|----------|--------------|
| Barren | Grey, dark grey (Moon/Mercury) |
| Desert | Tan, orange, rust, dark brown |
| Temperate | Blue (ocean below sea level), green, brown, white (ice caps) |
| Ocean | Deep blue, white cloud swirls, small land masses |
| Frozen | White, pale blue, grey rock |
| Molten | Black, deep red, emissive orange cracks |

### 5.3 Gas giant rendering

- Icosphere, NO vertex displacement (no solid surface)
- Vertex colors from latitude-based band function:
  - 8–16 bands, alternating light zones and dark belts
  - Band edges perturbed by noise for swirling boundaries
  - Color palette seeded per planet: Jupiter (cream/orange/brown), Saturn (gold/tan), ice giant (cyan/teal)
- Storm features: seeded elliptical regions with distinct colors (Great Red Spot equivalent)

### 5.4 Atmosphere shell

- Separate icosphere at 1.02–1.05x planet radius
- Alpha-blended, back-faces only (visible from outside, planet shows through)
- Fresnel rim effect: `opacity = pow(1.0 - abs(dot(view_dir, normal)), scattering_power) * max_opacity`
- Parameters per sub-type:

| Sub-type | Color | Scattering power | Max opacity |
|----------|-------|-------------------|-------------|
| Temperate | (0.4, 0.6, 1.0) | 3.0 | 0.6 |
| Ocean | (0.3, 0.5, 0.9) | 2.5 | 0.7 |
| Desert | (0.8, 0.5, 0.3) | 4.0 | 0.3 |
| Frozen | (0.5, 0.6, 0.8) | 4.0 | 0.2 |
| Molten | (0.9, 0.4, 0.2) | 2.0 | 0.5 |
| Gas giant | (0.6, 0.5, 0.4) | 2.0 | 0.4 |
| Ice giant | (0.3, 0.6, 0.8) | 2.5 | 0.5 |
| Barren | None | — | 0.0 |

### 5.5 Ring systems

- Flat annular disc mesh in planet's equatorial plane (tilted by axial_tilt)
- 48–96 radial segments, 8–16 concentric rings
- Vertex color from radial distance: dense regions bright, gaps transparent
- Alpha-blended, depth write disabled
- Ring shadow on planet: in planet fragment shader, project fragment into ring plane, darken if within ring extent

### 5.6 Star rendering

- Emissive icosphere (bypasses lighting — it IS the light source)
- Surface detail: Voronoi cellular noise for convection cells. Cell centers bright yellow-white, edges darker orange.
- Corona: additive-blended billboard quad, 3–5x star radius, radial falloff `pow(1 - dist, 2.5)`
- Color from existing star temperature → RGB (blackbody, already implemented)
- Ray streamers: 4–8 thin elongated quads radiating from center, slowly rotating (optional, nice-to-have)

### 5.7 Scale transition: star field → 3D object

Stars in the universe are currently rendered as points in the star field. When the player is in a system, the system's star must transition from a point to a 3D object:

- When distance to star < threshold (star subtends > 4 pixels): remove from star field, spawn 3D star mesh
- When distance > threshold: remove 3D mesh, star returns to star field
- Same for planets: when subtending > 2 pixels, spawn 3D mesh

This requires the game loop to track "active system" — the solar system currently being rendered as 3D objects.

---

## 6. Navigation

### 6.1 Window markers

- Query nearest 10–15 stars within 50 ly of `galactic_position`
- For each, project their galactic position to screen space
- Render as HUD overlay: small diamond marker + catalog name + distance in ly
- Only show markers within the forward hemisphere (behind-camera stars hidden)
- Color: dim grey for distant (>20 ly), brighter for closer, highlight for locked target

### 6.2 Star catalog names

Derived from ObjectId: `"SEC {sector_x:04}.{sector_z:04} / S-{system:03}"`.
Example: `"SEC 0042.0017 / S-003"`.

### 6.3 Lock-on targeting

- While seated at helm, click a window marker to lock on
- Locked target: marker becomes a bracket `[ ]`, helm monitor shows distance + ETA
- Ship does NOT auto-align — player points the ship manually. The bracket stays on screen regardless of ship orientation, showing where the target is.
- Engage warp: if locked, gravity well auto-drop at destination. If not locked, free warp.
- Press lock-on key again or Escape to clear lock

### 6.4 Gravity well auto-drop

During warp, each frame check:
- If locked target: distance to target star. When < 50 AU (system edge), disengage warp.
- If no target: check distance to ALL nearby stars (query from universe). When < 50 AU of any star, disengage warp.
- On drop: flash effect, "SYSTEM ENTERED" on helm monitor, system overview populated

### 6.5 Bookmarks

Data structure: `Vec<Bookmark>` where:
```rust
struct Bookmark {
    star_id: ObjectId,
    galactic_position: WorldPos,
    catalog_name: String,
    nickname: Option<String>,
    timestamp: f64, // game time when bookmarked
}
```

Helm key to bookmark current system (e.g., B while seated). Persisted to save file (RON format per project conventions). Full nav console UI deferred to later sub-project.

---

## 7. Helm Monitor Updates

### 7.1 System overview (on arrival)

When entering a system, the helm monitor switches to system view:
```
═══ SYSTEM ENTERED ═══
SEC 0042.0017 / S-003
Class G2 — 5,778 K
Bodies: 6 planets, 12 moons

[1] Rocky    0.4 AU  2,100 km  Barren
[2] Rocky    0.9 AU  5,800 km  Temperate ★
[3] Rocky    1.6 AU  3,200 km  Desert
[4] GasGiant 5.2 AU 71,000 km  Cold ⊕4 moons
[5] GasGiant 9.8 AU 55,000 km  Warm ◎rings ⊕2
[6] IceGiant 19 AU  24,000 km  Cyan ⊕3 moons
```

The ★ marks habitable zone planets. ◎ marks ringed planets. ⊕N shows moon count.

### 7.2 Target info

When locked on a star:
```
TARGET: SEC 0042.0017 / S-003
Distance: 4.37 ly
ETA at current speed: 2m 31s
```

---

## 8. Architecture

### 8.1 New: `sa_universe/system.rs` extensions

Extend existing `Planet` struct with new fields (sub_type, atmosphere, rings, moons, etc.). Add `Moon` struct. Extend `generate_system()` with star-type-dependent planet counts, sub-type assignment, moon generation.

### 8.2 New: `sa_render/src/planet.rs`

Planet rendering module:
- `PlanetMesh` — icosphere generation at variable subdivision levels
- `generate_rocky_mesh(planet, lod) → MeshData` — noise displacement + biome colors
- `generate_gas_giant_mesh(planet, lod) → MeshData` — band coloring
- `generate_atmosphere_mesh(planet) → MeshData` — shell mesh
- `generate_ring_mesh(ring_params) → MeshData` — annular disc

### 8.3 New: `sa_render/src/star_renderer.rs`

Star-as-3D-object rendering:
- Emissive icosphere mesh with Voronoi surface detail
- Corona billboard with additive blend pipeline
- Separate from the existing `SkyRenderer` (which handles the galaxy)

### 8.4 New: `spaceaway/src/solar_system.rs`

Game-level solar system management:
- `ActiveSystem` — the currently loaded system (star + planets + moons as 3D objects)
- LOD management: spawn/despawn 3D meshes based on angular size
- Orbital position computation (circular orbits, 30x time)
- System entry/exit detection
- Draw command generation for renderer

### 8.5 New: `spaceaway/src/navigation.rs`

Navigation system:
- Nearby star query for window markers
- Lock-on target state
- Gravity well detection during warp
- Bookmark storage
- Screen-space projection for HUD markers

### 8.6 Modified: `sa_render/src/renderer.rs`

- Add planet/atmosphere/ring rendering passes (after geometry, before HUD)
- Planet atmosphere uses alpha blending (separate pass)
- Ring system uses alpha blending (separate pass)
- Star corona uses additive blending (separate pass)

### 8.7 Modified: `spaceaway/src/main.rs`

- Game loop: update active system, compute orbital positions, generate draw commands
- Navigation: update window markers, handle lock-on clicks
- Warp: gravity well auto-drop check
- Helm monitor: system overview display

---

## 9. Performance Budget

| Component | Estimated cost | Notes |
|-----------|---------------|-------|
| System generation | < 1 ms (once on arrival) | Deterministic, pure math |
| Orbital positions | < 0.01 ms/frame | Simple trig per body |
| Planet mesh generation | 5–50 ms per LOD change | Background thread, swap when ready |
| Visible planet rendering | 1–3 draw calls | Only 1–3 planets close enough for 3D |
| Atmosphere shells | 1–3 draw calls | Cheap Fresnel fragment shader |
| Ring system | 1 draw call | Alpha blended |
| Star + corona | 2 draw calls | Emissive + additive billboard |
| Window markers | < 0.5 ms | HUD overlay, 10–15 text labels |
| Star proximity check | < 0.01 ms | Only during warp |

Total budget: well under 2 ms/frame for the entire solar system. 60 FPS easily maintained.

---

## 10. Deferred to Future Sub-projects

### Sub-project 2: CDLOD Terrain + Landing
- `sa_terrain` crate with cube-sphere quadtree
- OpenSimplex2 fBm heightmap (5–6 octaves, domain warping)
- Edge stitching + vertex morphing
- Space-to-surface seamless transition
- Planetary gravity (walk on surface)
- Sky dome from surface (atmosphere, sun, sunset)
- Surface vehicles for traversal

### Sub-project 3: Surface Features + Exploration
- 3D density field + Marching Cubes for caves/overhangs
- Resource deposits visible on surface (sensor-guided)
- Abandoned ruins / civilization remnants (seeded placement)
- Biome diversity (forests as color regions, not individual trees)
- Volcanic emissive lava, ice world subsurface scattering
- Surface weather effects

### Sub-project 4: Navigation Console + Ship Database
- Full 3D star map on navigation monitor
- Search/filter by star type, distance, explored status
- Route planning (multi-jump)
- Ship database with bookmarks, notes, discovery log
- Sensor station integration (scan planets for details before landing)

### Sub-project 5: Asteroid Belts + Mining
- Physical asteroid objects (not just visual rings)
- Approach individual asteroids at impulse
- Mining mechanics (gather hydrogen, minerals, rare materials)
- Exotic matter deposits in nebula cores
- Gas giant scooping for fuel

### Future Systems (not yet scoped)
- `sa_audio` — engine sounds, warp hum, atmosphere entry rumble, silence of space
- Multiplayer (Phase 6) — P2P coop, crew consensus for warp jumps
- Station docking — trading, refueling, mission boards
- Ship upgrades — better drives, sensors, hull, life support
- Hazardous environments — radiation, pressure, temperature requiring equipment
