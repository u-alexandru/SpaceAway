# Ship Design Guide — Construction Reference

## Design Philosophy

The ship is a small cooperative exploration vessel for 1-4 crew. Think utilitarian sci-fi: the Rocinante (The Expanse), not the Enterprise. Every surface is functional. The exterior has a coherent hull silhouette — not boxes stuck together.

## Hull Profile

All sections share a **hexagonal cross-section** (6-sided). This is the single most important design choice — it gives the ship a consistent, recognizable silhouette that looks like a spacecraft instead of Minecraft.

```
Hexagonal cross-section (looking from front, +Z toward viewer):

          ____
         /    \      <- top angled panels (30° from horizontal)
        /      \
       |        |    <- side walls (vertical)
        \      /
         \____/      <- bottom angled panels (30° from horizontal)

Width: varies by section (narrow at bow/stern, wider at rooms)
Height: 3.0m constant for all habitable sections
```

The hex profile vertices (at standard width 4.0m, height 3.0m, centered at origin):
- Top: (-1.5, 1.5, z), (1.5, 1.5, z)
- Upper sides: (-2.0, 0.5, z), (2.0, 0.5, z)
- Lower sides: (-2.0, -0.5, z), (2.0, -0.5, z)
- Bottom: (-1.5, -1.5, z), (1.5, -1.5, z)

## Standard Dimensions

| Measurement | Value |
|---|---|
| Standard hull width | 4.0m |
| Standard hull height | 3.0m |
| Cockpit width (at front) | 2.0m (tapers from 4.0m) |
| Room width | 5.0m (wider hex) |
| Corridor length | 3.0m |
| Room length | 5.0m |
| Floor height (Y=0 inside) | -1.0m from hull center |
| Ceiling height | +1.2m from floor = Y=0.2 from center |
| Wall thickness | 0.15m |
| Door width | 1.2m |
| Door height | 2.0m |

## Connection Standard

All connections happen at the hex profile boundary. Connection points are at the CENTER of the hex face at Z boundaries.

- **Fore connection**: center of hex at Z = -length/2
- **Aft connection**: center of hex at Z = +length/2
- **Port connection**: center of hex at X = -width/2 (side opening)
- **Starboard connection**: center of hex at X = +width/2 (side opening)

When sections have different widths, a **transition piece** blends between them (a short frustum-like hex that goes from narrow to wide).

## Color Palette

| Surface | Color RGB |
|---|---|
| Hull exterior (primary) | [0.35, 0.35, 0.38] dark steel |
| Hull exterior (accent panels) | [0.28, 0.30, 0.35] darker steel |
| Interior walls | [0.52, 0.54, 0.56] light gray |
| Floor | [0.30, 0.30, 0.32] dark |
| Ceiling | [0.45, 0.45, 0.48] medium |
| Helm accent | [0.15, 0.35, 0.65] blue |
| Nav accent | [0.15, 0.55, 0.35] green |
| Sensors accent | [0.45, 0.18, 0.55] purple |
| Engineering accent | [0.65, 0.45, 0.15] amber |
| Engine accent | [0.60, 0.18, 0.18] red |
| Airlock accent | [0.65, 0.55, 0.10] yellow |
| Window glass | [0.15, 0.20, 0.30] dark blue-gray |
| Console screen | [0.10, 0.25, 0.40] blue-black |

## Ship Layout (bow → stern, along +Z axis)

```
Section 1: COCKPIT (Z = 0 to 4)
  - Tapered hex: front width 2.0m → rear width 4.0m
  - Angled window panels on front faces
  - Interior: helm console, 2 seats
  - Connection: aft

Section 2: CORRIDOR (Z = 4 to 7)
  - Standard hex 4.0m wide
  - Connects cockpit to forward rooms
  - Connections: fore, aft

Section 3: NAV/SENSORS ROOM (Z = 7 to 12)
  - Wider hex 5.0m wide
  - Transition pieces on both ends (4m → 5m → 4m)
  - Interior: nav console (port), sensors console (starboard)
  - Connections: fore, aft, port (for future expansion)

Section 4: CORRIDOR (Z = 12 to 15)
  - Standard hex 4.0m
  - Connections: fore, aft

Section 5: ENGINEERING ROOM (Z = 15 to 20)
  - Wider hex 5.0m
  - Transition pieces on both ends
  - Interior: engineering console, power grid display
  - Connections: fore, aft, starboard (airlock)

Section 6: AIRLOCK (attached to engineering starboard)
  - Small hex 2.5m wide, 2.5m long
  - Two doors: inner (connects to engineering) and outer (to space)
  - Connections: inner

Section 7: CORRIDOR (Z = 20 to 23)
  - Standard hex 4.0m, narrowing to 3.0m at rear
  - Connections: fore, aft

Section 8: ENGINE ROOM (Z = 23 to 28)
  - Narrows from 3.0m to 2.0m (tapered stern)
  - Two engine nacelles (cylinders) extending from rear
  - Engine nozzles (cones) at the very back
  - Interior: engine controls, fuel display
  - Connection: fore

Total ship length: ~28m
```

## Exterior Detail Guidelines

- Add **panel line ridges**: thin raised strips along hull sections (0.02m raised, 0.1m wide)
- Add **running lights**: small colored quads at wing tips and along the spine
- **Window frames**: slightly recessed darker panels where windows are
- **Antenna**: thin cylinders on top of the cockpit
- Each section should have slightly different shade to break up the monotony

## Interior Detail Guidelines

- **Floor**: always at Y = hull_center_y - 1.0 (walking surface)
- **Ceiling**: at Y = floor + 2.2
- **Wall panels**: inset 0.15m from hull, lighter color
- **Consoles**: angled wedge shapes at stations, with colored screen face
- **Door frames**: visible frame geometry at every connection passage
- **Overhead pipes/conduits**: thin cylinders running along ceiling edges

## How Each Part Function Should Work

Each part function builds BOTH exterior hull AND interior:

```rust
fn hull_section(/* params */) -> Part {
    let mut meshes = Vec::new();

    // 1. Build exterior hex hull panels
    meshes.push(build_hex_hull(width_front, width_back, length, HULL_COLOR));

    // 2. Build interior floor
    meshes.push(build_floor(interior_width, length, FLOOR_COLOR));

    // 3. Build interior ceiling
    meshes.push(build_ceiling(interior_width, length, CEILING_COLOR));

    // 4. Build interior wall panels (inset from hull)
    meshes.push(build_interior_walls(interior_width, length, WALL_COLOR));

    // 5. Add detail: door frames, consoles, pipes
    meshes.push(build_door_frame(DOOR_WIDTH, DOOR_HEIGHT));

    // 6. Merge all
    let mesh = Mesh::merge(&meshes);

    Part { mesh, connections: vec![...] }
}
```
