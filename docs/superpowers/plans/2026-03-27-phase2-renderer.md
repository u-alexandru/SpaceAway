# Phase 2: Renderer & Input — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the sa_render and sa_input crates, producing a working renderer with flat-shaded low-poly geometry, a directional light, depth buffer, camera system with mouse/keyboard input, and a star field background. End result: fly a camera through a test scene with lit geometry and stars.

**Architecture:** sa_render owns the wgpu renderer (extracted from main.rs), manages the render pipeline, camera, mesh storage, and star field. sa_input provides an abstracted input system over winit events. The game binary wires them together. GPU initialization moves from main.rs into sa_render. The renderer uses a forward pipeline: depth pre-pass → geometry with flat shading + directional light → star field → post-processing (bloom only for now).

**Tech Stack:** wgpu 24, winit 0.30, glam, bytemuck (for GPU buffer data), sa_math (WorldPos/LocalPos, origin rebasing)

---

## File Structure

```
crates/
├── sa_render/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Re-exports
│       ├── gpu.rs              # GpuContext: device, queue, surface, adapter
│       ├── camera.rs           # Camera (view/projection matrices, origin rebasing)
│       ├── vertex.rs           # Vertex type, mesh data structures
│       ├── mesh.rs             # MeshStore: upload/manage GPU mesh buffers
│       ├── pipeline.rs         # Render pipeline creation (flat-shaded + lit)
│       ├── star_field.rs       # Procedural star point rendering
│       ├── renderer.rs         # Renderer: orchestrates frame rendering
│       └── shaders/
│           ├── geometry.wgsl   # Vertex + fragment shader (flat shading + directional light)
│           └── stars.wgsl      # Star point rendering shader
├── sa_input/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Re-exports
│       ├── keyboard.rs         # Keyboard state tracking
│       ├── mouse.rs            # Mouse state (delta, buttons)
│       └── input_state.rs      # Combined InputState struct
└── spaceaway/
    └── src/
        └── main.rs             # Updated: uses sa_render + sa_input
```

---

### Task 1: sa_input Crate — Input State Tracking

**Files:**
- Create: `crates/sa_input/Cargo.toml`
- Create: `crates/sa_input/src/lib.rs`
- Create: `crates/sa_input/src/keyboard.rs`
- Create: `crates/sa_input/src/mouse.rs`
- Create: `crates/sa_input/src/input_state.rs`
- Modify: `Cargo.toml` (workspace root — add sa_input member and dependency)

- [ ] **Step 1: Add sa_input to workspace**

Add `"crates/sa_input"` to the workspace members in root `Cargo.toml`, and add under `[workspace.dependencies]`:
```toml
sa_input = { path = "crates/sa_input" }
```

Create `crates/sa_input/Cargo.toml`:
```toml
[package]
name = "sa_input"
version.workspace = true
edition.workspace = true

[dependencies]
winit.workspace = true
log.workspace = true
```

- [ ] **Step 2: Write failing tests for KeyboardState**

```rust
// crates/sa_input/src/keyboard.rs
#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    #[test]
    fn key_not_pressed_by_default() {
        let kb = KeyboardState::new();
        assert!(!kb.is_pressed(KeyCode::KeyW));
    }

    #[test]
    fn press_and_release() {
        let mut kb = KeyboardState::new();
        kb.set_pressed(KeyCode::KeyW, true);
        assert!(kb.is_pressed(KeyCode::KeyW));
        kb.set_pressed(KeyCode::KeyW, false);
        assert!(!kb.is_pressed(KeyCode::KeyW));
    }

    #[test]
    fn multiple_keys() {
        let mut kb = KeyboardState::new();
        kb.set_pressed(KeyCode::KeyW, true);
        kb.set_pressed(KeyCode::KeyA, true);
        assert!(kb.is_pressed(KeyCode::KeyW));
        assert!(kb.is_pressed(KeyCode::KeyA));
        assert!(!kb.is_pressed(KeyCode::KeyS));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p sa_input`
Expected: FAIL — KeyboardState not defined.

- [ ] **Step 4: Implement KeyboardState**

```rust
// crates/sa_input/src/keyboard.rs
use std::collections::HashSet;
use winit::keyboard::KeyCode;

pub struct KeyboardState {
    pressed: HashSet<KeyCode>,
}

impl KeyboardState {
    pub fn new() -> Self {
        Self {
            pressed: HashSet::new(),
        }
    }

    pub fn set_pressed(&mut self, key: KeyCode, pressed: bool) {
        if pressed {
            self.pressed.insert(key);
        } else {
            self.pressed.remove(&key);
        }
    }

    pub fn is_pressed(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p sa_input`
Expected: All 3 tests PASS.

- [ ] **Step 6: Write failing tests for MouseState**

```rust
// crates/sa_input/src/mouse.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_delta_is_zero() {
        let mouse = MouseState::new();
        let (dx, dy) = mouse.delta();
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }

    #[test]
    fn accumulate_and_clear() {
        let mut mouse = MouseState::new();
        mouse.accumulate_delta(10.0, -5.0);
        mouse.accumulate_delta(3.0, 2.0);
        let (dx, dy) = mouse.delta();
        assert_eq!(dx, 13.0);
        assert_eq!(dy, -3.0);
        mouse.clear_delta();
        let (dx, dy) = mouse.delta();
        assert_eq!(dx, 0.0);
        assert_eq!(dy, 0.0);
    }
}
```

- [ ] **Step 7: Implement MouseState**

```rust
// crates/sa_input/src/mouse.rs
pub struct MouseState {
    delta_x: f32,
    delta_y: f32,
}

impl MouseState {
    pub fn new() -> Self {
        Self {
            delta_x: 0.0,
            delta_y: 0.0,
        }
    }

    pub fn accumulate_delta(&mut self, dx: f32, dy: f32) {
        self.delta_x += dx;
        self.delta_y += dy;
    }

    pub fn delta(&self) -> (f32, f32) {
        (self.delta_x, self.delta_y)
    }

    pub fn clear_delta(&mut self) {
        self.delta_x = 0.0;
        self.delta_y = 0.0;
    }
}

impl Default for MouseState {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p sa_input`
Expected: All 5 tests PASS.

- [ ] **Step 9: Create InputState and lib.rs**

```rust
// crates/sa_input/src/input_state.rs
use crate::keyboard::KeyboardState;
use crate::mouse::MouseState;

pub struct InputState {
    pub keyboard: KeyboardState,
    pub mouse: MouseState,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keyboard: KeyboardState::new(),
            mouse: MouseState::new(),
        }
    }

    /// Call at end of frame to clear per-frame state (mouse delta).
    pub fn end_frame(&mut self) {
        self.mouse.clear_delta();
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}
```

```rust
// crates/sa_input/src/lib.rs
pub mod keyboard;
pub mod mouse;
pub mod input_state;

pub use input_state::InputState;
pub use keyboard::KeyboardState;
pub use mouse::MouseState;
```

- [ ] **Step 10: Run clippy and all tests**

Run: `cargo clippy -p sa_input -- -D warnings && cargo test -p sa_input`
Expected: Clean, all tests pass.

- [ ] **Step 11: Commit**

```bash
git add crates/sa_input/ Cargo.toml
git commit -m "feat(sa_input): add keyboard and mouse input state tracking"
```

---

### Task 2: sa_render Crate — GPU Context

**Files:**
- Create: `crates/sa_render/Cargo.toml`
- Create: `crates/sa_render/src/lib.rs`
- Create: `crates/sa_render/src/gpu.rs`
- Modify: `Cargo.toml` (workspace root — add sa_render member, bytemuck dep)

- [ ] **Step 1: Add sa_render to workspace**

Add `"crates/sa_render"` to workspace members in root `Cargo.toml`. Add under `[workspace.dependencies]`:
```toml
bytemuck = { version = "1", features = ["derive"] }
sa_render = { path = "crates/sa_render" }
```

Create `crates/sa_render/Cargo.toml`:
```toml
[package]
name = "sa_render"
version.workspace = true
edition.workspace = true

[dependencies]
wgpu.workspace = true
winit.workspace = true
glam.workspace = true
bytemuck.workspace = true
log.workspace = true
sa_core.workspace = true
sa_math.workspace = true
pollster.workspace = true
```

- [ ] **Step 2: Implement GpuContext**

Extract the GPU initialization from main.rs into a reusable struct.

```rust
// crates/sa_render/src/gpu.rs
use std::sync::Arc;
use winit::window::Window;

pub struct GpuContext {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    pub fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("Failed to find a suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SpaceAway Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
            None,
        ))
        .expect("Failed to create GPU device");

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Surface not supported by adapter");
        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }
}
```

- [ ] **Step 3: Create stub lib.rs**

```rust
// crates/sa_render/src/lib.rs
pub mod gpu;

pub use gpu::GpuContext;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p sa_render`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_render/ Cargo.toml
git commit -m "feat(sa_render): add GpuContext extracted from main.rs"
```

---

### Task 3: sa_render — Vertex Type and Mesh Storage

**Files:**
- Create: `crates/sa_render/src/vertex.rs`
- Create: `crates/sa_render/src/mesh.rs`

- [ ] **Step 1: Write failing tests for Vertex**

```rust
// crates/sa_render/src/vertex.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_size() {
        // position (3xf32) + color (3xf32) + normal (3xf32) = 36 bytes
        assert_eq!(std::mem::size_of::<Vertex>(), 36);
    }

    #[test]
    fn vertex_layout_has_three_attributes() {
        let layout = Vertex::layout();
        assert_eq!(layout.attributes.len(), 3);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_render`
Expected: FAIL.

- [ ] **Step 3: Implement Vertex**

```rust
// crates/sa_render/src/vertex.rs
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
}

impl Vertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        static ATTRIBUTES: &[wgpu::VertexAttribute] = &[
            // position
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            // color
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            // normal
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x3,
            },
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRIBUTES,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_render`
Expected: All 2 tests PASS.

- [ ] **Step 5: Implement MeshData and MeshStore**

```rust
// crates/sa_render/src/mesh.rs
use crate::vertex::Vertex;
use sa_core::{Handle, HandleGenerator};
use std::collections::HashMap;

/// CPU-side mesh data before upload.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// A GPU-uploaded mesh.
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

pub struct MeshMarker;

/// Manages GPU mesh buffers.
pub struct MeshStore {
    meshes: HashMap<Handle<MeshMarker>, GpuMesh>,
    handle_gen: HandleGenerator,
}

impl MeshStore {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            handle_gen: HandleGenerator::new(),
        }
    }

    pub fn upload(&mut self, device: &wgpu::Device, data: &MeshData) -> Handle<MeshMarker> {
        use wgpu::util::DeviceExt;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Index Buffer"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let handle = self.handle_gen.next();
        self.meshes.insert(
            handle,
            GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: data.indices.len() as u32,
            },
        );
        handle
    }

    pub fn get(&self, handle: Handle<MeshMarker>) -> Option<&GpuMesh> {
        self.meshes.get(&handle)
    }
}

impl Default for MeshStore {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 6: Update lib.rs**

```rust
// crates/sa_render/src/lib.rs
pub mod gpu;
pub mod vertex;
pub mod mesh;

pub use gpu::GpuContext;
pub use vertex::Vertex;
pub use mesh::{MeshData, MeshStore, MeshMarker};
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p sa_render`
Expected: Compiles.

- [ ] **Step 8: Commit**

```bash
git add crates/sa_render/src/
git commit -m "feat(sa_render): add Vertex type and MeshStore for GPU mesh management"
```

---

### Task 4: sa_render — Camera System

**Files:**
- Create: `crates/sa_render/src/camera.rs`

- [ ] **Step 1: Write failing tests for Camera**

```rust
// crates/sa_render/src/camera.rs
#[cfg(test)]
mod tests {
    use super::*;
    use sa_math::WorldPos;

    #[test]
    fn default_camera_looks_at_negative_z() {
        let cam = Camera::new();
        let forward = cam.forward();
        assert!(forward.z < -0.9); // looking into -Z
        assert!(forward.x.abs() < 0.01);
    }

    #[test]
    fn move_forward_changes_position() {
        let mut cam = Camera::new();
        let before = cam.position;
        cam.move_forward(1.0);
        assert!(cam.position.z < before.z); // moved into -Z
    }

    #[test]
    fn yaw_rotates_horizontally() {
        let mut cam = Camera::new();
        cam.rotate(std::f32::consts::FRAC_PI_2, 0.0); // 90 degrees left
        let forward = cam.forward();
        assert!(forward.x.abs() > 0.9); // now looking along X
    }

    #[test]
    fn pitch_clamped() {
        let mut cam = Camera::new();
        cam.rotate(0.0, 100.0); // way past 90 degrees
        // pitch should be clamped to just under PI/2
        assert!(cam.pitch.abs() < std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn view_projection_is_valid() {
        let cam = Camera::new();
        let vp = cam.view_projection_matrix(16.0 / 9.0);
        // Should not contain NaN
        let cols = vp.to_cols_array();
        for val in cols {
            assert!(!val.is_nan());
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sa_render`
Expected: FAIL — Camera not defined.

- [ ] **Step 3: Implement Camera**

```rust
// crates/sa_render/src/camera.rs
use glam::{Mat4, Vec3};
use sa_math::WorldPos;

pub struct Camera {
    pub position: WorldPos,
    pub yaw: f32,   // radians, 0 = looking into -Z
    pub pitch: f32,  // radians, 0 = horizontal
    pub fov_y: f32,  // radians
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: WorldPos::ORIGIN,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: std::f32::consts::FRAC_PI_4, // 45 degrees
            near: 0.1,
            far: 100_000.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch += delta_pitch;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);
    }

    pub fn move_forward(&mut self, amount: f32) {
        let fwd = self.forward();
        self.position.x += fwd.x as f64 * amount as f64;
        self.position.y += fwd.y as f64 * amount as f64;
        self.position.z += fwd.z as f64 * amount as f64;
    }

    pub fn move_right(&mut self, amount: f32) {
        let r = self.right();
        self.position.x += r.x as f64 * amount as f64;
        self.position.y += r.y as f64 * amount as f64;
        self.position.z += r.z as f64 * amount as f64;
    }

    pub fn move_up(&mut self, amount: f32) {
        self.position.y += amount as f64;
    }

    pub fn view_matrix(&self) -> Mat4 {
        // Camera-relative: always at origin, looking in forward direction
        let forward = self.forward();
        let eye = Vec3::ZERO; // origin-rebased
        Mat4::look_to_rh(eye, forward, Vec3::Y)
    }

    pub fn projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect_ratio, self.near, self.far)
    }

    pub fn view_projection_matrix(&self, aspect_ratio: f32) -> Mat4 {
        self.projection_matrix(aspect_ratio) * self.view_matrix()
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sa_render`
Expected: All 5 camera tests PASS.

- [ ] **Step 5: Update lib.rs**

Add `pub mod camera;` and `pub use camera::Camera;` to `crates/sa_render/src/lib.rs`.

- [ ] **Step 6: Commit**

```bash
git add crates/sa_render/src/
git commit -m "feat(sa_render): add camera system with origin-relative view projection"
```

---

### Task 5: sa_render — Geometry Shader (WGSL)

**Files:**
- Create: `crates/sa_render/src/shaders/geometry.wgsl`

- [ ] **Step 1: Write the geometry shader**

```wgsl
// crates/sa_render/src/shaders/geometry.wgsl

// Uniforms: view-projection matrix and light direction
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec3<f32>,
    _pad: f32,
    light_color: vec3<f32>,
    _pad2: f32,
    ambient: vec3<f32>,
    _pad3: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Per-instance model matrix
struct Instance {
    @location(3) model_0: vec4<f32>,
    @location(4) model_1: vec4<f32>,
    @location(5) model_2: vec4<f32>,
    @location(6) model_3: vec4<f32>,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(vertex: VertexInput, instance: Instance) -> VertexOutput {
    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );

    let world_pos = model * vec4<f32>(vertex.position, 1.0);
    // Normal matrix: upper 3x3 of model (assumes uniform scale)
    let world_normal = normalize((model * vec4<f32>(vertex.normal, 0.0)).xyz);

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * world_pos;
    out.color = vertex.color;
    out.world_normal = world_normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let l = normalize(-uniforms.light_dir); // light points toward surface
    let ndotl = max(dot(n, l), 0.0);
    let diffuse = uniforms.light_color * ndotl;
    let color = in.color * (uniforms.ambient + diffuse);
    return vec4<f32>(color, 1.0);
}
```

- [ ] **Step 2: Commit**

```bash
git add crates/sa_render/src/shaders/
git commit -m "feat(sa_render): add flat-shaded geometry WGSL shader with directional light"
```

---

### Task 6: sa_render — Render Pipeline

**Files:**
- Create: `crates/sa_render/src/pipeline.rs`

- [ ] **Step 1: Implement the render pipeline**

```rust
// crates/sa_render/src/pipeline.rs
use crate::vertex::Vertex;

/// Per-instance data: a 4x4 model matrix stored as 4 vec4 columns.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub model: [[f32; 4]; 4],
}

impl InstanceRaw {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        static ATTRIBUTES: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 48,
                shader_location: 6,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRIBUTES,
        }
    }
}

/// Uniform buffer layout matching the shader.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub _pad: f32,
    pub light_color: [f32; 3],
    pub _pad2: f32,
    pub ambient: [f32; 3],
    pub _pad3: f32,
}

pub struct GeometryPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub depth_texture: wgpu::TextureView,
}

impl GeometryPipeline {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Geometry Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/geometry.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Geometry Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Geometry Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let depth_texture = Self::create_depth_texture(device, width, height);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Geometry Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout(), InstanceRaw::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            bind_group_layout,
            depth_texture,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth_texture = Self::create_depth_texture(device, width, height);
    }

    fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }
}
```

- [ ] **Step 2: Update lib.rs**

Add `pub mod pipeline;` and `pub use pipeline::{GeometryPipeline, InstanceRaw, Uniforms};` to lib.rs.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p sa_render`
Expected: Compiles (shader compiled at build time via include_str!).

- [ ] **Step 4: Commit**

```bash
git add crates/sa_render/src/
git commit -m "feat(sa_render): add geometry render pipeline with depth buffer and instancing"
```

---

### Task 7: sa_render — Star Field Shader and Renderer

**Files:**
- Create: `crates/sa_render/src/shaders/stars.wgsl`
- Create: `crates/sa_render/src/star_field.rs`

- [ ] **Step 1: Write star field shader**

Stars are rendered as points from a vertex buffer of positions + brightness.

```wgsl
// crates/sa_render/src/shaders/stars.wgsl

struct StarUniforms {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: StarUniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,  // direction from origin (unit sphere * distance)
    @location(1) brightness: f32,       // 0.0 - 1.0
    @location(2) color: vec3<f32>,      // star color (warm/cool based on type)
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @builtin(point_size) point_size: f32,
    @location(0) star_color: vec3<f32>,
    @location(1) star_brightness: f32,
};

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    // Stars at infinity: use direction only (w=0 effectively, but we place far away)
    let pos = vec4<f32>(vertex.position * 90000.0, 1.0);

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * pos;
    out.point_size = max(vertex.brightness * 3.0, 1.0);
    out.star_color = vertex.color;
    out.star_brightness = vertex.brightness;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.star_color * in.star_brightness, 1.0);
}
```

- [ ] **Step 2: Write StarField renderer**

```rust
// crates/sa_render/src/star_field.rs
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StarVertex {
    pub position: [f32; 3],
    pub brightness: f32,
    pub color: [f32; 3],
    pub _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StarUniforms {
    pub view_proj: [[f32; 4]; 4],
}

pub struct StarField {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub star_count: u32,
}

impl StarField {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, stars: &[StarVertex]) -> Self {
        use wgpu::util::DeviceExt;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Star Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/stars.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Star Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Star Uniforms"),
            size: std::mem::size_of::<StarUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Star Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Star Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Star Vertices"),
            contents: bytemuck::cast_slice(stars),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Star Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<StarVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::PointList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false, // Stars don't write depth
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            vertex_buffer,
            uniform_buffer,
            bind_group,
            star_count: stars.len() as u32,
        }
    }
}

/// Generate random stars on a unit sphere with varied brightness and color.
pub fn generate_stars(count: u32, seed: u64) -> Vec<StarVertex> {
    let mut stars = Vec::with_capacity(count as usize);
    // Simple deterministic PRNG (xorshift64)
    let mut state = seed.wrapping_add(1);
    let mut rand_f32 = || -> f32 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state as f32) / (u64::MAX as f32)
    };

    for _ in 0..count {
        // Uniform point on sphere via rejection sampling
        let (x, y, z) = loop {
            let x = rand_f32() * 2.0 - 1.0;
            let y = rand_f32() * 2.0 - 1.0;
            let z = rand_f32() * 2.0 - 1.0;
            let len_sq = x * x + y * y + z * z;
            if len_sq > 0.001 && len_sq <= 1.0 {
                let len = len_sq.sqrt();
                break (x / len, y / len, z / len);
            }
        };

        let brightness = rand_f32() * 0.8 + 0.2; // 0.2 - 1.0

        // Star color temperature: warm (red/orange) to cool (blue/white)
        let temp = rand_f32();
        let color = if temp < 0.3 {
            [1.0, 0.85, 0.7] // warm
        } else if temp < 0.7 {
            [1.0, 1.0, 1.0] // white
        } else {
            [0.8, 0.9, 1.0] // cool blue
        };

        stars.push(StarVertex {
            position: [x, y, z],
            brightness,
            color,
            _pad: 0.0,
        });
    }
    stars
}
```

- [ ] **Step 3: Update lib.rs**

Add `pub mod star_field;` and `pub use star_field::{StarField, StarVertex, generate_stars};` to lib.rs.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p sa_render`
Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add crates/sa_render/src/
git commit -m "feat(sa_render): add star field renderer with procedural star generation"
```

---

### Task 8: sa_render — Renderer Orchestrator

**Files:**
- Create: `crates/sa_render/src/renderer.rs`

- [ ] **Step 1: Implement the Renderer**

This orchestrates the full frame: clear → geometry → stars.

```rust
// crates/sa_render/src/renderer.rs
use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::mesh::{GpuMesh, MeshStore, MeshMarker};
use crate::pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
use crate::star_field::{StarField, StarUniforms};
use glam::{Mat4, Vec3};
use sa_core::Handle;

/// A draw command: which mesh to render and where.
pub struct DrawCommand {
    pub mesh: Handle<MeshMarker>,
    pub model_matrix: Mat4,
}

pub struct Renderer {
    pub geometry_pipeline: GeometryPipeline,
    pub star_field: StarField,
    pub mesh_store: MeshStore,
}

impl Renderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let geometry_pipeline = GeometryPipeline::new(
            &gpu.device,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
        );

        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);

        Self {
            geometry_pipeline,
            star_field,
            mesh_store: MeshStore::new(),
        }
    }

    pub fn resize(&mut self, gpu: &GpuContext) {
        self.geometry_pipeline
            .resize(&gpu.device, gpu.config.width, gpu.config.height);
    }

    pub fn render_frame(
        &self,
        gpu: &GpuContext,
        camera: &Camera,
        draw_commands: &[DrawCommand],
        light_dir: Vec3,
    ) {
        let aspect = gpu.aspect_ratio();
        let view_proj = camera.view_projection_matrix(aspect);

        // Update geometry uniforms
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: light_dir.normalize().to_array(),
            _pad: 0.0,
            light_color: [1.0, 0.95, 0.9],
            _pad2: 0.0,
            ambient: [0.02, 0.02, 0.03],
            _pad3: 0.0,
        };
        gpu.queue.write_buffer(
            &self.geometry_pipeline.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        // Update star uniforms (view-proj without translation for skybox effect)
        let star_view = camera.view_matrix();
        let star_vp = camera.projection_matrix(aspect) * star_view;
        let star_uniforms = StarUniforms {
            view_proj: star_vp.to_cols_array_2d(),
        };
        gpu.queue.write_buffer(
            &self.star_field.uniform_buffer,
            0,
            bytemuck::bytes_of(&star_uniforms),
        );

        let frame = match gpu.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        // Render pass: clear + geometry + stars
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.005,
                            g: 0.005,
                            b: 0.015,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.geometry_pipeline.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // Draw geometry
            if !draw_commands.is_empty() {
                pass.set_pipeline(&self.geometry_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                for cmd in draw_commands {
                    if let Some(mesh) = self.mesh_store.get(cmd.mesh) {
                        // Create a temp instance buffer for this draw
                        let instance = InstanceRaw {
                            model: cmd.model_matrix.to_cols_array_2d(),
                        };
                        let instance_buffer = gpu.device.create_buffer_init(
                            &wgpu::util::BufferInitDescriptor {
                                label: Some("Instance Buffer"),
                                contents: bytemuck::bytes_of(&instance),
                                usage: wgpu::BufferUsages::VERTEX,
                            },
                        );

                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, instance_buffer.slice(..));
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                    }
                }
            }

            // Draw stars (after geometry so they appear behind)
            pass.set_pipeline(&self.star_field.pipeline);
            pass.set_bind_group(0, &self.star_field.bind_group, &[]);
            pass.set_vertex_buffer(0, self.star_field.vertex_buffer.slice(..));
            pass.draw(0..self.star_field.star_count, 0..1);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
```

Note: The `create_buffer_init` call above requires `use wgpu::util::DeviceExt;` — make sure this import is present.

- [ ] **Step 2: Update lib.rs with final re-exports**

```rust
// crates/sa_render/src/lib.rs
pub mod camera;
pub mod gpu;
pub mod mesh;
pub mod pipeline;
pub mod renderer;
pub mod star_field;
pub mod vertex;

pub use camera::Camera;
pub use gpu::GpuContext;
pub use mesh::{MeshData, MeshMarker, MeshStore};
pub use pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
pub use renderer::{DrawCommand, Renderer};
pub use star_field::{generate_stars, StarField, StarVertex};
pub use vertex::Vertex;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p sa_render`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add crates/sa_render/src/
git commit -m "feat(sa_render): add Renderer orchestrator for full frame rendering"
```

---

### Task 9: Integration — Update Game Binary

**Files:**
- Modify: `crates/spaceaway/Cargo.toml` (add sa_render, sa_input deps)
- Rewrite: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Update spaceaway Cargo.toml**

Add to `[dependencies]`:
```toml
sa_render.workspace = true
sa_input.workspace = true
glam.workspace = true
bytemuck.workspace = true
```

- [ ] **Step 2: Rewrite main.rs**

Replace the entire main.rs with the new version that uses sa_render and sa_input. Creates a test scene with a few colored cubes and a flying camera.

```rust
// crates/spaceaway/src/main.rs
use glam::{Mat4, Vec3};
use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use sa_input::InputState;
use sa_render::{Camera, DrawCommand, GpuContext, MeshData, Renderer, Vertex};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{CursorGrabMode, Window, WindowId};

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    camera: Camera,
    input: InputState,
    world: GameWorld,
    events: EventBus,
    time: FrameTime,
    schedule: Schedule,
    last_frame: Instant,
    cube_mesh: Option<sa_core::Handle<sa_render::MeshMarker>>,
    cursor_grabbed: bool,
}

impl App {
    fn new() -> Self {
        let mut camera = Camera::new();
        camera.position = sa_math::WorldPos::new(0.0, 2.0, 10.0);

        Self {
            window: None,
            gpu: None,
            renderer: None,
            camera,
            input: InputState::new(),
            world: GameWorld::new(),
            events: EventBus::new(),
            time: FrameTime::new(),
            schedule: Schedule::new(),
            last_frame: Instant::now(),
            cube_mesh: None,
            cursor_grabbed: false,
        }
    }

    fn setup_scene(&mut self) {
        let renderer = self.renderer.as_mut().unwrap();
        let gpu = self.gpu.as_ref().unwrap();

        // Create a simple cube mesh
        let mesh_data = make_cube();
        let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
        self.cube_mesh = Some(handle);
    }

    fn update(&mut self) {
        let dt = self.time.delta_seconds() as f32;
        let speed = 10.0 * dt;

        // Camera movement
        if self.input.keyboard.is_pressed(KeyCode::KeyW) {
            self.camera.move_forward(speed);
        }
        if self.input.keyboard.is_pressed(KeyCode::KeyS) {
            self.camera.move_forward(-speed);
        }
        if self.input.keyboard.is_pressed(KeyCode::KeyA) {
            self.camera.move_right(-speed);
        }
        if self.input.keyboard.is_pressed(KeyCode::KeyD) {
            self.camera.move_right(speed);
        }
        if self.input.keyboard.is_pressed(KeyCode::Space) {
            self.camera.move_up(speed);
        }
        if self.input.keyboard.is_pressed(KeyCode::ShiftLeft) {
            self.camera.move_up(-speed);
        }

        // Mouse look
        let (dx, dy) = self.input.mouse.delta();
        let sensitivity = 0.003;
        self.camera.rotate(-dx * sensitivity, -dy * sensitivity);
    }

    fn grab_cursor(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
            self.cursor_grabbed = true;
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("SpaceAway")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());

            let gpu = GpuContext::new(window.clone());
            let renderer = Renderer::new(&gpu);

            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.window = Some(window);
            self.last_frame = Instant::now();
            self.setup_scene();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    self.input.keyboard.set_pressed(code, event.state.is_pressed());

                    // Escape to release cursor
                    if code == KeyCode::Escape && event.state.is_pressed() {
                        if let Some(window) = &self.window {
                            let _ = window.set_cursor_grab(CursorGrabMode::None);
                            window.set_cursor_visible(true);
                            self.cursor_grabbed = false;
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, .. } => {
                if state.is_pressed() && !self.cursor_grabbed {
                    self.grab_cursor();
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size.width, new_size.height);
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(gpu);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed = now - self.last_frame;
                self.last_frame = now;
                self.time.advance(elapsed);

                self.schedule.run(&mut self.world, &mut self.events, &self.time);
                self.update();

                // Build draw commands: a few cubes at different positions
                if let (Some(gpu), Some(renderer), Some(cube)) =
                    (&self.gpu, &self.renderer, self.cube_mesh)
                {
                    let commands = vec![
                        DrawCommand {
                            mesh: cube,
                            model_matrix: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0)),
                        },
                        DrawCommand {
                            mesh: cube,
                            model_matrix: Mat4::from_translation(Vec3::new(5.0, 0.0, -3.0)),
                        },
                        DrawCommand {
                            mesh: cube,
                            model_matrix: Mat4::from_translation(Vec3::new(-4.0, 1.0, -6.0))
                                * Mat4::from_rotation_y(0.5),
                        },
                        DrawCommand {
                            mesh: cube,
                            model_matrix: Mat4::from_translation(Vec3::new(2.0, -1.0, -10.0))
                                * Mat4::from_scale(Vec3::splat(2.0)),
                        },
                    ];

                    // Light from upper-right, slightly behind
                    let light_dir = Vec3::new(0.5, -0.8, -0.3);
                    renderer.render_frame(gpu, &self.camera, &commands, light_dir);
                }

                self.events.flush();
                self.input.end_frame();

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.cursor_grabbed {
                self.input.mouse.accumulate_delta(delta.0 as f32, delta.1 as f32);
            }
        }
    }
}

/// Generate a unit cube with flat-shaded faces and per-face colors.
fn make_cube() -> MeshData {
    let faces: &[([f32; 3], [f32; 3], [[f32; 3]; 4])] = &[
        // (normal, color, vertices)
        ([0.0, 0.0, 1.0],  [0.6, 0.6, 0.7], [[-1.0,-1.0, 1.0],[ 1.0,-1.0, 1.0],[ 1.0, 1.0, 1.0],[-1.0, 1.0, 1.0]]),  // front
        ([0.0, 0.0,-1.0],  [0.5, 0.5, 0.6], [[ 1.0,-1.0,-1.0],[-1.0,-1.0,-1.0],[-1.0, 1.0,-1.0],[ 1.0, 1.0,-1.0]]),  // back
        ([0.0, 1.0, 0.0],  [0.7, 0.7, 0.8], [[-1.0, 1.0, 1.0],[ 1.0, 1.0, 1.0],[ 1.0, 1.0,-1.0],[-1.0, 1.0,-1.0]]),  // top
        ([0.0,-1.0, 0.0],  [0.4, 0.4, 0.5], [[-1.0,-1.0,-1.0],[ 1.0,-1.0,-1.0],[ 1.0,-1.0, 1.0],[-1.0,-1.0, 1.0]]),  // bottom
        ([1.0, 0.0, 0.0],  [0.55, 0.55, 0.65], [[ 1.0,-1.0, 1.0],[ 1.0,-1.0,-1.0],[ 1.0, 1.0,-1.0],[ 1.0, 1.0, 1.0]]),  // right
        ([-1.0, 0.0, 0.0], [0.5, 0.5, 0.6], [[-1.0,-1.0,-1.0],[-1.0,-1.0, 1.0],[-1.0, 1.0, 1.0],[-1.0, 1.0,-1.0]]),  // left
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for (normal, color, verts) in faces {
        let base = vertices.len() as u32;
        for v in verts {
            vertices.push(Vertex {
                position: *v,
                color: *color,
                normal: *normal,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    MeshData { vertices, indices }
}

fn main() {
    env_logger::init();
    log::info!("SpaceAway starting...");

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p spaceaway`
Expected: Compiles.

- [ ] **Step 4: Run and verify visually**

Run: `cargo run -p spaceaway`
Expected: Window opens showing:
- Near-black void background
- Stars visible as points in the sky
- Several gray cubes lit by a directional light
- WASD + mouse look to fly around (click to capture mouse, Escape to release)
- Space/Shift for up/down

- [ ] **Step 5: Run clippy on entire workspace**

Run: `cargo clippy --workspace -- -D warnings`
Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add crates/spaceaway/
git commit -m "feat(spaceaway): integrate sa_render and sa_input with flyable camera and test scene"
```

---

### Task 10: Final Verification

- [ ] **Step 1: Run cargo check**

Run: `cargo check --workspace`
Expected: Clean.

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: Clean.

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass (30 from Phase 1 + new sa_input + sa_render tests).

- [ ] **Step 4: Run the game**

Run: `cargo run -p spaceaway`
Expected: Flyable camera, lit cubes, star field. Close with window X.
