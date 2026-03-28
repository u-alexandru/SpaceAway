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

### 3.2 Thick Bulkheads (v2)

v2 uses thick bulkheads (0.3m depth) with visible door frame inner walls:
- Two bulkhead faces (fore + aft) separated by BULKHEAD_DEPTH
- **Centered on the section boundary** — extends half the depth each direction
- Door frame jambs and lintel are **direct quads** connecting the fore/aft door opening edges

See rule B-5 through B-8 below for the critical construction rules.

### 3.3 Placement Rules

| Rule  | Requirement |
|-------|-------------|
| B-1   | Every fore/aft connection MUST have a bulkhead at its z position |
| B-2   | Bulkhead interior_width matches the part's interior width at that face |
| B-3   | Terminal faces (cockpit nose, engine rear) use hex_cap, not bulkheads |
| B-4   | Bulkhead is placed at z=0 for fore connections, z=length for aft connections |
| B-5   | Thick bulkheads MUST be **centered** on the section boundary (±half depth), never extending entirely into one section |
| B-6   | Door frame inner walls (jambs, lintel) MUST use **direct quads** between fore/aft door opening corners — NEVER use `box_mesh` which creates overlapping faces with the bulkhead and causes Z-fighting |
| B-7   | No geometry from the door frame may be coplanar with the bulkhead hex face — all frame geometry must be perpendicular to the bulkhead plane |
| B-8   | Bulkhead hex face width must match the hull width at that boundary, but frame inner walls sit at the door opening edges only |

### 3.4 Why Bulkheads Replace Door Frames

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
| V-TS1 | No co-planar duplicate faces with same color and opposing normals (R-TS6) |

---

## 8. Two-Sided Rendering Standard

Backface culling is disabled for the entire render pipeline. This means every
triangle is visible from both the front and back side. The geometry shader uses
`@builtin(front_facing)` to flip normals for back-faces, ensuring correct
lighting from both viewing directions.

### 8.1 Rules

| Rule  | Requirement |
|-------|-------------|
| R-TS1 | Backface culling is DISABLED. All triangles are visible from both sides. |
| R-TS2 | The geometry shader uses `@builtin(front_facing)` to flip normals for back-faces, ensuring correct lighting from both sides. |
| R-TS3 | Mesh generators should produce faces with normals pointing toward the PRIMARY viewing direction (exterior = outward, interior = inward from the room). |
| R-TS4 | Single-sided panels (floors, ceilings, bulkheads) are acceptable -- they render correctly from both sides due to R-TS2. |
| R-TS5 | The `hex_hull()` double-sided generation (exterior + 0.05m inset interior) is REQUIRED for hull panels because exterior and interior have different COLORS. A single face can only have one color, so two faces with different colors are needed. |
| R-TS6 | For same-color surfaces viewed from both sides (floors, bulkheads), a SINGLE face is sufficient. Do NOT duplicate geometry with flipped normals for same-color surfaces. |
| R-TS7 | When two faces MUST overlap at the same position (unavoidable), add a 0.02-0.05m offset between them to prevent Z-fighting. |
| R-TS8 | The `ambient` lighting term in the shader prevents back-faces from being completely black even before the front_facing fix -- but the fix makes them properly lit. |
| R-TS9 | When building composite geometry (e.g. thick bulkhead = two faces + frame), NEVER use `box_mesh` for structural elements that share edges with another mesh. The box vertices overlap with the parent mesh vertices, causing Z-fighting. Use **direct quads** (`push_quad`) with vertices placed exactly at the shared edge positions instead. |

### 8.2 Impact on Interior Geometry

The following interior surfaces are single-sided and rely on R-TS2 for correct
back-face rendering:

| Surface               | Normal direction | Single-sided? | Notes |
|-----------------------|------------------|---------------|-------|
| Floor                 | +Y (upward)      | Yes           | Visible from below through sub-floor gaps |
| Ceiling               | -Y (downward)    | Yes           | Visible from above through over-ceiling gaps |
| Interior side walls   | Inward (+/-X)    | Yes           | Visible from hull side |
| Bulkheads             | -Z (fore-facing) | Yes           | Visible from both sides of the doorway |
| Hull exterior         | Outward          | No (R-TS5)    | Paired with interior face at different color |
| Hull interior         | Inward           | No (R-TS5)    | Paired with exterior face at different color |

Bulkheads previously had front and back faces with the same color. Per R-TS6,
the back faces have been removed -- the shader handles back-face lighting
automatically.

---

## Appendix: Constants

```rust
const FLOOR_Y: f32      = -1.0;   // Interior floor Y position
const CEILING_Y: f32    = 1.2;    // Interior ceiling Y position (2.2m headroom)
const WALL_INSET: f32   = 0.15;   // Interior wall offset from hull edge
const DOOR_W: f32       = 1.2;    // Standard door width
const DOOR_H: f32       = 2.0;    // Standard door height
```
