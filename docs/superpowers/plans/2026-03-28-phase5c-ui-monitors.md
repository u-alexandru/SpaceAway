# Phase 5c: UI & Monitors --- Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a minimal HUD overlay and an in-world helm monitor using egui rendered via egui-wgpu. The HUD shows context-sensitive crosshair icons based on the hovered interactable. The helm monitor renders ship data (speed, throttle, engine state) onto an offscreen texture mapped to the cockpit screen mesh. All UI is code-driven --- no asset files.

**Architecture:** New `ui/` module inside the `spaceaway` crate (the game binary). egui is a UI concern, not an engine concern, so it lives in the game crate. Three files: `mod.rs` (UiSystem coordinator), `hud.rs` (HUD overlay with context crosshair), `helm_screen.rs` (helm monitor layout). A new `screen.wgsl` shader in `sa_render` renders textured quads for in-world monitors. The existing geometry pipeline is unchanged.

**Key Design Decisions:**
- egui 0.34 + egui-wgpu 0.34 (latest stable, compatible with wgpu 24 already in workspace)
- HUD renders as egui overlay directly to the screen framebuffer after the 3D scene
- Helm monitor renders egui to a 256x256 offscreen wgpu texture, then that texture is mapped onto the screen mesh via the new screen pipeline
- The `Renderer::render_frame` method is refactored to return the encoder and frame instead of submitting, so egui can render after the 3D pass
- Context crosshair icons are drawn with egui's `Painter` API (geometric shapes, no image assets)

**Tech Stack:** Rust, egui 0.34, egui-wgpu 0.34, wgpu 24, sa_ship (InteractionSystem, InteractableKind)

---

## File Structure

```
crates/spaceaway/src/
    ui/
    +-- mod.rs              # UiSystem: egui context, egui-wgpu renderer, orchestration
    +-- hud.rs              # HUD overlay: context crosshair icons
    +-- helm_screen.rs      # Helm monitor layout: speed, throttle, engine state

crates/sa_render/src/
    +-- screen_pipeline.rs  # NEW: textured quad pipeline for monitors
    +-- shaders/screen.wgsl # NEW: texture-sampling shader for monitor surfaces

Root:
    Cargo.toml              # MODIFIED: add egui, egui-wgpu workspace deps
crates/spaceaway/Cargo.toml # MODIFIED: add egui, egui-wgpu deps
crates/sa_render/Cargo.toml # MODIFIED: add bytemuck (if not present)
crates/sa_render/src/lib.rs # MODIFIED: add screen_pipeline module, re-export
crates/sa_render/src/renderer.rs # MODIFIED: refactor render_frame to expose encoder
crates/spaceaway/src/main.rs # MODIFIED: integrate UiSystem into game loop
```

---

### Task 1: Add egui + egui-wgpu Dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/spaceaway/Cargo.toml`

- [ ] **Step 1: Add workspace dependencies**

Add to root `Cargo.toml` under `[workspace.dependencies]`:
```toml
egui = "0.34"
egui-wgpu = { version = "0.34", default-features = false }
```

We disable default features on egui-wgpu to avoid pulling in winit integration and webgl --- we only need the raw wgpu renderer.

- [ ] **Step 2: Add to spaceaway Cargo.toml**

Add to `crates/spaceaway/Cargo.toml` under `[dependencies]`:
```toml
egui.workspace = true
egui-wgpu.workspace = true
```

- [ ] **Step 3: Verify compilation**

Run `cargo check -p spaceaway` to confirm the new deps resolve and compile.

**Verification:** `cargo check -p spaceaway` succeeds.

---

### Task 2: HUD Overlay --- egui Integration with wgpu

**Files:**
- Create: `crates/spaceaway/src/ui/mod.rs`
- Create: `crates/spaceaway/src/ui/hud.rs`
- Modify: `crates/spaceaway/src/main.rs` (add `mod ui`, initialize UiSystem, call update/render)
- Modify: `crates/sa_render/src/renderer.rs` (refactor to expose encoder for egui pass)

- [ ] **Step 1: Refactor Renderer::render_frame**

The current `render_frame` creates the encoder, runs the render pass, submits, and presents --- all internally. We need to split this so the caller can run egui after the 3D pass but before submit.

New approach: `render_frame` returns a `FrameContext` struct containing the encoder, frame, and view. The caller (main.rs) then runs egui, then calls `submit_frame`.

```rust
pub struct FrameContext {
    pub encoder: wgpu::CommandEncoder,
    pub frame: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

// render_frame now returns Option<FrameContext> instead of submitting
pub fn render_frame(...) -> Option<FrameContext> { ... }

// New: submit after all passes are done
pub fn submit_frame(gpu: &GpuContext, ctx: FrameContext) {
    gpu.queue.submit(std::iter::once(ctx.encoder.finish()));
    ctx.frame.present();
}
```

- [ ] **Step 2: Create ui/mod.rs --- UiSystem**

The UiSystem holds the egui context and egui-wgpu renderer. It provides `update()` (runs egui layouts) and `render_hud()` (renders egui to the screen framebuffer).

```rust
pub struct UiSystem {
    egui_ctx: egui::Context,
    egui_renderer: egui_wgpu::Renderer,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
}

impl UiSystem {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Self;
    pub fn resize(&mut self, width: u32, height: u32);
    pub fn update(&mut self, hud_state: &HudState);
    pub fn render_hud(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView);
}
```

`HudState` carries the hovered interactable kind into the UI layer:
```rust
pub struct HudState {
    pub hovered_kind: Option<sa_ship::InteractableKind>,
    pub screen_width: u32,
    pub screen_height: u32,
}
```

- [ ] **Step 3: Create ui/hud.rs --- test label**

Start with a simple proof-of-concept: render "SpaceAway" text in the top-left corner using `egui::Area`.

```rust
pub fn draw_hud(ctx: &egui::Context, state: &HudState) {
    egui::Area::new(egui::Id::new("hud_test"))
        .fixed_pos(egui::pos2(10.0, 10.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("SpaceAway")
                    .color(egui::Color32::from_white_alpha(180))
                    .size(14.0),
            );
        });
}
```

- [ ] **Step 4: Integrate in main.rs**

Add `mod ui;` to main.rs. Add `ui_system: Option<UiSystem>` to App. Initialize in `resumed()` after creating the renderer. In `about_to_wait` render section:

1. Call `renderer.render_frame(...)` which returns `Option<FrameContext>`
2. If Some, call `ui_system.update(hud_state)`
3. Call `ui_system.render_hud(device, queue, &mut ctx.encoder, &ctx.view)`
4. Call `Renderer::submit_frame(gpu, ctx)`

- [ ] **Step 5: Verify**

Run `cargo check -p spaceaway`. Run `cargo test --workspace`.

**Verification:** Game compiles. "SpaceAway" text is visible in top-left corner when running.

---

### Task 3: Context Crosshair Icons

**Files:**
- Modify: `crates/spaceaway/src/ui/hud.rs`

- [ ] **Step 1: Replace test label with crosshair rendering**

In `draw_hud`, use egui's `Painter` API to draw context-sensitive icons at screen center.

The center is `(screen_width / 2, screen_height / 2)`. Icons are drawn based on `hud_state.hovered_kind`:

| InteractableKind | Icon | Drawing |
|------------------|------|---------|
| None (nothing) | Tiny dot | `circle_filled(center, 2.0, white_alpha(100))` |
| Lever | Grab (two vertical bars) | Two `line_segment` calls, 10px tall, 6px apart |
| Button / Switch | Press (circle + dot) | `circle_stroke(center, 8.0)` + `circle_filled(center, 3.0)` |
| HelmSeat | Sit (chevron) | Two `line_segment` forming a V/chair shape |
| Screen | Eye (oval + dot) | `circle_stroke` ellipse approximation + `circle_filled` |

All drawn in white with moderate alpha (180). When hovered, brighter (240).

- [ ] **Step 2: Verify**

`cargo check -p spaceaway`. `cargo test --workspace`.

**Verification:** Crosshair icons change based on what the player looks at.

---

### Task 4: Screen Pipeline for Textured Quads

**Files:**
- Create: `crates/sa_render/src/shaders/screen.wgsl`
- Create: `crates/sa_render/src/screen_pipeline.rs`
- Modify: `crates/sa_render/src/lib.rs` (add module, re-export)
- Modify: `crates/sa_render/Cargo.toml` (if needed)

- [ ] **Step 1: Write screen.wgsl shader**

Simple textured quad shader. Takes the same uniforms bind group (group 0) as geometry for view_proj, plus a texture bind group (group 1) for the screen texture and sampler. Uses instance model matrix for positioning.

Vertex input: position (vec3), uv (vec2). Instance input: model matrix (4x vec4).
Fragment: samples the texture at the interpolated UV coordinates.

- [ ] **Step 2: Create ScreenPipeline struct**

```rust
pub struct ScreenVertex { pub position: [f32; 3], pub uv: [f32; 2] }

pub struct ScreenPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}
```

The pipeline uses:
- Bind group 0: same uniform buffer layout as GeometryPipeline (view_proj, lighting --- though lighting is unused, sharing the layout simplifies things)
- Bind group 1: texture_2d + sampler for the screen content
- Vertex buffers: ScreenVertex (slot 0) + InstanceRaw (slot 1, reused from geometry)
- Alpha blending for the screen surface
- Depth test enabled (screen is part of the 3D scene)

Helper method: `create_screen_bind_group(device, texture_view) -> wgpu::BindGroup` --- creates a bind group for a specific screen texture.

- [ ] **Step 3: Add to lib.rs**

Add `pub mod screen_pipeline;` and re-export `ScreenPipeline`.

- [ ] **Step 4: Verify**

`cargo check -p sa_render`. `cargo test --workspace`.

**Verification:** sa_render compiles with the new pipeline. No runtime test yet (pipeline needs a texture to bind).

---

### Task 5: Helm Monitor --- Render-to-Texture

**Files:**
- Create: `crates/spaceaway/src/ui/helm_screen.rs`
- Modify: `crates/spaceaway/src/ui/mod.rs` (add monitor rendering)

- [ ] **Step 1: Create helm_screen.rs layout**

Define the egui layout for the helm monitor:

```rust
pub struct HelmData {
    pub speed: f32,
    pub throttle: f32,
    pub engine_on: bool,
}

pub fn draw_helm_screen(ctx: &egui::Context, data: &HelmData) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(13, 13, 20)))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Title
                ui.label(RichText::new("HELM").color(HELM_BLUE).size(16.0));
                ui.separator();
                // Speed
                ui.label(RichText::new(format!("{:.1} m/s", data.speed)).color(HELM_BLUE).size(24.0));
                // Throttle bar
                ui.label(RichText::new(format!("THR {:.0}%", data.throttle * 100.0)).size(14.0));
                // Engine state
                let engine_color = if data.engine_on { Color32::from_rgb(50, 200, 80) } else { Color32::from_rgb(180, 40, 40) };
                ui.label(RichText::new(if data.engine_on { "ENGINE ON" } else { "ENGINE OFF" }).color(engine_color).size(14.0));
            });
        });
}
```

HELM_BLUE = `Color32::from_rgb(38, 89, 166)` (from spec [0.15, 0.35, 0.65]).

- [ ] **Step 2: Add offscreen texture management to UiSystem**

Add to UiSystem:
```rust
monitor_texture: wgpu::Texture,       // 256x256, RENDER_ATTACHMENT | TEXTURE_BINDING
monitor_texture_view: wgpu::TextureView,
monitor_egui_renderer: egui_wgpu::Renderer,  // separate renderer for offscreen
```

Create the texture with `Rgba8UnormSrgb` format, 256x256.

New method:
```rust
pub fn render_helm_monitor(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, helm_data: &HelmData);
```

This method:
1. Creates a new egui context run with 256x256 screen size
2. Calls `draw_helm_screen` to build the layout
3. Tessellates and uploads textures
4. Begins a render pass targeting `monitor_texture_view`
5. Renders egui into that pass

- [ ] **Step 3: Expose monitor texture view**

Add a getter `pub fn helm_texture_view(&self) -> &wgpu::TextureView` so the renderer can bind it in the screen pipeline.

- [ ] **Step 4: Verify**

`cargo check -p spaceaway`. `cargo test --workspace`.

**Verification:** Compiles. Monitor texture is created and rendered to each frame (not yet visible --- Task 6 wires it into the scene).

---

### Task 6: Wire Monitor into the Scene

**Files:**
- Modify: `crates/spaceaway/src/main.rs` (add screen pipeline usage, create quad mesh, bind monitor texture)
- Modify: `crates/sa_render/src/renderer.rs` (add ScreenPipeline to Renderer, add screen draw method)

- [ ] **Step 1: Add ScreenPipeline to Renderer**

Add `screen_pipeline: ScreenPipeline` field to `Renderer`. Initialize in `Renderer::new()`.

- [ ] **Step 2: Create screen quad mesh in main.rs**

A quad mesh at the Speed Display position (0.0, 0.3, 0.8) in ship space, sized 0.4 x 0.25 (matching the screen interactable dimensions). The quad faces -Z (toward the pilot).

Vertices: 4 corners with UV coordinates (0,0), (1,0), (1,1), (0,1). Indices: 2 triangles.

Upload this as a GPU vertex buffer + index buffer.

- [ ] **Step 3: Draw the screen quad in the render pass**

After geometry draws but before egui HUD, within the same render pass:
1. Set the screen pipeline
2. Bind the uniform bind group (group 0, same as geometry --- view_proj)
3. Bind the monitor texture bind group (group 1)
4. Set the screen quad vertex buffer + instance buffer (with ship transform)
5. Draw indexed (6 indices, 1 instance)

Store the bind group and update it if the texture changes.

- [ ] **Step 4: Orchestrate the render order in main.rs**

The full render order per frame becomes:
1. `ui_system.render_helm_monitor(...)` --- render egui to offscreen texture
2. `renderer.render_frame(...)` --- 3D scene (sky, geometry including screen quad, stars, nebulae) returns FrameContext
3. `ui_system.render_hud(...)` --- HUD overlay into the FrameContext encoder
4. `Renderer::submit_frame(...)` --- submit and present

- [ ] **Step 5: Verify**

`cargo check -p spaceaway`. `cargo test --workspace`. `cargo clippy --workspace -- -D warnings`.

**Verification:** The helm monitor shows live speed, throttle %, and engine state on the cockpit screen mesh. The HUD crosshair is visible at screen center. Both update every frame.

---

## Completion Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] HUD overlay renders via egui on the screen framebuffer
- [ ] Context crosshair icons change based on hovered interactable
- [ ] Helm monitor renders to offscreen 256x256 texture
- [ ] Screen quad displays the helm texture in the cockpit
- [ ] No regressions in existing functionality
