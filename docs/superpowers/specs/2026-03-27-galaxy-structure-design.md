# Phase 4.5: Galaxy Structure & Atmosphere — Design Spec

## Overview

Replace the uniform star density with a realistic spiral galaxy model. Add visual layers that make the sky feel like deep space: a galactic core glow, a Milky Way band of unresolved stars, nebula regions, and distant galaxies. All layers are optimized for performance — screen-space effects, precomputed cubemaps, and billboard sprites.

## Goals

- The sky should look like a real view from inside a spiral galaxy
- Spiral arms, galactic disc, and core bulge drive star placement density
- Bright band across the sky when looking through the galactic disc (Milky Way)
- Warm galactic core glow visible from anywhere
- Colorful nebula regions near spiral arms
- Faint distant galaxies at extreme range
- Zero quality loss, minimal performance cost

## Galaxy Density Model

Modifies the existing `sector_density()` in sa_universe. The galaxy model is a pure density function — given (x, y, z) in light-years from galactic center, returns a density multiplier.

### Components

**Disc:** `exp(-|y| / half_thickness)` where half_thickness = 500 ly. Stars are concentrated in a thin plane. Above/below the plane, density drops exponentially.

**Spiral Arms:** Two logarithmic spirals. For a point at angle θ from center (in the xz plane), the arm centerline is at `θ_arm = k * ln(r) + offset` where k controls tightness. Density boost is gaussian: `exp(-dist_to_arm² / arm_width²)` where arm_width ≈ 2000 ly. Two arms offset by π radians.

**Bulge:** Spherical core, `exp(-r / bulge_radius)` where bulge_radius = 5000 ly. Adds to arm density (not multiplied). Higher density = more stars, older/redder population.

**Base:** Constant 0.1 density floor everywhere — even between arms, some stars exist.

**Final formula:** `disc(y) * (arm_boost(x, z) + bulge(x, y, z) + base_density)`

### Star Population Variation

Stars near the galactic core tend to be older (redder, lower mass bias). Stars in spiral arms include younger blue giants (slightly higher mass bias). This is implemented as a small mass distribution modifier based on galactic position — not a separate generation path.

## Rendering Layers

Five layers rendered in order, back to front:

### Layer 1: Milky Way Band (Precomputed Cubemap)

The accumulated light of millions of unresolved stars seen through the galactic disc.

- **Generation:** 6-face cubemap, 256x256 per face. For each texel, cast a ray from the observer and numerically integrate galaxy density along it (32 samples, max distance ~50,000 ly). The result is accumulated brightness and color tint (warmer/redder toward center, bluer in arms).
- **Rendering:** Fullscreen sky quad sampled from the cubemap. Rendered first (behind everything). Additive blending.
- **Regeneration:** Only when player moves >5000 ly from last generation point. In practice, almost never during on-foot gameplay — only when flying between sectors in a ship.
- **Performance:** Generation is ~10ms (one-time). Rendering is trivial (single textured quad).

### Layer 2: Galactic Core Glow

A soft bright glow in the direction of the galactic center.

- **Rendering:** Screen-space effect in fragment shader. Calculate angle between view direction and direction to galactic center. Apply soft gaussian falloff: `intensity = core_brightness * exp(-angle² / spread²)`. Color: warm gold-white (0.95, 0.85, 0.6).
- **Blending:** Additive, rendered after the Milky Way band, before stars.
- **Performance:** Trivial — one dot product and exp per fragment, no texture.

### Layer 3: Star Field (Existing, Modified Density)

The existing procedural star rendering, now using the spiral galaxy density model.

- **Change:** `sector_density()` calls the galaxy density function instead of simple exponential falloff.
- **No rendering changes** — same billboard quads, same shader, same culling.

### Layer 4: Nebula Sprites

Colored gas cloud regions near spiral arms and star-forming areas.

- **Placement:** Seeded from galaxy coordinates. Nebulae spawn preferentially near spiral arm centerlines and at higher densities. ~50-100 nebulae exist in the nearby galaxy, 5-15 visible at any time.
- **Properties per nebula:** Position (WorldPos), size (50-500 ly radius), base color (seeded — reds, blues, purples, greens), opacity (0.1-0.4), seed for noise pattern.
- **Rendering:** Camera-facing billboard quads. Fragment shader generates procedural noise (fBm, 3-4 octaves) from the nebula's seed to create cloudy, organic shapes. Alpha blended.
- **Rendered after stars** so they don't occlude star points but add color/atmosphere.
- **Performance:** 5-15 fullscreen-ish quads with simple noise shader. ~0.5ms.

### Layer 5: Distant Galaxies

Faint elliptical smudges at extreme distance.

- **Placement:** 20-30 fixed positions at 1M+ ly from galactic center. Seeded deterministically from master seed.
- **Rendering:** Small billboard sprites (10-50 pixels on screen). Simple elliptical gradient in fragment shader with slight rotation per galaxy.
- **Performance:** Negligible — 20-30 tiny quads.

## Render Order

1. Clear to near-black
2. Milky Way cubemap (sky quad)
3. Galactic core glow (additive fullscreen)
4. Star field (billboard quads)
5. Nebula sprites (alpha blended billboards)
6. Distant galaxies (tiny billboards)
7. Geometry (cubes, ship, etc.)
8. UI overlay

## File Changes

### sa_universe modifications:
- `sector.rs`: Replace `sector_density()` with call to galaxy density model
- New `galaxy.rs`: Galaxy density function (disc, arms, bulge, base), nebula placement, distant galaxy positions

### sa_render additions:
- New `sky.rs`: Milky Way cubemap generation and rendering, galactic core glow
- New `shaders/sky.wgsl`: Milky Way cubemap sampling + core glow
- New `nebula.rs`: Nebula sprite management and rendering
- New `shaders/nebula.wgsl`: Procedural noise nebula fragment shader
- Modify `renderer.rs`: Add sky and nebula passes to render order

## Performance Budget

| Layer | Cost | Frequency |
|-------|------|-----------|
| Milky Way cubemap generation | ~10ms | Once per 5000 ly moved |
| Milky Way cubemap render | ~0.1ms | Every frame |
| Galactic core glow | ~0.1ms | Every frame |
| Star field (existing) | ~1-2ms | Every frame |
| Nebula sprites (5-15) | ~0.5ms | Every frame |
| Distant galaxies (20-30) | ~0.1ms | Every frame |
| **Total added per frame** | **~0.8ms** | |

Total sky rendering should stay well under 4ms per frame, leaving plenty of headroom.

## What This Doesn't Include

- Full volumetric raymarched nebulae (future phase — these are billboard sprites)
- Dust lane occlusion (darkening stars behind dust — could add later as a screen-space effect)
- Star color variation by galactic position (subtle, could add later)
- Galaxy rotation / dynamics (static structure is fine)
