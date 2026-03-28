# Smooth Navigation Design

Fixes three gameplay flow problems: lock-on targeting the wrong star, warp overshooting destinations, and the speed dead zone after overshooting.

---

## 1. Smart Lock-On

**Problem:** Tab locks the nearest star by distance, not what the player is looking at.

**Fix:** Lock the star with the smallest angle from camera forward direction.

Algorithm:
```
for each nearby star:
    dir = normalize(star_pos - camera_pos)
    alignment = dot(camera_forward, dir)

best = star with highest alignment where alignment > 0.7
```

Only considers stars in a ~45° forward cone (dot > 0.7). If nothing is in the cone, no lock. The visor reticle shows where the locked target is — point the ship at it.

---

## 2. Predictive Gravity Well

**Problem:** At max warp (0.158 ly/s), the ship travels 390 AU per frame. The 50 AU gravity well check misses entirely — the ship teleports through the system.

**Fix:** Ray-segment intersection. Each warp frame:

```
segment_start = galactic_position_before
segment_end = galactic_position_after (= start + velocity * dt)

for each nearby star:
    closest_point_on_segment = project star onto segment
    distance = |closest_point - star_pos|
    if distance < 50 AU:
        place ship at 50 AU from star on the approach side
        disengage warp
```

This guarantees the ship ALWAYS stops at the well boundary, regardless of speed. No frame-rate dependent behavior.

---

## 3. Targeted Deceleration

**Problem:** Warp drops you at full speed — either you overshoot or you're going too fast for a clean arrival.

**Fix:** When locked on a target and in warp, automatically reduce speed based on distance:

| Distance to target | Effective speed |
|-------------------|----------------|
| > 1.0 ly | Full warp speed (throttle-controlled) |
| 1.0 → 0.1 ly | Linear ramp: 100% → 10% of warp speed |
| 0.1 → 0.001 ly | Ramp: 10% → ~cruise equivalent |
| < 50 AU (0.0008 ly) | Disengage into system |

Formula: `effective_speed = warp_speed * clamp(distance_ly / 1.0, 0.01, 1.0)`

### Visual and audio during deceleration:
- Star streaks gradually shorten (speed drives streak_factor)
- Warp tunnel fades as speed drops below warp visual threshold
- Computer voice: "approaching destination" when deceleration begins (~1 ly out)
- Helm monitor ETA counts down accurately using the deceleration curve
- Final moment: clean drop into system with the existing flash effect

### Deceleration duration:
At max warp approaching from far away:
- At 1 ly: speed = 0.158 ly/s → crosses 1 ly in ~6 seconds while decelerating
- At 0.1 ly: speed = 0.016 ly/s → crosses 0.1 ly in ~6 seconds
- Total approach from 1 ly: ~12-15 seconds of visible deceleration
- Feels like a real arrival sequence, not a teleport

---

## 4. Free Warp (No Target)

When warping without a locked target:

- **No deceleration** — no target to approach, full speed until manual disengage
- **Predictive gravity well still active** — ray-segment check catches any star in the path
- **3-second warning** before gravity well intersection:
  - Computer voice: "proximity alert"
  - Visor flashes amber
  - Gives the player a moment of "wait, what's that?"
- **At well boundary:** sudden drop (no smooth decel — this was unplanned)
- **If no stars:** fly until manual disengage or exotic fuel exhaustion

The 3-second warning is computed by: checking if the ship will intersect a gravity well within `3 * current_speed * dt` lookahead distance.

---

## 5. Audio Triggers

| Event | Voice/Sound | Priority |
|-------|------------|----------|
| Target locked (Tab) | SFX: confirmation beep | — |
| Deceleration starts (~1 ly) | Voice: "approaching destination" | Medium |
| System entry (targeted) | Voice: "all systems ready" | Medium |
| Proximity alert (free warp, 3s warning) | Voice: "alert" + alarm SFX | High |
| System entry (free warp, unplanned) | Voice: "alert" | High |

---

## 6. Implementation Changes

All modifications to existing files:

| File | Change |
|------|--------|
| `navigation.rs` | Smart lock-on: angle-based targeting instead of distance-based |
| `navigation.rs` | Predictive gravity well: ray-segment intersection instead of point-in-sphere |
| `navigation.rs` | 3-second proximity warning for free warp |
| `drive_integration.rs` | Deceleration curve: reduce effective warp speed based on distance to locked target |
| `main.rs` | Wire deceleration into galactic_position update, trigger voice/audio at approach milestones |
| `catalog.rs` | Add new voice/SFX IDs if needed |

No new crates, no new modules. Focused refinement of existing systems.

---

## 7. Edge Cases

- **Target behind you:** deceleration doesn't apply if `dot(velocity_dir, target_dir) < 0` — you're flying away from the target, not toward it
- **Multiple gravity wells:** predictive check finds the FIRST intersection along the segment
- **Lock-on during warp:** allowed. Deceleration kicks in immediately based on new target distance
- **Disengage warp manually during deceleration:** works normally, drops to impulse wherever you are
- **Target star reached, system loaded, player warps again:** the old system unloads, new warp begins. Deceleration applies to new target if locked.
