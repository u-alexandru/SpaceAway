use sa_render::{MeshData, Vertex};

/// Convert sa_meshgen::Mesh -> sa_render::MeshData.
/// Simple field-by-field copy; no GPU dependencies in sa_meshgen.
pub fn meshgen_to_render(mesh: &sa_meshgen::Mesh) -> MeshData {
    let vertices = mesh
        .vertices
        .iter()
        .map(|v| Vertex {
            position: v.position,
            color: v.color,
            normal: v.normal,
        })
        .collect();
    MeshData {
        vertices,
        indices: mesh.indices.clone(),
    }
}

/// All ship parts (v2) for visual cycling.
pub fn all_ship_parts() -> Vec<(&'static str, sa_meshgen::Mesh)> {
    use sa_meshgen::ship_parts_v2::*;
    vec![
        ("cockpit_v2", hull_cockpit_v2().mesh),
        ("corridor_v2", hull_corridor_v2(4.0).mesh),
        ("transition_5_6.5", hull_transition_v2(5.0, 6.5, 1.0).mesh),
        ("nav_room_v2", hull_room_v2("nav", sa_meshgen::colors::ACCENT_NAVIGATION, &[]).mesh),
        ("eng_room_v2", hull_room_v2("eng", sa_meshgen::colors::ACCENT_ENGINEERING, &[]).mesh),
        ("transition_6.5_4", hull_transition_v2(6.5, 4.0, 1.0).mesh),
        ("engine_v2", hull_engine_section_v2().mesh),
    ]
}

/// Build the full ship using the modular assembly system (v2).
pub fn assemble_ship() -> sa_meshgen::Mesh {
    sa_meshgen::ship_parts_v2::assemble_ship_v2()
}

/// v1 ship assembly (preserved for reference).
#[allow(dead_code)]
pub fn assemble_ship_v1() -> sa_meshgen::Mesh {
    use sa_meshgen::assembly::attach;
    use sa_meshgen::ship_parts::*;

    let cockpit = hull_cockpit();
    let corr1 = hull_corridor(3.0);
    let trans1 = hull_transition(4.0, 5.0, 1.0);
    let nav_room = hull_room("nav", sa_meshgen::colors::ACCENT_NAVIGATION, &[]);
    let trans2 = hull_transition(5.0, 4.0, 1.0);
    let corr2 = hull_corridor(3.0);
    let trans3 = hull_transition(4.0, 5.0, 1.0);
    let eng_room = hull_room("eng", sa_meshgen::colors::ACCENT_ENGINEERING, &[]);
    let trans4 = hull_transition(5.0, 3.5, 1.0);
    let engine = hull_engine_section();

    let ship = attach(&cockpit, "aft", &corr1, "fore");
    let ship = attach(&ship, "aft", &trans1, "fore");
    let ship = attach(&ship, "aft", &nav_room, "fore");
    let ship = attach(&ship, "aft", &trans2, "fore");
    let ship = attach(&ship, "aft", &corr2, "fore");
    let ship = attach(&ship, "aft", &trans3, "fore");
    let ship = attach(&ship, "aft", &eng_room, "fore");
    let ship = attach(&ship, "aft", &trans4, "fore");
    let ship = attach(&ship, "aft", &engine, "fore");

    ship.mesh
}

pub fn make_cube() -> MeshData {
    type CubeFace = ([f32; 3], [f32; 3], [[f32; 3]; 4]);
    let faces: &[CubeFace] = &[
        (
            [0.0, 0.0, 1.0],
            [0.6, 0.6, 0.7],
            [
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [0.5, 0.5, 0.6],
            [
                [1.0, -1.0, -1.0],
                [-1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [1.0, 1.0, -1.0],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [0.7, 0.7, 0.8],
            [
                [-1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, -1.0],
                [-1.0, 1.0, -1.0],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [0.4, 0.4, 0.5],
            [
                [-1.0, -1.0, -1.0],
                [1.0, -1.0, -1.0],
                [1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0],
            ],
        ),
        (
            [1.0, 0.0, 0.0],
            [0.55, 0.55, 0.65],
            [
                [1.0, -1.0, 1.0],
                [1.0, -1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, 1.0, 1.0],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [0.5, 0.5, 0.6],
            [
                [-1.0, -1.0, -1.0],
                [-1.0, -1.0, 1.0],
                [-1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0],
            ],
        ),
    ];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (normal, color, verts) in faces {
        let base = vertices.len() as u32;
        for v in verts {
            vertices.push(Vertex {
                position: *v,
                color: *color,
                normal: *normal,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    MeshData { vertices, indices }
}
