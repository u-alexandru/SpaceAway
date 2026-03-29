# CDLOD Terrain Phase 2: Collision + Gravity — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship collides with terrain (can't fly through the ground) and gravity transitions from ship-local to planet-radial as you descend. The ship can rest on terrain but no formal landing state yet (Phase 3).

**Architecture:** Add HeightField colliders managed by terrain_integration.rs within 500m of the ship. Implement gravity.rs in sa_terrain for altitude-based gravity blending. Apply gravity force to ship body in the game loop. Physics anchor rebasing keeps colliders positioned with f32 precision.

**Tech Stack:** rapier3d (HeightField colliders, raycasts), nalgebra, glam, sa_terrain gravity module

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `crates/sa_terrain/src/gravity.rs` | Gravity direction + magnitude from altitude (pure math) |

### Modified Files

| File | Change |
|------|--------|
| `crates/sa_terrain/src/lib.rs` | Add `pub mod gravity;` (module already declared but file missing) |
| `crates/spaceaway/src/terrain_integration.rs` | Add collider management, physics anchor, gravity computation |
| `crates/spaceaway/src/main.rs` | Apply gravity force to ship, pass terrain gravity to player controller |
| `crates/sa_physics/src/world.rs` | Add `remove_collider()` method |
| `crates/sa_physics/src/lib.rs` | Re-export `remove_collider` |
| `crates/spaceaway/src/ship_colliders.rs` | Add `TERRAIN` collision group constant |

---

### Task 1: Gravity module in sa_terrain

**Files:**
- Create: `crates/sa_terrain/src/gravity.rs`
- Modify: `crates/sa_terrain/src/lib.rs`

- [ ] **Step 1: Write gravity module with tests**

```rust
//! Planetary gravity: direction and magnitude from altitude.
//!
//! Blends smoothly from ship-local gravity (in space) to planet-radial
//! gravity (on surface) based on altitude within the transition zone.

/// Gravity transition result.
#[derive(Debug, Clone, Copy)]
pub struct GravityState {
    /// Gravity direction (unit vector, pointing "down").
    pub direction: [f32; 3],
    /// Gravity magnitude in m/s².
    pub magnitude: f32,
    /// Blend factor: 0.0 = full ship gravity, 1.0 = full planet gravity.
    pub blend: f32,
}

/// Compute the gravity state for a given position near a planet.
///
/// `ship_pos_planet_relative`: ship position relative to planet center, in meters.
/// `ship_down`: ship's local "down" direction (unit vector).
/// `planet_radius_m`: planet radius in meters.
/// `surface_gravity_ms2`: surface gravity in m/s².
/// `ship_gravity_ms2`: ship's artificial gravity in m/s² (typically 9.81).
pub fn compute_gravity(
    ship_pos_planet_relative: [f64; 3],
    ship_down: [f32; 3],
    planet_radius_m: f64,
    surface_gravity_ms2: f32,
    ship_gravity_ms2: f32,
) -> GravityState {
    let dist = (ship_pos_planet_relative[0] * ship_pos_planet_relative[0]
        + ship_pos_planet_relative[1] * ship_pos_planet_relative[1]
        + ship_pos_planet_relative[2] * ship_pos_planet_relative[2])
    .sqrt();

    let altitude = dist - planet_radius_m;
    let atmosphere_top = planet_radius_m * 0.2; // transition zone height (gameplay param)

    if altitude > atmosphere_top {
        // Above transition zone: pure ship gravity
        return GravityState {
            direction: ship_down,
            magnitude: ship_gravity_ms2,
            blend: 0.0,
        };
    }

    // Planet "down" direction: toward planet center = -normalize(position)
    let planet_down = if dist > 1.0 {
        [
            -(ship_pos_planet_relative[0] / dist) as f32,
            -(ship_pos_planet_relative[1] / dist) as f32,
            -(ship_pos_planet_relative[2] / dist) as f32,
        ]
    } else {
        [0.0, -1.0, 0.0] // fallback
    };

    // Blend factor: 0 at atmosphere_top, 1 at surface
    let t = if atmosphere_top > 0.0 {
        (1.0 - altitude / atmosphere_top).clamp(0.0, 1.0) as f32
    } else {
        1.0
    };

    // Slerp direction (with antiparallel guard)
    let dot = ship_down[0] * planet_down[0]
        + ship_down[1] * planet_down[1]
        + ship_down[2] * planet_down[2];

    let direction = if dot < -0.99 {
        // Nearly opposite — use planet_down directly at high blend
        if t > 0.5 { planet_down } else { ship_down }
    } else {
        // Lerp + normalize (cheaper than slerp, good enough for smooth transition)
        let dx = ship_down[0] + (planet_down[0] - ship_down[0]) * t;
        let dy = ship_down[1] + (planet_down[1] - ship_down[1]) * t;
        let dz = ship_down[2] + (planet_down[2] - ship_down[2]) * t;
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        if len > 1e-6 {
            [dx / len, dy / len, dz / len]
        } else {
            planet_down
        }
    };

    // Lerp magnitude
    let magnitude = ship_gravity_ms2 + (surface_gravity_ms2 - ship_gravity_ms2) * t;

    GravityState {
        direction,
        magnitude,
        blend: t,
    }
}

/// Compute surface gravity from planet mass and radius ratios.
/// `mass_ratio`: planet mass / Earth mass.
/// `radius_ratio`: planet radius / Earth radius.
pub fn surface_gravity(mass_ratio: f32, radius_ratio: f32) -> f32 {
    if radius_ratio < 0.001 {
        return 0.0;
    }
    9.81 * mass_ratio / (radius_ratio * radius_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn above_atmosphere_is_ship_gravity() {
        let state = compute_gravity(
            [0.0, 0.0, 10_000_000.0], // 10,000 km from center
            [0.0, -1.0, 0.0],          // ship down
            6_371_000.0,                // Earth radius
            9.81,
            9.81,
        );
        assert_eq!(state.blend, 0.0);
        assert!((state.magnitude - 9.81).abs() < 0.01);
        assert!((state.direction[1] - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn on_surface_is_planet_gravity() {
        let state = compute_gravity(
            [0.0, 0.0, 6_371_000.0], // on surface of +Z
            [0.0, -1.0, 0.0],
            6_371_000.0,
            9.81,
            9.81,
        );
        assert!(state.blend > 0.99, "blend should be ~1.0 on surface, got {}", state.blend);
        // Planet down should point toward center = -Z direction
        assert!(state.direction[2] < -0.9, "gravity should point -Z on +Z surface, got {:?}", state.direction);
    }

    #[test]
    fn mid_transition_blends() {
        let atmosphere_top = 6_371_000.0 * 0.2; // ~1,274 km
        let mid_altitude = atmosphere_top / 2.0;
        let dist = 6_371_000.0 + mid_altitude;
        let state = compute_gravity(
            [0.0, 0.0, dist],
            [0.0, -1.0, 0.0],
            6_371_000.0,
            9.81,
            9.81,
        );
        assert!(state.blend > 0.3 && state.blend < 0.7,
            "mid-transition blend should be ~0.5, got {}", state.blend);
    }

    #[test]
    fn surface_gravity_earth() {
        let g = surface_gravity(1.0, 1.0);
        assert!((g - 9.81).abs() < 0.01);
    }

    #[test]
    fn surface_gravity_super_earth() {
        // 5 Earth masses, 1.58 Earth radii
        let g = surface_gravity(5.0, 1.58);
        assert!((g - 19.6).abs() < 0.5, "super-Earth g={g}");
    }

    #[test]
    fn surface_gravity_small_moon() {
        // 0.05 Earth masses, 0.44 Earth radii
        let g = surface_gravity(0.05, 0.44);
        assert!((g - 2.5).abs() < 0.5, "small moon g={g}");
    }

    #[test]
    fn different_ship_and_planet_gravity() {
        // High-gravity planet (20 m/s²), ship has 9.81
        let state = compute_gravity(
            [0.0, 0.0, 6_371_000.0], // on surface
            [0.0, -1.0, 0.0],
            6_371_000.0,
            20.0, // high surface gravity
            9.81,
        );
        // On surface, magnitude should be surface gravity
        assert!(state.magnitude > 19.0, "on-surface should use planet gravity, got {}", state.magnitude);
    }
}
```

- [ ] **Step 2: Ensure lib.rs has gravity module**

The `pub mod gravity;` line should already exist in `crates/sa_terrain/src/lib.rs` from Phase 1 skeleton. If the file `gravity.rs` didn't exist, the crate wouldn't compile — but Phase 1 created empty placeholder files. Verify `pub mod gravity;` is declared. If not, add it.

- [ ] **Step 3: Run tests**

```bash
cargo test -p sa_terrain -- gravity
```

Expected: all 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/sa_terrain/src/gravity.rs crates/sa_terrain/src/lib.rs
git commit -m "feat(terrain): gravity module with altitude-based blending"
```

---

### Task 2: Add TERRAIN collision group and remove_collider

**Files:**
- Modify: `crates/spaceaway/src/ship_colliders.rs`
- Modify: `crates/sa_physics/src/world.rs`
- Modify: `crates/sa_physics/src/lib.rs`

- [ ] **Step 1: Add TERRAIN collision group**

In `crates/spaceaway/src/ship_colliders.rs`, after the existing group constants, add:

```rust
/// Terrain colliders (ground surface for planet landing).
pub const TERRAIN: Group = Group::GROUP_5;     // 0x0010
```

- [ ] **Step 2: Add remove_collider to PhysicsWorld**

In `crates/sa_physics/src/world.rs`, add this method to `impl PhysicsWorld`:

```rust
    /// Removes a collider from the physics world.
    /// Returns the removed collider, or None if the handle was invalid.
    pub fn remove_collider(&mut self, handle: ColliderHandle) -> Option<Collider> {
        self.collider_set.remove(
            handle,
            &mut self.island_manager,
            &mut self.rigid_body_set,
            true, // wake_up parent body
        )
    }

    /// Removes a rigid body and all its attached colliders.
    pub fn remove_rigid_body(&mut self, handle: RigidBodyHandle) -> Option<RigidBody> {
        self.rigid_body_set.remove(
            handle,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            true,
        )
    }
```

- [ ] **Step 3: Re-export in sa_physics lib.rs**

No re-export needed — these are methods on PhysicsWorld, not standalone functions.

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p sa_physics
cargo check -p spaceaway
```

- [ ] **Step 5: Commit**

```bash
git add crates/sa_physics/src/world.rs crates/spaceaway/src/ship_colliders.rs
git commit -m "feat(terrain): TERRAIN collision group and collider removal methods"
```

---

### Task 3: Terrain collider management in terrain_integration.rs

**Files:**
- Modify: `crates/spaceaway/src/terrain_integration.rs`

This is the core task. Add HeightField collider creation/removal within 500m of the ship, plus physics anchor rebasing.

- [ ] **Step 1: Add collider state to TerrainManager**

Add these fields to the `TerrainManager` struct:

```rust
    /// Rigid body handle for terrain colliders (static body at anchor position).
    terrain_body: Option<rapier3d::prelude::RigidBodyHandle>,
    /// Active terrain colliders keyed by ChunkKey.
    colliders: HashMap<ChunkKey, rapier3d::prelude::ColliderHandle>,
    /// Physics anchor point in planet-relative meters (f64).
    /// All collider positions are relative to this anchor.
    anchor_f64: [f64; 3],
    /// Collision radius around the ship (meters).
    collision_radius: f64,
```

Initialize in `TerrainManager::new()`:

```rust
    terrain_body: None,
    colliders: HashMap::new(),
    anchor_f64: [0.0; 3],
    collision_radius: 500.0,
```

- [ ] **Step 2: Add collider creation helper**

Add a method to create a HeightField collider from chunk data:

```rust
    /// Create a HeightField collider for a terrain chunk.
    fn create_terrain_collider(
        &mut self,
        chunk: &ChunkData,
        physics: &mut sa_physics::PhysicsWorld,
    ) {
        use sa_terrain::chunk::GRID_SIZE;

        // Ensure terrain body exists
        if self.terrain_body.is_none() {
            let body = rapier3d::prelude::RigidBodyBuilder::fixed().build();
            self.terrain_body = Some(physics.add_rigid_body(body));
        }
        let body_handle = self.terrain_body.unwrap();

        // Build height matrix (GRID_SIZE × GRID_SIZE)
        let nrows = GRID_SIZE as usize;
        let ncols = GRID_SIZE as usize;
        let heights = nalgebra::DMatrix::from_fn(nrows, ncols, |r, c| {
            chunk.heights[r * ncols + c]
        });

        // Chunk size on the sphere surface (approximate, for scaling)
        let subdivs = 1u32 << chunk.key.lod;
        let face_size = 2.0 * self.config.radius_m / subdivs as f64;
        let scale_xz = face_size as f32;
        let scale_y = (self.config.displacement_fraction * self.config.radius_m as f32);

        let collider = rapier3d::prelude::ColliderBuilder::heightfield(
            heights,
            nalgebra::Vector3::new(scale_xz, scale_y, scale_xz),
        )
        .collision_groups(rapier3d::prelude::InteractionGroups::new(
            crate::ship_colliders::TERRAIN,
            crate::ship_colliders::PLAYER | crate::ship_colliders::SHIP_HULL,
        ))
        .friction(0.8)
        .restitution(0.1)
        .build();

        // Position collider relative to anchor
        let offset_x = (chunk.center_f64[0] - self.anchor_f64[0]) as f32;
        let offset_y = (chunk.center_f64[1] - self.anchor_f64[1]) as f32;
        let offset_z = (chunk.center_f64[2] - self.anchor_f64[2]) as f32;

        // Orient collider to match local tangent plane
        // The HeightField is flat (XZ plane), but on a sphere we need to rotate it
        // to align with the local surface. For small chunks (<50m) curvature is negligible,
        // so we rotate the collider to face outward from planet center.
        let center_dir = nalgebra::Vector3::new(
            chunk.center_f64[0] as f32,
            chunk.center_f64[1] as f32,
            chunk.center_f64[2] as f32,
        ).normalize();
        let up = nalgebra::Vector3::new(0.0, 1.0, 0.0);
        let rotation = if center_dir.dot(&up).abs() > 0.999 {
            // Near poles, use different reference axis
            let fwd = nalgebra::Vector3::new(1.0, 0.0, 0.0);
            nalgebra::UnitQuaternion::face_towards(&center_dir, &fwd)
        } else {
            nalgebra::UnitQuaternion::face_towards(&center_dir, &up)
        };

        let handle = physics.add_collider(
            collider,
            body_handle,
        );

        // Set collider position relative to parent body
        if let Some(collider_ref) = physics.collider_set.get_mut(handle) {
            collider_ref.set_position_wrt_parent(
                nalgebra::Isometry3::from_parts(
                    nalgebra::Translation3::new(offset_x, offset_y, offset_z),
                    rotation,
                )
            );
        }

        self.colliders.insert(chunk.key, handle);
    }
```

- [ ] **Step 3: Add collider update to TerrainManager::update()**

Extend the `update()` method to manage colliders. After the streaming update, add collider lifecycle:

```rust
    /// Update colliders: create for nearby chunks, remove for distant chunks.
    fn update_colliders(
        &mut self,
        ship_pos_planet_m: [f64; 3],
        new_chunks: &[ChunkData],
        removed_keys: &[ChunkKey],
        physics: &mut sa_physics::PhysicsWorld,
    ) {
        // Remove colliders for evicted chunks
        for key in removed_keys {
            if let Some(handle) = self.colliders.remove(key) {
                physics.remove_collider(handle);
            }
        }

        // Create colliders for new chunks within collision radius
        for chunk in new_chunks {
            let dx = chunk.center_f64[0] - ship_pos_planet_m[0];
            let dy = chunk.center_f64[1] - ship_pos_planet_m[1];
            let dz = chunk.center_f64[2] - ship_pos_planet_m[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();

            if dist < self.collision_radius && !self.colliders.contains_key(&chunk.key) {
                self.create_terrain_collider(chunk, physics);
            }
        }

        // Remove colliders that are now out of range
        let keys_to_remove: Vec<ChunkKey> = self.colliders.keys()
            .filter(|key| {
                let center = chunk_center_from_key(key, self.config.radius_m);
                let dx = center[0] - ship_pos_planet_m[0];
                let dy = center[1] - ship_pos_planet_m[1];
                let dz = center[2] - ship_pos_planet_m[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                dist > self.collision_radius * 1.5 // hysteresis
            })
            .cloned()
            .collect();

        for key in keys_to_remove {
            if let Some(handle) = self.colliders.remove(&key) {
                physics.remove_collider(handle);
            }
        }
    }
```

- [ ] **Step 4: Add physics anchor rebasing**

```rust
    /// Rebase the physics anchor if the ship has moved too far from it.
    /// Shifts all terrain collider positions by the delta.
    fn rebase_anchor_if_needed(
        &mut self,
        ship_pos_planet_m: [f64; 3],
        physics: &mut sa_physics::PhysicsWorld,
    ) {
        let dx = ship_pos_planet_m[0] - self.anchor_f64[0];
        let dy = ship_pos_planet_m[1] - self.anchor_f64[1];
        let dz = ship_pos_planet_m[2] - self.anchor_f64[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist < 100.0 {
            return; // close enough
        }

        let old_anchor = self.anchor_f64;
        self.anchor_f64 = ship_pos_planet_m;

        // Shift all collider positions
        let shift_x = (old_anchor[0] - self.anchor_f64[0]) as f32;
        let shift_y = (old_anchor[1] - self.anchor_f64[1]) as f32;
        let shift_z = (old_anchor[2] - self.anchor_f64[2]) as f32;

        for handle in self.colliders.values() {
            if let Some(collider) = physics.collider_set.get_mut(*handle) {
                let pos = collider.position_wrt_parent().unwrap().clone();
                let new_translation = nalgebra::Translation3::new(
                    pos.translation.x + shift_x,
                    pos.translation.y + shift_y,
                    pos.translation.z + shift_z,
                );
                collider.set_position_wrt_parent(
                    nalgebra::Isometry3::from_parts(new_translation, pos.rotation)
                );
            }
        }

        // Shift the terrain body itself if needed
        if let Some(body_handle) = self.terrain_body {
            if let Some(body) = physics.get_body_mut(body_handle) {
                let pos = body.position().clone();
                body.set_position(
                    nalgebra::Isometry3::from_parts(
                        nalgebra::Translation3::new(
                            pos.translation.x + shift_x,
                            pos.translation.y + shift_y,
                            pos.translation.z + shift_z,
                        ),
                        pos.rotation,
                    ),
                    true,
                );
            }
        }

        physics.sync_collider_positions();
        physics.update_query_pipeline();

        log::debug!("Physics anchor rebased: shift=({shift_x:.1}, {shift_y:.1}, {shift_z:.1})");
    }
```

- [ ] **Step 5: Add gravity computation to TerrainFrameResult**

Extend `TerrainFrameResult` with gravity info:

```rust
pub struct TerrainFrameResult {
    pub draw_commands: Vec<DrawCommand>,
    pub hidden_body_index: Option<usize>,
    /// Gravity state for the ship (None if terrain not active).
    pub gravity: Option<sa_terrain::gravity::GravityState>,
    /// Ship position relative to planet center in meters (for physics).
    pub ship_planet_pos_m: Option<[f64; 3]>,
}
```

- [ ] **Step 6: Update the main `update()` method signature**

Add `physics: &mut sa_physics::PhysicsWorld` and `ship_down: [f32; 3]` parameters, compute gravity, call collider update:

```rust
pub fn update(
    &mut self,
    camera_galactic_ly: WorldPos,
    planet_center_ly: WorldPos,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
    physics: &mut sa_physics::PhysicsWorld,
    ship_down: [f32; 3],
    surface_gravity: f32,
) -> TerrainFrameResult
```

Inside `update()`, after streaming, add:

```rust
        // Compute gravity
        let gravity = sa_terrain::gravity::compute_gravity(
            cam_planet_m,
            ship_down,
            self.config.radius_m,
            surface_gravity,
            9.81, // ship artificial gravity
        );

        // Update colliders
        self.rebase_anchor_if_needed(cam_planet_m, physics);
        self.update_colliders(cam_planet_m, &new_chunks, &removed_keys, physics);

        TerrainFrameResult {
            draw_commands,
            hidden_body_index: Some(self.body_index),
            gravity: Some(gravity),
            ship_planet_pos_m: Some(cam_planet_m),
        }
```

- [ ] **Step 7: Add cleanup on deactivation**

Add a `cleanup()` method to remove all colliders and the terrain body:

```rust
    /// Remove all terrain colliders and the terrain body from physics.
    pub fn cleanup(&mut self, physics: &mut sa_physics::PhysicsWorld) {
        for handle in self.colliders.values() {
            physics.remove_collider(*handle);
        }
        self.colliders.clear();
        if let Some(body_handle) = self.terrain_body.take() {
            physics.remove_rigid_body(body_handle);
        }
    }
```

- [ ] **Step 8: Add surface_gravity to TerrainManager**

Store surface gravity in the manager (computed from planet data on activation):

```rust
    /// Surface gravity in m/s² for this planet.
    surface_gravity_ms2: f32,
```

And expose it:

```rust
    pub fn surface_gravity(&self) -> f32 {
        self.surface_gravity_ms2
    }
```

Update `TerrainManager::new()` to accept and store it. Update `find_terrain_planet()` return type to include surface gravity.

- [ ] **Step 9: Verify compilation**

```bash
cargo check -p spaceaway
```

Fix any compilation errors (the update() signature change will break the call site in main.rs — that's expected, fixed in Task 4).

- [ ] **Step 10: Commit**

```bash
git add crates/spaceaway/src/terrain_integration.rs
git commit -m "feat(terrain): collider management, physics anchor, gravity in terrain integration"
```

---

### Task 4: Wire gravity and colliders into game loop

**Files:**
- Modify: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Update terrain update call with new parameters**

Find the terrain update block in RedrawRequested. Update the `terrain_mgr.update()` call to pass physics and ship_down:

```rust
let terrain_commands: Vec<DrawCommand> = if let Some(terrain_mgr) = &mut self.terrain {
    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
        let planet_pos = self.active_system.as_ref()
            .and_then(|sys| {
                let positions = sys.compute_positions_ly_pub();
                positions.get(terrain_mgr.body_index()).copied()
            })
            .unwrap_or(WorldPos::ORIGIN);

        // Ship "down" direction (ship-local -Y rotated by ship orientation)
        let ship_down = if let Some(ship) = &self.ship {
            if let Some(body) = self.physics.get_body(ship.body_handle) {
                let rot = body.rotation();
                let down = rot * nalgebra::Vector3::new(0.0, -1.0, 0.0);
                [down.x, down.y, down.z]
            } else {
                [0.0, -1.0, 0.0]
            }
        } else {
            [0.0, -1.0, 0.0]
        };

        let result = terrain_mgr.update(
            self.galactic_position,
            planet_pos,
            &mut renderer.mesh_store,
            &gpu.device,
            &mut self.physics,
            ship_down,
            terrain_mgr.surface_gravity(),
        );

        if let Some(sys) = &mut self.active_system {
            sys.hidden_body_index = result.hidden_body_index;
        }

        // Store gravity for ship force application below
        // (save to a local variable, used after this block)
        self.terrain_gravity = result.gravity;

        result.draw_commands
    } else {
        vec![]
    }
} else {
    self.terrain_gravity = None;
    vec![]
};
```

- [ ] **Step 2: Add terrain_gravity field to App**

```rust
    /// Current terrain gravity state (when near a planet).
    terrain_gravity: Option<sa_terrain::gravity::GravityState>,
```

Initialize as `None` in `App::new()`.

- [ ] **Step 3: Apply gravity force to ship**

In the helm mode section where the ship body is integrated, after thrust application but before position integration, add gravity force:

```rust
// Apply terrain gravity to ship (when near a planet)
if let Some(gravity) = &self.terrain_gravity {
    if gravity.blend > 0.01 {
        // Apply gravity as acceleration (force = mass * gravity)
        let grav_accel = nalgebra::Vector3::new(
            gravity.direction[0] * gravity.magnitude,
            gravity.direction[1] * gravity.magnitude,
            gravity.direction[2] * gravity.magnitude,
        );
        let vel = body.linvel() + grav_accel * physics_dt;
        body.set_linvel(vel, true);
    }
}
```

This goes in BOTH the helm mode (where `physics.step()` is called) and the walk mode (manual integration).

- [ ] **Step 4: Cleanup terrain on deactivation**

Where terrain is deactivated (both in the deactivation check and where active_system is cleared), call cleanup:

```rust
if terrain_mgr.should_deactivate(self.galactic_position) {
    terrain_mgr.cleanup(&mut self.physics);
    // ... rest of deactivation
}
```

- [ ] **Step 5: Update find_terrain_planet to return surface gravity**

In terrain_integration.rs, modify `find_terrain_planet()` to also return `surface_gravity_ms2`. Use the planet_data accessor to compute it:

```rust
let surface_grav = sa_terrain::gravity::surface_gravity(mass_earth, radius_earth);
```

Return it alongside the TerrainConfig. Update `TerrainManager::new()` signature.

- [ ] **Step 6: Verify compilation and test**

```bash
cargo check -p spaceaway
cargo test --workspace
```

Fix any issues.

- [ ] **Step 7: Commit**

```bash
git add crates/spaceaway/src/main.rs crates/spaceaway/src/terrain_integration.rs
git commit -m "feat(terrain): apply planetary gravity to ship, wire colliders into game loop"
```

---

### Task 5: Integration testing and verification

**Files:**
- Various (fixes only)

- [ ] **Step 1: Run all tests**

```bash
cargo test --workspace
```

Expected: all tests pass including new gravity tests.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -p sa_terrain -- -D warnings
cargo clippy -p sa_physics -- -D warnings
```

Fix any warnings.

- [ ] **Step 3: Visual test checklist**

Run the game and verify:

1. Press 8 to teleport to a system, Tab to lock a planet, cruise toward it
2. As you approach: ship should feel gravity pulling it toward the planet
3. Gravity should increase as altitude decreases
4. Ship should NOT pass through the terrain surface (collision works)
5. If you cut thrust, ship should fall toward the planet surface
6. Ship should come to rest on terrain (no bouncing through)
7. Flying away: gravity should fade, colliders should be removed
8. No physics jitter or teleporting (anchor rebase working correctly)
9. Performance: steady 60fps with colliders active

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix(terrain): Phase 2 integration testing fixes"
```

---

## Self-Review

**Spec coverage (Phase 2):**

| Spec Requirement | Task |
|-----------------|------|
| HeightField colliders near player | Task 3 |
| Physics anchor rebasing at 100m | Task 3 |
| Post-rebase sync calls | Task 3 |
| Gravity altitude blending | Task 1 |
| Surface gravity formula | Task 1 |
| Antiparallel slerp guard | Task 1 |
| atmosphere_top = 1.2× radius | Task 1 |
| TERRAIN collision group | Task 2 |
| Collider removal on eviction | Task 2, 3 |
| Ship gravity force application | Task 4 |
| Cleanup on deactivation | Task 4 |
| 500m collision radius | Task 3 |

**Phase 3+ items correctly excluded:** No landing detection, no landed state, no surface walking, no ship kinematic mode.

**Placeholder scan:** No TBD/TODO found. All methods have complete code.

**Type consistency:** `GravityState` used consistently. `TerrainFrameResult` extended with gravity field. `update()` signature matches between declaration and call site (after Task 4 updates main.rs).
