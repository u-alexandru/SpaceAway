# Phase 5c: UI & Monitors — Design Spec

## Overview

Add in-world ship monitors and a minimal HUD using egui rendered to wgpu textures. All UI is code-driven — no asset files, fully AI-agent-producible.

## Visual Style

Dark backgrounds with station-colored accents. Functional, utilitarian, readable. Each station has its own accent color matching the ship design guide:

| Station | Accent Color |
|---------|-------------|
| Helm | Blue [0.15, 0.35, 0.65] |
| Navigation | Green [0.15, 0.55, 0.35] |
| Sensors | Purple [0.45, 0.18, 0.55] |
| Engineering | Amber [0.65, 0.45, 0.15] |

Monitor background: near-black [0.05, 0.05, 0.08]. Text: white/light gray. Data values: accent color.

## Scope (Phase 5c)

### In-World Monitor: Helm Screen

One physical monitor in the cockpit displaying:
- Ship speed (m/s)
- Throttle percentage
- Engine state (ON/OFF)
- Heading (simplified compass)

Rendered via egui to an offscreen wgpu texture, mapped onto the screen mesh in the cockpit.

### HUD: Context Crosshair

Minimal screen overlay:
- **Context icon at screen center** — changes based on what the player looks at:
  - Nothing: tiny dot (or invisible)
  - Lever: grab icon (two parallel vertical lines)
  - Button: press icon (circle with inner dot)
  - Helm seat: sit icon (angle bracket / chair shape)
  - Screen: eye icon (oval with dot)
- **No text labels** — icons only, player learns through visual language
- **No ship data on HUD** — all ship info on in-world monitors

### HUD: Player Vitals (placeholder)

- Small indicators at screen edges
- Only appear when relevant (O2 drops, health changes)
- Not functional yet — just the rendering infrastructure for Phase 5b
- Clean, minimal, unobtrusive

## Architecture

### New Dependencies

```toml
egui = "0.31"
egui-wgpu = "0.31"
```

### Rendering Pipeline

```
Frame:
1. 3D scene renders (ship, stars, sky)
2. For each in-world monitor:
   a. Run egui layout code for this monitor
   b. egui-wgpu renders to offscreen texture (256x256 or 512x512)
   c. Monitor mesh uses this texture in its draw command
3. HUD overlay:
   a. Run egui layout for HUD (crosshair, icons)
   b. egui-wgpu renders directly to the screen framebuffer
```

### Module Structure

New module in sa_render or spaceaway:

```
crates/spaceaway/src/
├── ui/
│   ├── mod.rs          # UI system coordinator
│   ├── hud.rs          # HUD overlay (crosshair, context icons, vitals)
│   ├── monitors.rs     # In-world monitor rendering
│   └── helm_screen.rs  # Helm monitor layout (speed, throttle, engine)
```

### Monitor Rendering Flow

1. Create a wgpu::Texture with RENDER_ATTACHMENT usage (offscreen target)
2. Each frame, run egui context for this monitor
3. egui-wgpu renders the UI to the offscreen texture
4. The ship's screen mesh has a material that samples this texture
5. The geometry shader needs a texture sampler (currently vertex-color only)

**Challenge:** Our renderer currently uses vertex colors only — no textures. For monitors, we need a textured material path. Options:

- **Option A:** Separate render pipeline for textured quads (the monitor face). Simple, clean, minimal changes to existing code.
- **Option B:** Add texture support to the existing geometry pipeline. More general but larger change.

**Recommend Option A** — a dedicated "screen pipeline" that renders textured quads at monitor positions. The existing geometry pipeline stays unchanged.

### Context Icon Rendering

Icons drawn with egui's Painter API in the HUD overlay:
```rust
// Tiny dot (nothing hovered)
painter.circle_filled(center, 2.0, Color32::from_white_alpha(120));

// Grab icon (lever hovered)
painter.line_segment([top_left, bottom_left], stroke);
painter.line_segment([top_right, bottom_right], stroke);

// Press icon (button hovered)
painter.circle_stroke(center, 8.0, stroke);
painter.circle_filled(center, 3.0, color);
```

Pure code — Claude can iterate on exact shapes, sizes, colors without any asset pipeline.

### Interaction with Monitors

Future enhancement (not Phase 5c):
1. Raycast hits monitor mesh → get UV coordinates
2. Map UV to egui screen coordinates
3. Feed mouse position to egui as input events
4. egui handles widget interactions (buttons, sliders on screen)

For Phase 5c, monitors are display-only. Interaction stays with the physical lever/button/seat.

## What This Does NOT Include

- Interactive monitors (clicking buttons ON the screen) — future
- Navigation star map display — needs universe query integration
- Sensor scan display — needs sensor system
- Engineering power grid display — needs sa_survival
- Text rendering on non-monitor surfaces
- Font loading (egui has built-in fonts)

## Testing

- Helm screen renders readable text with correct data
- HUD crosshair visible at screen center
- Context icons change based on hovered interactable
- Monitor texture updates every frame without flickering
- Performance: <1ms per monitor render
