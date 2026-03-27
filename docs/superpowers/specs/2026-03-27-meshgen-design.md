# Mesh Generation System — Design Spec

## Overview

A programmatic mesh generation system that lets Claude Code produce low-poly 3D assets directly in Rust. No external tools (Blender, OpenSCAD) — everything compiles into the engine. Designed for AI-agent productivity: pure functions, parameters in → vertices out, deterministic, testable.

## Architecture

New crate `sa_meshgen` at the Engine layer. Three modules:

### primitives.rs — Shape Generators

Each function returns `MeshData` (the engine's existing mesh type: `Vec<Vertex>` + `Vec<u32>`). All shapes are flat-shaded with per-face vertex colors. Triangle count targets B-C range (20-200 triangles per part).

**Primitives:**
- `box_mesh(width, height, depth, color)` — cuboid with 6 faces (12 triangles). The fundamental building block.
- `cylinder_mesh(radius, height, sides, color)` — N-sided cylinder with caps. 8 sides = 48 tri, 12 sides = 72 tri.
- `cone_mesh(base_radius, top_radius, height, sides, color)` — frustum/cone. top_radius=0 for a point.
- `wedge_mesh(width, height, depth, color)` — triangular prism. For ramps, angled hull sections, roof lines.
- `arc_mesh(inner_r, outer_r, height, angle_degrees, sides, color)` — curved wall section. For rounded corridors, engine housings.

**Mesh utilities:**
- `merge(meshes: &[MeshData]) -> MeshData` — combine multiple meshes into one (concatenate vertices, offset indices).
- `transform(mesh: &MeshData, matrix: Mat4) -> MeshData` — apply translation/rotation/scale to all vertices and normals.
- `color_all(mesh: &mut MeshData, color: [f32; 3])` — recolor all vertices.
- `flip_normals(mesh: &mut MeshData)` — invert normals (for interior faces of rooms).

### csg.rs — Boolean Operations

Uses the `csgrs` crate for constructive solid geometry. Wraps csgrs types with conversion to/from our `MeshData`.

- `csg_union(a: &MeshData, b: &MeshData) -> MeshData`
- `csg_difference(a: &MeshData, b: &MeshData) -> MeshData`
- `csg_intersect(a: &MeshData, b: &MeshData) -> MeshData`

These are expensive — use only when boolean operations add genuine value (cutting holes, complex intersections). For simple assembly, `merge()` + `transform()` is preferred.

### assembly.rs — Modular Part Composition

Connection-point-based assembly system for snapping parts together.

**ConnectPoint:**
```
struct ConnectPoint {
    id: &'static str,    // e.g., "fore", "aft", "port_door", "starboard_door"
    position: Vec3,       // local-space position of the connection
    normal: Vec3,         // outward-facing direction (which way this opening faces)
}
```

**Part:**
```
struct Part {
    mesh: MeshData,
    connections: Vec<ConnectPoint>,
}
```

**Assembly:** `attach(base: &Part, base_conn: &str, attach: &Part, attach_conn: &str) -> MeshData`

Snaps `attach` to `base` by aligning the two named connection points: the attach part is translated so its connection point meets the base's connection point, and rotated so the normals face opposite directions (parts face each other at the join).

## Ship Part Catalog

Minimal set for the Phase 5 ship:

| Part | Dimensions | Connections | Description |
|------|-----------|-------------|-------------|
| hull_corridor | 3m × 2.5m × 2m | fore, aft | Rectangular tube, open both ends |
| hull_room | 5m × 5m × 3m | fore, aft, port, starboard (optional) | Box room with selectable door openings |
| hull_cockpit | 5m × 4m × 2.5m, tapered | aft | Angled front with window panel |
| hull_engine | 4m × 3m × 3m | fore | Rear section with nacelle geometry |
| hull_airlock | 2m × 2.5m × 2m | inner, outer | Small room with two door frames |
| console | 1.2m × 0.8m × 0.6m | — | Angled control panel for stations |
| floor_grate | variable × 0.05m × variable | — | Flat walkable panel |
| wall_panel | variable × variable × 0.1m | — | Interior wall segment |
| door_frame | 1.5m × 2m × 0.2m | — | Rectangular frame for doorways |

Ship layout (bow to stern):
```
cockpit → corridor → room(nav+sensors) → corridor → room(engineering) → corridor → engine_room
                                                                      ↳ airlock
```

## Color Palette

Flat vertex colors, no textures. Consistent palette:
- Hull exterior: dark gray `[0.3, 0.3, 0.35]`
- Interior walls: medium gray `[0.5, 0.5, 0.55]`
- Floor: dark `[0.25, 0.25, 0.28]`
- Console screens: blue-tint `[0.2, 0.3, 0.5]`
- Helm accent: blue `[0.2, 0.4, 0.7]`
- Engineering accent: amber `[0.7, 0.5, 0.2]`
- Sensors accent: purple `[0.5, 0.2, 0.6]`
- Navigation accent: green `[0.2, 0.6, 0.4]`
- Engine accent: red `[0.7, 0.2, 0.2]`
- Airlock: warning yellow `[0.7, 0.6, 0.1]`

## Testing Strategy

Every primitive and part is a pure function → fully testable without GPU.

**Primitive tests:**
- Vertex count matches expected (e.g., box = 24 vertices, 36 indices)
- No degenerate triangles (area > 0)
- All normals point outward (dot with centroid-to-vertex > 0)
- Bounding box matches input dimensions

**CSG tests:**
- Difference of overlapping boxes reduces volume
- Union of two boxes produces valid mesh
- Result has no orphaned vertices

**Assembly tests:**
- Connected parts share edge at connection point (no visible gap)
- Attached part is correctly oriented (normals face away from join)

**Visual test:** Key 6 in the game binary cycles through all generated parts, rendering each one individually for visual inspection.

## Dependencies

- `csgrs` crate for boolean operations
- `sa_math` for Vec3, Mat4 (via glam)
- `sa_render` for Vertex and MeshData types (or define compatible types and convert)

## What This Does NOT Include

- Texture UV generation (flat colors only)
- LOD generation (single detail level per part)
- Animated parts (doors open/close is a game logic concern, not mesh generation)
- Procedural ship generation (the ship layout is hand-authored in code, parts are procedural)
