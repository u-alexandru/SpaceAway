# Visor HUD Design

Minimal, immersive heads-up display projected on the suit's visor glass. Everything on-screen comes from the suit — one cohesive visual system that can fail.

---

## 1. Design Philosophy

- **Diegetic first**: ship information lives on in-ship monitors. The HUD is the SUIT, not the game.
- **Minimal**: crosshair, suit vitals, locked target. Nothing else.
- **Visor aesthetic**: cool blue-green holographic tint, slightly transparent, projected-on-glass feel. Sci-fi font (Orbitron). Not flat UI — embedded in the world.
- **Can fail**: when suit power drops, the visor degrades (flicker, jitter, color shift, death).

---

## 2. HUD Elements

| Element | Position | Always visible? | Trigger |
|---------|----------|----------------|---------|
| **Crosshair** | Screen center | Yes (when cursor grabbed) | Always in gameplay |
| **Suit O2** | Bottom-left corner | Yes (very dim when full) | Always |
| **Suit Power** | Bottom-right corner | Yes (very dim when full) | Always |
| **Locked target reticle** | Projected at target screen position | Only when target locked | Tab key locks star |
| **Warning vignette** | Screen edges | Below 80% suit O2 or power | Automatic |
| **Visor degradation** | Full screen | Below 20% suit power | Automatic |

### Crosshair
- Small thin ring (3-4px radius) with a center dot
- Visor blue-green color: `rgba(120, 220, 210, 0.6)`
- Subtle glow: draw twice — dim/larger ring behind, sharp/smaller in front

### Suit O2 (bottom-left)
- Text: `O2 98%` in Orbitron font
- At 100%: very dim (`alpha 0.15`) — you forget it's there
- Below 50%: brighter (`alpha 0.5`), color shifts toward amber
- Below 20%: bright, pulses, red tint
- Position: bottom-left, ~40px from edges, slight perspective skew to feel like glass projection

### Suit Power (bottom-right)
- Text: `PWR 100%` in Orbitron font
- Same alpha/color behavior as O2
- Position: bottom-right, mirrored from O2

### Locked Target Reticle
- Thin ring (~12px radius) at the target's projected screen position
- Inside: small text with catalog name + distance: `SEC 0042.0017 / 4.2 ly`
- Same visor blue-green color
- When target is off-screen: small chevron `>` at the screen edge pointing toward it
- Glow effect: same double-draw as crosshair

### Warning Vignette
- Red-orange gradient at screen edges
- Intensity scales with how critical the situation is:
  - O2 < 80% or Power < 80%: very subtle glow
  - O2 < 30% or Power < 30%: visible red tinge
  - O2 < 10% or Power < 10%: pulsing red, hard to ignore
- Rendered as a fullscreen egui rect with radial gradient (darker center, colored edges)

### Visor Degradation (suit power < 20%)
- **20-10%**: occasional flicker (text alpha drops to 0 for 1-2 frames randomly)
- **10-5%**: frequent flicker + position jitter (text offset by 1-3px randomly) + color shift from blue-green to amber/yellow
- **5-0%**: heavy flicker, strong jitter, color desaturating
- **0%**: visor dies with a sweep animation (text fades from top to bottom over 0.5 seconds). Then nothing. Black edges of screen, no HUD at all.

**Future (documented, not built now):**
- Glass frost/fog creep at screen edges when suit thermal fails (needs shader)

---

## 3. Suit Resources

New `SuitResources` struct (separate from `ShipResources`):

```rust
pub struct SuitResources {
    pub oxygen: f32,    // 0.0-1.0, drains when ship O2 unavailable
    pub power: f32,     // 0.0-1.0, drains when ship power unavailable
}
```

### Behavior
- **Ship powered + O2 available**: suit stays at 100% (recharged by ship)
- **Ship O2 lost**: suit O2 drains at 0.002/s (~8 minutes of emergency air)
- **Ship power lost**: suit power drains at 0.001/s (~16 minutes of battery)
- **Suit O2 at 0**: player starts taking damage (future) or blacks out
- **Suit power at 0**: visor dies, no HUD, still alive but blind to instruments

### Integration
- Updated each frame alongside `ShipResources`
- Ship recharges suit at 0.01/s when resources available (fast recharge)
- Suit never drains while ship is functional (no gameplay impact during normal operation)

---

## 4. Visor Font

**Orbitron** — free geometric sci-fi font from Google Fonts.
- License: Open Font License (free for any use)
- Download TTF, place in `resources/fonts/Orbitron-Regular.ttf`
- Load into egui via `ctx.fonts()` at startup
- Use for ALL visor text (O2, Power, target info, crosshair labels)
- Size scaled by `font_scale()` for screen resolution

**Fallback**: if font fails to load, use egui's default monospace.

---

## 5. Rendering Approach

All visor elements rendered via **egui Painter API** in the existing HUD render pass:

### Glow effect (double-draw)
```
// Outer glow (larger, dimmer, blurred feel)
painter.circle_stroke(center, radius + 2.0, Stroke::new(3.0, glow_color_dim));
// Inner sharp (smaller, brighter)
painter.circle_stroke(center, radius, Stroke::new(1.5, glow_color_bright));
```

### Glass projection feel
- All text at `alpha 0.5-0.7` (never fully opaque — you see through it)
- Slight green-blue tint: base color `rgb(120, 220, 210)`
- When warning: shifts to `rgb(220, 160, 80)` (amber) then `rgb(220, 80, 60)` (red)

### Screen-space target projection
- Compute target's galactic position → camera-relative offset → project with view_proj matrix → screen XY
- If behind camera (clip.w < 0): show edge chevron instead
- If off-screen: clamp to screen edge with directional chevron

---

## 6. Architecture

### Files

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/sa_survival/src/suit.rs` | Create | SuitResources struct + drain/recharge logic |
| `crates/spaceaway/src/ui/visor.rs` | Create | All visor HUD drawing (crosshair, vitals, target, vignette, degradation) |
| `crates/spaceaway/src/ui/mod.rs` | Modify | Load Orbitron font, call visor rendering |
| `crates/spaceaway/src/ui/hud.rs` | Modify | Remove old crosshair/fuel bars, delegate to visor |
| `crates/spaceaway/src/main.rs` | Modify | Update suit resources, pass to HUD, project target position |
| `resources/fonts/Orbitron-Regular.ttf` | Create | Downloaded font file (committed) |

### Data flow

```
main.rs each frame:
  1. Update SuitResources from ShipResources state
  2. Project locked target to screen position (view_proj * world_pos)
  3. Pass VisorState to HUD render:
     - suit_o2, suit_power
     - target_screen_pos (Option<[f32;2]>), target_name, target_distance
     - crosshair visible (cursor_grabbed)
  4. visor.rs draws everything via egui Painter
```

---

## 7. Future Enhancements (documented, not built)

- **Glass frost shader**: when suit thermal regulation fails (power 0%), frost creeps from screen edges. Needs a dedicated post-process shader.
- **Helmet breath fog**: brief fog on visor when exiting airlock into vacuum.
- **Coop companion markers**: visor shows teammate positions when in multiplayer.
- **Damage indicators**: directional damage markers on visor edges when ship is hit.
- **Compass ring**: subtle compass bearing around the crosshair for orientation.
