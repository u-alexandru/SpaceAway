# Debugging & Profiling Tools — Research

## Available Tools for Our Stack

### 1. wgpu-profiler (GPU Timing)
**Crate:** `wgpu-profiler`
**What it does:** Per-pass GPU timing using hardware timer queries. Shows exactly how long each render pass takes on the GPU.
**Key features:**
- Thread-safe, pools timer queries automatically
- No GPU stalling (non-blocking)
- Chrome trace export (flamegraph visualization)
- Tracy integration (real-time profiling viewer)
- Puffin integration

**Usage:**
```rust
// Wrap render passes with profiling scopes
profiler.begin_scope("geometry_pass", &mut encoder, &device);
// ... render ...
profiler.end_scope(&mut encoder);
```

**Value for us:** Shows if the bottleneck is the sky shader, star rendering, monitors, or geometry. Essential for the FPS issue.

### 2. Puffin (CPU Frame Profiling)
**Crate:** `puffin`
**What it does:** Lightweight CPU profiling with ~50-200ns overhead per scope. Shows frame breakdown as a flamegraph.
**Key features:**
- `puffin::profile_function!()` and `puffin::profile_scope!("name")` macros
- `puffin_http` sends data over TCP to the `puffin_viewer` GUI app
- Real-time frame-by-frame visualization

**Usage:**
```rust
fn update(&mut self) {
    puffin::profile_function!();
    {
        puffin::profile_scope!("physics");
        self.physics.step(dt);
    }
    {
        puffin::profile_scope!("rendering");
        self.render();
    }
}
```

**Value for us:** Shows where CPU time goes (physics step, star generation, egui rendering, game logic). The 26 FPS issue might be CPU-bound not GPU-bound.

### 3. Tracy (Full System Profiler)
**Crate:** `tracy-client` (Rust bindings)
**What it does:** Real-time nanosecond-resolution frame profiler. Shows CPU, GPU, memory, locks.
**Key features:**
- Frame-oriented (designed for games)
- Worst-case frame detection
- CPU + GPU unified timeline
- Memory allocation tracking
- Supports Rust via bindings

**Value for us:** The most powerful option but requires running the Tracy server application separately. Better for deep investigation of specific performance issues.

### 4. Rapier Debug Renderer
**What it does:** Visualizes physics colliders as wireframe lines. Shows what rapier "sees."
**Limitation:** Only available through bevy_rapier plugin (we use raw rapier3d). We'd need to implement our own collider visualization.
**Alternative for us:** We can render collider shapes as wireframe meshes using our existing geometry pipeline. Draw each collider as a semi-transparent colored mesh.

## Recommended Implementation Plan

### Phase 1: Built-in debug overlay (DONE)
Implemented in `/tmp/spaceaway_debug.json` + window title:
- Per-system timing: phys_step, query_pipeline, move_shape, render, total, fps
- Player/ship positions, velocities, player-ship offset
- Interaction state (hovered, dragging, ray debug)
- Physics stats (body count, collider count)
- VSync toggle (V key) for uncapped benchmarking

### Phase 2: Puffin integration (WHEN NEEDED)
Add `puffin` for detailed CPU profiling:
- Wrap each major system in `profile_scope!`
- Run `puffin_viewer` to see flamegraph
- Lightweight, minimal overhead

### Phase 3: wgpu-profiler (WHEN GPU IS BOTTLENECK)
Add GPU pass timing:
- Profile each render pass (sky, geometry, stars, monitors, HUD)
- Export to chrome trace for visualization

### Phase 4: Collider visualization (WHEN PHYSICS BUGS)
Render collider shapes as semi-transparent wireframes:
- Toggle with a debug key (F3)
- Shows floor, walls, bulkheads, door openings
- Helps diagnose "stuck on door" issues

## Dependencies (when implementing)

```toml
# Phase 2
puffin = "0.19"
puffin_http = "0.16"

# Phase 3
wgpu-profiler = "0.18"
```

## For AI Agent Debugging

The debug JSON file at `/tmp/spaceaway_debug.json` is the primary tool for AI-assisted debugging. Currently includes:
- Per-frame timing breakdown (phys_step, query_pipeline, move_shape, render, fps)
- Player and ship positions, velocities, grounded state
- Player-to-ship offset (should be constant when standing still)
- Interaction raycast debug (origin, direction, hit info)
- Physics world stats (body count, collider count)

### Remaining enhancements:
- Add collider positions dump (on keypress)
- Add ship rotation as euler angles (more readable than quaternion)

## Sources

- [wgpu-profiler](https://github.com/Wumpf/wgpu-profiler)
- [Puffin profiler](https://github.com/EmbarkStudios/puffin)
- [Tracy profiler](https://github.com/wolfpld/tracy)
- [Rapier Debug Render](https://docs.rs/bevy_rapier3d/latest/bevy_rapier3d/render/struct.RapierDebugRenderPlugin.html)
