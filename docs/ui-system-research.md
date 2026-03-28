# UI & Monitor System — Research & Recommendations

## Overview

SpaceAway needs two kinds of UI:
1. **Diegetic (in-world)**: Ship monitors, console screens, status displays — rendered on 3D surfaces inside the ship
2. **HUD overlay**: Player vitals, interaction prompts, debug info — rendered on screen

Both use the same technique: **render-to-texture** with egui.

## Recommended Stack: egui + wgpu render-to-texture

### Why egui

- **Pure Rust, immediate-mode** — no markup files, no retained state. Claude Code writes `ui.label("Speed: 50 m/s")` and it works.
- **Rich widgets** — labels, buttons, sliders, progress bars, plots, tables, color pickers, text input
- **wgpu integration** — `egui-wgpu` crate renders egui draw commands to any wgpu texture
- **Render-to-texture proven** — Bevy's official example renders egui to a texture and maps it onto 3D surfaces
- **AI-agent friendly** — all UI is code, no asset files, fully testable

### How Diegetic Monitors Work

```
Frame loop:
1. For each ship monitor/screen:
   a. Begin egui context for this monitor
   b. Run layout code (labels, gauges, buttons)
   c. egui produces draw commands
   d. egui-wgpu renders to an offscreen wgpu::Texture (one per monitor)
   e. The monitor's 3D mesh uses this texture as its material
2. For HUD:
   a. egui renders to the main screen as an overlay after 3D rendering
```

### Interaction with In-World Screens

1. Player looks at a monitor (raycast hits the screen mesh)
2. Calculate UV coordinates of the hit point on the screen surface
3. Map UV to egui screen coordinates
4. Feed mouse position + clicks to egui as input
5. egui handles hover/click on its widgets
6. This reuses our existing interaction raycast system

### Performance

- Each monitor: one offscreen render pass per frame (~0.1-0.5ms depending on complexity)
- 4-5 monitors in the cockpit: ~1-2ms total
- egui is extremely lightweight — designed for real-time applications
- Monitors can update at reduced frequency (e.g., 10 Hz) if needed

### Dependencies to Add (when implementing)

```toml
egui = "0.31"
egui-wgpu = "0.31"
```

### What Each Monitor Displays

| Monitor | Location | Content |
|---------|----------|---------|
| Speed Display | Cockpit, above helm | Ship speed, throttle %, engine state |
| Navigation | Nav station | Star map, waypoints, current sector |
| Sensors | Sensors station | Scan results, anomaly detection |
| Engineering | Engineering station | Power grid, system status, damage |
| Debug (dev only) | Toggleable overlay | Physics state, FPS, collider count |

### Alternative Approaches Considered

| Approach | Verdict | Reason |
|----------|---------|--------|
| HTML/CSS (embedded Chromium) | Rejected | Too heavy (100MB+ dependency), overkill for game UI |
| Custom 2D draw (rectangles + text) | Backup | Would work but reinventing egui's functionality |
| WGSL shader-based | Too limited | Good for simple gauges, bad for text/tables |
| Dear ImGui (C++) | Wrong language | Not native Rust, would need bindings |

### Sources

- [Bevy: Render UI to Texture](https://bevy.org/examples/ui-user-interface/render-ui-to-texture/)
- [egui — Rust immediate-mode GUI](https://github.com/emilk/egui)
- [wgpu Render to Texture](https://sotrh.github.io/learn-wgpu/showcase/windowless/)
- [Diegetic UI (Unreal)](https://forums.unrealengine.com/t/diegetic-hud-ui/50881)
- [Unity World Space UI](https://app.daily.dev/posts/ui-toolkit-tutorial-world-space-render-mode-r7f2jlnaz)
