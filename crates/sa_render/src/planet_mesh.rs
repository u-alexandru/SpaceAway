//! Planet and star mesh generation with noise-based terrain and biome colors.
//!
//! Produces flat-shaded `MeshData` for rocky planets, gas giants, and stars
//! using icosphere subdivision + procedural noise displacement/coloring.

use crate::icosphere::generate_icosphere;
use crate::mesh::MeshData;
use crate::vertex::Vertex;
use noise::{NoiseFn, OpenSimplex};
use sa_universe::PlanetSubType;

/// Build a rocky planet mesh with noise-displaced terrain and biome colors.
pub fn build_rocky_planet_mesh(
    subdivisions: u32,
    radius_m: f32,
    sub_type: PlanetSubType,
    seed: u64,
) -> MeshData {
    let ico = generate_icosphere(subdivisions);
    let noise_gen = OpenSimplex::new(seed as u32);
    let seed_offset = (seed % 10000) as f64 * 0.1;
    let amplitude = displacement_amplitude(sub_type);

    // Compute displaced positions and noise values per vertex.
    let mut displaced: Vec<[f32; 3]> = Vec::with_capacity(ico.positions.len());
    let mut noise_vals: Vec<f32> = Vec::with_capacity(ico.positions.len());

    for p in &ico.positions {
        let (x, y, z) = (p[0] as f64, p[1] as f64, p[2] as f64);
        let n = fbm(&noise_gen, x + seed_offset, y, z, 4);
        let disp = n as f32 * amplitude * radius_m;
        let r = radius_m + disp;
        displaced.push([p[0] * r, p[1] * r, p[2] * r]);
        noise_vals.push(n as f32);
    }

    expand_triangles_rocky(&ico.indices, &displaced, &ico.positions, &noise_vals, sub_type, seed)
}

/// Build a gas giant mesh with latitude-band coloring (no terrain displacement).
pub fn build_gas_giant_mesh(
    subdivisions: u32,
    radius_m: f32,
    sub_type: PlanetSubType,
    seed: u64,
) -> MeshData {
    let ico = generate_icosphere(subdivisions);
    let noise_gen = OpenSimplex::new(seed as u32);
    let num_bands = 10 + (seed % 5) as usize;
    let palette = gas_giant_palette(sub_type);

    let scaled: Vec<[f32; 3]> = ico
        .positions
        .iter()
        .map(|p| [p[0] * radius_m, p[1] * radius_m, p[2] * radius_m])
        .collect();

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for tri in ico.indices.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let pa = scaled[ia];
        let pb = scaled[ib];
        let pc = scaled[ic];

        let normal = face_normal(pa, pb, pc);

        // Average unit-sphere Y for latitude.
        let avg_y = (ico.positions[ia][1] + ico.positions[ib][1] + ico.positions[ic][1]) / 3.0;
        let lat = avg_y.abs();

        // Average longitude for noise perturbation.
        let avg_x = (ico.positions[ia][0] + ico.positions[ib][0] + ico.positions[ic][0]) / 3.0;
        let avg_z = (ico.positions[ia][2] + ico.positions[ib][2] + ico.positions[ic][2]) / 3.0;
        let lon = avg_z.atan2(avg_x);

        // Perturb latitude with noise for swirling band edges.
        let perturb = noise_gen.get([lon as f64 * 3.0, avg_y as f64 * 5.0, seed as f64 * 0.01])
            as f32
            * 0.06;
        let effective_lat = (lat + perturb).clamp(0.0, 1.0);

        let band_idx = ((effective_lat * num_bands as f32) as usize).min(num_bands - 1);
        let color = palette[band_idx % palette.len()];

        // Per-face variation.
        let face_idx = vertices.len() as u32 / 3;
        let var = face_variation(face_idx, seed);
        let color = [
            (color[0] + var * 0.03).clamp(0.0, 1.5),
            (color[1] + var * 0.03).clamp(0.0, 1.5),
            (color[2] + var * 0.03).clamp(0.0, 1.5),
        ];

        let base = vertices.len() as u32;
        vertices.push(Vertex { position: pa, color, normal });
        vertices.push(Vertex { position: pb, color, normal });
        vertices.push(Vertex { position: pc, color, normal });
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    MeshData { vertices, indices }
}

/// Build a star mesh with cellular-style surface variation.
pub fn build_star_mesh(
    subdivisions: u32,
    radius_m: f32,
    color: [f32; 3],
    seed: u64,
) -> MeshData {
    let ico = generate_icosphere(subdivisions);
    let noise_gen = OpenSimplex::new(seed as u32);
    let seed_offset = (seed % 10000) as f64 * 0.1;

    let scaled: Vec<[f32; 3]> = ico
        .positions
        .iter()
        .map(|p| [p[0] * radius_m, p[1] * radius_m, p[2] * radius_m])
        .collect();

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for tri in ico.indices.chunks_exact(3) {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let pa = scaled[ia];
        let pb = scaled[ib];
        let pc = scaled[ic];

        let normal = face_normal(pa, pb, pc);

        // Sample noise at face centroid on unit sphere for cellular pattern.
        let cx = (ico.positions[ia][0] + ico.positions[ib][0] + ico.positions[ic][0]) / 3.0;
        let cy = (ico.positions[ia][1] + ico.positions[ib][1] + ico.positions[ic][1]) / 3.0;
        let cz = (ico.positions[ia][2] + ico.positions[ib][2] + ico.positions[ic][2]) / 3.0;

        let n = noise_gen.get([
            cx as f64 * 6.0 + seed_offset,
            cy as f64 * 6.0,
            cz as f64 * 6.0,
        ]) as f32;

        // Map noise to brightness: high = bright cell center, low = dark edge.
        let t = (n * 0.5 + 0.5).clamp(0.0, 1.0);
        let bright = 0.7 + t * 0.5; // 0.7 to 1.2
        let face_color = [
            (color[0] * bright).clamp(0.0, 1.5),
            (color[1] * bright).clamp(0.0, 1.5),
            (color[2] * bright).clamp(0.0, 1.5),
        ];

        let base = vertices.len() as u32;
        vertices.push(Vertex { position: pa, color: face_color, normal });
        vertices.push(Vertex { position: pb, color: face_color, normal });
        vertices.push(Vertex { position: pc, color: face_color, normal });
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    MeshData { vertices, indices }
}

/// Build an atmosphere shell mesh slightly larger than the planet (1.03x radius).
///
/// Colors the shell with the atmosphere color modulated by opacity, with
/// slight latitude-based brightness variation for visual interest.
pub fn build_atmosphere_mesh(
    subdivisions: u32,
    planet_radius_m: f32,
    atmo: &sa_universe::AtmosphereParams,
) -> MeshData {
    let atmo_radius = planet_radius_m * 1.03;
    let ico = generate_icosphere(subdivisions);

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for tri in ico.indices.chunks_exact(3) {
        let p0 = ico.positions[tri[0] as usize];
        let p1 = ico.positions[tri[1] as usize];
        let p2 = ico.positions[tri[2] as usize];

        // Scale to atmosphere radius
        let v0 = [p0[0] * atmo_radius, p0[1] * atmo_radius, p0[2] * atmo_radius];
        let v1 = [p1[0] * atmo_radius, p1[1] * atmo_radius, p1[2] * atmo_radius];
        let v2 = [p2[0] * atmo_radius, p2[1] * atmo_radius, p2[2] * atmo_radius];

        let normal = face_normal(v0, v1, v2);

        // Slightly vary brightness by latitude for visual interest
        let avg_y = (p0[1] + p1[1] + p2[1]) / 3.0; // latitude proxy on unit sphere
        let lat_factor = 0.8 + 0.2 * (1.0 - avg_y.abs()); // brighter at equator
        let color = [
            atmo.color[0] * atmo.opacity * lat_factor * 0.6,
            atmo.color[1] * atmo.opacity * lat_factor * 0.6,
            atmo.color[2] * atmo.opacity * lat_factor * 0.6,
        ];

        let base = vertices.len() as u32;
        vertices.push(Vertex { position: v0, color, normal });
        vertices.push(Vertex { position: v1, color, normal });
        vertices.push(Vertex { position: v2, color, normal });
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    MeshData { vertices, indices }
}

/// Build a flat annular ring mesh in the planet's equatorial plane.
///
/// The ring spans from `inner_ratio * radius` to `outer_ratio * radius`,
/// tilted by the planet's axial tilt. Radial density variation creates
/// visual gaps and bands seeded deterministically.
pub fn build_ring_mesh(
    planet_radius_m: f32,
    ring: &sa_universe::RingParams,
    axial_tilt_deg: f32,
    seed: u64,
) -> MeshData {
    let inner_r = planet_radius_m * ring.inner_ratio;
    let outer_r = planet_radius_m * ring.outer_ratio;
    let segments = 64_u32;
    let num_rings = 10_u32;
    let tilt_rad = axial_tilt_deg * std::f32::consts::PI / 180.0;
    let cos_tilt = tilt_rad.cos();
    let sin_tilt = tilt_rad.sin();

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let make_point = |radius: f32, angle: f32| -> [f32; 3] {
        let x = radius * angle.cos();
        let flat_z = radius * angle.sin();
        // Apply axial tilt (rotate around X axis)
        let y = flat_z * sin_tilt;
        let z = flat_z * cos_tilt;
        [x, y, z]
    };

    for s in 0..segments {
        let angle0 = (s as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
        let angle1 = ((s + 1) as f32 / segments as f32) * 2.0 * std::f32::consts::PI;

        for r in 0..num_rings {
            let r0 = inner_r + (outer_r - inner_r) * (r as f32 / num_rings as f32);
            let r1 = inner_r + (outer_r - inner_r) * ((r + 1) as f32 / num_rings as f32);

            // Radial density: seeded variation for gap positions
            let radial_t = (r as f32 + 0.5) / num_rings as f32;
            let gap_noise = (radial_t * 7.0 + seed as f32 * 0.01).sin() * 0.5 + 0.5;
            let density = gap_noise * 0.8 + 0.2; // never fully transparent
            let color = [
                ring.color[0] * density,
                ring.color[1] * density,
                ring.color[2] * density,
            ];

            // Normal: up vector (tilted by axial tilt)
            let normal = [0.0, cos_tilt, sin_tilt];

            let p00 = make_point(r0, angle0);
            let p10 = make_point(r1, angle0);
            let p11 = make_point(r1, angle1);
            let p01 = make_point(r0, angle1);

            let base = vertices.len() as u32;
            vertices.push(Vertex { position: p00, color, normal });
            vertices.push(Vertex { position: p10, color, normal });
            vertices.push(Vertex { position: p11, color, normal });
            vertices.push(Vertex { position: p01, color, normal });
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    MeshData { vertices, indices }
}

/// Build a corona glow disc mesh at 4x star radius with radial falloff.
///
/// The disc lies in the XY plane (normal facing +Z) and should be oriented
/// toward the camera via the model matrix at draw time (billboard).
pub fn build_corona_mesh(star_radius_m: f32, color: [f32; 3]) -> MeshData {
    let corona_radius = star_radius_m * 4.0;
    let segments = 32_u32;
    let num_rings = 8_u32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let normal = [0.0, 0.0, 1.0]; // facing camera

    for s in 0..segments {
        let angle0 = (s as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
        let angle1 = ((s + 1) as f32 / segments as f32) * 2.0 * std::f32::consts::PI;

        for r in 0..num_rings {
            let r0_frac = r as f32 / num_rings as f32;
            let r1_frac = (r + 1) as f32 / num_rings as f32;
            let r0 = corona_radius * r0_frac;
            let r1 = corona_radius * r1_frac;

            // Radial falloff: bright at center, fading to transparent at edge
            let brightness0 = (1.0 - r0_frac).powf(2.5);
            let brightness1 = (1.0 - r1_frac).powf(2.5);
            let c0 = [color[0] * brightness0, color[1] * brightness0, color[2] * brightness0];
            let c1 = [color[0] * brightness1, color[1] * brightness1, color[2] * brightness1];

            let p00 = [r0 * angle0.cos(), r0 * angle0.sin(), 0.0];
            let p10 = [r1 * angle0.cos(), r1 * angle0.sin(), 0.0];
            let p11 = [r1 * angle1.cos(), r1 * angle1.sin(), 0.0];
            let p01 = [r0 * angle1.cos(), r0 * angle1.sin(), 0.0];

            let base = vertices.len() as u32;
            vertices.push(Vertex { position: p00, color: c0, normal });
            vertices.push(Vertex { position: p10, color: c1, normal });
            vertices.push(Vertex { position: p11, color: c1, normal });
            vertices.push(Vertex { position: p01, color: c0, normal });
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    MeshData { vertices, indices }
}

// --- Private helpers ---

fn fbm(noise: &OpenSimplex, x: f64, y: f64, z: f64, octaves: u32) -> f64 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;
    for _ in 0..octaves {
        value += amplitude * noise.get([x * frequency, y * frequency, z * frequency]);
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    value
}

fn displacement_amplitude(sub_type: PlanetSubType) -> f32 {
    match sub_type {
        PlanetSubType::Molten => 0.01,
        PlanetSubType::Barren => 0.02,
        PlanetSubType::Frozen => 0.02,
        PlanetSubType::Temperate | PlanetSubType::Ocean => 0.03,
        PlanetSubType::Desert => 0.04,
        _ => 0.03,
    }
}

fn biome_color(
    sub_type: PlanetSubType,
    height_norm: f32,
    latitude: f32,
    _seed: u64,
) -> [f32; 3] {
    let h = height_norm;
    let lat = latitude;
    match sub_type {
        PlanetSubType::Barren => [0.35 + h * 0.1, 0.33 + h * 0.1, 0.32 + h * 0.1],
        PlanetSubType::Desert => [0.7 + h * 0.1, 0.5 - lat * 0.1, 0.3 - h * 0.05],
        PlanetSubType::Temperate => {
            if h < -0.3 {
                [0.1, 0.2, 0.5]
            } else if lat > 0.8 {
                [0.85, 0.87, 0.9]
            } else if h > 0.5 {
                [0.5, 0.48, 0.45]
            } else {
                [0.2, 0.4, 0.15]
            }
        }
        PlanetSubType::Ocean => {
            if h > 0.0 {
                [0.15, 0.25, 0.55]
            } else {
                [0.08, 0.15, 0.45]
            }
        }
        PlanetSubType::Frozen => [0.8 + h * 0.05, 0.82 + h * 0.05, 0.88 + h * 0.03],
        PlanetSubType::Molten => {
            if h < -0.2 {
                [0.9, 0.3, 0.05]
            } else {
                [0.12, 0.1, 0.08]
            }
        }
        _ => [0.5, 0.5, 0.5],
    }
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let nx = u[1] * v[2] - u[2] * v[1];
    let ny = u[2] * v[0] - u[0] * v[2];
    let nz = u[0] * v[1] - u[1] * v[0];
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len < 1e-10 {
        [0.0, 1.0, 0.0]
    } else {
        [nx / len, ny / len, nz / len]
    }
}

/// Simple hash for per-face color variation.
fn face_variation(face_idx: u32, seed: u64) -> f32 {
    let h = face_idx as u64 ^ seed.wrapping_mul(2654435761);
    let h = h.wrapping_mul(0x517cc1b727220a95);
    ((h >> 40) as f32 / 16777216.0) - 0.5 // -0.5 to 0.5
}

fn expand_triangles_rocky(
    indices: &[u32],
    displaced: &[[f32; 3]],
    unit_pos: &[[f32; 3]],
    noise_vals: &[f32],
    sub_type: PlanetSubType,
    seed: u64,
) -> MeshData {
    let num_tris = indices.len() / 3;
    let mut vertices = Vec::with_capacity(num_tris * 3);
    let mut out_indices = Vec::with_capacity(num_tris * 3);

    for (fi, tri) in indices.chunks_exact(3).enumerate() {
        let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let pa = displaced[ia];
        let pb = displaced[ib];
        let pc = displaced[ic];

        let normal = face_normal(pa, pb, pc);

        // Average noise value and latitude for the face.
        let avg_noise = (noise_vals[ia] + noise_vals[ib] + noise_vals[ic]) / 3.0;
        let avg_lat =
            (unit_pos[ia][1].abs() + unit_pos[ib][1].abs() + unit_pos[ic][1].abs()) / 3.0;

        let mut color = biome_color(sub_type, avg_noise, avg_lat, seed);

        // Per-face variation.
        let var = face_variation(fi as u32, seed);
        color[0] = (color[0] + var * 0.04).clamp(0.0, 1.5);
        color[1] = (color[1] + var * 0.04).clamp(0.0, 1.5);
        color[2] = (color[2] + var * 0.04).clamp(0.0, 1.5);

        let base = vertices.len() as u32;
        vertices.push(Vertex { position: pa, color, normal });
        vertices.push(Vertex { position: pb, color, normal });
        vertices.push(Vertex { position: pc, color, normal });
        out_indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    MeshData { vertices, indices: out_indices }
}

fn gas_giant_palette(sub_type: PlanetSubType) -> Vec<[f32; 3]> {
    match sub_type {
        PlanetSubType::ColdGiant => vec![
            [0.85, 0.75, 0.6],
            [0.8, 0.55, 0.3],
            [0.55, 0.35, 0.2],
            [0.75, 0.65, 0.5],
        ],
        PlanetSubType::WarmGiant => vec![
            [0.8, 0.7, 0.4],
            [0.7, 0.6, 0.45],
            [0.75, 0.72, 0.65],
        ],
        PlanetSubType::HotGiant => vec![
            [0.15, 0.12, 0.25],
            [0.2, 0.18, 0.22],
        ],
        PlanetSubType::CyanIce => vec![
            [0.3, 0.7, 0.8],
            [0.2, 0.5, 0.6],
        ],
        PlanetSubType::TealIce => vec![
            [0.15, 0.45, 0.5],
            [0.1, 0.3, 0.55],
        ],
        _ => vec![[0.5, 0.5, 0.5]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sa_universe::PlanetSubType;

    #[test]
    fn rocky_planet_mesh_not_empty() {
        let mesh = build_rocky_planet_mesh(3, 6_371_000.0, PlanetSubType::Temperate, 42);
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn rocky_planet_has_flat_normals() {
        let mesh = build_rocky_planet_mesh(2, 1_000_000.0, PlanetSubType::Barren, 77);
        for tri in mesh.indices.chunks_exact(3) {
            let na = mesh.vertices[tri[0] as usize].normal;
            let nb = mesh.vertices[tri[1] as usize].normal;
            assert!(
                (na[0] - nb[0]).abs() < 1e-4
                    && (na[1] - nb[1]).abs() < 1e-4
                    && (na[2] - nb[2]).abs() < 1e-4
            );
        }
    }

    #[test]
    fn gas_giant_at_planet_radius() {
        let r = 69_911_000.0_f32;
        let mesh = build_gas_giant_mesh(3, r, PlanetSubType::ColdGiant, 42);
        for v in &mesh.vertices {
            let vr = (v.position[0] * v.position[0]
                + v.position[1] * v.position[1]
                + v.position[2] * v.position[2])
            .sqrt();
            assert!(
                (vr - r).abs() / r < 0.001,
                "gas giant vertex should be at planet radius, r={vr}"
            );
        }
    }

    #[test]
    fn different_seeds_different_terrain() {
        let a = build_rocky_planet_mesh(2, 1_000_000.0, PlanetSubType::Barren, 42);
        let b = build_rocky_planet_mesh(2, 1_000_000.0, PlanetSubType::Barren, 99);
        let differs = a
            .vertices
            .iter()
            .zip(b.vertices.iter())
            .any(|(va, vb)| (va.position[0] - vb.position[0]).abs() > 0.1);
        assert!(differs, "Different seeds should produce different terrain");
    }

    #[test]
    fn star_mesh_not_empty() {
        let mesh = build_star_mesh(3, 696_000_000.0, [1.0, 0.9, 0.7], 42);
        assert!(!mesh.vertices.is_empty());
    }

    #[test]
    fn atmosphere_mesh_larger_than_planet() {
        let atmo = sa_universe::AtmosphereParams {
            color: [0.4, 0.6, 1.0],
            opacity: 0.6,
            scattering_power: 3.0,
        };
        let mesh = build_atmosphere_mesh(2, 6_371_000.0, &atmo);
        assert!(!mesh.vertices.is_empty());
        // All vertices should be at ~1.03x planet radius
        for v in &mesh.vertices {
            let r = (v.position[0] * v.position[0]
                + v.position[1] * v.position[1]
                + v.position[2] * v.position[2])
            .sqrt();
            let expected = 6_371_000.0 * 1.03;
            assert!(
                (r - expected).abs() / expected < 0.01,
                "atmosphere vertex at wrong radius: {r}"
            );
        }
    }

    #[test]
    fn ring_mesh_in_correct_range() {
        let ring = sa_universe::RingParams {
            inner_ratio: 1.4,
            outer_ratio: 2.3,
            color: [0.7, 0.6, 0.5],
        };
        let mesh = build_ring_mesh(60_000_000.0, &ring, 15.0, 42);
        assert!(!mesh.vertices.is_empty());
        // Vertices should be between inner and outer radius
        for v in &mesh.vertices {
            let r = (v.position[0] * v.position[0]
                + v.position[1] * v.position[1]
                + v.position[2] * v.position[2])
            .sqrt();
            let inner = 60_000_000.0 * 1.4 * 0.99;
            let outer = 60_000_000.0 * 2.3 * 1.01;
            assert!(
                r >= inner && r <= outer,
                "ring vertex out of range: {r} (expected {inner}..{outer})"
            );
        }
    }

    #[test]
    fn corona_mesh_large() {
        let mesh = build_corona_mesh(696_000_000.0, [1.0, 0.9, 0.7]);
        assert!(!mesh.vertices.is_empty());
        // Some vertices should be at ~4x star radius
        let max_r = mesh
            .vertices
            .iter()
            .map(|v| {
                (v.position[0] * v.position[0]
                    + v.position[1] * v.position[1]
                    + v.position[2] * v.position[2])
                .sqrt()
            })
            .fold(0.0_f32, f32::max);
        assert!(
            max_r > 696_000_000.0 * 3.5,
            "corona should extend well beyond star, max_r={max_r}"
        );
    }

    #[test]
    fn all_sub_types_produce_valid_mesh() {
        let rocky_types = [
            PlanetSubType::Barren,
            PlanetSubType::Desert,
            PlanetSubType::Temperate,
            PlanetSubType::Ocean,
            PlanetSubType::Frozen,
            PlanetSubType::Molten,
        ];
        for st in &rocky_types {
            let mesh = build_rocky_planet_mesh(2, 1_000_000.0, *st, 42);
            assert!(!mesh.vertices.is_empty(), "sub_type {:?} should produce mesh", st);
            for v in &mesh.vertices {
                for c in &v.color {
                    assert!(
                        *c >= 0.0 && *c <= 2.0,
                        "color out of range: {c} for {:?}",
                        st
                    );
                }
            }
        }
    }
}
