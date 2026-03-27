# Interior Standards

Engineering specification for ship interior mesh generation in `sa_meshgen`.
Companion to `modular-ship-standards.md`.

---

## 1. Interior Dimensions

All values are relative to the hex hull center at Y=0.

| Feature         | Y position | Notes                                      |
|-----------------|------------|--------------------------------------------|
| Hull top        | +1.500     | h * 0.5 = 3.0 * 0.5                       |
| Ceiling         | +1.200     | CEILING_Y constant                         |
| Hull center     |  0.000     | Origin                                     |
| Floor           | -1.000     | FLOOR_Y constant                           |
| Hull bottom     | -1.500     | -h * 0.5 = -3.0 * 0.5                     |

Derived values:

| Measurement     | Value  | Derivation                                 |
|-----------------|--------|--------------------------------------------|
| Headroom        | 2.2 m  | ceiling_y - floor_y = 1.2 - (-1.0)        |
| Sub-floor space | 0.5 m  | floor_y - hull_bottom = -1.0 - (-1.5)     |
| Over-ceiling    | 0.3 m  | hull_top - ceiling_y = 1.5 - 1.2          |

The 2.2m headroom accommodates a standing human (1.8m) with 0.4m clearance.
The 0.5m sub-floor space houses machinery, wiring, and plumbing.
The 0.3m over-ceiling space houses lighting, ventilation ducting, and cabling.

---

## 2. Door Opening

| Property        | Value  |
|-----------------|--------|
| Width           | 1.2 m  |
| Height          | 2.0 m  |
| Bottom          | floor_y (-1.0) |
| Top             | floor_y + door_h = +1.0 |
| Centered on     | X = 0  |

The door opening is rectangular. Its bottom is flush with the floor.
Its top is at y=+1.0, leaving a 0.2m lintel between the door top and the ceiling.

---

## 3. Bulkhead System

At **every section boundary** (where two parts connect), there MUST be a
**bulkhead wall**: a solid panel filling the interior cross-section from floor
to ceiling, with a rectangular doorway cut out.

### 3.1 Bulkhead Geometry

The bulkhead is produced by `hull::bulkhead_with_door()`:

```rust
pub fn bulkhead_with_door(
    interior_width: f32,  // wall-to-wall interior span
    floor_y: f32,         // floor Y position (-1.0)
    ceiling_y: f32,       // ceiling Y position (+1.2)
    door_w: f32,          // door opening width (1.2)
    door_h: f32,          // door opening height (2.0)
    color: [f32; 3],      // bulkhead surface color
) -> Mesh
```

The function builds three rectangular panels (each with front and back faces):

1. **Left panel**: x from `-interior_width/2` to `-door_w/2`, y from `floor_y` to `ceiling_y`
2. **Right panel**: x from `+door_w/2` to `+interior_width/2`, y from `floor_y` to `ceiling_y`
3. **Lintel**: x from `-door_w/2` to `+door_w/2`, y from `floor_y + door_h` to `ceiling_y`

The panels are 0.1m thick (centered on z=0). The caller translates the
bulkhead to the correct Z position for each section boundary.

### 3.2 Placement Rules

| Rule  | Requirement |
|-------|-------------|
| B-1   | Every fore/aft connection MUST have a bulkhead at its z position |
| B-2   | Bulkhead interior_width matches the part's interior width at that face |
| B-3   | Terminal faces (cockpit nose, engine rear) use hex_cap, not bulkheads |
| B-4   | Bulkhead is placed at z=0 for fore connections, z=length for aft connections |

### 3.3 Why Bulkheads Replace Door Frames

The previous `door_frame_mesh()` only produced the frame bars (left, right, top)
as thin rectangles floating in space. There was no wall around the frame, so:

- The frame appeared to float in mid-air with no surrounding structure
- There was no physical separation between sections
- The interior looked like one continuous tube rather than compartmented rooms

The bulkhead fills the entire interior cross-section, providing:

- Visual separation between sections
- A solid wall that makes the doorway meaningful
- Structural character that makes the ship feel real

---

## 4. Wall Panels

The hex hull interior faces (offset 0.05m inward) serve as the outer walls.
The lower hex facets angle inward below Y=0, creating a distinctive spacecraft
hull shape. No additional rectangular wall panels are added.

**Rationale (Option A):** The angled hex hull interior IS the wall. The angular
lower sections give the ship character and read as a real spacecraft hull rather
than a rectangular room. The triangular gaps at the floor-hull junction are
features, not defects.

---

## 5. Console Placement

Consoles sit directly on the floor. The console origin is at its base, so it
should be placed at y=FLOOR_Y.

| Property        | Value  |
|-----------------|--------|
| Base height     | 0.6 m  |
| Screen height   | 0.4 m  |
| Total height    | 1.0 m  |
| Depth           | 0.5 m  |
| Base color      | [0.40, 0.40, 0.42] |
| Screen color    | station accent color |

---

## 6. Interior Color Rules

| Surface         | Color                 | RGB                      |
|-----------------|-----------------------|--------------------------|
| Floor           | dark grey             | [0.30, 0.30, 0.32]      |
| Ceiling         | medium grey           | [0.45, 0.45, 0.48]      |
| Interior walls  | light grey            | [0.52, 0.54, 0.56]      |
| Bulkheads       | slightly darker       | [0.42, 0.42, 0.45]      |
| Console base    | medium-dark grey      | [0.40, 0.40, 0.42]      |
| Console screen  | station accent color  | varies per station       |

Bulkheads are intentionally darker than the interior wall color to make
section boundaries visually distinct when walking through the ship.

---

## 7. Validation Rules for Interiors

| Rule  | Check |
|-------|-------|
| V-I1  | Floor y must be -1.0 +/- 0.01 |
| V-I2  | Ceiling y must be +1.2 +/- 0.01 |
| V-I3  | Floor width must be <= hull_width - 0.3 (doesn't poke through hull) |
| V-I4  | Every fore/aft connection must have a bulkhead_with_door at its z position |
| V-I5  | Door dimensions must be 1.2m wide x 2.0m tall |

---

## Appendix: Constants

```rust
const FLOOR_Y: f32      = -1.0;   // Interior floor Y position
const CEILING_Y: f32    = 1.2;    // Interior ceiling Y position (2.2m headroom)
const WALL_INSET: f32   = 0.15;   // Interior wall offset from hull edge
const DOOR_W: f32       = 1.2;    // Standard door width
const DOOR_H: f32       = 2.0;    // Standard door height
```
