# Smooth Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix warp overshooting, wrong lock-on targeting, and add smooth deceleration on approach — making star-to-star travel reliable and cinematic.

**Architecture:** Modify navigation.rs (angle-based lock, predictive gravity well), drive_integration.rs (deceleration curve), and main.rs (wire deceleration + proximity warnings). No new files.

**Tech Stack:** Existing navigation, drive, and audio systems.

**Spec:** `docs/superpowers/specs/2026-03-29-smooth-navigation-design.md`

---

## File Structure

| File | Change |
|------|--------|
| `crates/spaceaway/src/navigation.rs` | Angle-based lock_target, predictive check_gravity_well, proximity warning |
| `crates/spaceaway/src/drive_integration.rs` | Deceleration curve function |
| `crates/spaceaway/src/main.rs` | Wire deceleration into warp, proximity warning timer, audio triggers |

---

### Task 1: Smart angle-based lock-on + predictive gravity well

**Files:**
- Modify: `crates/spaceaway/src/navigation.rs`

- [ ] **Step 1: Replace lock_target with angle-based targeting**

Add a new method and update the Tab key handler to use it:

```rust
/// Lock the star most aligned with the camera forward direction.
/// Only considers stars within a ~45° cone (dot > 0.7).
pub fn lock_nearest_to_crosshair(&mut self, camera_forward: [f32; 3], galactic_pos: WorldPos) {
    let fwd = [camera_forward[0] as f64, camera_forward[1] as f64, camera_forward[2] as f64];
    let fwd_len = (fwd[0]*fwd[0] + fwd[1]*fwd[1] + fwd[2]*fwd[2]).sqrt();
    if fwd_len < 1e-10 { return; }
    let fwd = [fwd[0]/fwd_len, fwd[1]/fwd_len, fwd[2]/fwd_len];

    let mut best_dot = 0.7; // minimum threshold (~45° cone)
    let mut best_idx = None;

    for (i, star) in self.nearby_stars.iter().enumerate() {
        let dx = star.galactic_pos.x - galactic_pos.x;
        let dy = star.galactic_pos.y - galactic_pos.y;
        let dz = star.galactic_pos.z - galactic_pos.z;
        let len = (dx*dx + dy*dy + dz*dz).sqrt();
        if len < 0.01 { continue; }
        let dir = [dx/len, dy/len, dz/len];
        let dot = fwd[0]*dir[0] + fwd[1]*dir[1] + fwd[2]*dir[2];
        if dot > best_dot {
            best_dot = dot;
            best_idx = Some(i);
        }
    }

    if let Some(idx) = best_idx {
        self.locked_target = Some(self.nearby_stars[idx].clone());
    }
}
```

- [ ] **Step 2: Replace check_gravity_well with predictive ray-segment version**

```rust
/// Predictive gravity well check: does the line segment from pos_before to pos_after
/// pass within 50 AU of any star? Returns the star and the exact drop position.
pub fn check_gravity_well_predictive(
    &self,
    pos_before: WorldPos,
    pos_after: WorldPos,
) -> Option<(NavStar, WorldPos)> {
    let au_in_ly: f64 = 1.581e-5;
    let well_radius = 50.0 * au_in_ly;

    // Segment vector
    let seg = [
        pos_after.x - pos_before.x,
        pos_after.y - pos_before.y,
        pos_after.z - pos_before.z,
    ];
    let seg_len_sq = seg[0]*seg[0] + seg[1]*seg[1] + seg[2]*seg[2];
    if seg_len_sq < 1e-30 { return None; }

    let mut closest_t = f64::MAX;
    let mut hit_star = None;
    let mut hit_pos = pos_after;

    // Check locked target first, then all nearby stars
    let targets: Vec<&NavStar> = self.locked_target.iter()
        .chain(self.nearby_stars.iter())
        .collect();

    for star in targets {
        // Vector from segment start to star
        let to_star = [
            star.galactic_pos.x - pos_before.x,
            star.galactic_pos.y - pos_before.y,
            star.galactic_pos.z - pos_before.z,
        ];

        // Project star onto segment: t = dot(to_star, seg) / dot(seg, seg)
        let dot = to_star[0]*seg[0] + to_star[1]*seg[1] + to_star[2]*seg[2];
        let t = (dot / seg_len_sq).clamp(0.0, 1.0);

        // Closest point on segment to star
        let closest = [
            pos_before.x + seg[0] * t,
            pos_before.y + seg[1] * t,
            pos_before.z + seg[2] * t,
        ];

        let dx = closest[0] - star.galactic_pos.x;
        let dy = closest[1] - star.galactic_pos.y;
        let dz = closest[2] - star.galactic_pos.z;
        let dist = (dx*dx + dy*dy + dz*dz).sqrt();

        if dist < well_radius && t < closest_t {
            closest_t = t;
            // Place ship at well_radius from star, on the approach side
            let approach_dir = [
                pos_before.x - star.galactic_pos.x,
                pos_before.y - star.galactic_pos.y,
                pos_before.z - star.galactic_pos.z,
            ];
            let a_len = (approach_dir[0]*approach_dir[0] + approach_dir[1]*approach_dir[1] + approach_dir[2]*approach_dir[2]).sqrt();
            if a_len > 1e-10 {
                hit_pos = WorldPos::new(
                    star.galactic_pos.x + approach_dir[0]/a_len * well_radius,
                    star.galactic_pos.y + approach_dir[1]/a_len * well_radius,
                    star.galactic_pos.z + approach_dir[2]/a_len * well_radius,
                );
            }
            hit_star = Some(star.clone());
        }
    }

    hit_star.map(|star| (star, hit_pos))
}
```

- [ ] **Step 3: Add proximity warning check for free warp**

```rust
/// Check if the ship will pass through a gravity well within `lookahead` ly.
/// Returns the star if a proximity alert should fire.
pub fn check_proximity_warning(
    &self,
    pos: WorldPos,
    velocity_dir: [f64; 3],
    lookahead_ly: f64,
) -> Option<&NavStar> {
    let au_in_ly: f64 = 1.581e-5;
    let well_radius = 50.0 * au_in_ly;

    let future_pos = WorldPos::new(
        pos.x + velocity_dir[0] * lookahead_ly,
        pos.y + velocity_dir[1] * lookahead_ly,
        pos.z + velocity_dir[2] * lookahead_ly,
    );

    // Use the same ray-segment logic but with the lookahead segment
    for star in &self.nearby_stars {
        let seg = [future_pos.x - pos.x, future_pos.y - pos.y, future_pos.z - pos.z];
        let seg_len_sq = seg[0]*seg[0] + seg[1]*seg[1] + seg[2]*seg[2];
        if seg_len_sq < 1e-30 { continue; }

        let to_star = [
            star.galactic_pos.x - pos.x,
            star.galactic_pos.y - pos.y,
            star.galactic_pos.z - pos.z,
        ];
        let dot = to_star[0]*seg[0] + to_star[1]*seg[1] + to_star[2]*seg[2];
        let t = (dot / seg_len_sq).clamp(0.0, 1.0);

        let closest = [pos.x + seg[0]*t, pos.y + seg[1]*t, pos.z + seg[2]*t];
        let dx = closest[0] - star.galactic_pos.x;
        let dy = closest[1] - star.galactic_pos.y;
        let dz = closest[2] - star.galactic_pos.z;
        let dist = (dx*dx + dy*dy + dz*dz).sqrt();

        if dist < well_radius {
            return Some(star);
        }
    }
    None
}
```

- [ ] **Step 4: Add tests**

```rust
#[test]
fn lock_nearest_to_crosshair_picks_aimed_star() {
    let mut nav = Navigation::new(MasterSeed(42));
    nav.update_nearby(WorldPos::new(5.0, 0.0, 5.0));
    if nav.nearby_stars.len() >= 2 {
        // Aim at the first star
        let star = &nav.nearby_stars[0];
        let dx = (star.galactic_pos.x - 5.0) as f32;
        let dy = (star.galactic_pos.y) as f32;
        let dz = (star.galactic_pos.z - 5.0) as f32;
        let len = (dx*dx + dy*dy + dz*dz).sqrt();
        let fwd = [dx/len, dy/len, dz/len];
        nav.lock_nearest_to_crosshair(fwd, WorldPos::new(5.0, 0.0, 5.0));
        assert!(nav.locked_target.is_some());
    }
}

#[test]
fn predictive_gravity_well_catches_flythrough() {
    let mut nav = Navigation::new(MasterSeed(42));
    nav.update_nearby(WorldPos::ORIGIN);
    if let Some(star) = nav.nearby_stars.first() {
        // Fly a segment that passes through the star
        let before = WorldPos::new(star.galactic_pos.x - 0.1, star.galactic_pos.y, star.galactic_pos.z);
        let after = WorldPos::new(star.galactic_pos.x + 0.1, star.galactic_pos.y, star.galactic_pos.z);
        let result = nav.check_gravity_well_predictive(before, after);
        assert!(result.is_some(), "should detect flythrough");
    }
}
```

- [ ] **Step 5: Build and test**

Run: `cargo test -p spaceaway navigation`

- [ ] **Step 6: Commit**

```bash
git commit -m "feat: angle-based lock-on + predictive gravity well detection"
```

---

### Task 2: Deceleration curve

**Files:**
- Modify: `crates/spaceaway/src/drive_integration.rs`

- [ ] **Step 1: Add deceleration function**

```rust
/// Compute the warp deceleration multiplier based on distance to locked target.
/// Returns 1.0 (full speed) when far, ramps down to 0.01 when close.
/// Returns None if no deceleration should apply (no target or too far).
pub fn warp_deceleration(distance_to_target_ly: f64) -> f64 {
    if distance_to_target_ly > 1.0 {
        1.0 // full speed
    } else if distance_to_target_ly > 0.1 {
        // Linear ramp: 1.0 at 1ly → 0.1 at 0.1ly
        0.1 + 0.9 * ((distance_to_target_ly - 0.1) / 0.9)
    } else if distance_to_target_ly > 0.001 {
        // Ramp: 0.1 at 0.1ly → 0.01 at 0.001ly
        0.01 + 0.09 * ((distance_to_target_ly - 0.001) / 0.099)
    } else {
        0.01 // minimum
    }
}
```

- [ ] **Step 2: Add decelerated position delta function**

```rust
/// Like galactic_position_delta but with deceleration toward a target.
/// `target_distance_ly`: distance to locked target (None = no deceleration).
pub fn galactic_position_delta_decel(
    drive: &DriveController,
    direction: [f64; 3],
    dt: f64,
    target_distance_ly: Option<f64>,
) -> ([f64; 3], f64) {
    let base_speed = drive.current_speed_ly_s();
    if base_speed < 1e-20 {
        return ([0.0, 0.0, 0.0], 0.0);
    }

    let decel = target_distance_ly
        .map(|d| warp_deceleration(d))
        .unwrap_or(1.0);
    let effective_speed = base_speed * decel;

    let len = (direction[0]*direction[0] + direction[1]*direction[1] + direction[2]*direction[2]).sqrt();
    if len < 1e-10 {
        return ([0.0, 0.0, 0.0], 0.0);
    }
    let d = [direction[0]/len, direction[1]/len, direction[2]/len];

    let delta = [
        d[0] * effective_speed * dt,
        d[1] * effective_speed * dt,
        d[2] * effective_speed * dt,
    ];
    (delta, effective_speed)
}
```

- [ ] **Step 3: Add tests**

```rust
#[test]
fn deceleration_full_speed_far() {
    assert!((warp_deceleration(5.0) - 1.0).abs() < 1e-6);
}

#[test]
fn deceleration_reduced_close() {
    let d = warp_deceleration(0.5);
    assert!(d < 1.0 && d > 0.1, "at 0.5ly should be between 0.1 and 1.0, got {d}");
}

#[test]
fn deceleration_very_slow_near() {
    let d = warp_deceleration(0.01);
    assert!(d < 0.15, "at 0.01ly should be very slow, got {d}");
}
```

- [ ] **Step 4: Build and test**

Run: `cargo test -p spaceaway drive_integration`

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: warp deceleration curve — smooth approach to targets"
```

---

### Task 3: Wire into game loop + audio

**Files:**
- Modify: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Update Tab key to use angle-based lock**

Find the Tab key handler (both in the general handler and helm handler). Replace:
```rust
self.navigation.lock_target(0);
```
with:
```rust
let fwd = self.camera.forward();
self.navigation.lock_nearest_to_crosshair(
    [fwd.x, fwd.y, fwd.z],
    self.galactic_position,
);
```

- [ ] **Step 2: Replace gravity well check with predictive version**

Find where `check_gravity_well` is called during warp. Replace the point-check with the ray-segment version:

```rust
// Save position before warp movement
let pos_before = self.galactic_position;

// Apply warp movement (with deceleration)
let target_dist = self.navigation.locked_target.as_ref()
    .map(|t| self.galactic_position.distance_to(t.galactic_pos));
let (delta, effective_speed) = drive_integration::galactic_position_delta_decel(
    &self.drive, direction, dt as f64, target_dist,
);
self.galactic_position.x += delta[0];
self.galactic_position.y += delta[1];
self.galactic_position.z += delta[2];

let pos_after = self.galactic_position;

// Predictive gravity well check (catches flythroughs)
if self.active_system.is_none() {
    if let Some((nav_star, drop_pos)) = self.navigation.check_gravity_well_predictive(pos_before, pos_after) {
        self.galactic_position = drop_pos; // place at well boundary, not inside
        self.drive.request_disengage();
        // ... load system, clear target, audio ...
    }
}
```

- [ ] **Step 3: Add proximity warning for free warp**

Add a `proximity_warned: bool` field to App (prevents repeated warnings). Before the gravity well check:

```rust
// Free warp proximity warning (3 seconds lookahead)
if self.navigation.locked_target.is_none() && !self.proximity_warned {
    let lookahead = effective_speed * 3.0; // 3 seconds ahead
    if self.navigation.check_proximity_warning(
        self.galactic_position, direction_f64, lookahead
    ).is_some() {
        self.audio.announce(sa_audio::VoiceId::Alert);
        self.proximity_warned = true;
        log::info!("PROXIMITY ALERT — star ahead");
    }
}
// Reset warning flag when entering a system or disengaging
```

- [ ] **Step 4: Add "approaching destination" voice trigger**

When deceleration begins (target_dist < 1.0 ly and was > 1.0 ly last frame):

```rust
if let Some(dist) = target_dist {
    if dist < 1.0 && self.prev_target_dist.map_or(true, |d| d >= 1.0) {
        self.audio.announce(sa_audio::VoiceId::AllSystemsReady); // or a new "approaching" voice
        log::info!("Approaching destination — deceleration engaged");
    }
    self.prev_target_dist = Some(dist);
}
```

- [ ] **Step 5: Build and test manually**

Run the game:
1. Tab → should lock star you're aiming at (not nearest)
2. Engage warp → smooth deceleration in the last ~1 ly
3. Auto-drop at 50 AU — no overshoot
4. Free warp past a star → "alert" voice + auto-drop

- [ ] **Step 6: Commit**

```bash
git commit -m "feat: smooth warp navigation — deceleration, predictive wells, proximity warning"
```

---

## Summary

| Task | What it builds |
|------|---------------|
| 1 | Angle-based lock-on + predictive ray-segment gravity well + proximity warning |
| 2 | Warp deceleration curve (full speed → gradual slow → clean stop) |
| 3 | Game loop wiring — deceleration, predictive check, audio triggers |
