# Phase 5a: Ship Core --- Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the ship flyable. Create the `sa_ship` crate with interactable objects (levers, buttons, switches, screens), a helm seated mode for piloting, and Newtonian flight physics. The player walks inside the ship cockpit, physically interacts with controls, sits at the helm, and flies through space. All ship functions are controlled by in-world interactables --- no keyboard shortcuts for ship systems.

**Architecture:** New `sa_ship` crate at the Game Logic layer. Five modules: `interactable.rs` (component types and state), `interaction.rs` (raycast system and drag state machine), `ship.rs` (ship entity with physics body), `helm.rs` (seated mode and flight controls), `station.rs` (station definitions and interactable placement data). Interactable meshes are generated in `sa_meshgen`. The game scene changes from ground-plane-with-cubes to ship-in-space.

**Key Design Decisions:**
- Phase 5a simplification: the ship is STATIC while the player walks inside. It only moves when someone is seated at the helm. This avoids the reference-frame problem of walking inside a moving ship (Phase 5b).
- Physics world uses zero gravity. Player gets simulated mag-boot gravity (downward force relative to ship local Y) applied each frame.
- Interactable colliders are SENSORS (not solid) --- they detect raycasts but do not block player movement.
- The `sa_input` `MouseState` currently only tracks delta. We need to add left-button-pressed tracking for click/drag detection.
- Rapier3d `QueryPipeline` is used for raycasting. It must be added to `PhysicsWorld` and updated each frame.

**Tech Stack:** Rust, rapier3d 0.22 (QueryPipeline, Ray, sensor colliders), glam, sa_physics, sa_input, sa_ecs, sa_meshgen

---

## File Structure

```
crates/sa_ship/
+-- Cargo.toml
+-- src/
    +-- lib.rs              # Module declarations, re-exports
    +-- interactable.rs     # InteractableKind, Interactable, state management
    +-- interaction.rs      # InteractionSystem, raycast, drag state machine
    +-- ship.rs             # Ship struct, thrust/rcs/torque application
    +-- helm.rs             # HelmState, HelmController, seated flight controls
    +-- station.rs          # Station enum, StationConfig, cockpit layout data

crates/sa_meshgen/src/
    +-- interactables.rs    # NEW: lever_mesh, button_mesh, switch_mesh, screen_mesh, helm_seat_mesh

crates/sa_input/src/
    +-- mouse.rs            # MODIFIED: add left_pressed tracking

crates/sa_physics/src/
    +-- world.rs            # MODIFIED: add QueryPipeline, update_query_pipeline(), cast_ray()

crates/spaceaway/src/
    +-- main.rs             # MODIFIED: ship scene, interaction loop, helm integration
    +-- ship_setup.rs       # NEW: ship scene construction extracted from main.rs
```

---

### Task 1: sa_ship Crate Setup + Interactable Types

**Files:**
- Modify: `Cargo.toml` (workspace root --- add sa_ship member and dependency)
- Create: `crates/sa_ship/Cargo.toml`
- Create: `crates/sa_ship/src/lib.rs`
- Create: `crates/sa_ship/src/interactable.rs`

- [ ] **Step 1: Add sa_ship to workspace**

Modify root `Cargo.toml`. Add `"crates/sa_ship"` to workspace members (before `"crates/spaceaway"`). Add under `[workspace.dependencies]`:

```toml
sa_ship = { path = "crates/sa_ship" }
```

The members array becomes:
```toml
members = [
    "crates/sa_core",
    "crates/sa_math",
    "crates/sa_ecs",
    "crates/sa_input",
    "crates/sa_render",
    "crates/sa_physics",
    "crates/sa_player",
    "crates/sa_universe",
    "crates/sa_meshgen",
    "crates/sa_ship",
    "crates/spaceaway",
]
```

- [ ] **Step 2: Create sa_ship Cargo.toml**

Create `crates/sa_ship/Cargo.toml`:
```toml
[package]
name = "sa_ship"
version.workspace = true
edition.workspace = true

[dependencies]
sa_core.workspace = true
sa_math.workspace = true
sa_physics.workspace = true
sa_ecs.workspace = true
sa_input.workspace = true
rapier3d.workspace = true
glam.workspace = true
nalgebra.workspace = true
```

- [ ] **Step 3: Create lib.rs**

Create `crates/sa_ship/src/lib.rs`:
```rust
pub mod interactable;
pub mod interaction;
pub mod ship;
pub mod helm;
pub mod station;

pub use interactable::{Interactable, InteractableKind, ButtonMode};
pub use interaction::InteractionSystem;
pub use ship::Ship;
pub use helm::{HelmState, HelmController};
pub use station::{Station, StationConfig};
```

- [ ] **Step 4: Create interactable.rs with types and state**

Create `crates/sa_ship/src/interactable.rs`:
```rust
use rapier3d::prelude::ColliderHandle;

/// The kind of interactable object and its state.
#[derive(Clone, Debug)]
pub enum InteractableKind {
    /// Analog control, click-and-drag. position: 0.0 (bottom) to 1.0 (top).
    Lever { position: f32 },

    /// Discrete click. Momentary = active while held, Toggle = click on/off.
    Button { pressed: bool, mode: ButtonMode },

    /// Multi-position, click to cycle. Wraps around.
    Switch { position: u8, num_positions: u8 },

    /// Display only. No interaction.
    Screen { text_lines: Vec<String> },

    /// Helm seat. Click to enter seated mode.
    HelmSeat,
}

/// Button behavior mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonMode {
    /// Active only while mouse is held down.
    Momentary,
    /// Toggles on/off with each click.
    Toggle,
}

/// An interactable object in the ship.
#[derive(Clone, Debug)]
pub struct Interactable {
    pub kind: InteractableKind,
    /// The sensor collider used for raycast detection.
    pub collider_handle: ColliderHandle,
    /// Human-readable label for UI hints.
    pub label: String,
}

impl Interactable {
    /// Create a new lever interactable at position 0.0.
    pub fn lever(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Lever { position: 0.0 },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new toggle button interactable.
    pub fn toggle_button(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Button {
                pressed: false,
                mode: ButtonMode::Toggle,
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new momentary button interactable.
    pub fn momentary_button(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Button {
                pressed: false,
                mode: ButtonMode::Momentary,
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new switch interactable.
    pub fn switch(
        collider_handle: ColliderHandle,
        num_positions: u8,
        label: impl Into<String>,
    ) -> Self {
        Self {
            kind: InteractableKind::Switch {
                position: 0,
                num_positions,
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a new screen interactable.
    pub fn screen(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::Screen {
                text_lines: Vec::new(),
            },
            collider_handle,
            label: label.into(),
        }
    }

    /// Create a helm seat interactable.
    pub fn helm_seat(collider_handle: ColliderHandle, label: impl Into<String>) -> Self {
        Self {
            kind: InteractableKind::HelmSeat,
            collider_handle,
            label: label.into(),
        }
    }

    // --- State manipulation ---

    /// Set lever position, clamped to 0.0..=1.0. No-op if not a lever.
    pub fn set_lever_position(&mut self, pos: f32) {
        if let InteractableKind::Lever { position } = &mut self.kind {
            *position = pos.clamp(0.0, 1.0);
        }
    }

    /// Get lever position. Returns None if not a lever.
    pub fn lever_position(&self) -> Option<f32> {
        if let InteractableKind::Lever { position } = &self.kind {
            Some(*position)
        } else {
            None
        }
    }

    /// Press a button. For Toggle: flips state. For Momentary: sets pressed = true.
    pub fn press_button(&mut self) {
        if let InteractableKind::Button { pressed, mode } = &mut self.kind {
            match mode {
                ButtonMode::Toggle => *pressed = !*pressed,
                ButtonMode::Momentary => *pressed = true,
            }
        }
    }

    /// Release a button. For Momentary: sets pressed = false. Toggle: no-op.
    pub fn release_button(&mut self) {
        if let InteractableKind::Button { pressed, mode } = &mut self.kind {
            if *mode == ButtonMode::Momentary {
                *pressed = false;
            }
        }
    }

    /// Get button pressed state. Returns None if not a button.
    pub fn is_button_pressed(&self) -> Option<bool> {
        if let InteractableKind::Button { pressed, .. } = &self.kind {
            Some(*pressed)
        } else {
            None
        }
    }

    /// Advance switch to next position (wraps around). No-op if not a switch.
    pub fn cycle_switch(&mut self) {
        if let InteractableKind::Switch {
            position,
            num_positions,
        } = &mut self.kind
        {
            *position = (*position + 1) % *num_positions;
        }
    }

    /// Get switch position. Returns None if not a switch.
    pub fn switch_position(&self) -> Option<u8> {
        if let InteractableKind::Switch { position, .. } = &self.kind {
            Some(*position)
        } else {
            None
        }
    }

    /// Update screen text lines. No-op if not a screen.
    pub fn set_screen_text(&mut self, lines: Vec<String>) {
        if let InteractableKind::Screen { text_lines } = &mut self.kind {
            *text_lines = lines;
        }
    }

    /// Returns true if this is a helm seat.
    pub fn is_helm_seat(&self) -> bool {
        matches!(self.kind, InteractableKind::HelmSeat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::ColliderHandle;

    /// Helper: create a dummy collider handle for testing.
    fn dummy_handle() -> ColliderHandle {
        // ColliderHandle is an Index; use from_raw_parts with generation 0
        ColliderHandle::from_raw_parts(0, 0)
    }

    #[test]
    fn lever_position_clamped_low() {
        let mut lever = Interactable::lever(dummy_handle(), "throttle");
        lever.set_lever_position(-0.5);
        assert_eq!(lever.lever_position(), Some(0.0));
    }

    #[test]
    fn lever_position_clamped_high() {
        let mut lever = Interactable::lever(dummy_handle(), "throttle");
        lever.set_lever_position(1.5);
        assert_eq!(lever.lever_position(), Some(1.0));
    }

    #[test]
    fn lever_position_normal() {
        let mut lever = Interactable::lever(dummy_handle(), "throttle");
        lever.set_lever_position(0.75);
        assert!((lever.lever_position().unwrap() - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn lever_initial_position_zero() {
        let lever = Interactable::lever(dummy_handle(), "throttle");
        assert_eq!(lever.lever_position(), Some(0.0));
    }

    #[test]
    fn toggle_button_flips() {
        let mut btn = Interactable::toggle_button(dummy_handle(), "engine");
        assert_eq!(btn.is_button_pressed(), Some(false));
        btn.press_button();
        assert_eq!(btn.is_button_pressed(), Some(true));
        btn.press_button();
        assert_eq!(btn.is_button_pressed(), Some(false));
    }

    #[test]
    fn toggle_button_release_is_noop() {
        let mut btn = Interactable::toggle_button(dummy_handle(), "engine");
        btn.press_button(); // true
        btn.release_button(); // should stay true (toggle ignores release)
        assert_eq!(btn.is_button_pressed(), Some(true));
    }

    #[test]
    fn momentary_button_held() {
        let mut btn = Interactable::momentary_button(dummy_handle(), "fire");
        btn.press_button();
        assert_eq!(btn.is_button_pressed(), Some(true));
        btn.release_button();
        assert_eq!(btn.is_button_pressed(), Some(false));
    }

    #[test]
    fn switch_cycles_through_positions() {
        let mut sw = Interactable::switch(dummy_handle(), 3, "mode");
        assert_eq!(sw.switch_position(), Some(0));
        sw.cycle_switch();
        assert_eq!(sw.switch_position(), Some(1));
        sw.cycle_switch();
        assert_eq!(sw.switch_position(), Some(2));
        sw.cycle_switch();
        assert_eq!(sw.switch_position(), Some(0)); // wraps
    }

    #[test]
    fn screen_text_update() {
        let mut screen = Interactable::screen(dummy_handle(), "speed");
        screen.set_screen_text(vec!["Speed: 0 m/s".into()]);
        if let InteractableKind::Screen { text_lines } = &screen.kind {
            assert_eq!(text_lines.len(), 1);
            assert_eq!(text_lines[0], "Speed: 0 m/s");
        } else {
            panic!("expected screen");
        }
    }

    #[test]
    fn helm_seat_detected() {
        let seat = Interactable::helm_seat(dummy_handle(), "helm");
        assert!(seat.is_helm_seat());
    }

    #[test]
    fn lever_position_on_non_lever_returns_none() {
        let btn = Interactable::toggle_button(dummy_handle(), "x");
        assert_eq!(btn.lever_position(), None);
    }

    #[test]
    fn button_pressed_on_non_button_returns_none() {
        let lever = Interactable::lever(dummy_handle(), "x");
        assert_eq!(lever.is_button_pressed(), None);
    }
}
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check -p sa_ship 2>&1 | head -20
cargo test -p sa_ship 2>&1 | tail -20
```

All 12 tests in interactable.rs should pass. The other modules (interaction, ship, helm, station) will be created as stubs so lib.rs compiles --- create them as empty files for now:

Create `crates/sa_ship/src/interaction.rs`:
```rust
//! Raycast interaction system. Implemented in Task 2.

pub struct InteractionSystem;
```

Create `crates/sa_ship/src/ship.rs`:
```rust
//! Ship entity. Implemented in Task 3.

pub struct Ship;
```

Create `crates/sa_ship/src/helm.rs`:
```rust
//! Helm seated mode. Implemented in Task 4.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelmState {
    Standing,
    Seated,
}

pub struct HelmController;
```

Create `crates/sa_ship/src/station.rs`:
```rust
//! Station definitions. Implemented in Task 5.

pub enum Station {}
pub struct StationConfig;
```

---

### Task 2: Interaction System (Raycast + Drag State Machine)

**Depends on:** Task 1

**Files:**
- Modify: `crates/sa_physics/src/world.rs` (add QueryPipeline + cast_ray)
- Modify: `crates/sa_physics/src/lib.rs` (re-export new items)
- Modify: `crates/sa_input/src/mouse.rs` (add left_pressed tracking)
- Replace: `crates/sa_ship/src/interaction.rs`

- [ ] **Step 1: Add QueryPipeline to PhysicsWorld**

Modify `crates/sa_physics/src/world.rs`. Add a `query_pipeline: QueryPipeline` field to `PhysicsWorld`, initialize it in constructors, add `update_query_pipeline()` method that must be called after `step()`, and add `cast_ray()`:

```rust
use rapier3d::prelude::*;

pub struct PhysicsWorld {
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    gravity: nalgebra::Vector3<f32>,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
}
```

Add to both `new()` and `with_gravity()`:
```rust
query_pipeline: QueryPipeline::new(),
```

Add method to update the query pipeline (must be called after `step()`):
```rust
/// Updates the query pipeline for raycasting. Call after `step()`.
pub fn update_query_pipeline(&mut self) {
    self.query_pipeline.update(&self.collider_set);
}
```

Add the `cast_ray` method:
```rust
/// Cast a ray and return the first hit collider handle and distance.
/// `filter` controls which colliders are considered (e.g., sensors only).
/// Returns None if no hit within `max_toi`.
pub fn cast_ray(
    &self,
    origin: nalgebra::Point3<f32>,
    direction: nalgebra::Vector3<f32>,
    max_toi: f32,
    solid: bool,
    filter: QueryFilter,
) -> Option<(ColliderHandle, f32)> {
    let ray = Ray::new(origin, direction);
    self.query_pipeline.cast_ray(
        &self.rigid_body_set,
        &self.collider_set,
        &ray,
        max_toi,
        solid,
        filter,
    )
}
```

- [ ] **Step 2: Re-export new types from sa_physics**

Modify `crates/sa_physics/src/lib.rs` to add:
```rust
pub use rapier3d::prelude::{QueryFilter, Ray, ColliderHandle};
```

Note: `ColliderHandle` may already be exported --- check. The key new export is `QueryFilter`.

- [ ] **Step 3: Add left button tracking to MouseState**

Modify `crates/sa_input/src/mouse.rs`:

```rust
pub struct MouseState {
    delta_x: f32,
    delta_y: f32,
    left_pressed: bool,
    left_just_pressed: bool,
    left_just_released: bool,
}

impl MouseState {
    pub fn new() -> Self {
        Self {
            delta_x: 0.0,
            delta_y: 0.0,
            left_pressed: false,
            left_just_pressed: false,
            left_just_released: false,
        }
    }

    pub fn accumulate_delta(&mut self, dx: f32, dy: f32) {
        self.delta_x += dx;
        self.delta_y += dy;
    }

    pub fn delta(&self) -> (f32, f32) { (self.delta_x, self.delta_y) }

    pub fn clear_delta(&mut self) {
        self.delta_x = 0.0;
        self.delta_y = 0.0;
        self.left_just_pressed = false;
        self.left_just_released = false;
    }

    /// Call when left mouse button state changes.
    pub fn set_left_pressed(&mut self, pressed: bool) {
        if pressed && !self.left_pressed {
            self.left_just_pressed = true;
        }
        if !pressed && self.left_pressed {
            self.left_just_released = true;
        }
        self.left_pressed = pressed;
    }

    pub fn left_pressed(&self) -> bool { self.left_pressed }
    pub fn left_just_pressed(&self) -> bool { self.left_just_pressed }
    pub fn left_just_released(&self) -> bool { self.left_just_released }
}
```

Update the existing tests to still pass, and add new tests:
```rust
#[test]
fn left_button_just_pressed() {
    let mut mouse = MouseState::new();
    assert!(!mouse.left_pressed());
    assert!(!mouse.left_just_pressed());
    mouse.set_left_pressed(true);
    assert!(mouse.left_pressed());
    assert!(mouse.left_just_pressed());
    mouse.clear_delta();
    assert!(!mouse.left_just_pressed()); // cleared
    assert!(mouse.left_pressed()); // still held
}

#[test]
fn left_button_just_released() {
    let mut mouse = MouseState::new();
    mouse.set_left_pressed(true);
    mouse.clear_delta();
    mouse.set_left_pressed(false);
    assert!(!mouse.left_pressed());
    assert!(mouse.left_just_released());
}
```

**IMPORTANT:** Also wire up `set_left_pressed` in `main.rs` `WindowEvent::MouseInput`. In the existing handler:
```rust
WindowEvent::MouseInput { state, button, .. } => {
    if button == winit::event::MouseButton::Left {
        self.input.mouse.set_left_pressed(state.is_pressed());
    }
    if state.is_pressed() && !self.cursor_grabbed {
        self.grab_cursor();
    }
}
```

- [ ] **Step 4: Implement InteractionSystem**

Replace `crates/sa_ship/src/interaction.rs`:

```rust
//! Raycast-based interaction system for ship interactables.
//!
//! Each frame: cast a ray from camera forward, detect hover, handle
//! click/drag for levers, click for buttons/switches, click for helm seat.

use crate::interactable::Interactable;
use rapier3d::prelude::*;
use std::collections::HashMap;

/// Unique identifier for an interactable (index into the interactables vec).
pub type InteractableId = usize;

/// The drag state machine for lever interaction.
#[derive(Clone, Debug)]
enum DragState {
    /// Not dragging anything.
    Idle,
    /// Currently dragging a lever. Stores the lever ID and its position
    /// at the start of the drag.
    Dragging {
        target: InteractableId,
        start_position: f32,
    },
}

/// The interaction system manages raycasting and interactable state.
pub struct InteractionSystem {
    /// All registered interactables, keyed by ID.
    interactables: Vec<Interactable>,
    /// Map from collider handle to interactable ID for fast lookup after raycast.
    collider_to_id: HashMap<ColliderHandle, InteractableId>,
    /// Currently hovered interactable (if any).
    hovered: Option<InteractableId>,
    /// Current drag state.
    drag: DragState,
    /// Maximum raycast distance in meters.
    max_range: f32,
    /// Mouse Y sensitivity for lever dragging (position change per pixel).
    lever_sensitivity: f32,
}

impl InteractionSystem {
    pub fn new() -> Self {
        Self {
            interactables: Vec::new(),
            collider_to_id: HashMap::new(),
            hovered: None,
            drag: DragState::Idle,
            max_range: 1.5,
            lever_sensitivity: 0.003,
        }
    }

    /// Register an interactable and return its ID.
    pub fn register(&mut self, interactable: Interactable) -> InteractableId {
        let id = self.interactables.len();
        self.collider_to_id
            .insert(interactable.collider_handle, id);
        self.interactables.push(interactable);
        id
    }

    /// Get a reference to an interactable by ID.
    pub fn get(&self, id: InteractableId) -> Option<&Interactable> {
        self.interactables.get(id)
    }

    /// Get a mutable reference to an interactable by ID.
    pub fn get_mut(&mut self, id: InteractableId) -> Option<&mut Interactable> {
        self.interactables.get_mut(id)
    }

    /// Currently hovered interactable ID.
    pub fn hovered(&self) -> Option<InteractableId> {
        self.hovered
    }

    /// Returns true if currently dragging a lever.
    pub fn is_dragging(&self) -> bool {
        matches!(self.drag, DragState::Dragging { .. })
    }

    /// Update the interaction system. Call once per frame.
    ///
    /// - `ray_origin`: camera eye position (world space)
    /// - `ray_dir`: camera forward direction (normalized)
    /// - `mouse_dy`: mouse Y delta this frame (pixels)
    /// - `left_just_pressed`: true on the frame left mouse button was pressed
    /// - `left_pressed`: true while left mouse button is held
    /// - `left_just_released`: true on the frame left mouse button was released
    /// - `physics`: physics world (for raycasting)
    ///
    /// Returns `Some(InteractableId)` if a helm seat was clicked (caller
    /// should enter seated mode).
    pub fn update(
        &mut self,
        ray_origin: [f32; 3],
        ray_dir: [f32; 3],
        mouse_dy: f32,
        left_just_pressed: bool,
        left_pressed: bool,
        left_just_released: bool,
        physics: &sa_physics::PhysicsWorld,
    ) -> Option<InteractableId> {
        let mut helm_seat_clicked = None;

        // --- Raycast for hover ---
        let origin = nalgebra::Point3::new(ray_origin[0], ray_origin[1], ray_origin[2]);
        let direction = nalgebra::Vector3::new(ray_dir[0], ray_dir[1], ray_dir[2]);

        // Only hit sensors (interactable colliders are sensors)
        let filter = QueryFilter::default().predicate(&|_handle, collider: &Collider| {
            collider.is_sensor()
        });

        let hit = physics.cast_ray(origin, direction, self.max_range, true, filter);

        self.hovered = hit.and_then(|(collider_handle, _toi)| {
            self.collider_to_id.get(&collider_handle).copied()
        });

        // --- Handle drag state machine ---
        match &self.drag {
            DragState::Idle => {
                if left_just_pressed {
                    if let Some(id) = self.hovered {
                        let interactable = &self.interactables[id];
                        match &interactable.kind {
                            crate::interactable::InteractableKind::Lever { position } => {
                                self.drag = DragState::Dragging {
                                    target: id,
                                    start_position: *position,
                                };
                            }
                            crate::interactable::InteractableKind::Button { .. } => {
                                self.interactables[id].press_button();
                            }
                            crate::interactable::InteractableKind::Switch { .. } => {
                                self.interactables[id].cycle_switch();
                            }
                            crate::interactable::InteractableKind::HelmSeat => {
                                helm_seat_clicked = Some(id);
                            }
                            crate::interactable::InteractableKind::Screen { .. } => {
                                // Screens are not interactive
                            }
                        }
                    }
                }
            }
            DragState::Dragging { target, .. } => {
                let target_id = *target;
                if left_pressed {
                    // Update lever position from mouse Y delta.
                    // Negative dy = mouse moves up = lever goes up = position increases.
                    if let Some(interactable) = self.interactables.get_mut(target_id) {
                        let current = interactable.lever_position().unwrap_or(0.0);
                        let new_pos = current - mouse_dy * self.lever_sensitivity;
                        interactable.set_lever_position(new_pos);
                    }
                }
                if left_just_released || !left_pressed {
                    // Release: momentary buttons depress, lever stays
                    self.drag = DragState::Idle;
                }
            }
        }

        // Release momentary buttons when mouse is released (even if not dragging)
        if left_just_released {
            for interactable in &mut self.interactables {
                interactable.release_button();
            }
        }

        helm_seat_clicked
    }
}

impl Default for InteractionSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactable::Interactable;

    fn dummy_handle(idx: u32) -> ColliderHandle {
        ColliderHandle::from_raw_parts(idx, 0)
    }

    #[test]
    fn register_and_get() {
        let mut sys = InteractionSystem::new();
        let lever = Interactable::lever(dummy_handle(0), "throttle");
        let id = sys.register(lever);
        assert!(sys.get(id).is_some());
        assert_eq!(sys.get(id).unwrap().label, "throttle");
    }

    #[test]
    fn drag_delta_maps_to_lever_position() {
        let mut sys = InteractionSystem::new();
        let lever = Interactable::lever(dummy_handle(0), "throttle");
        let id = sys.register(lever);

        // Simulate: start drag, then drag mouse up (negative dy)
        sys.hovered = Some(id);
        sys.drag = DragState::Dragging {
            target: id,
            start_position: 0.0,
        };

        // Drag mouse up by 100 pixels -> position increases by 100 * 0.003 = 0.3
        let physics = sa_physics::PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        sys.update(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            -100.0, // mouse moved up
            false,   // not just pressed
            true,    // held
            false,   // not just released
            &physics,
        );

        let pos = sys.get(id).unwrap().lever_position().unwrap();
        assert!(
            (pos - 0.3).abs() < 0.01,
            "lever should be at ~0.3 after dragging up 100px, got {pos}"
        );
    }

    #[test]
    fn lever_position_clamped_during_drag() {
        let mut sys = InteractionSystem::new();
        let mut lever = Interactable::lever(dummy_handle(0), "throttle");
        lever.set_lever_position(0.95);
        let id = sys.register(lever);

        sys.drag = DragState::Dragging {
            target: id,
            start_position: 0.95,
        };

        let physics = sa_physics::PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        // Drag way up -> should clamp to 1.0
        sys.update(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            -1000.0,
            false,
            true,
            false,
            &physics,
        );

        let pos = sys.get(id).unwrap().lever_position().unwrap();
        assert!(
            (pos - 1.0).abs() < f32::EPSILON,
            "lever should clamp to 1.0, got {pos}"
        );
    }
}
```

- [ ] **Step 5: Verify**

```bash
cargo test -p sa_physics 2>&1 | tail -10
cargo test -p sa_input 2>&1 | tail -10
cargo test -p sa_ship 2>&1 | tail -10
cargo check -p spaceaway 2>&1 | head -20
```

---

### Task 3: Ship Entity

**Depends on:** Task 1

**Files:**
- Replace: `crates/sa_ship/src/ship.rs`

- [ ] **Step 1: Implement Ship struct**

Replace `crates/sa_ship/src/ship.rs`:

```rust
//! Ship entity: physics body, propulsion state, force application.

use rapier3d::prelude::*;
use sa_physics::PhysicsWorld;

/// Ship physics and propulsion state.
pub struct Ship {
    /// The rapier3d rigid body handle for the ship.
    pub body_handle: RigidBodyHandle,
    /// Current throttle level (0.0 to 1.0), set by the thrust lever.
    pub throttle: f32,
    /// Whether the engine is on, set by the engine button.
    pub engine_on: bool,
    /// Maximum forward thrust in Newtons.
    pub max_thrust: f32,
    /// Maximum lateral/vertical thrust (RCS) in Newtons.
    pub max_rcs_thrust: f32,
    /// Maximum rotational torque in Nm.
    pub max_torque: f32,
}

impl Ship {
    /// Ship physics constants.
    pub const MASS: f32 = 50_000.0;
    pub const DEFAULT_MAX_THRUST: f32 = 500_000.0;
    pub const DEFAULT_MAX_RCS: f32 = 50_000.0;
    pub const DEFAULT_MAX_TORQUE: f32 = 100_000.0;

    /// Create a new ship and register its rigid body in the physics world.
    /// The ship spawns at the given position with zero velocity.
    /// The body has no gravity, no linear damping, and no angular damping
    /// (Newtonian behavior --- momentum persists until countered).
    pub fn new(physics: &mut PhysicsWorld, x: f32, y: f32, z: f32) -> Self {
        let body = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector3::new(x, y, z))
            .additional_mass(Self::MASS)
            .gravity_scale(0.0) // No gravity on the ship (space)
            .linear_damping(0.0) // No drag
            .angular_damping(0.5) // Slight angular damping for controllability
            .ccd_enabled(true)
            .build();
        let body_handle = physics.add_rigid_body(body);

        // Attach a large box collider for the ship hull (rough approximation).
        // Width ~5m, height ~3m, length ~30m. Center offset along +Z.
        let collider = ColliderBuilder::cuboid(2.5, 1.5, 15.0)
            .translation(nalgebra::Vector3::new(0.0, 0.0, 15.0))
            .build();
        physics.add_collider(collider, body_handle);

        Self {
            body_handle,
            throttle: 0.0,
            engine_on: false,
            max_thrust: Self::DEFAULT_MAX_THRUST,
            max_rcs_thrust: Self::DEFAULT_MAX_RCS,
            max_torque: Self::DEFAULT_MAX_TORQUE,
        }
    }

    /// Apply forward thrust along the ship's local -Z axis (nose direction).
    /// Effective thrust = throttle * max_thrust * engine_on.
    /// Call once per frame while helm is active.
    pub fn apply_thrust(&self, physics: &mut PhysicsWorld) {
        if !self.engine_on {
            return;
        }
        let force_magnitude = self.throttle * self.max_thrust;
        if force_magnitude.abs() < 0.01 {
            return;
        }
        if let Some(body) = physics.get_body(self.body_handle) {
            // Ship forward is local -Z (nose points toward -Z in ship space).
            let rotation = *body.rotation();
            let forward = rotation * nalgebra::Vector3::new(0.0, 0.0, -1.0);
            let force = forward * force_magnitude;
            drop(body);
            if let Some(body) = physics.get_body_mut(self.body_handle) {
                body.add_force(force, true);
            }
        }
    }

    /// Apply RCS (lateral/vertical) thrust.
    /// `lateral`: -1.0 (left) to 1.0 (right)
    /// `vertical`: -1.0 (down) to 1.0 (up)
    /// `longitudinal`: -1.0 (backward) to 1.0 (forward) --- direct W/S when at helm
    pub fn apply_rcs(
        &self,
        physics: &mut PhysicsWorld,
        lateral: f32,
        vertical: f32,
        longitudinal: f32,
    ) {
        let lat = lateral.clamp(-1.0, 1.0);
        let vert = vertical.clamp(-1.0, 1.0);
        let longi = longitudinal.clamp(-1.0, 1.0);
        if lat.abs() < 0.01 && vert.abs() < 0.01 && longi.abs() < 0.01 {
            return;
        }

        if let Some(body) = physics.get_body(self.body_handle) {
            let rotation = *body.rotation();
            let right = rotation * nalgebra::Vector3::new(1.0, 0.0, 0.0);
            let up = rotation * nalgebra::Vector3::new(0.0, 1.0, 0.0);
            let forward = rotation * nalgebra::Vector3::new(0.0, 0.0, -1.0);

            let force = right * lat * self.max_rcs_thrust
                + up * vert * self.max_rcs_thrust
                + forward * longi * self.max_rcs_thrust;
            drop(body);
            if let Some(body) = physics.get_body_mut(self.body_handle) {
                body.add_force(force, true);
            }
        }
    }

    /// Apply rotational torque.
    /// `pitch`: -1.0 (nose down) to 1.0 (nose up)
    /// `yaw`: -1.0 (nose left) to 1.0 (nose right)
    /// `roll`: -1.0 (roll left) to 1.0 (roll right)
    pub fn apply_rotation(
        &self,
        physics: &mut PhysicsWorld,
        pitch: f32,
        yaw: f32,
        roll: f32,
    ) {
        let p = pitch.clamp(-1.0, 1.0);
        let y = yaw.clamp(-1.0, 1.0);
        let r = roll.clamp(-1.0, 1.0);
        if p.abs() < 0.01 && y.abs() < 0.01 && r.abs() < 0.01 {
            return;
        }

        if let Some(body) = physics.get_body(self.body_handle) {
            let rotation = *body.rotation();
            // Torque axes in ship local space:
            // pitch = rotation around local X
            // yaw = rotation around local Y
            // roll = rotation around local Z
            let local_torque = nalgebra::Vector3::new(
                p * self.max_torque,
                y * self.max_torque,
                r * self.max_torque,
            );
            let world_torque = rotation * local_torque;
            drop(body);
            if let Some(body) = physics.get_body_mut(self.body_handle) {
                body.add_torque(world_torque, true);
            }
        }
    }

    /// Get the ship's current position.
    pub fn position(&self, physics: &PhysicsWorld) -> Option<(f32, f32, f32)> {
        physics.get_body(self.body_handle).map(|b| {
            let t = b.translation();
            (t.x, t.y, t.z)
        })
    }

    /// Get the ship's current rotation as a quaternion (x, y, z, w).
    pub fn rotation(&self, physics: &PhysicsWorld) -> Option<(f32, f32, f32, f32)> {
        physics.get_body(self.body_handle).map(|b| {
            let r = b.rotation();
            (r.i, r.j, r.k, r.w)
        })
    }

    /// Get the ship's current linear velocity magnitude (m/s).
    pub fn speed(&self, physics: &PhysicsWorld) -> f32 {
        physics
            .get_body(self.body_handle)
            .map(|b| b.linvel().magnitude())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_g_world() -> PhysicsWorld {
        PhysicsWorld::with_gravity(0.0, 0.0, 0.0)
    }

    #[test]
    fn ship_creates_body() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        assert!(physics.get_body(ship.body_handle).is_some());
    }

    #[test]
    fn ship_has_correct_mass() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        let body = physics.get_body(ship.body_handle).unwrap();
        let mass = body.mass();
        // Mass should be ~50000 (additional_mass + collider mass)
        assert!(
            mass > 49_000.0,
            "ship mass should be ~50000, got {mass}"
        );
    }

    #[test]
    fn thrust_applies_force_when_engine_on() {
        let mut physics = zero_g_world();
        let mut ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        ship.engine_on = true;
        ship.throttle = 1.0;

        ship.apply_thrust(&mut physics);
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        let vel = body.linvel();
        // Ship forward is -Z, so velocity Z should be negative
        assert!(
            vel.z < 0.0,
            "ship should move in -Z direction, vel.z = {}",
            vel.z
        );
    }

    #[test]
    fn no_thrust_when_engine_off() {
        let mut physics = zero_g_world();
        let mut ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        ship.engine_on = false;
        ship.throttle = 1.0;

        ship.apply_thrust(&mut physics);
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        let speed = body.linvel().magnitude();
        assert!(
            speed < 0.001,
            "ship should not move with engine off, speed = {speed}"
        );
    }

    #[test]
    fn momentum_persists_no_drag() {
        let mut physics = zero_g_world();
        let mut ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        ship.engine_on = true;
        ship.throttle = 1.0;

        // Apply thrust for one frame
        ship.apply_thrust(&mut physics);
        physics.step(1.0 / 60.0);

        let speed_after_thrust = physics
            .get_body(ship.body_handle)
            .unwrap()
            .linvel()
            .magnitude();

        // Coast for 60 frames with no thrust
        ship.engine_on = false;
        for _ in 0..60 {
            physics.step(1.0 / 60.0);
        }

        let speed_after_coast = physics
            .get_body(ship.body_handle)
            .unwrap()
            .linvel()
            .magnitude();

        assert!(
            (speed_after_coast - speed_after_thrust).abs() < 0.1,
            "momentum should persist: thrust={speed_after_thrust}, coast={speed_after_coast}"
        );
    }

    #[test]
    fn rcs_lateral_moves_sideways() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);

        ship.apply_rcs(&mut physics, 1.0, 0.0, 0.0); // right
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        assert!(
            body.linvel().x > 0.0,
            "ship should move right, vel.x = {}",
            body.linvel().x
        );
    }

    #[test]
    fn rotation_torque_spins_ship() {
        let mut physics = zero_g_world();
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);

        ship.apply_rotation(&mut physics, 0.0, 1.0, 0.0); // yaw right
        physics.step(1.0 / 60.0);

        let body = physics.get_body(ship.body_handle).unwrap();
        let angvel = body.angvel();
        assert!(
            angvel.magnitude() > 0.0,
            "ship should have angular velocity after torque"
        );
    }
}
```

- [ ] **Step 2: Verify**

```bash
cargo test -p sa_ship -- ship 2>&1 | tail -15
```

---

### Task 4: Helm Seated Mode

**Depends on:** Task 2, Task 3

**Files:**
- Replace: `crates/sa_ship/src/helm.rs`

- [ ] **Step 1: Implement HelmController**

Replace `crates/sa_ship/src/helm.rs`:

```rust
//! Helm seated mode: when the player sits at the helm, mouse controls
//! ship rotation, WASD controls thrust/RCS, and the camera locks to the
//! helm viewpoint.

use crate::ship::Ship;
use glam::Vec3;
use sa_input::InputState;
use sa_physics::PhysicsWorld;
use winit::keyboard::KeyCode;

/// Whether the player is standing or seated at the helm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelmState {
    Standing,
    Seated,
}

/// Mouse sensitivity for ship rotation while seated.
const HELM_MOUSE_SENSITIVITY: f32 = 0.0002;

/// Controls the helm seated mode.
pub struct HelmController {
    pub state: HelmState,
    /// Helm viewpoint offset from ship origin (local space).
    /// This is where the camera goes when seated.
    pub viewpoint_offset: Vec3,
}

impl HelmController {
    /// Create a new helm controller.
    /// `viewpoint_offset` is the camera position relative to the ship body
    /// origin when seated (e.g., cockpit position).
    pub fn new(viewpoint_offset: Vec3) -> Self {
        Self {
            state: HelmState::Standing,
            viewpoint_offset,
        }
    }

    /// Enter seated mode.
    pub fn sit_down(&mut self) {
        self.state = HelmState::Seated;
    }

    /// Exit seated mode.
    pub fn stand_up(&mut self) {
        self.state = HelmState::Standing;
    }

    /// Returns true if seated at the helm.
    pub fn is_seated(&self) -> bool {
        self.state == HelmState::Seated
    }

    /// Process input while seated and apply forces to the ship.
    /// Returns `true` if the player wants to stand up (left click).
    ///
    /// Input mapping while seated:
    /// - Mouse X -> yaw
    /// - Mouse Y -> pitch
    /// - W/S -> forward/backward RCS
    /// - A/D -> lateral RCS
    /// - Space -> thrust up
    /// - ShiftLeft -> thrust down
    /// - Q/E -> roll
    pub fn update_seated(
        &self,
        ship: &Ship,
        physics: &mut PhysicsWorld,
        input: &InputState,
        _dt: f32,
    ) -> bool {
        if self.state != HelmState::Seated {
            return false;
        }

        // Mouse -> rotation
        let (dx, dy) = input.mouse.delta();
        let yaw = -dx * HELM_MOUSE_SENSITIVITY;
        let pitch = dy * HELM_MOUSE_SENSITIVITY;

        let mut roll = 0.0;
        if input.keyboard.is_pressed(KeyCode::KeyQ) {
            roll -= 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyE) {
            roll += 1.0;
        }

        ship.apply_rotation(physics, pitch, yaw, roll);

        // WASD + Space/Shift -> RCS
        let mut longitudinal = 0.0_f32;
        let mut lateral = 0.0_f32;
        let mut vertical = 0.0_f32;

        if input.keyboard.is_pressed(KeyCode::KeyW) {
            longitudinal += 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyS) {
            longitudinal -= 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyA) {
            lateral -= 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::KeyD) {
            lateral += 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::Space) {
            vertical += 1.0;
        }
        if input.keyboard.is_pressed(KeyCode::ShiftLeft) {
            vertical -= 1.0;
        }

        ship.apply_rcs(physics, lateral, vertical, longitudinal);

        // Apply main engine thrust (throttle lever controls magnitude)
        ship.apply_thrust(physics);

        // Stand up on left click
        input.mouse.left_just_pressed()
    }

    /// Compute the camera world position when seated.
    /// Uses the ship body's position and rotation to transform the viewpoint offset.
    pub fn camera_position(&self, physics: &PhysicsWorld, ship: &Ship) -> Option<(f32, f32, f32)> {
        physics.get_body(ship.body_handle).map(|body| {
            let ship_pos = body.translation();
            let ship_rot = body.rotation();
            let offset = nalgebra::Vector3::new(
                self.viewpoint_offset.x,
                self.viewpoint_offset.y,
                self.viewpoint_offset.z,
            );
            let world_offset = ship_rot * offset;
            (
                ship_pos.x + world_offset.x,
                ship_pos.y + world_offset.y,
                ship_pos.z + world_offset.z,
            )
        })
    }

    /// Get the ship's forward direction for camera orientation when seated.
    /// Returns (yaw, pitch) matching the ship's current orientation.
    pub fn camera_orientation(
        &self,
        physics: &PhysicsWorld,
        ship: &Ship,
    ) -> Option<(f32, f32)> {
        physics.get_body(ship.body_handle).map(|body| {
            let rot = body.rotation();
            // Ship forward is -Z in local space
            let forward = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
            let yaw = forward.x.atan2(-forward.z);
            let pitch = forward.y.asin();
            (yaw, pitch)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_is_standing() {
        let helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));
        assert_eq!(helm.state, HelmState::Standing);
        assert!(!helm.is_seated());
    }

    #[test]
    fn sit_down_and_stand_up() {
        let mut helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));
        helm.sit_down();
        assert!(helm.is_seated());
        helm.stand_up();
        assert!(!helm.is_seated());
    }

    #[test]
    fn camera_position_when_seated() {
        let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        let helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));

        let (cx, cy, cz) = helm.camera_position(&physics, &ship).unwrap();
        // Ship at origin, viewpoint offset (0, 0.8, 1.0), no rotation
        assert!((cx - 0.0).abs() < 0.1, "cx = {cx}");
        assert!((cy - 0.8).abs() < 0.1, "cy = {cy}");
        assert!((cz - 1.0).abs() < 0.1, "cz = {cz}");
    }

    #[test]
    fn update_seated_returns_false_when_standing() {
        let helm = HelmController::new(Vec3::new(0.0, 0.8, 1.0));
        let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);
        let ship = Ship::new(&mut physics, 0.0, 0.0, 0.0);
        let input = InputState::new();
        let wants_stand = helm.update_seated(&ship, &mut physics, &input, 1.0 / 60.0);
        assert!(!wants_stand);
    }
}
```

- [ ] **Step 2: Verify**

```bash
cargo test -p sa_ship -- helm 2>&1 | tail -15
```

---

### Task 5: Station Definitions

**Depends on:** Task 1

**Files:**
- Replace: `crates/sa_ship/src/station.rs`

- [ ] **Step 1: Implement station definitions**

Replace `crates/sa_ship/src/station.rs`:

```rust
//! Station definitions: where interactables are placed in the ship.
//!
//! Each station is a named location (Helm, Nav, Engineering, etc.) with
//! a list of interactable positions. Phase 5a only defines the cockpit.

use glam::Vec3;

/// Named stations aboard the ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Station {
    Cockpit,
    Navigation,
    Sensors,
    Engineering,
    EngineRoom,
}

/// Configuration for an interactable placement.
#[derive(Clone, Debug)]
pub struct InteractablePlacement {
    /// What kind of interactable to place.
    pub kind: PlacementKind,
    /// Position in ship local space.
    pub position: Vec3,
    /// Human-readable label.
    pub label: String,
    /// Collider half-extents for raycast detection.
    pub collider_half_extents: Vec3,
}

/// What to place.
#[derive(Clone, Debug)]
pub enum PlacementKind {
    Lever,
    ToggleButton,
    MomentaryButton,
    Switch { num_positions: u8 },
    Screen { width: f32, height: f32 },
    HelmSeat,
}

/// Configuration for a station.
#[derive(Clone, Debug)]
pub struct StationConfig {
    pub station: Station,
    pub interactables: Vec<InteractablePlacement>,
}

/// Cockpit station layout.
///
/// The cockpit mesh spans z=0 to z=4 in ship local space.
/// Floor is at y=-1.0. The helm seat is centered near the front.
///
/// Positions are in ship local space (relative to ship body origin at (0,0,0)):
/// - Helm seat: center of cockpit, slightly forward
/// - Thrust lever: right side of helm
/// - Engine button: left side of helm
/// - Speed screen: above helm, slightly forward
pub fn cockpit_layout() -> StationConfig {
    StationConfig {
        station: Station::Cockpit,
        interactables: vec![
            InteractablePlacement {
                kind: PlacementKind::HelmSeat,
                position: Vec3::new(0.0, -0.5, 1.5),
                label: "Helm Seat".into(),
                collider_half_extents: Vec3::new(0.3, 0.4, 0.3),
            },
            InteractablePlacement {
                kind: PlacementKind::Lever,
                position: Vec3::new(0.6, -0.2, 1.2),
                label: "Thrust Lever".into(),
                collider_half_extents: Vec3::new(0.08, 0.2, 0.05),
            },
            InteractablePlacement {
                kind: PlacementKind::ToggleButton,
                position: Vec3::new(-0.6, -0.2, 1.2),
                label: "Engine On/Off".into(),
                collider_half_extents: Vec3::new(0.06, 0.06, 0.04),
            },
            InteractablePlacement {
                kind: PlacementKind::Screen {
                    width: 0.4,
                    height: 0.25,
                },
                position: Vec3::new(0.0, 0.3, 0.8),
                label: "Speed Display".into(),
                collider_half_extents: Vec3::new(0.2, 0.125, 0.02),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cockpit_has_four_interactables() {
        let layout = cockpit_layout();
        assert_eq!(layout.interactables.len(), 4);
    }

    #[test]
    fn cockpit_has_helm_seat() {
        let layout = cockpit_layout();
        let has_helm = layout.interactables.iter().any(|i| {
            matches!(i.kind, PlacementKind::HelmSeat)
        });
        assert!(has_helm, "cockpit should have a helm seat");
    }

    #[test]
    fn cockpit_has_thrust_lever() {
        let layout = cockpit_layout();
        let has_lever = layout.interactables.iter().any(|i| {
            matches!(i.kind, PlacementKind::Lever) && i.label == "Thrust Lever"
        });
        assert!(has_lever, "cockpit should have a thrust lever");
    }

    #[test]
    fn cockpit_has_engine_button() {
        let layout = cockpit_layout();
        let has_button = layout.interactables.iter().any(|i| {
            matches!(i.kind, PlacementKind::ToggleButton) && i.label == "Engine On/Off"
        });
        assert!(has_button, "cockpit should have an engine on/off button");
    }

    #[test]
    fn cockpit_has_speed_screen() {
        let layout = cockpit_layout();
        let has_screen = layout.interactables.iter().any(|i| {
            matches!(i.kind, PlacementKind::Screen { .. }) && i.label == "Speed Display"
        });
        assert!(has_screen, "cockpit should have a speed display");
    }

    #[test]
    fn all_positions_inside_cockpit() {
        let layout = cockpit_layout();
        for i in &layout.interactables {
            // Cockpit interior is roughly x=-2..2, y=-1..1.2, z=0..4
            assert!(
                i.position.x.abs() < 2.5 && i.position.y > -1.5
                    && i.position.y < 1.5 && i.position.z > 0.0
                    && i.position.z < 4.0,
                "interactable '{}' at {:?} should be inside cockpit bounds",
                i.label, i.position
            );
        }
    }
}
```

- [ ] **Step 2: Verify**

```bash
cargo test -p sa_ship -- station 2>&1 | tail -10
```

---

### Task 6: Interactable Meshes (sa_meshgen)

**Depends on:** None (independent)

**Files:**
- Create: `crates/sa_meshgen/src/interactables.rs`
- Modify: `crates/sa_meshgen/src/lib.rs` (add module)

- [ ] **Step 1: Add module declaration**

Modify `crates/sa_meshgen/src/lib.rs` --- add after `pub mod validate;`:
```rust
pub mod interactables;
```

- [ ] **Step 2: Create interactable mesh generators**

Create `crates/sa_meshgen/src/interactables.rs`:

```rust
//! Mesh generators for ship interactable objects.
//!
//! Each function returns a `Mesh` centered at origin. The caller positions
//! them in ship space using `mesh.transform()`.

use crate::colors;
use crate::mesh::Mesh;
use crate::primitives::{box_mesh, cylinder_mesh};
use glam::{Mat4, Vec3};

/// Lever mesh: a thin cylinder rod on a box track with a box base.
///
/// - Rod: cylinder, radius 0.02, height 0.3, oriented along Y
/// - Track: thin box 0.05 x 0.4 x 0.02 (visual guide for the lever range)
/// - Base: box 0.1 x 0.02 x 0.06
///
/// The rod is positioned at `lever_position` (0.0 = bottom, 1.0 = top)
/// along the track's 0.4m travel range. Default: bottom (0.0).
pub fn lever_mesh(lever_position: f32) -> Mesh {
    let pos = lever_position.clamp(0.0, 1.0);
    let travel = 0.4; // total travel distance
    let rod_y_offset = -travel / 2.0 + pos * travel;

    let mut meshes = Vec::new();

    // Track (thin vertical guide)
    let track = box_mesh(0.05, travel, 0.02, colors::HULL_ACCENT);
    meshes.push(track);

    // Base plate
    let base = box_mesh(0.1, 0.02, 0.06, colors::FLOOR);
    let base = base.transform(Mat4::from_translation(Vec3::new(0.0, -travel / 2.0 - 0.01, 0.0)));
    meshes.push(base);

    // Rod (cylinder along Y)
    let rod = cylinder_mesh(0.02, 0.3, 6, colors::ACCENT_HELM);
    let rod = rod.transform(Mat4::from_translation(Vec3::new(0.0, rod_y_offset, 0.03)));
    meshes.push(rod);

    Mesh::merge(&meshes)
}

/// Button mesh: a small raised box on a base plate.
///
/// - Button face: box 0.08 x 0.08 x 0.04
/// - Base plate: box 0.12 x 0.12 x 0.02
///
/// `pressed`: if true, the button face is depressed (lower Y offset).
pub fn button_mesh(pressed: bool) -> Mesh {
    let mut meshes = Vec::new();

    // Base plate
    let base = box_mesh(0.12, 0.02, 0.12, colors::FLOOR);
    meshes.push(base);

    // Button face
    let button_y = if pressed { 0.015 } else { 0.03 };
    let button = box_mesh(0.08, 0.04, 0.08, colors::ACCENT_ENGINE);
    let button = button.transform(Mat4::from_translation(Vec3::new(0.0, button_y, 0.0)));
    meshes.push(button);

    Mesh::merge(&meshes)
}

/// Switch mesh: an angled handle on a base.
///
/// - Base: box 0.1 x 0.02 x 0.06
/// - Handle: box 0.04 x 0.12 x 0.03, angled based on position
///
/// `position`: current position (0 to num_positions-1)
/// `num_positions`: total number of positions
pub fn switch_mesh(position: u8, num_positions: u8) -> Mesh {
    let mut meshes = Vec::new();

    // Base
    let base = box_mesh(0.1, 0.02, 0.06, colors::FLOOR);
    meshes.push(base);

    // Handle angle: spread positions across -30 to +30 degrees
    let max_angle = std::f32::consts::FRAC_PI_6; // 30 degrees
    let t = if num_positions > 1 {
        position as f32 / (num_positions - 1) as f32
    } else {
        0.5
    };
    let angle = -max_angle + t * 2.0 * max_angle;

    let handle = box_mesh(0.04, 0.12, 0.03, colors::ACCENT_ENGINEERING);
    let handle = handle.transform(
        Mat4::from_translation(Vec3::new(0.0, 0.07, 0.0))
            * Mat4::from_rotation_z(angle),
    );
    meshes.push(handle);

    Mesh::merge(&meshes)
}

/// Screen mesh: a flat panel with a colored face.
///
/// - Panel: box width x height x 0.02
/// - Screen face is colored CONSOLE_SCREEN
pub fn screen_mesh(width: f32, height: f32) -> Mesh {
    box_mesh(width, height, 0.02, colors::CONSOLE_SCREEN)
}

/// Helm seat mesh: a box seat with a box back.
///
/// - Seat: box 0.5 x 0.1 x 0.5
/// - Back: box 0.5 x 0.6 x 0.1, positioned behind and above seat
pub fn helm_seat_mesh() -> Mesh {
    let mut meshes = Vec::new();

    // Seat (horizontal surface)
    let seat = box_mesh(0.5, 0.1, 0.5, colors::HULL_ACCENT);
    meshes.push(seat);

    // Back (vertical surface behind seat)
    let back = box_mesh(0.5, 0.6, 0.1, colors::HULL_ACCENT);
    let back = back.transform(Mat4::from_translation(Vec3::new(0.0, 0.35, -0.2)));
    meshes.push(back);

    Mesh::merge(&meshes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn assert_mesh_valid(mesh: &Mesh, name: &str) {
        assert!(
            !mesh.vertices.is_empty(),
            "{name} should have vertices"
        );
        assert!(
            !mesh.indices.is_empty(),
            "{name} should have indices"
        );
        // All indices valid
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.vertices.len(),
                "{name}: index {idx} out of bounds (len={})",
                mesh.vertices.len()
            );
        }
        // No degenerate triangles
        for tri in mesh.indices.chunks_exact(3) {
            let a = Vec3::from(mesh.vertices[tri[0] as usize].position);
            let b = Vec3::from(mesh.vertices[tri[1] as usize].position);
            let c = Vec3::from(mesh.vertices[tri[2] as usize].position);
            let area = (b - a).cross(c - a).length() / 2.0;
            assert!(
                area > 1e-8,
                "{name}: degenerate triangle with area {area}"
            );
        }
    }

    #[test]
    fn lever_mesh_valid() {
        assert_mesh_valid(&lever_mesh(0.0), "lever_0");
        assert_mesh_valid(&lever_mesh(0.5), "lever_0.5");
        assert_mesh_valid(&lever_mesh(1.0), "lever_1");
    }

    #[test]
    fn button_mesh_valid() {
        assert_mesh_valid(&button_mesh(false), "button_up");
        assert_mesh_valid(&button_mesh(true), "button_down");
    }

    #[test]
    fn switch_mesh_valid() {
        assert_mesh_valid(&switch_mesh(0, 3), "switch_0");
        assert_mesh_valid(&switch_mesh(1, 3), "switch_1");
        assert_mesh_valid(&switch_mesh(2, 3), "switch_2");
    }

    #[test]
    fn screen_mesh_valid() {
        assert_mesh_valid(&screen_mesh(0.4, 0.25), "screen");
    }

    #[test]
    fn helm_seat_mesh_valid() {
        assert_mesh_valid(&helm_seat_mesh(), "helm_seat");
    }

    #[test]
    fn lever_rod_moves_with_position() {
        let low = lever_mesh(0.0);
        let high = lever_mesh(1.0);
        let (low_min, low_max) = low.bounding_box();
        let (high_min, high_max) = high.bounding_box();
        // The rod at position 1.0 should be higher than at 0.0
        assert!(
            high_max.y > low_max.y,
            "lever at 1.0 should be taller: high_max.y={}, low_max.y={}",
            high_max.y, low_max.y
        );
    }

    #[test]
    fn button_depresses_when_pressed() {
        let up = button_mesh(false);
        let down = button_mesh(true);
        let (_, up_max) = up.bounding_box();
        let (_, down_max) = down.bounding_box();
        assert!(
            up_max.y > down_max.y,
            "unpressed button should be taller: up={}, down={}",
            up_max.y, down_max.y
        );
    }

    #[test]
    fn helm_seat_has_back() {
        let mesh = helm_seat_mesh();
        let (_, max) = mesh.bounding_box();
        // Back extends above the seat
        assert!(
            max.y > 0.3,
            "helm seat should have a back, max.y = {}",
            max.y
        );
    }
}
```

- [ ] **Step 3: Verify**

```bash
cargo test -p sa_meshgen -- interactables 2>&1 | tail -20
```

---

### Task 7: Ship Scene Setup

**Depends on:** Task 3, Task 5, Task 6

**Files:**
- Create: `crates/spaceaway/src/ship_setup.rs`
- Modify: `crates/spaceaway/src/main.rs`
- Modify: `crates/spaceaway/Cargo.toml` (add sa_ship dependency)

- [ ] **Step 1: Add sa_ship dependency to spaceaway**

Check `crates/spaceaway/Cargo.toml` and add:
```toml
sa_ship.workspace = true
```

- [ ] **Step 2: Create ship_setup.rs**

Create `crates/spaceaway/src/ship_setup.rs`. This module handles creating the ship physics body, assembling the ship mesh, creating interactable colliders, and registering interactables.

```rust
//! Ship scene setup: creates the ship body, meshes, and interactables.

use glam::{Mat4, Vec3};
use rapier3d::prelude::*;
use sa_meshgen::interactables;
use sa_physics::PhysicsWorld;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_ship::station::{cockpit_layout, PlacementKind};

/// IDs for the key interactables, so the game loop can read their state.
pub struct ShipInteractableIds {
    pub throttle_lever: usize,
    pub engine_button: usize,
    pub speed_screen: usize,
    pub helm_seat: usize,
}

/// All the meshes needed for the ship scene.
pub struct ShipMeshes {
    /// The assembled ship hull mesh.
    pub hull: sa_meshgen::Mesh,
    /// Interactable meshes, paired with their positions in ship space.
    pub interactable_meshes: Vec<(sa_meshgen::Mesh, Vec3)>,
}

/// Create the ship physics body and register interactables.
///
/// Returns the Ship, the InteractionSystem with registered interactables,
/// and the interactable IDs for game-loop wiring.
pub fn create_ship_and_interactables(
    physics: &mut PhysicsWorld,
) -> (Ship, InteractionSystem, ShipInteractableIds) {
    // Create ship body at world origin
    let ship = Ship::new(physics, 0.0, 0.0, 0.0);

    let mut interaction = InteractionSystem::new();
    let layout = cockpit_layout();

    let mut throttle_lever = 0;
    let mut engine_button = 0;
    let mut speed_screen = 0;
    let mut helm_seat = 0;

    for placement in &layout.interactables {
        // Create a sensor collider for raycast detection.
        // Sensors don't generate contact forces --- they only detect raycasts.
        let half = placement.collider_half_extents;
        let collider = ColliderBuilder::cuboid(half.x, half.y, half.z)
            .translation(nalgebra::Vector3::new(
                placement.position.x,
                placement.position.y,
                placement.position.z,
            ))
            .sensor(true)
            .build();
        let collider_handle = physics.add_collider(collider, ship.body_handle);

        let id = match &placement.kind {
            PlacementKind::Lever => {
                let id = interaction.register(
                    sa_ship::Interactable::lever(collider_handle, &placement.label),
                );
                throttle_lever = id;
                id
            }
            PlacementKind::ToggleButton => {
                let id = interaction.register(
                    sa_ship::Interactable::toggle_button(collider_handle, &placement.label),
                );
                engine_button = id;
                id
            }
            PlacementKind::MomentaryButton => {
                interaction.register(
                    sa_ship::Interactable::momentary_button(collider_handle, &placement.label),
                )
            }
            PlacementKind::Switch { num_positions } => {
                interaction.register(
                    sa_ship::Interactable::switch(collider_handle, *num_positions, &placement.label),
                )
            }
            PlacementKind::Screen { .. } => {
                let id = interaction.register(
                    sa_ship::Interactable::screen(collider_handle, &placement.label),
                );
                speed_screen = id;
                id
            }
            PlacementKind::HelmSeat => {
                let id = interaction.register(
                    sa_ship::Interactable::helm_seat(collider_handle, &placement.label),
                );
                helm_seat = id;
                id
            }
        };
        let _ = id; // suppress unused warning for non-tracked ones
    }

    let ids = ShipInteractableIds {
        throttle_lever,
        engine_button,
        speed_screen,
        helm_seat,
    };

    (ship, interaction, ids)
}

/// Generate all meshes for the ship scene.
pub fn generate_ship_meshes() -> ShipMeshes {
    // Assemble the hull
    let hull = crate::assemble_ship();

    // Generate interactable meshes at cockpit positions
    let layout = cockpit_layout();
    let mut interactable_meshes = Vec::new();

    for placement in &layout.interactables {
        let mesh = match &placement.kind {
            PlacementKind::Lever => interactables::lever_mesh(0.0),
            PlacementKind::ToggleButton | PlacementKind::MomentaryButton => {
                interactables::button_mesh(false)
            }
            PlacementKind::Switch { .. } => interactables::switch_mesh(0, 3),
            PlacementKind::Screen { width, height } => {
                interactables::screen_mesh(*width, *height)
            }
            PlacementKind::HelmSeat => interactables::helm_seat_mesh(),
        };
        interactable_meshes.push((mesh, placement.position));
    }

    ShipMeshes {
        hull,
        interactable_meshes,
    }
}
```

- [ ] **Step 3: Modify main.rs --- App struct and initialization**

Add new fields to the `App` struct in main.rs:

```rust
// Add these use statements at the top:
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_ship::helm::{HelmState, HelmController};

// Add to App struct:
ship: Option<Ship>,
interaction: Option<InteractionSystem>,
ship_ids: Option<ship_setup::ShipInteractableIds>,
helm: Option<HelmController>,
interactable_meshes: Vec<sa_core::Handle<sa_render::MeshMarker>>,
```

Also add `mod ship_setup;` at the top of main.rs.

- [ ] **Step 4: Modify App::new() --- switch to zero-gravity space**

Replace the ground plane and obstacles setup in `App::new()` with:

```rust
fn new() -> Self {
    // Zero gravity for space
    let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);

    // Create ship and interactables
    let (ship, interaction, ids) =
        ship_setup::create_ship_and_interactables(&mut physics);

    // Helm controller: viewpoint is cockpit seat position + eye height
    let helm = HelmController::new(glam::Vec3::new(0.0, 0.3, 1.5));

    // Spawn player inside the cockpit (near the helm seat)
    // Player gets simulated gravity via force each frame
    let player = PlayerController::spawn(&mut physics, 0.0, 0.0, 2.5);

    let camera = Camera::new();
    let universe = Universe::new(MasterSeed(42));

    Self {
        // ... existing fields ...
        physics,
        player: Some(player),
        ship: Some(ship),
        interaction: Some(interaction),
        ship_ids: Some(ids),
        helm: Some(helm),
        interactable_meshes: Vec::new(),
        // ... rest of existing fields ...
        // Remove: cube_mesh (replaced by ship meshes)
        // Keep view_mode, ship_part_mesh, ship_part_index for debug
    }
}
```

- [ ] **Step 5: Modify setup_scene() --- upload ship meshes**

In `setup_scene()`, after existing star/nebula setup, add ship mesh upload:

```rust
// Generate and upload ship hull mesh
let ship_meshes = ship_setup::generate_ship_meshes();
let hull_data = meshgen_to_render(&ship_meshes.hull);
let hull_handle = renderer.mesh_store.upload(&gpu.device, &hull_data);
self.ship_part_mesh = Some(hull_handle);

// Upload interactable meshes
for (mesh, _pos) in &ship_meshes.interactable_meshes {
    let data = meshgen_to_render(mesh);
    let handle = renderer.mesh_store.upload(&gpu.device, &data);
    self.interactable_meshes.push(handle);
}
```

- [ ] **Step 6: Verify compilation**

```bash
cargo check -p spaceaway 2>&1 | head -30
```

This task is about wiring the data structures. Full game-loop integration happens in Tasks 8-10.

---

### Task 8: Interaction Integration

**Depends on:** Task 2, Task 7

**Files:**
- Modify: `crates/spaceaway/src/main.rs` (game loop)

- [ ] **Step 1: Wire up MouseInput for left button**

In `WindowEvent::MouseInput`, add the left button state tracking (if not done in Task 2 Step 3):

```rust
WindowEvent::MouseInput { state, button, .. } => {
    if button == winit::event::MouseButton::Left {
        self.input.mouse.set_left_pressed(state.is_pressed());
    }
    if state.is_pressed() && !self.cursor_grabbed {
        self.grab_cursor();
    }
}
```

- [ ] **Step 2: Add simulated mag-boot gravity for the player**

In the walk-mode physics update section (where `player.update()` is called), add after `player.update()`:

```rust
// Simulated mag-boot gravity: apply downward force in ship local space.
// Phase 5a simplification: ship is at identity rotation, so local Y = world Y.
if let Some(player) = &self.player {
    if let Some(body) = self.physics.get_body_mut(player.body_handle) {
        // ~1g on a ~80kg player = ~785 N downward
        body.add_force(nalgebra::Vector3::new(0.0, -785.0, 0.0), true);
    }
}
```

- [ ] **Step 3: Update query pipeline after physics step**

After `self.physics.step(physics_dt)`, add:

```rust
self.physics.update_query_pipeline();
```

- [ ] **Step 4: Run interaction system each frame**

After the player/physics update, before rendering, add the interaction update:

```rust
// --- Interaction ---
if let (Some(interaction), Some(player)) = (&mut self.interaction, &self.player) {
    // Only run interaction when standing (not seated at helm)
    let is_standing = self.helm.as_ref()
        .map(|h| !h.is_seated())
        .unwrap_or(true);

    if is_standing {
        let eye_pos = player.position(&self.physics);
        let fwd = player.forward();
        let ray_origin = [eye_pos.x as f32, eye_pos.y as f32, eye_pos.z as f32];
        let ray_dir = [fwd.x, fwd.y, fwd.z];

        let (_, mouse_dy) = self.input.mouse.delta();

        let helm_clicked = interaction.update(
            ray_origin,
            ray_dir,
            mouse_dy,
            self.input.mouse.left_just_pressed(),
            self.input.mouse.left_pressed(),
            self.input.mouse.left_just_released(),
            &self.physics,
        );

        // If helm seat was clicked, enter seated mode
        if helm_clicked.is_some() {
            if let Some(helm) = &mut self.helm {
                helm.sit_down();
                // Disable player physics body while seated
                if let Some(body) = self.physics.get_body_mut(player.body_handle) {
                    body.set_enabled(false);
                }
                log::info!("Entered helm seated mode");
            }
        }
    }
}
```

- [ ] **Step 5: Sync interactable state to ship**

After the interaction update, sync lever position to ship throttle and button to engine_on:

```rust
// Sync interactable state -> ship state
if let (Some(interaction), Some(ship), Some(ids)) =
    (&self.interaction, &mut self.ship, &self.ship_ids)
{
    // Throttle lever -> ship.throttle
    if let Some(lever) = interaction.get(ids.throttle_lever) {
        ship.throttle = lever.lever_position().unwrap_or(0.0);
    }
    // Engine button -> ship.engine_on
    if let Some(button) = interaction.get(ids.engine_button) {
        ship.engine_on = button.is_button_pressed().unwrap_or(false);
    }
    // Speed screen: update with current speed
    // (done via get_mut on the interaction system)
}
// Update speed screen text
if let (Some(interaction), Some(ship)) = (&mut self.interaction, &self.ship) {
    if let Some(ids) = &self.ship_ids {
        let speed = ship.speed(&self.physics);
        if let Some(screen) = interaction.get_mut(ids.speed_screen) {
            screen.set_screen_text(vec![
                format!("Speed: {:.1} m/s", speed),
                format!("Throttle: {:.0}%", ship.throttle * 100.0),
                format!("Engine: {}", if ship.engine_on { "ON" } else { "OFF" }),
            ]);
        }
    }
}
```

- [ ] **Step 6: Verify**

```bash
cargo check -p spaceaway 2>&1 | head -20
```

---

### Task 9: Flight Integration

**Depends on:** Task 4, Task 8

**Files:**
- Modify: `crates/spaceaway/src/main.rs` (game loop)

- [ ] **Step 1: Add helm flight controls to the game loop**

In the player/physics section, add a branch for seated mode. The flow should be:

```rust
if self.fly_mode {
    // ... existing fly mode code (unchanged) ...
} else if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
    // --- Seated at helm: control ship ---
    if let (Some(helm), Some(ship)) = (&self.helm, &self.ship) {
        let wants_stand = helm.update_seated(ship, &mut self.physics, &self.input, dt);

        if wants_stand {
            // Stand up
            if let Some(helm) = &mut self.helm {
                helm.stand_up();
                // Re-enable player physics body
                if let Some(player) = &self.player {
                    if let Some(body) = self.physics.get_body_mut(player.body_handle) {
                        body.set_enabled(true);
                    }
                }
                log::info!("Left helm seated mode");
            }
        }

        // Apply main engine thrust
        ship.apply_thrust(&mut self.physics);
    }

    // Physics step
    let physics_dt = dt.min(1.0 / 30.0);
    if physics_dt > 0.0 {
        self.physics.step(physics_dt);
    }

    // Camera follows ship orientation
    if let (Some(helm), Some(ship)) = (&self.helm, &self.ship) {
        if let Some((cx, cy, cz)) = helm.camera_position(&self.physics, ship) {
            self.camera.position = WorldPos::new(cx as f64, cy as f64, cz as f64);
        }
        if let Some((yaw, pitch)) = helm.camera_orientation(&self.physics, ship) {
            self.camera.yaw = yaw;
            self.camera.pitch = pitch;
        }
    }
} else {
    // --- Walk mode (existing code) ---
    if let Some(player) = &mut self.player {
        player.update(&mut self.physics, &self.input, dt);
    }

    // Simulated mag-boot gravity
    if let Some(player) = &self.player {
        if let Some(body) = self.physics.get_body_mut(player.body_handle) {
            body.add_force(nalgebra::Vector3::new(0.0, -785.0, 0.0), true);
        }
    }

    let physics_dt = dt.min(1.0 / 30.0);
    if physics_dt > 0.0 {
        self.physics.step(physics_dt);
    }

    if let Some(player) = &self.player {
        self.camera.position = player.position(&self.physics);
        self.camera.yaw = player.yaw;
        self.camera.pitch = player.pitch;
    }
}

// Always update query pipeline after physics step
self.physics.update_query_pipeline();
```

- [ ] **Step 2: Update rendering to show ship + interactables**

Replace the draw commands section to always render the ship when in normal view mode:

```rust
let commands = if self.view_mode == 6 || self.view_mode == 7 {
    // Debug: ship part viewing
    if let Some(ship_mesh) = self.ship_part_mesh {
        vec![DrawCommand {
            mesh: ship_mesh,
            model_matrix: Mat4::IDENTITY,
        }]
    } else {
        vec![]
    }
} else {
    // Normal gameplay: render ship hull + interactables
    let mut cmds = Vec::new();

    // Ship hull
    if let Some(hull_handle) = self.ship_part_mesh {
        cmds.push(DrawCommand {
            mesh: hull_handle,
            model_matrix: Mat4::IDENTITY,
        });
    }

    // Interactable meshes at their positions
    let layout = sa_ship::station::cockpit_layout();
    for (i, handle) in self.interactable_meshes.iter().enumerate() {
        if let Some(placement) = layout.interactables.get(i) {
            let pos = placement.position;
            cmds.push(DrawCommand {
                mesh: *handle,
                model_matrix: Mat4::from_translation(pos),
            });
        }
    }

    cmds
};
```

- [ ] **Step 3: Remove old ground plane and obstacle code**

In `App::new()`, remove:
- `sa_physics::add_ground(&mut physics, 0.0);`
- The three `spawn_static_body` / `attach_box_collider` calls for obs1, obs2, obs3
- The `cube_mesh` field (or keep for debug, set to None)
- The ground-plane/cube draw commands in the render section (replaced in Step 2)

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p spaceaway 2>&1 | head -20
```

---

### Task 10: Final Integration + Verification

**Depends on:** Task 8, Task 9

**Files:**
- Possibly modify: any file from previous tasks for bug fixes

- [ ] **Step 1: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -30
```

All tests across all crates should pass. Fix any failures.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace 2>&1 | tail -30
```

Fix any warnings. Common issues:
- Unused variables from partial integration
- Redundant clones
- Missing `#[allow(dead_code)]` on fields not yet used

- [ ] **Step 3: Manual integration test checklist**

Run the game (`cargo run -p spaceaway`) and verify:

1. Game starts in space (no ground plane, stars visible)
2. Player spawns inside the ship cockpit
3. Player can walk around inside the ship (WASD, mouse look)
4. Simulated gravity keeps player on the floor
5. Walk toward the thrust lever --- crosshair/hover hint appears
6. Click and drag the thrust lever up and down
7. Click the engine on/off button --- it toggles
8. Click the helm seat --- camera locks to helm viewpoint
9. Mouse controls ship rotation (pitch/yaw)
10. WASD applies thrust/RCS
11. Q/E rolls the ship
12. Ship moves through space, stars update
13. Click to stand up from helm
14. Ship keeps drifting (Newtonian momentum)
15. Player can walk around while ship drifts
16. F key still works for fly mode / external view
17. Key 0 returns to normal view
18. Performance is smooth (>30 FPS)

- [ ] **Step 4: Document any known issues**

If any items from the checklist do not work, document them as known issues. Common Phase 5a limitations:

- Ship does not move while player walks inside (by design --- Phase 5b)
- No visual feedback for hover state (no crosshair change yet)
- Screen text is not rendered (no text rendering yet --- just data stored)
- Lever mesh does not visually update position in real-time (would need per-frame mesh re-upload or transform update --- Phase 5b)
- No sound effects

---

## Dependency Graph

```
Task 1 (crate setup + interactables)
  |
  +-- Task 2 (interaction system) --+
  |                                  |
  +-- Task 3 (ship entity) ---------+-- Task 7 (ship scene setup) -- Task 8 (interaction wiring)
  |                                  |                                    |
  +-- Task 5 (station definitions) --+                                    |
                                                                          |
Task 4 (helm) ----------------------------------------------------------+-- Task 9 (flight)
                                                                          |
Task 6 (interactable meshes) -- Task 7                                    |
                                                                          |
                                                                     Task 10 (final)
```

Tasks that can run in parallel:
- Tasks 1, 6 are independent
- Tasks 2, 3, 5 depend only on Task 1
- Task 4 depends on Tasks 2, 3
- Task 7 depends on Tasks 3, 5, 6
- Task 8 depends on Tasks 2, 7
- Task 9 depends on Tasks 4, 8
- Task 10 depends on Tasks 8, 9

**Maximum parallelism schedule:**
1. Tasks 1 + 6 (parallel)
2. Tasks 2 + 3 + 5 (parallel, after Task 1)
3. Tasks 4 + 7 (parallel, after their deps)
4. Task 8 (after Tasks 2, 7)
5. Task 9 (after Tasks 4, 8)
6. Task 10 (after Task 9)
