# Performance Techniques

Documented techniques used in SpaceAway to maintain 60+ FPS. Each section explains the problem, the solution, and where the implementation lives.

## Ship-Local Collision Detection

**Problem:** 108 interior colliders attached to the ship body require AABB rebuilds every frame when the ship moves. At 400+ m/s, `physics.step()` took 47ms (19 FPS).

**Solution:** All interior colliders are placed at fixed LOCAL positions (relative to ship origin at 0,0,0) on a fixed rigid body that never moves. The player controller transforms to local space for collision sweeps, then transforms results back to world space.

**How it works:**
1. Record ship position/rotation before and after integration
2. "Carry" the player with the ship: compute local offset from old ship position, place at new ship position (instant teleport, no collision cost)
3. `PlayerController::update()` transforms player world position to ship-local space
4. `move_shape()` sweeps only walk distance (0-0.08m) against stationary colliders
5. Transform result back to world space

**Performance:** phys_step 47ms → 0.03ms at any speed. O(1) regardless of ship velocity or collider count.

**Files:**
- `crates/sa_player/src/controller.rs` — `update()` with ship-local transform
- `crates/spaceaway/src/main.rs` — ship integration + player carry step
- `crates/spaceaway/src/ship_colliders.rs` — 108 colliders at local origin

**Industry precedent:** Same technique used by Star Citizen, KSP, Elite Dangerous for walkable interiors on moving vehicles.

## Manual Ship Integration

**Problem:** Rapier's `physics.step()` rebuilds broad phase for all bodies, expensive with many colliders even on fixed bodies.

**Solution:** Skip `physics.step()` entirely in walk mode. Manually integrate the ship: `v += a*dt`, `p += v*dt`, rotation via small-angle quaternion approximation. The ship is the only dynamic body, so full solver is unnecessary.

**Files:**
- `crates/spaceaway/src/main.rs` — walk mode physics section (search "MANUAL ship integration")

## Half-Resolution Sky Rendering

**Problem:** Analytical sky shader (per-pixel ray-marched galaxy density) took ~13ms at full resolution — dominated frame time.

**Solution:** Three-part optimization:

### 1. Half-Res Offscreen Render
Sky renders to a texture at half width × half height (1/4 pixel count), then blits to the main framebuffer with bilinear filtering. The galaxy band is smooth/low-frequency, so half-res is visually indistinguishable from full-res.

### 2. Fused Density Function
The original shader called `galaxy_density()` and `dust_density()` separately — each computing `r`, `theta`, and spiral arm distances independently. Fused into `galaxy_sample()` that returns `vec2(emission, dust)` in one pass. Halves transcendental operations per sample.

### 3. Logarithmic Sample Spacing
8 log-spaced ray-march samples replace 16 uniform samples. More samples near the observer (where density changes matter), fewer at distance. Proper `dt` weighting ensures correct integration. Half the samples, same visual quality.

**Performance:** Render 14.34ms → 5.96ms (2.4x faster).

**Files:**
- `crates/sa_render/src/shaders/sky.wgsl` — optimized galaxy shader
- `crates/sa_render/src/shaders/sky_blit.wgsl` — fullscreen blit shader
- `crates/sa_render/src/sky.rs` — `SkyRenderer` with offscreen texture + blit pipeline

## Kinematic Character Controller

**Problem:** Dynamic player body created reaction forces (785N gravity counterforce, 24,000N wall impulses) that pushed/rotated the ship. Force-based and velocity-based movement both had fundamental issues with reference frames.

**Solution:** Kinematic-position-based body with `KinematicCharacterController::move_shape()`. Swept capsule collision handles walls, slopes, steps. Zero reaction forces on environment. Gravity tracked manually (vertical_velocity field).

**Key details:**
- `snap_to_ground: 0.5m` + `offset: 0.05m` prevents ground oscillation
- Rising-edge jump detection (only on first frame Space is pressed)
- Grounded grace period: stays grounded if vertical_velocity < 0.5 (prevents quaking)

**Files:**
- `crates/sa_player/src/controller.rs` — `PlayerController`

## Origin Rebasing (Double-Precision Coordinates)

**Problem:** f32 loses precision beyond ~10km from origin. Universe positions span light-years.

**Solution:** Game logic uses `WorldPos` (f64). At render time, subtract camera position from all model matrices in f64, then cast to f32. Everything rendered is camera-relative with sub-millimeter precision regardless of distance from origin.

**Files:**
- `crates/sa_math/src/lib.rs` — `WorldPos`, `LocalPos` types
- `crates/sa_render/src/renderer.rs` — origin rebasing in `render_frame()`

## Analytical Sky Shader (vs Cubemap)

**Problem:** Cubemap skyboxes have visible seams at face edges and can't represent parallax (stars at different distances).

**Solution:** Per-pixel analytical galaxy density computation directly in the fragment shader. Ray-march through a density model with 4 logarithmic spiral arms, exponential disc, spherical bulge, and Beer-Lambert dust absorption. All visible stars are real objects projected from the universe database.

**Files:**
- `crates/sa_render/src/shaders/sky.wgsl` — galaxy density model
- `crates/sa_universe/` — procedural star generation

## Deterministic Procedural Generation

**Problem:** Sequential RNG requires generating content in order. Can't random-access a specific sector.

**Solution:** xxHash64 coordinate hashing — any sector can be generated in O(1) from its coordinates alone. No sequential state, perfectly deterministic, reproducible across sessions and players.

**Files:**
- `crates/sa_universe/src/seed.rs` — coordinate hashing
- `crates/sa_universe/src/sector.rs` — Poisson-disk star placement

## Debug & Benchmarking Tools

### Live Debug JSON
Every 30 frames, game state is written to `/tmp/spaceaway_debug.json`: player/ship positions, timing breakdown (phys_step, query_pipeline, move_shape, render), collision stats, interaction state. Readable by AI agents or external tools.

### VSync Toggle
Press **V** to toggle between VSync (60 FPS cap, Fifo) and uncapped (Immediate). Uncapped mode reveals true GPU/CPU time without present() blocking. Essential for benchmarking — with VSync on, timings are masked by the 16.67ms frame budget.

**Files:**
- `crates/spaceaway/src/main.rs` — `write_debug_state()`, key handler
- `crates/sa_render/src/gpu.rs` — `toggle_vsync()`

## Reversed-Z Infinite Depth Buffer

**Problem:** 1:1 scale planets are at AU distances (millions to billions of meters). Standard depth buffers with finite far plane clip distant objects. Extending the far plane (e.g., 100km → 100M km) destroys near-plane precision — ship interior Z-fights.

**Solution:** Reversed-Z with infinite far projection. Depth 1.0 at the near plane (0.1m), depth 0.0 at infinity. No far clip plane exists.

**Why it works:** IEEE 754 floating point has more precision near 0.0 than near 1.0. Standard Z maps near→0.0 (wasting precision) and far→1.0 (where precision is concentrated but not needed). Reversing puts the precision where it matters — near the camera.

**Precision at various distances:**
- 0.1m: ~0.001mm (ship interior screws and buttons)
- 10m: ~0.01mm (ship corridors, character detail)
- 1km: ~0.1m (ship exterior from nearby)
- 100km: ~10m (planet surface features)
- 1M km: ~10km (planet-to-planet visual separation)
- 1 AU: ~100km (still renders — no clip!)

**Implementation:**
- Projection matrix: custom reversed infinite projection (not glam's `perspective_rh`)
- All pipelines: `CompareFunction::GreaterEqual` (instead of Less)
- Depth clear: `0.0` (instead of 1.0)
- Sky/background shaders: depth output near 0.0 (far), not near 1.0
- Depth format: `Depth32Float` (unchanged)

**Rule for new pipelines:** Any new render pipeline MUST use `GreaterEqual` depth compare and understand that 0.0 = far, 1.0 = near. This is the opposite of the default convention.

**Files:**
- `crates/sa_render/src/camera.rs` — reversed infinite projection matrix
- `crates/sa_render/src/pipeline.rs` — geometry depth compare
- `crates/sa_render/src/screen_pipeline.rs` — screen quad depth compare
- `crates/sa_render/src/nebula.rs` — nebula depth compare
- `crates/sa_render/src/star_field.rs` — star depth compare
- `crates/sa_render/src/renderer.rs` — depth clear value
- `crates/sa_render/src/shaders/sky.wgsl` — sky depth output
- `crates/sa_render/src/shaders/sky_blit.wgsl` — sky blit depth output

**Industry precedent:** Unreal Engine 4/5, Unity HDRP, Space Engine, Outerra, most modern AAA engines.

## Key Performance Numbers (March 2026)

| Metric | Before | After | Technique |
|--------|--------|-------|-----------|
| physics.step at 400 m/s | 47ms | 0.03ms | Ship-local collision |
| Sky render | 14.3ms | 6.0ms | Half-res + fused + log samples |
| Total frame (uncapped) | 47+ms | 8.2ms | Combined |
| FPS (uncapped) | 19 | 113 | Combined |
| FPS (VSync) | 19-37 | 60 | Combined |
