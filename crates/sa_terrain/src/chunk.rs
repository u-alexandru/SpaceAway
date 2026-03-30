//! Chunk mesh generation: 33×33 grid on a cube-sphere face, with skirt vertices.

use std::sync::OnceLock;

use crate::{ChunkData, ChunkKey, ChunkType, TerrainConfig, TerrainVertex};
use crate::cube_sphere::{CubeFace, cube_to_sphere};
use crate::heightmap::{make_terrain_noise, make_warp_noise, sample_height};
use crate::biome::biome_color;

// Re-export so downstream code that imports from chunk still works.
pub use crate::config::{GRID_SIZE, CELLS};

/// Cached shared index buffer — all terrain chunks use the same topology.
/// Generated once on first access, then reused for every chunk.
static SHARED_INDICES: OnceLock<Vec<u32>> = OnceLock::new();

/// Generate the shared index buffer for all terrain chunks.
/// All 33×33 grid chunks + skirts use identical index connectivity.
fn generate_shared_indices() -> Vec<u32> {
    let n = GRID_SIZE as usize;

    // Grid indices: 32×32 quads, 2 triangles each
    let mut indices: Vec<u32> = Vec::with_capacity(CELLS as usize * CELLS as usize * 6 + 4 * (n - 1) * 6);
    for row in 0..CELLS as usize {
        for col in 0..CELLS as usize {
            let i00 = (row * n + col) as u32;
            let i10 = ((row + 1) * n + col) as u32;
            let i01 = (row * n + col + 1) as u32;
            let i11 = ((row + 1) * n + col + 1) as u32;
            indices.push(i00);
            indices.push(i10);
            indices.push(i01);
            indices.push(i01);
            indices.push(i10);
            indices.push(i11);
        }
    }

    // Skirt indices
    let skirt_base = (n * n) as u32;
    let skirt_bottom_base = skirt_base;
    let skirt_top_base = skirt_bottom_base + n as u32;
    let skirt_left_base = skirt_top_base + n as u32;
    let skirt_right_base = skirt_left_base + n as u32;

    fn add_skirt_quad(indices: &mut Vec<u32>, g0: u32, g1: u32, s0: u32, s1: u32) {
        indices.push(g0); indices.push(s0); indices.push(g1);
        indices.push(g1); indices.push(s0); indices.push(s1);
    }

    for col in 0..n - 1 {
        let g0 = col as u32;
        let g1 = (col + 1) as u32;
        add_skirt_quad(&mut indices, g0, g1, skirt_bottom_base + col as u32, skirt_bottom_base + col as u32 + 1);
    }
    for col in 0..n - 1 {
        let g0 = ((n - 1) * n + col) as u32;
        let g1 = ((n - 1) * n + col + 1) as u32;
        add_skirt_quad(&mut indices, g0, g1, skirt_top_base + col as u32, skirt_top_base + col as u32 + 1);
    }
    for row in 0..n - 1 {
        let g0 = (row * n) as u32;
        let g1 = ((row + 1) * n) as u32;
        add_skirt_quad(&mut indices, g0, g1, skirt_left_base + row as u32, skirt_left_base + row as u32 + 1);
    }
    for row in 0..n - 1 {
        let g0 = (row * n + (n - 1)) as u32;
        let g1 = ((row + 1) * n + (n - 1)) as u32;
        add_skirt_quad(&mut indices, g0, g1, skirt_right_base + row as u32, skirt_right_base + row as u32 + 1);
    }

    indices
}

/// Get the shared index buffer for all terrain chunks.
/// Thread-safe, lazy-initialized on first call.
pub fn shared_indices() -> &'static [u32] {
    SHARED_INDICES.get_or_init(generate_shared_indices)
}

/// Generate the full mesh for a terrain chunk.
///
/// The chunk occupies a (CELLS × CELLS) patch on the cube face identified by
/// `key`. Positions are stored patch-local (f32, relative to the chunk center).
#[profiling::function]
pub fn generate_chunk(key: ChunkKey, config: &TerrainConfig) -> ChunkData {
    let face = CubeFace::ALL[key.face as usize];

    // How many chunks tile one face edge at this LOD level.
    let tiles = 1u32 << key.lod;

    // UV extent for this chunk: map [0, tiles] → [-1, +1].
    let tile_size = 2.0 / tiles as f64;
    let u_start = -1.0 + key.x as f64 * tile_size;
    let v_start = -1.0 + key.y as f64 * tile_size;

    // Noise setup.
    let noise = make_terrain_noise(config.noise_seed);
    let warp = make_warp_noise(config.noise_seed);

    // Frequency scale: MUST be constant across all LOD levels so the same
    // world point produces the same height regardless of LOD. Variable freq
    // causes height mismatches and visible seams at LOD boundaries.
    let freq_scale = 2.0;

    let amplitude = config.radius_m * config.displacement_fraction as f64;

    // -----------------------------------------------------------------------
    // Pass 1: sample heights and compute displaced world positions (f64).
    // -----------------------------------------------------------------------
    let n = GRID_SIZE as usize;
    let mut dirs = vec![[0.0f64; 3]; n * n];
    let mut heights = vec![0.0f32; n * n];
    // World positions (f64, planet-centred metres) before we subtract centre.
    let mut world_pos = vec![[0.0f64; 3]; n * n];

    for row in 0..n {
        for col in 0..n {
            let u = u_start + col as f64 / CELLS as f64 * tile_size;
            let v = v_start + row as f64 / CELLS as f64 * tile_size;

            let dir = cube_to_sphere(face, u, v);
            let h = sample_height(&noise, &warp, dir, freq_scale);

            // Center displacement around radius: h ∈ [0,1] → (h-0.5) maps
            // to [-0.5, 0.5], so terrain is symmetric about the base radius.
            let r = config.radius_m + (h as f64 - 0.5) * amplitude;
            let pos = [dir[0] * r, dir[1] * r, dir[2] * r];

            let idx = row * n + col;
            dirs[idx] = dir;
            heights[idx] = h;
            world_pos[idx] = pos;
        }
    }

    // -----------------------------------------------------------------------
    // Chunk center: average of the 4 grid corners (f64 precision).
    // -----------------------------------------------------------------------
    let c00 = world_pos[0];
    let c01 = world_pos[n - 1];
    let c10 = world_pos[(n - 1) * n];
    let c11 = world_pos[(n - 1) * n + (n - 1)];
    let center_f64 = [
        (c00[0] + c01[0] + c10[0] + c11[0]) * 0.25,
        (c00[1] + c01[1] + c10[1] + c11[1]) * 0.25,
        (c00[2] + c01[2] + c10[2] + c11[2]) * 0.25,
    ];

    // -----------------------------------------------------------------------
    // Convert to patch-local f32.
    // -----------------------------------------------------------------------
    let mut local_pos = vec![[0.0f32; 3]; n * n];
    for i in 0..n * n {
        local_pos[i] = [
            (world_pos[i][0] - center_f64[0]) as f32,
            (world_pos[i][1] - center_f64[1]) as f32,
            (world_pos[i][2] - center_f64[2]) as f32,
        ];
    }

    // -----------------------------------------------------------------------
    // min / max height
    // -----------------------------------------------------------------------
    let mut min_height = f32::MAX;
    let mut max_height = f32::MIN;
    for &h in &heights {
        if h < min_height { min_height = h; }
        if h > max_height { max_height = h; }
    }

    // -----------------------------------------------------------------------
    // Biome colors.
    // -----------------------------------------------------------------------
    let mut colors = vec![[0.0f32; 3]; n * n];
    for i in 0..n * n {
        let dir = dirs[i];
        // latitude: 0 at equator (dir.y == 0), 1 at pole (|dir.y| == 1).
        let latitude = dir[1].abs() as f32;
        colors[i] = biome_color(config.sub_type, heights[i], latitude);
    }

    // Grid + skirt indices are shared across all chunks (same topology).
    // Cloned from the cached shared buffer instead of regenerated per chunk.

    // -----------------------------------------------------------------------
    // Smooth normals: accumulate face normals per vertex then normalise.
    // -----------------------------------------------------------------------
    let mut normals = vec![[0.0f32; 3]; n * n];
    for row in 0..CELLS as usize {
        for col in 0..CELLS as usize {
            let i00 = row * n + col;
            let i10 = (row + 1) * n + col;
            let i01 = row * n + col + 1;
            let i11 = (row + 1) * n + col + 1;

            let p00 = local_pos[i00];
            let p10 = local_pos[i10];
            let p01 = local_pos[i01];
            let p11 = local_pos[i11];

            // Triangle 1: i00, i10, i01
            let n1 = face_normal(p00, p10, p01);
            for &vi in &[i00, i10, i01] {
                normals[vi][0] += n1[0];
                normals[vi][1] += n1[1];
                normals[vi][2] += n1[2];
            }

            // Triangle 2: i01, i10, i11
            let n2 = face_normal(p01, p10, p11);
            for &vi in &[i01, i10, i11] {
                normals[vi][0] += n2[0];
                normals[vi][1] += n2[1];
                normals[vi][2] += n2[2];
            }
        }
    }

    // Normalise grid normals.
    for nm in normals.iter_mut() {
        let len = (nm[0] * nm[0] + nm[1] * nm[1] + nm[2] * nm[2]).sqrt();
        if len > 1e-8 {
            nm[0] /= len;
            nm[1] /= len;
            nm[2] /= len;
        }
    }

    // -----------------------------------------------------------------------
    // Morph targets: parent-LOD positions for CDLOD vertex morphing.
    // -----------------------------------------------------------------------
    let mut morph_targets = vec![[0.0f32; 3]; n * n];
    for gy in 0..n {
        for gx in 0..n {
            let idx = gy * n + gx;
            let even_x = gx % 2 == 0;
            let even_y = gy % 2 == 0;
            if even_x && even_y {
                morph_targets[idx] = local_pos[idx];
            } else if !even_x && even_y {
                let left = gy * n + (gx - 1);
                let right = gy * n + (gx + 1).min(n - 1);
                morph_targets[idx] = avg_pos(local_pos[left], local_pos[right]);
            } else if even_x && !even_y {
                let top = (gy - 1) * n + gx;
                let bottom = ((gy + 1).min(n - 1)) * n + gx;
                morph_targets[idx] = avg_pos(local_pos[top], local_pos[bottom]);
            } else {
                let tl = (gy - 1) * n + (gx - 1);
                let tr = (gy - 1) * n + (gx + 1).min(n - 1);
                let bl = ((gy + 1).min(n - 1)) * n + (gx - 1);
                let br = ((gy + 1).min(n - 1)) * n + (gx + 1).min(n - 1);
                morph_targets[idx] = [
                    (local_pos[tl][0] + local_pos[tr][0] + local_pos[bl][0] + local_pos[br][0]) * 0.25,
                    (local_pos[tl][1] + local_pos[tr][1] + local_pos[bl][1] + local_pos[br][1]) * 0.25,
                    (local_pos[tl][2] + local_pos[tr][2] + local_pos[bl][2] + local_pos[br][2]) * 0.25,
                ];
            }
        }
    }

    // -----------------------------------------------------------------------
    // Build grid vertex array.
    // -----------------------------------------------------------------------
    let mut vertices: Vec<TerrainVertex> = Vec::with_capacity(n * n + 4 * n);
    for i in 0..n * n {
        vertices.push(TerrainVertex {
            position: local_pos[i],
            color: colors[i],
            normal: normals[i],
            morph_target: morph_targets[i],
        });
    }

    // -----------------------------------------------------------------------
    // Skirt: one vertex per edge vertex, pushed inward toward planet centre.
    //
    // "Toward planet centre" in patch-local space = -(world_pos - center).
    // We drop the skirt vertex by at least 1.0 m along that direction.
    // -----------------------------------------------------------------------
    // Skirt drop: enough to hide LOD seams but not visible from below.
    // Scales with LOD: at coarse levels, displacement is large (mountains),
    // at fine levels it's small (local terrain variation). Always capped at
    // 500m to prevent the previous bug where Earth-sized planets had 254km
    // skirt drops (radius_m * displacement_fraction * 2 = 254,840m).
    let subdivs_at_lod = (1u64 << key.lod) as f64;
    let face_size = 2.0 * config.radius_m / subdivs_at_lod;
    let displacement_at_lod = (config.radius_m * config.displacement_fraction as f64)
        .min(face_size * 0.5);
    let skirt_drop_max = (displacement_at_lod * 2.0).min(500.0);
    let skirt_drop_min = 2.0_f32;

    let skirt_base = vertices.len() as u32; // = n*n

    // Helper: index of skirt vertex for a given edge grid index.
    // They are appended in four passes: bottom row, top row, left col, right col.
    // We track where each starts.
    let skirt_bottom_base = skirt_base;                   // n verts (row 0)
    let skirt_top_base = skirt_bottom_base + n as u32;    // n verts (row n-1)
    let skirt_left_base = skirt_top_base + n as u32;      // n verts (col 0)
    let skirt_right_base = skirt_left_base + n as u32;    // n verts (col n-1)

    // Four edge passes: (grid_idx, skirt_base_for_this_edge)
    let edge_passes: [(Vec<usize>, u32); 4] = [
        // bottom row
        ((0..n).collect(), skirt_bottom_base),
        // top row
        ((0..n).map(|col| (n - 1) * n + col).collect(), skirt_top_base),
        // left column
        ((0..n).map(|row| row * n).collect(), skirt_left_base),
        // right column
        ((0..n).map(|row| row * n + (n - 1)).collect(), skirt_right_base),
    ];

    for (grid_indices, _skirt_start) in &edge_passes {
        for &gi in grid_indices {
            let lp = local_pos[gi];
            // Direction toward planet centre = -sphere_direction (radially inward).
            // Using dirs[gi] (unit sphere direction) ensures the drop is radial,
            // not tangential. Previous code used -local_pos which is toward the
            // chunk center (tangential), not toward the planet center.
            let nx = -dirs[gi][0] as f32;
            let ny = -dirs[gi][1] as f32;
            let nz = -dirs[gi][2] as f32;

            let drop = (skirt_drop_max as f32).max(skirt_drop_min);

            let skirt_pos = [
                lp[0] + nx * drop,
                lp[1] + ny * drop,
                lp[2] + nz * drop,
            ];
            // Skirt inherits the color and normal of the edge vertex.
            // Skirts don't morph — morph_target equals position.
            vertices.push(TerrainVertex {
                position: skirt_pos,
                color: colors[gi],
                normal: normals[gi],
                morph_target: skirt_pos,
            });
        }
    }

    ChunkData {
        key,
        center_f64,
        vertices,
        indices: shared_indices().to_vec(),
        heights,
        min_height,
        max_height,
        chunk_type: ChunkType::Heightmap,
    }
}

/// Average two positions (midpoint).
#[inline]
fn avg_pos(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

/// Compute the face normal for a triangle (a, b, c) using cross product.
/// Returns a non-normalised (area-weighted) normal for smooth accumulation.
#[inline]
fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TerrainConfig;
    use sa_universe::PlanetSubType;

    fn test_config() -> TerrainConfig {
        TerrainConfig {
            radius_m: 6_371_000.0,
            noise_seed: 42,
            sub_type: PlanetSubType::Temperate,
            displacement_fraction: 0.02,
        }
    }

    #[test]
    fn chunk_has_correct_grid_vertices() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        assert!(chunk.vertices.len() >= 1089, "too few vertices: {}", chunk.vertices.len());
        assert!(chunk.vertices.len() < 1300, "too many vertices: {}", chunk.vertices.len());
    }

    #[test]
    fn chunk_has_correct_triangle_count() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        let tri_count = chunk.indices.len() / 3;
        assert!(tri_count >= 2048, "too few triangles: {tri_count}");
    }

    #[test]
    fn chunk_heights_are_33x33() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        assert_eq!(chunk.heights.len(), (GRID_SIZE * GRID_SIZE) as usize);
    }

    #[test]
    fn chunk_normals_are_normalized() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        for (i, v) in chunk.vertices.iter().take(1089).enumerate() {
            let len = (v.normal[0] * v.normal[0] + v.normal[1] * v.normal[1] + v.normal[2] * v.normal[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "vertex {i} normal not normalized: len={len}",
            );
        }
    }

    #[test]
    fn deterministic_chunks() {
        let key = ChunkKey { face: 4, lod: 5, x: 3, y: 7 };
        let config = test_config();
        let c1 = generate_chunk(key, &config);
        let c2 = generate_chunk(key, &config);
        assert_eq!(c1.vertices.len(), c2.vertices.len());
        for (a, b) in c1.heights.iter().zip(c2.heights.iter()) {
            assert!((a - b).abs() < 1e-6, "heights differ");
        }
    }

    #[test]
    fn min_max_height_valid() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        assert!(chunk.min_height <= chunk.max_height);
        assert!(chunk.min_height >= 0.0);
        assert!(chunk.max_height <= 1.0);
    }

    #[test]
    fn skirt_drop_capped_at_500m() {
        // Earth-sized planet: radius=6,371km, displacement=0.02
        // OLD bug: drop = 6,371,000 * 0.02 * 2 = 254,840m (254km!)
        // FIX: drop should be capped at 500m max
        let config = test_config(); // radius=6,371,000, displacement=0.02
        let n = GRID_SIZE as usize;
        let skirt_start = n * n; // = 1089

        // For each LOD, compare skirt vertices to their corresponding edge
        // vertex. The distance between them is the actual skirt drop.
        for lod in [0u8, 5, 10, 15] {
            let key = ChunkKey { face: 0, lod, x: 0, y: 0 };
            let chunk = generate_chunk(key, &config);

            // Bottom-row skirt: vertices [skirt_start..skirt_start+33]
            // correspond to grid row 0: vertices [0..33]
            let mut max_drop = 0.0_f32;
            for col in 0..n {
                let grid_v = &chunk.vertices[col];
                let skirt_v = &chunk.vertices[skirt_start + col];
                let dx = skirt_v.position[0] - grid_v.position[0];
                let dy = skirt_v.position[1] - grid_v.position[1];
                let dz = skirt_v.position[2] - grid_v.position[2];
                let drop_dist = (dx * dx + dy * dy + dz * dz).sqrt();
                if drop_dist > max_drop {
                    max_drop = drop_dist;
                }
            }

            assert!(
                max_drop <= 600.0,
                "LOD {lod}: max skirt drop = {max_drop:.0}m exceeds 600m (500m cap + margin)",
            );
            assert!(
                max_drop >= 2.0,
                "LOD {lod}: skirt drop {max_drop:.1}m below 2m minimum",
            );
        }
    }

    #[test]
    fn even_vertices_morph_to_self() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        let n = GRID_SIZE as usize;
        for gy in (0..n).step_by(2) {
            for gx in (0..n).step_by(2) {
                let idx = gy * n + gx;
                let v = &chunk.vertices[idx];
                assert_eq!(v.position, v.morph_target,
                    "even vertex ({gx},{gy}) morph_target should equal position");
            }
        }
    }

    #[test]
    fn odd_vertices_morph_to_neighbor_average() {
        let key = ChunkKey { face: 4, lod: 5, x: 0, y: 0 };
        let chunk = generate_chunk(key, &test_config());
        let left = &chunk.vertices[0];
        let right = &chunk.vertices[2];
        let mid = &chunk.vertices[1];
        let expected = [
            (left.position[0] + right.position[0]) * 0.5,
            (left.position[1] + right.position[1]) * 0.5,
            (left.position[2] + right.position[2]) * 0.5,
        ];
        for i in 0..3 {
            assert!((mid.morph_target[i] - expected[i]).abs() < 1e-4,
                "odd vertex morph_target[{i}] = {}, expected {}", mid.morph_target[i], expected[i]);
        }
    }
}
