//! Chunk mesh generation: 33×33 grid on a cube-sphere face, with skirt vertices.

use crate::{ChunkData, ChunkKey, TerrainConfig, TerrainVertex};
use crate::cube_sphere::{CubeFace, cube_to_sphere};
use crate::heightmap::{make_terrain_noise, make_warp_noise, sample_height};
use crate::biome::biome_color;

/// Number of vertices along one edge of the chunk grid (32 cells + 1).
pub const GRID_SIZE: u32 = 33;
/// Number of cells along one edge.
pub const CELLS: u32 = 32;

/// Generate the full mesh for a terrain chunk.
///
/// The chunk occupies a (CELLS × CELLS) patch on the cube face identified by
/// `key`. Positions are stored patch-local (f32, relative to the chunk center).
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

    // Frequency scale: higher LOD → higher frequency detail.
    // At lod 0 the whole face is one chunk, lod 5 → 32 chunks/axis.
    let freq_scale = 2.0 * (1u64 << key.lod) as f64;

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

            let r = config.radius_m + h as f64 * amplitude;
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

    // -----------------------------------------------------------------------
    // Grid indices (32×32 quads, 2 triangles each).
    // -----------------------------------------------------------------------
    let mut indices: Vec<u32> = Vec::with_capacity(CELLS as usize * CELLS as usize * 6);
    for row in 0..CELLS as usize {
        for col in 0..CELLS as usize {
            let i00 = (row * n + col) as u32;
            let i10 = ((row + 1) * n + col) as u32;
            let i01 = (row * n + col + 1) as u32;
            let i11 = ((row + 1) * n + col + 1) as u32;
            // Triangle 1
            indices.push(i00);
            indices.push(i10);
            indices.push(i01);
            // Triangle 2
            indices.push(i01);
            indices.push(i10);
            indices.push(i11);
        }
    }

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
    // Build grid vertex array.
    // -----------------------------------------------------------------------
    let mut vertices: Vec<TerrainVertex> = Vec::with_capacity(n * n + 4 * n);
    for i in 0..n * n {
        vertices.push(TerrainVertex {
            position: local_pos[i],
            color: colors[i],
            normal: normals[i],
        });
    }

    // -----------------------------------------------------------------------
    // Skirt: one vertex per edge vertex, pushed inward toward planet centre.
    //
    // "Toward planet centre" in patch-local space = -(world_pos - center).
    // We drop the skirt vertex by at least 1.0 m along that direction.
    // -----------------------------------------------------------------------
    let skirt_drop_fraction = 0.002; // 0.2 % of radius
    let skirt_drop_min = 1.0_f32;

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
            // Direction toward planet centre in local space (invert the
            // vector from centre to vertex).
            let dx = -lp[0];
            let dy = -lp[1];
            let dz = -lp[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-8);
            let nx = dx / dist;
            let ny = dy / dist;
            let nz = dz / dist;

            let drop = (config.radius_m as f32 * skirt_drop_fraction).max(skirt_drop_min);

            let skirt_pos = [
                lp[0] + nx * drop,
                lp[1] + ny * drop,
                lp[2] + nz * drop,
            ];
            // Skirt inherits the color and normal of the edge vertex.
            vertices.push(TerrainVertex {
                position: skirt_pos,
                color: colors[gi],
                normal: normals[gi],
            });
        }
    }

    // -----------------------------------------------------------------------
    // Skirt triangles.
    // -----------------------------------------------------------------------
    // For each edge pair (grid_edge[i], grid_edge[i+1]) and the matching skirt
    // vertices, emit 2 triangles forming a quad.
    fn add_skirt_quad(
        indices: &mut Vec<u32>,
        g0: u32, g1: u32,  // grid edge vertices
        s0: u32, s1: u32,  // corresponding skirt vertices
    ) {
        indices.push(g0);
        indices.push(s0);
        indices.push(g1);

        indices.push(g1);
        indices.push(s0);
        indices.push(s1);
    }

    // Bottom row (row 0, col 0..n-1)
    for col in 0..n - 1 {
        let g0 = col as u32;
        let g1 = (col + 1) as u32;
        let s0 = skirt_bottom_base + col as u32;
        let s1 = skirt_bottom_base + col as u32 + 1;
        add_skirt_quad(&mut indices, g0, g1, s0, s1);
    }

    // Top row (row n-1, col 0..n-1)
    for col in 0..n - 1 {
        let g0 = ((n - 1) * n + col) as u32;
        let g1 = ((n - 1) * n + col + 1) as u32;
        let s0 = skirt_top_base + col as u32;
        let s1 = skirt_top_base + col as u32 + 1;
        add_skirt_quad(&mut indices, g0, g1, s0, s1);
    }

    // Left column (col 0, row 0..n-1)
    for row in 0..n - 1 {
        let g0 = (row * n) as u32;
        let g1 = ((row + 1) * n) as u32;
        let s0 = skirt_left_base + row as u32;
        let s1 = skirt_left_base + row as u32 + 1;
        add_skirt_quad(&mut indices, g0, g1, s0, s1);
    }

    // Right column (col n-1, row 0..n-1)
    for row in 0..n - 1 {
        let g0 = (row * n + (n - 1)) as u32;
        let g1 = ((row + 1) * n + (n - 1)) as u32;
        let s0 = skirt_right_base + row as u32;
        let s1 = skirt_right_base + row as u32 + 1;
        add_skirt_quad(&mut indices, g0, g1, s0, s1);
    }

    ChunkData {
        key,
        center_f64,
        vertices,
        indices,
        heights,
        min_height,
        max_height,
    }
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
}
