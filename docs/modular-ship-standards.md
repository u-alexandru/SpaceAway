# Modular Ship Construction Standards

Engineering specification for all mesh generation in `sa_meshgen`.
Every part builder, connection point, and assembly operation MUST conform to this document.

---

## 1. Root Cause Analysis

### 1.1 The hex_ring() function

`hex_ring(width, height, z)` produces six vertices centered on the XY origin at the
given Z position. For the standard corridor (width=4.0, height=3.0):

| Index | Label        | Formula                          | Position (w=4, h=3) |
|-------|--------------|----------------------------------|----------------------|
| 0     | top-left     | (-w*0.375, +h*0.5, z)           | (-1.500, +1.500, z) |
| 1     | top-right    | (+w*0.375, +h*0.5, z)           | (+1.500, +1.500, z) |
| 2     | right        | (+w*0.5,    0.0,   z)           | (+2.000,  0.000, z) |
| 3     | bottom-right | (+w*0.375, -h*0.5, z)           | (+1.500, -1.500, z) |
| 4     | bottom-left  | (-w*0.375, -h*0.5, z)           | (-1.500, -1.500, z) |
| 5     | left         | (-w*0.5,    0.0,   z)           | (-2.000,  0.000, z) |

The hex ring center is at (0.0, 0.0, z). The Y centroid of the six vertices is 0.0
because they are vertically symmetric about Y=0.

For the wider room (width=5.0, height=3.0):

| Index | Label        | Position (w=5, h=3) |
|-------|--------------|----------------------|
| 0     | top-left     | (-1.875, +1.500, z) |
| 1     | top-right    | (+1.875, +1.500, z) |
| 2     | right        | (+2.500,  0.000, z) |
| 3     | bottom-right | (+1.875, -1.500, z) |
| 4     | bottom-left  | (-1.875, -1.500, z) |
| 5     | left         | (-2.500,  0.000, z) |

Key observation: `height` is always 3.0 across all parts, so the Y coordinates of
the ring are fixed at +1.5, 0.0, -1.5 regardless of width. Only X changes with width.

### 1.2 The hex_hull() function

`hex_hull(front_width, back_width, height, length, color)` builds the hull:
- Front ring at z=0 with `front_width`
- Back ring at z=`length` with `back_width`
- Six exterior quad strips connecting front to back
- Six interior quad strips offset 0.05m inward along the face normal

This means each part's hull occupies local z=[0, length]. This is correct.

### 1.3 The attach() function

`attach(base, base_conn_id, attach_part, attach_conn_id)` does:

1. Computes target direction: `target_dir = -base_conn.normal`
2. Computes rotation: `rot = rotation_between(attach_conn.normal, target_dir)`
3. Applies rotation to attach connection position: `rotated_pos = rot * attach_conn.position`
4. Computes translation: `translation = base_conn.position - rotated_pos`
5. Builds transform: `translate * rotate`
6. Transforms all vertices and remaining connection points of the attach part

### 1.4 Concrete trace: corridor -> transition (the failure case)

**Setup:**

- Corridor: `hull_corridor(3.0)` -- width=4.0, length=3.0
  - fore connection: position=(0, 0, 0), normal=(0, 0, -1)
  - aft connection: position=(0, 0, 3), normal=(0, 0, +1)
  - Hull spans z=0 to z=3, hex ring width=4.0 at both ends

- Transition: `hull_transition(4.0, 5.0, 1.0)` -- front=4.0, back=5.0, length=1.0
  - fore connection: position=(0, 0, 0), normal=(0, 0, -1)
  - aft connection: position=(0, 0, 1), normal=(0, 0, +1)
  - Hull spans z=0 to z=1, ring width=4.0 at z=0, width=5.0 at z=1

**attach(corridor, "aft", transition, "fore"):**

1. base_conn = corridor.aft: position=(0,0,3), normal=(0,0,+1)
2. attach_conn = transition.fore: position=(0,0,0), normal=(0,0,-1)
3. target_dir = -(0,0,+1) = (0,0,-1)
4. rot = rotation_between((0,0,-1), (0,0,-1)) = IDENTITY (already aligned)
5. rotated_attach_pos = IDENTITY * (0,0,0) = (0,0,0)
6. translation = (0,0,3) - (0,0,0) = (0,0,3)
7. transform = translate(0,0,3) * IDENTITY

**Result:** The transition part is translated +3 in Z. Its hull, which was at
local z=[0,1], is now at world z=[3,4].

At the boundary z=3:
- Corridor's aft ring: `hex_ring(4.0, 3.0, 3.0)` -- width=4.0 at z=3
- Transition's fore ring after transform: `hex_ring(4.0, 3.0, 0.0)` shifted to z=3 -- width=4.0 at z=3

**These rings match exactly.** The vertex positions are identical at the seam.

### 1.5 The ACTUAL failure: hull_room() with built-in transitions

The `hull_room()` function embeds its own fore and aft transitions internally:

```
z=0..1:   fore transition  STD_WIDTH(4.0) -> ROOM_WIDTH(5.0)
z=1..6:   main room body   ROOM_WIDTH(5.0)
z=6..7:   aft transition   ROOM_WIDTH(5.0) -> STD_WIDTH(4.0)
```

The room's connection points are:
- fore: position=(0, 0, 0), normal=(0, 0, -1)
- aft: position=(0, 0, 7), normal=(0, 0, +1)

The ring at the fore face (z=0) has width=4.0 (STD_WIDTH), and the ring at the aft
face (z=7) also has width=4.0 (STD_WIDTH). So when attached to a standard corridor,
the widths match.

**This architecture is internally consistent for fore/aft connections.** However, the
redundancy creates two problems:

**Problem 1: Double transitions.** If a caller also inserts a standalone
`hull_transition()` between a corridor and a room, the ship gets TWO transition
segments in series (the standalone one AND the room's built-in one). The test in
`ship_parts.rs` (line 618-650) does exactly this:

```rust
let trans1 = hull_transition(STD_WIDTH, ROOM_WIDTH, 1.0);   // standalone 4->5
let nav_room = hull_room("nav", ...);                        // has BUILT-IN 4->5
let ship = attach(&ship, "aft", &trans1, "fore");            // attaches 4->5
let ship = attach(&ship, "aft", &nav_room, "fore");          // attaches 4->5->5
```

At the seam between `trans1.aft` and `nav_room.fore`:
- trans1's aft ring: width=5.0 (ROOM_WIDTH) at trans1's z=1
- nav_room's fore ring: width=4.0 (STD_WIDTH) at nav_room's z=0

**WIDTH MISMATCH: 5.0 vs 4.0.** This creates a visible gap/overlap at the boundary.

The standalone transition widens from 4.0 to 5.0, then the room's built-in transition
ALSO widens from 4.0 to 5.0. The caller expected the room to start at 5.0 but it
starts at 4.0.

**Problem 2: Side (port/starboard) connection points.** The port connection is at:

```rust
position: Vec3::new(-ROOM_WIDTH / 2.0, 0.0, trans_len + room_len / 2.0)
         = (-2.5, 0.0, 3.5)
normal: Vec3::NEG_X  // (-1, 0, 0)
```

An airlock (width=2.5, height=3.0) attached via `attach(ship, "starboard", airlock,
"inner")` would need to be rotated so that its -Z normal aligns with +X. The
`rotation_between` function computes a 90-degree rotation around Y. But the airlock's
hex cross-section is in the XY plane -- after a 90-degree Y rotation, the hex ring
now lies in the YZ plane. The hex ring vertices, which assumed a Z-axis corridor, are
now perpendicular to the hull wall.

The airlock's hex ring at its "inner" face has width=2.5, but the room's hull surface
at the port connection is not a flat plane of width 2.5 -- it is a curved hex surface.
There is no matching hole cut in the room's hull. The airlock geometry simply
intersects the room hull without any proper opening.

### 1.6 Summary of Root Causes

| # | Bug | Where | Impact |
|---|-----|-------|--------|
| 1 | hull_room() embeds transitions, but callers ALSO insert standalone transitions | ship_parts.rs test, lines 618-630 | Width mismatch (5.0 vs 4.0) at room boundaries; visible seam gaps |
| 2 | Side connections (port/starboard) assume a flat interface but the hex hull has no cutout | hull_room() lines 298-310 | Airlock geometry intersects room hull; no proper doorway |
| 3 | Inconsistent design contract: some parts include transitions, some do not | hull_room() vs hull_cockpit() | Caller cannot predict what width to expect at a connection face |

---

## 2. Standard Coordinate System

All parts are built in a local coordinate frame with the following conventions:

### 2.1 Axes

| Axis | Direction    | Ship term     |
|------|-------------|---------------|
| +X   | Starboard   | Right         |
| -X   | Port        | Left          |
| +Y   | Dorsal      | Up            |
| -Y   | Ventral     | Down          |
| +Z   | Aft/Stern   | Backward      |
| -Z   | Fore/Bow    | Forward       |

### 2.2 Part Origin

The origin of every part is at the **center of the fore hex face**:
- X = 0.0 (centered horizontally)
- Y = 0.0 (centered vertically on the hex ring -- NOT on the floor)
- Z = 0.0 (fore face of the part)

### 2.3 Hull Extent

Every hex hull section spans from **z=0** (fore) to **z=length** (aft) in local
coordinates. The fore ring is at z=0; the aft ring is at z=length.

### 2.4 Vertical Reference Points

All values relative to Y=0 (hex ring center):

| Feature       | Y position | Derivation                       |
|---------------|------------|----------------------------------|
| Hull top      | +1.500     | h * 0.5 = 3.0 * 0.5             |
| Ceiling       | +1.200     | CEILING_Y constant               |
| Hex center    |  0.000     | Origin                           |
| Floor         | -1.000     | FLOOR_Y constant                 |
| Hull bottom   | -1.500     | -h * 0.5 = -3.0 * 0.5           |

The walkable interior height is: ceiling - floor = 1.2 - (-1.0) = **2.2 meters**.

### 2.5 Hex Ring Geometry

The hex cross-section is a vertically-symmetric hexagon with flat top and bottom:

```
          [0]----------[1]           y = +h/2
         /                \
        /                  \
      [5]                [2]         y = 0
        \                  /
         \                /
          [4]----------[3]           y = -h/2

  x: -w*0.5  -w*0.375      +w*0.375  +w*0.5
```

The hex is NOT a regular hexagon. The top and bottom edges are shorter than the side
edges. The ratio 0.375 = 3/8 determines the "flatness" of the top/bottom.

---

## 3. Connection Point Standard

### 3.1 Connection Point Definition

Every connection point has three properties:

| Property   | Type  | Description                                            |
|------------|-------|--------------------------------------------------------|
| id         | str   | Unique name within the part (e.g., "fore", "aft")      |
| position   | Vec3  | Center of the hex face at the boundary, in local space |
| normal     | Vec3  | Unit vector pointing OUTWARD from the part             |

### 3.2 Standard Connection IDs and Positions

For a part with width `w` at the fore face, width `w_aft` at the aft face, and
length `L`:

| ID          | Position                    | Normal      | Hex ring width at face |
|-------------|-----------------------------| ------------|------------------------|
| fore        | (0, 0, 0)                  | (0, 0, -1)  | w (fore width)         |
| aft         | (0, 0, L)                  | (0, 0, +1)  | w_aft (aft width)      |
| port        | (-w_mid/2, 0, z_mid)       | (-1, 0, 0)  | N/A (see Section 7)    |
| starboard   | (+w_mid/2, 0, z_mid)       | (+1, 0, 0)  | N/A (see Section 7)    |
| dorsal      | (0, +h/2, z_mid)           | (0, +1, 0)  | N/A (reserved)         |
| ventral     | (0, -h/2, z_mid)           | (0, -1, 0)  | N/A (reserved)         |

Where `w_mid` is the hull width at z_mid, and `z_mid` is defined per-part.

### 3.3 Connection Matching Rule

**Two parts connecting at a fore/aft boundary MUST produce identical hex ring vertices
at the shared face.**

Formally: if Part A has an aft connection at width `w_a` and Part B has a fore
connection at width `w_b`, then attaching A.aft to B.fore is valid if and only if
`w_a == w_b` (and both use the same height).

After the attach transform, the six vertices of A's aft ring and B's fore ring must
be pairwise identical to within epsilon = 1e-4 meters.

### 3.4 Connection Point Position Rule

The connection point position MUST be at the geometric center of the hex face:
- For fore/aft: (0, 0, z) where z is the face's Z coordinate
- The hex ring center IS (0, 0, z) because hex_ring() is symmetric about both X and Y

This is currently satisfied by the code: all fore/aft connections are at (0, 0, z).

### 3.5 Width Metadata (New Requirement)

Each fore/aft connection point MUST carry its hex ring width so that the validation
system can check compatibility without reconstructing the mesh:

```rust
pub struct ConnectPoint {
    pub id: &'static str,
    pub position: Vec3,
    pub normal: Vec3,
    pub width: f32,    // NEW: hex ring width at this face
    pub height: f32,   // NEW: hex ring height at this face (usually 3.0)
}
```

---

## 4. Part Construction Rules

Every part builder function MUST follow these rules:

### 4.1 Hull Geometry

| Rule | Requirement |
|------|-------------|
| 4.1.1 | Hull spans from z=0 to z=length in local coordinates |
| 4.1.2 | Fore hex ring at z=0 with the part's fore width |
| 4.1.3 | Aft hex ring at z=length with the part's aft width |
| 4.1.4 | Height is always STD_HEIGHT (3.0) unless explicitly documented |
| 4.1.5 | All exterior face normals point OUTWARD from the hull |
| 4.1.6 | All interior face normals point INWARD toward the interior |
| 4.1.7 | Interior hull faces are offset 0.05m inward along the face normal |

### 4.2 Connection Points

| Rule | Requirement |
|------|-------------|
| 4.2.1 | "fore" connection at position=(0, 0, 0), normal=(0, 0, -1), width=fore_width |
| 4.2.2 | "aft" connection at position=(0, 0, length), normal=(0, 0, +1), width=aft_width |
| 4.2.3 | Terminal parts (cockpit, engine) may omit fore or aft but MUST have at least one |
| 4.2.4 | Connection width MUST exactly equal the hex ring width at that face |
| 4.2.5 | Side connections require a cutout (see Section 7) |

### 4.3 Interior

| Rule | Requirement |
|------|-------------|
| 4.3.1 | Floor at y = FLOOR_Y (-1.0) |
| 4.3.2 | Ceiling at y = CEILING_Y (+1.2) |
| 4.3.3 | Side walls inset WALL_INSET (0.15m) from the hull's [5]-[2] X extent |
| 4.3.4 | Floor/ceiling/wall width is based on the NARROWER end if tapered |
| 4.3.5 | Interior spans the full z=[0, length] of the part |

### 4.4 No Embedded Transitions

| Rule | Requirement |
|------|-------------|
| 4.4.1 | A part MUST NOT embed transition geometry for adjacent parts |
| 4.4.2 | Each part's fore and aft faces are at a single, well-defined width |
| 4.4.3 | Width changes between parts are handled EXCLUSIVELY by transition pieces |

This is the critical rule that `hull_room()` violates. The room must be refactored to
have a constant width of 5.0 at both fore and aft, with separate transition parts
handling the 4.0-to-5.0 change.

### 4.5 Caps

| Rule | Requirement |
|------|-------------|
| 4.5.1 | A terminal face (no connection) MUST have a cap (hex_cap) |
| 4.5.2 | A connecting face (has a connection point) MUST NOT have a cap |
| 4.5.3 | Front caps use flip=false; back caps use flip=true |

---

## 5. Validation Functions Spec

### 5.1 validate_part(part) -> Result<(), Vec<String>>

Checks a single part in isolation. Returns Ok(()) if all checks pass, or Err with a
list of violation descriptions.

**Checks:**

| Check | Description |
|-------|-------------|
| V-P1  | Part has at least one connection point |
| V-P2  | No two connection points share the same id |
| V-P3  | Every "fore" connection has position.z == 0.0 (within epsilon) |
| V-P4  | Every "fore" connection has normal == (0, 0, -1) (within epsilon) |
| V-P5  | Every "aft" connection has normal == (0, 0, +1) (within epsilon) |
| V-P6  | Mesh has no degenerate triangles (area > 1e-6) |
| V-P7  | All mesh indices are in bounds |
| V-P8  | Mesh bounding box min.z >= -epsilon (no geometry behind the fore face) |
| V-P9  | If a "fore" connection exists with width W, then hex_ring(W, H, 0.0) vertices all appear in the mesh (within epsilon) |
| V-P10 | If an "aft" connection exists with width W at z=L, then hex_ring(W, H, L) vertices all appear in the mesh (within epsilon) |
| V-P11 | Connection point positions lie on the mesh bounding box boundary (within epsilon) |
| V-P12 | Every connection point normal is a unit vector |
| V-TS1 | No co-planar duplicate faces with same color and opposing normals (per R-TS6). Detects unnecessary double-sided geometry that should be single-sided. |

### 5.2 validate_connection(part_a, conn_a, part_b, conn_b) -> Result<(), Vec<String>>

Checks that two parts can legally connect at the specified connection points.

**Checks:**

| Check | Description |
|-------|-------------|
| V-C1  | conn_a.width == conn_b.width (within epsilon) |
| V-C2  | conn_a.height == conn_b.height (within epsilon) |
| V-C3  | conn_a.normal and conn_b.normal are anti-parallel after the attach transform would be applied (dot product == -1 within epsilon) |
| V-C4  | After simulating the attach transform, the six hex ring vertices at the boundary are pairwise equal (within epsilon = 1e-4) |
| V-C5  | Neither connection id is empty or unrecognized |

### 5.3 validate_assembly(parts_with_connections) -> Result<(), Vec<String>>

Checks a fully assembled ship.

**Input:** A list of (Part, transform) pairs representing the assembled ship, plus a
list of (part_index_a, conn_id_a, part_index_b, conn_id_b) describing which
connections were used.

**Checks:**

| Check | Description |
|-------|-------------|
| V-A1  | Every individual part passes validate_part() |
| V-A2  | Every used connection pair passes validate_connection() |
| V-A3  | No connection point is used more than once |
| V-A4  | The assembly graph is connected (every part is reachable) |
| V-A5  | The merged mesh has no degenerate triangles |
| V-A6  | The merged mesh bounding box is within expected ranges (configurable) |
| V-A7  | No two parts have overlapping bounding boxes beyond a threshold (optional, for detecting accidental intersections) |

---

## 6. Test Requirements

### 6.1 Per-Part Tests

Every part function MUST have tests that:

| Test | Description |
|------|-------------|
| T-P1 | `validate_part(part)` returns Ok(()) |
| T-P2 | Correct number of connection points |
| T-P3 | Connection ids match expected set |
| T-P4 | Mesh is non-empty (vertices > 0, indices > 0) |
| T-P5 | No degenerate triangles |
| T-P6 | Bounding box dimensions match expected (length, width, height within 10%) |
| T-P7 | Fore connection (if present) at z=0 with correct width |
| T-P8 | Aft connection (if present) at z=length with correct width |

### 6.2 Connection Pair Tests

Every pair of parts that can legally connect MUST have a test:

| Test | Description |
|------|-------------|
| T-C1 | `validate_connection()` returns Ok(()) |
| T-C2 | After attach(), the merged mesh bounding box Z-span equals sum of part lengths |
| T-C3 | Hex ring vertices at the boundary are pairwise identical (max distance < 1e-4) |
| T-C4 | The remaining connection points are correctly transformed |

**Mandatory connection pair test matrix:**

| Part A          | A conn | Part B           | B conn | Shared width |
|-----------------|--------|------------------|--------|--------------|
| cockpit         | aft    | corridor(3.0)    | fore   | 4.0          |
| corridor(3.0)   | aft    | transition(4->5) | fore   | 4.0          |
| transition(4->5)| aft    | room             | fore   | 5.0          |
| room            | aft    | transition(5->4) | fore   | 5.0          |
| transition(5->4)| aft    | corridor(3.0)    | fore   | 4.0          |
| corridor(3.0)   | aft    | engine_section   | fore   | 4.0          |

### 6.3 Full Ship Assembly Test

| Test | Description |
|------|-------------|
| T-A1 | `validate_assembly()` returns Ok(()) |
| T-A2 | Total Z-span within expected range (28-40m depending on configuration) |
| T-A3 | Maximum width matches ROOM_WIDTH (5.0m / 2 = 2.5m from center) |
| T-A4 | No NaN or Inf in any vertex position |

### 6.4 Vertex Position Assertions at Boundaries

For every connection boundary in the full ship, a test MUST assert that the hex ring
vertices match. Example for corridor(4.0) -> transition(4.0->5.0) at z=7.0:

```rust
let expected_ring = hex_ring(4.0, 3.0, 7.0);
// expected_ring[0] = (-1.500, +1.500, 7.0)
// expected_ring[1] = (+1.500, +1.500, 7.0)
// expected_ring[2] = (+2.000,  0.000, 7.0)
// expected_ring[3] = (+1.500, -1.500, 7.0)
// expected_ring[4] = (-1.500, -1.500, 7.0)
// expected_ring[5] = (-2.000,  0.000, 7.0)
```

For each expected vertex, the test scans the merged mesh and asserts that at least
one vertex exists within epsilon=1e-4 of that position.

---

## 7. Width Transition Rules

### 7.1 Chosen Approach: Standalone Transition Pieces (Option A)

Width changes between sections are handled EXCLUSIVELY by dedicated transition parts.
No part may embed transitions for its neighbors.

### 7.2 Transition Part Specification

`hull_transition(from_width, to_width, length)` produces a part where:

- Fore face: hex_ring at width=from_width, z=0
- Aft face: hex_ring at width=to_width, z=length
- fore connection: width=from_width
- aft connection: width=to_width
- The six hull quad strips linearly interpolate between the two ring sizes

### 7.3 Room Refactoring (Required)

`hull_room()` MUST be refactored to:
- Hull at constant ROOM_WIDTH (5.0) from z=0 to z=room_length
- fore connection at z=0, width=ROOM_WIDTH (5.0)
- aft connection at z=room_length, width=ROOM_WIDTH (5.0)
- NO built-in transitions

The caller assembles width changes explicitly:

```rust
// CORRECT: caller inserts transitions
let corridor   = hull_corridor(3.0);              // width=4.0
let trans_in   = hull_transition(4.0, 5.0, 1.0);  // 4.0 -> 5.0
let room       = hull_room("nav", color, &[]);     // width=5.0
let trans_out  = hull_transition(5.0, 4.0, 1.0);  // 5.0 -> 4.0

let ship = attach(&corridor, "aft", &trans_in, "fore");  // seam at 4.0
let ship = attach(&ship, "aft", &room, "fore");          // seam at 5.0
let ship = attach(&ship, "aft", &trans_out, "fore");     // seam at 5.0
```

### 7.4 Vertex Interpolation in Transitions

Within a transition piece of length L, from `w_fore` to `w_aft`, the hex ring at any
intermediate z (0 <= z <= L) has width:

```
w(z) = w_fore + (w_aft - w_fore) * (z / L)
```

The height remains constant at STD_HEIGHT. Only the X coordinates of the hex vertices
change; Y coordinates remain fixed.

For the standard transition (4.0 -> 5.0, length=1.0):

| z   | width | vertex[2].x (right) | vertex[5].x (left) |
|-----|-------|---------------------|---------------------|
| 0.0 | 4.0   | +2.000              | -2.000              |
| 0.5 | 4.5   | +2.250              | -2.250              |
| 1.0 | 5.0   | +2.500              | -2.500              |

### 7.5 Side Connections (Port/Starboard)

Side connections are currently NOT supported by the hex hull geometry. Attaching a
part to a side connection requires:

1. **A cutout in the host part's hull** at the connection location, shaped to match
   the attached part's cross-section
2. **A collar/adapter mesh** that bridges the hex hull surface to the attached part's
   hex face
3. **Proper normal handling** so the attached part's fore axis aligns with the side
   direction

Until cutout generation is implemented, side connections should be treated as
**structural attachment points only** (the attached part's mesh overlaps the host hull
without a proper opening). This MUST be documented on any part that exposes side
connections.

Future implementation:
- Define a rectangular or hex-shaped cutout region on the host hull
- Remove triangles inside the cutout region
- Generate a collar mesh bridging the cutout edge to the attached part's hex ring
- The collar handles the topology mismatch between the hex surface and the attached
  hex face

---

## 8. Two-Sided Rendering Standard

Backface culling is disabled (`cull_mode: None` in `pipeline.rs`), meaning every
triangle is visible from both sides. The geometry shader compensates for this with
a `@builtin(front_facing)` normal flip so that lighting is correct regardless of
viewing direction.

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

### 8.2 Rationale

Without R-TS2, back-faces receive zero diffuse lighting because `dot(normal,
light_dir)` is negative when the normal points away from the camera. The
`select(-n, n, front_facing)` flip in the fragment shader ensures the normal
always faces the camera, producing correct diffuse lighting on both sides.

This eliminates the need for duplicate geometry on same-color surfaces (R-TS6),
halving the triangle count for bulkheads, floors, ceilings, and interior walls.
Hull panels (`hex_hull()`) still require two faces because exterior and interior
have different colors (R-TS5).

### 8.3 Performance

Every triangle with backface culling disabled is rasterized from both sides,
doubling fragment workload for overlapping geometry. Removing unnecessary
duplicate faces (R-TS6) partially compensates by halving the triangle count for
those surfaces.

---

## Appendix A: Hex Ring Vertex Tables

### A.1 Standard Corridor (width=4.0, height=3.0)

```
Vertex  X        Y        Z
[0]    -1.500   +1.500   z
[1]    +1.500   +1.500   z
[2]    +2.000    0.000   z
[3]    +1.500   -1.500   z
[4]    -1.500   -1.500   z
[5]    -2.000    0.000   z
```

### A.2 Wide Room (width=5.0, height=3.0)

```
Vertex  X        Y        Z
[0]    -1.875   +1.500   z
[1]    +1.875   +1.500   z
[2]    +2.500    0.000   z
[3]    +1.875   -1.500   z
[4]    -1.875   -1.500   z
[5]    -2.500    0.000   z
```

### A.3 Cockpit Nose (width=2.0, height=3.0)

```
Vertex  X        Y        Z
[0]    -0.750   +1.500   z
[1]    +0.750   +1.500   z
[2]    +1.000    0.000   z
[3]    +0.750   -1.500   z
[4]    -0.750   -1.500   z
[5]    -1.000    0.000   z
```

### A.4 Engine Rear (width=2.5, height=3.0)

```
Vertex  X        Y        Z
[0]    -0.9375  +1.500   z
[1]    +0.9375  +1.500   z
[2]    +1.250    0.000   z
[3]    +0.9375  -1.500   z
[4]    -0.9375  -1.500   z
[5]    -1.250    0.000   z
```

---

## Appendix B: Constants Reference

v1 constants (ship_parts.rs):
```rust
const STD_WIDTH: f32    = 4.0;    // Standard corridor/passage width
const ROOM_WIDTH: f32   = 5.0;    // Wide room width
const STD_HEIGHT: f32   = 3.0;    // Universal hex cross-section height
const FLOOR_Y: f32      = -1.0;   // Interior floor Y position
const CEILING_Y: f32    = 1.2;    // Interior ceiling Y position (2.2m headroom)
const WALL_INSET: f32   = 0.15;   // Interior wall offset from hull edge
const HULL_INSET: f32   = 0.05;   // Interior hull panel offset (Z-fighting)
const DOOR_W: f32       = 1.2;    // Standard door width
const DOOR_H: f32       = 2.0;    // Standard door height
const FRAME_THICKNESS: f32 = 0.1; // Door frame thickness
```

v2 constants (ship_parts_v2.rs):
```rust
const STD_WIDTH: f32       = 5.0;   // Standard corridor/passage width (was 4.0)
const ROOM_WIDTH: f32      = 6.5;   // Wide room width (was 5.0)
const STD_HEIGHT: f32      = 3.0;   // Universal hex cross-section height
const FLOOR_Y: f32         = -1.0;  // Interior floor Y position
const CEILING_Y: f32       = 1.2;   // Interior ceiling Y position (2.2m headroom)
const WALL_INSET: f32      = 0.15;  // Interior wall offset from hull edge
const DOOR_W: f32          = 1.4;   // Standard door width (was 1.2)
const DOOR_H: f32          = 2.1;   // Standard door height (was 2.0)
const BULKHEAD_DEPTH: f32  = 0.3;   // Thick bulkhead depth along Z
```
