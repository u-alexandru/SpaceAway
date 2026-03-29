//! Menu background scene generation: celestial objects for the main menu.

use sa_core::Handle;
use sa_math::WorldPos;
use sa_render::{planet_mesh, Camera, MeshMarker, MeshStore};
use sa_universe::{generate_nebulae, generate_star, generate_system, MasterSeed, PlanetType, Rng64};

/// Camera at origin looking left (negative X) with given pitch.
fn origin_cam(pitch: f32) -> Camera {
    let mut cam = Camera::new();
    cam.position = WorldPos::ORIGIN;
    cam.yaw = -std::f32::consts::FRAC_PI_2;
    cam.pitch = pitch;
    cam
}

/// What celestial object is shown as the menu background.
pub enum MenuScene {
    /// Galaxy panorama — camera in galactic disc facing core.
    GalaxyPanorama { galactic_pos: WorldPos },
    /// Deep space — far from disc, minimal stars.
    DeepSpace { galactic_pos: WorldPos },
    /// Rocky planet close-up.
    Planet {
        galactic_pos: WorldPos,
        mesh: Handle<MeshMarker>,
        offset_m: [f32; 3],
    },
    /// Gas giant with rings.
    GasGiant {
        galactic_pos: WorldPos,
        meshes: Vec<Handle<MeshMarker>>,
        offsets: Vec<[f32; 3]>,
    },
    /// Star close-up with corona.
    Star {
        galactic_pos: WorldPos,
        mesh: Handle<MeshMarker>,
        offset_m: [f32; 3],
    },
    /// Nebula — camera positioned near a colorful nebula region.
    Nebula { galactic_pos: WorldPos },
    /// Binary stars — two stars of different colors.
    BinaryStar {
        galactic_pos: WorldPos,
        meshes: Vec<Handle<MeshMarker>>,
        offsets: Vec<[f32; 3]>,
    },
}

impl MenuScene {
    pub fn galactic_position(&self) -> WorldPos {
        match self {
            Self::GalaxyPanorama { galactic_pos }
            | Self::DeepSpace { galactic_pos }
            | Self::Planet { galactic_pos, .. }
            | Self::GasGiant { galactic_pos, .. }
            | Self::Star { galactic_pos, .. }
            | Self::Nebula { galactic_pos }
            | Self::BinaryStar { galactic_pos, .. } => *galactic_pos,
        }
    }
}

/// Select and generate a random scene based on spec weights.
pub fn generate_scene(
    rng: &mut Rng64,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
) -> (MenuScene, Camera) {
    let roll = rng.next_f32();
    if roll < 0.25 {
        make_planet_scene(rng, mesh_store, device)
    } else if roll < 0.45 {
        make_gas_giant_scene(rng, mesh_store, device)
    } else if roll < 0.65 {
        make_galaxy_scene(rng)
    } else if roll < 0.75 {
        make_nebula_scene(rng)
    } else if roll < 0.85 {
        make_star_scene(rng, mesh_store, device)
    } else if roll < 0.95 {
        make_deep_space_scene(rng)
    } else {
        make_binary_star_scene(rng, mesh_store, device)
    }
}

fn make_galaxy_scene(rng: &mut Rng64) -> (MenuScene, Camera) {
    let angle = rng.range_f32(0.0, std::f32::consts::TAU);
    let dist = rng.range_f32(15000.0, 30000.0);
    let x = dist * angle.cos();
    let z = dist * angle.sin();
    let y = rng.range_f32(-200.0, 200.0);
    let pos = WorldPos::new(x as f64, y as f64, z as f64);

    let mut cam = Camera::new();
    cam.position = pos;
    cam.yaw = (-x).atan2(z);
    cam.pitch = rng.range_f32(-0.1, 0.1);

    (MenuScene::GalaxyPanorama { galactic_pos: pos }, cam)
}

fn make_deep_space_scene(rng: &mut Rng64) -> (MenuScene, Camera) {
    let x = rng.range_f64(-5000.0, 5000.0);
    let sign = if rng.next_f32() > 0.5 { 1.0 } else { -1.0 };
    let y = rng.range_f64(5000.0, 15000.0) * sign;
    let z = rng.range_f64(-5000.0, 5000.0);
    let pos = WorldPos::new(x, y, z);

    let mut cam = Camera::new();
    cam.position = pos;
    cam.yaw = rng.range_f32(0.0, std::f32::consts::TAU);
    cam.pitch = rng.range_f32(-0.3, 0.3);

    (MenuScene::DeepSpace { galactic_pos: pos }, cam)
}

fn make_planet_scene(
    rng: &mut Rng64,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
) -> (MenuScene, Camera) {
    let star_seed = rng.next_u64();
    let star = generate_star(star_seed);
    let system = generate_system(&star, star_seed);

    if let Some(planet) = system.planets.iter().find(|p| p.planet_type == PlanetType::Rocky) {
        let radius_m = planet.radius_earth as f64 * 6_371_000.0;
        let mesh = planet_mesh::build_rocky_planet_mesh(
            4,
            radius_m as f32,
            planet.sub_type,
            planet.color_seed,
        );
        let handle = mesh_store.upload(device, &mesh);

        let view_dist = radius_m * 2.0;
        let offset = [-(view_dist as f32), (radius_m * 0.3) as f32, 0.0];
        (
            MenuScene::Planet { galactic_pos: WorldPos::ORIGIN, mesh: handle, offset_m: offset },
            origin_cam(-0.15),
        )
    } else {
        make_galaxy_scene(rng)
    }
}

fn make_gas_giant_scene(
    rng: &mut Rng64,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
) -> (MenuScene, Camera) {
    let star_seed = rng.next_u64();
    let star = generate_star(star_seed);
    let system = generate_system(&star, star_seed);

    // Find a gas giant, prefer one with rings
    let giant = system
        .planets
        .iter()
        .find(|p| p.planet_type == PlanetType::GasGiant && p.has_rings)
        .or_else(|| system.planets.iter().find(|p| p.planet_type == PlanetType::GasGiant));

    if let Some(planet) = giant {
        let radius_m = planet.radius_earth as f64 * 6_371_000.0;
        let body_mesh = planet_mesh::build_gas_giant_mesh(
            4,
            radius_m as f32,
            planet.sub_type,
            planet.color_seed,
        );

        let mut meshes = vec![mesh_store.upload(device, &body_mesh)];
        let mut offsets = vec![[0.0f32, 0.0, 0.0]];

        // Add rings if present
        if let Some(ref ring) = planet.ring_params {
            let ring_mesh = planet_mesh::build_ring_mesh(
                radius_m as f32,
                ring,
                planet.axial_tilt_deg,
                planet.color_seed,
            );
            meshes.push(mesh_store.upload(device, &ring_mesh));
            offsets.push([0.0, 0.0, 0.0]);
        }

        let view_dist = radius_m * 2.5;

        // Offset all meshes so planet is to the right of camera
        let base_offset = [-(view_dist as f32), (radius_m * 0.4) as f32, 0.0];
        for o in &mut offsets {
            o[0] += base_offset[0];
            o[1] += base_offset[1];
            o[2] += base_offset[2];
        }

        (
            MenuScene::GasGiant {
                galactic_pos: WorldPos::ORIGIN,
                meshes,
                offsets,
            },
            origin_cam(-0.25), // look down to see rings
        )
    } else {
        make_planet_scene(rng, mesh_store, device)
    }
}

fn make_star_scene(
    rng: &mut Rng64,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
) -> (MenuScene, Camera) {
    let star_seed = rng.next_u64();
    let star = generate_star(star_seed);
    let radius_m = star.radius as f64 * 696_000_000.0;

    let mesh = planet_mesh::build_star_mesh(3, radius_m as f32, star.color, star_seed);
    let handle = mesh_store.upload(device, &mesh);

    let view_dist = radius_m * 3.0;
    let offset = [-(view_dist as f32), (radius_m * 0.2) as f32, 0.0];
    (
        MenuScene::Star { galactic_pos: WorldPos::ORIGIN, mesh: handle, offset_m: offset },
        origin_cam(-0.05),
    )
}

fn make_nebula_scene(rng: &mut Rng64) -> (MenuScene, Camera) {
    let nebulae = generate_nebulae(MasterSeed(42));
    let idx = (rng.next_u64() as usize) % nebulae.len();
    let neb = &nebulae[idx];

    // Position camera at nebula edge, looking inward
    let offset_angle = rng.range_f32(0.0, std::f32::consts::TAU);
    let edge_dist = neb.radius * 0.8;
    let cx = neb.x + edge_dist * offset_angle.cos() as f64;
    let cz = neb.z + edge_dist * offset_angle.sin() as f64;
    let cy = neb.y + rng.range_f64(-50.0, 50.0);
    let pos = WorldPos::new(cx, cy, cz);

    let mut cam = Camera::new();
    cam.position = pos;
    // Face toward nebula center
    let dx = neb.x - cx;
    let dz = neb.z - cz;
    cam.yaw = (dx as f32).atan2(dz as f32);
    cam.pitch = rng.range_f32(-0.1, 0.1);

    (MenuScene::Nebula { galactic_pos: pos }, cam)
}

fn make_binary_star_scene(
    rng: &mut Rng64,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
) -> (MenuScene, Camera) {
    let seed_a = rng.next_u64();
    let seed_b = rng.next_u64();
    let star_a = generate_star(seed_a);
    let star_b = generate_star(seed_b);

    let r_a = star_a.radius as f64 * 696_000_000.0;
    let r_b = star_b.radius as f64 * 696_000_000.0;

    let mesh_a = planet_mesh::build_star_mesh(3, r_a as f32, star_a.color, seed_a);
    let mesh_b = planet_mesh::build_star_mesh(3, r_b as f32, star_b.color, seed_b);

    let ha = mesh_store.upload(device, &mesh_a);
    let hb = mesh_store.upload(device, &mesh_b);

    // Place stars side by side with some separation
    let separation = (r_a + r_b) * 3.0;
    let half = (separation / 2.0) as f32;
    let view_dist = separation * 2.0;

    (
        MenuScene::BinaryStar {
            galactic_pos: WorldPos::ORIGIN,
            meshes: vec![ha, hb],
            offsets: vec![
                [-(view_dist as f32), 0.0, -half],
                [-(view_dist as f32), 0.0, half],
            ],
        },
        origin_cam(-0.05),
    )
}
