//! Active solar system management.
//!
//! When the player enters a star system (warp gravity well drop), this module:
//! 1. Generates the system from the star's seed
//! 2. Builds 3D meshes for the star + planets + moons
//! 3. Computes orbital positions each frame (circular orbits, 30x time)
//! 4. Returns DrawCommands for the renderer
//!
//! **Coordinate contract**: The returned `DrawCommand` model matrices are in
//! camera-relative meters (pre-rebased). The caller must render them WITHOUT
//! the renderer's normal origin-rebasing pass (e.g. by setting cam_pos to
//! `WorldPos::ORIGIN` for these commands).

use glam::{Mat4, Vec3};
use sa_core::Handle;
use sa_math::WorldPos;
use sa_render::planet_mesh;
use sa_render::{DrawCommand, MeshMarker, MeshStore};
use sa_universe::sector::PlacedStar;
use sa_universe::{
    generate_system, ObjectId, Planet, PlanetType, PlanetarySystem, Star,
};

const AU_METERS: f64 = 1.496e11;
const EARTH_RADIUS_M: f64 = 6_371_000.0;
const SOLAR_RADIUS_M: f64 = 696_000_000.0;
const TIME_SCALE: f64 = 30.0;
const SECONDS_PER_YEAR: f64 = 365.25 * 24.0 * 3600.0;
const LY_TO_METERS: f64 = 9.461e15;

/// A loaded celestial body with its mesh and orbital parameters.
struct LoadedBody {
    mesh_handle: Handle<MeshMarker>,
    /// Orbital radius in meters from parent (0 for the star).
    orbital_radius_m: f64,
    /// Orbital period in seconds (real time; TIME_SCALE applied in position calc).
    orbital_period_s: f64,
    /// Initial orbital phase in radians.
    initial_phase: f64,
    /// Body radius in meters (for future angular-size LOD).
    #[allow(dead_code)]
    radius_m: f64,
    /// Index of parent body in `bodies` vec (-1 for star/planets that orbit the star).
    parent_index: i32,
    /// Human-readable label.
    pub label: String,
}

/// Active solar system state.
pub struct ActiveSystem {
    pub star: Star,
    pub star_id: ObjectId,
    pub system: PlanetarySystem,
    /// Star's position in galactic coordinates (light-years).
    pub star_galactic_pos: WorldPos,
    /// Loaded 3D bodies (star at index 0, then planets + moons).
    bodies: Vec<LoadedBody>,
    /// Accumulated game time in seconds for orbital calculation.
    game_time_s: f64,
    /// Catalog name for display.
    pub catalog_name: String,
}

impl ActiveSystem {
    /// Load a solar system: generate meshes for the star and all planets/moons.
    pub fn load(
        star_id: ObjectId,
        placed_star: &PlacedStar,
        mesh_store: &mut MeshStore,
        device: &wgpu::Device,
    ) -> Self {
        let star = &placed_star.star;
        let star_seed = star_id.0;
        let system = generate_system(star, star_seed);

        let catalog_name = format!(
            "SEC {:04}.{:04} / S-{:03}",
            star_id.sector_x(),
            star_id.sector_z(),
            star_id.system()
        );

        let mut bodies = Vec::new();

        // --- Star ---
        let star_radius_m = star.radius as f64 * SOLAR_RADIUS_M;
        let star_mesh =
            planet_mesh::build_star_mesh(3, star_radius_m as f32, star.color, star_seed);
        let handle = mesh_store.upload(device, &star_mesh);
        bodies.push(LoadedBody {
            mesh_handle: handle,
            orbital_radius_m: 0.0,
            orbital_period_s: 0.0,
            initial_phase: 0.0,
            radius_m: star_radius_m,
            parent_index: -1,
            label: "Star".to_string(),
        });

        // --- Planets + Moons ---
        for (i, planet) in system.planets.iter().enumerate() {
            let planet_body_index = bodies.len() as i32;
            push_planet_body(&mut bodies, mesh_store, device, planet, i);

            for (j, moon) in planet.moons.iter().enumerate() {
                let moon_radius_m = moon.radius_km as f64 * 1000.0;
                let moon_mesh = planet_mesh::build_rocky_planet_mesh(
                    3,
                    moon_radius_m as f32,
                    moon.sub_type,
                    planet.color_seed.wrapping_add(j as u64 + 1),
                );
                let mhandle = mesh_store.upload(device, &moon_mesh);
                bodies.push(LoadedBody {
                    mesh_handle: mhandle,
                    orbital_radius_m: moon.orbital_radius_km as f64 * 1000.0,
                    orbital_period_s: moon.orbital_period_hours as f64 * 3600.0,
                    initial_phase: moon.initial_phase as f64,
                    radius_m: moon_radius_m,
                    parent_index: planet_body_index,
                    label: format!("Moon {}{}", i + 1, (b'a' + j as u8) as char),
                });
            }
        }

        Self {
            star: star.clone(),
            star_id,
            system,
            star_galactic_pos: placed_star.position,
            bodies,
            game_time_s: 0.0,
            catalog_name,
        }
    }

    /// Advance time and return pre-rebased DrawCommands (camera-relative meters).
    ///
    /// `dt_real`: wall-clock seconds since last frame.
    /// `camera_galactic_pos`: the camera's galactic position (light-years).
    ///
    /// The returned DrawCommands have model_matrix translations in meters
    /// relative to the camera. They must NOT go through the renderer's
    /// origin-rebasing pass.
    pub fn update(
        &mut self,
        dt_real: f64,
        camera_galactic_pos: WorldPos,
    ) -> Vec<DrawCommand> {
        self.game_time_s += dt_real;

        // Compute world positions for all bodies (in light-years).
        let world_positions = self.compute_positions_ly();

        // Generate pre-rebased DrawCommands (camera-relative meters).
        let mut commands = Vec::with_capacity(self.bodies.len());
        for (i, body) in self.bodies.iter().enumerate() {
            let pos = world_positions[i];
            let dx_m = ((pos.x - camera_galactic_pos.x) * LY_TO_METERS) as f32;
            let dy_m = ((pos.y - camera_galactic_pos.y) * LY_TO_METERS) as f32;
            let dz_m = ((pos.z - camera_galactic_pos.z) * LY_TO_METERS) as f32;

            let model = Mat4::from_translation(Vec3::new(dx_m, dy_m, dz_m));
            commands.push(DrawCommand {
                mesh: body.mesh_handle,
                model_matrix: model,
            });
        }
        commands
    }

    /// Number of loaded bodies (star + planets + moons).
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Summary lines for the helm display.
    pub fn body_summary(&self) -> Vec<String> {
        self.system
            .planets
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let type_str = match p.planet_type {
                    PlanetType::Rocky => "Rocky",
                    PlanetType::GasGiant => "GasGiant",
                    PlanetType::IceGiant => "IceGiant",
                };
                let radius_km = p.radius_earth * 6371.0;
                let mut line = format!(
                    "[{}] {} {:.1}AU {:.0}km {:?}",
                    i + 1,
                    type_str,
                    p.orbital_radius_au,
                    radius_km,
                    p.sub_type
                );
                if p.has_rings {
                    line.push_str(" rings");
                }
                if !p.moons.is_empty() {
                    line.push_str(&format!(" +{}moon", p.moons.len()));
                }
                line
            })
            .collect()
    }

    /// Compute galactic positions (light-years) for every body.
    fn compute_positions_ly(&self) -> Vec<WorldPos> {
        let meters_to_ly = 1.0 / LY_TO_METERS;
        let mut positions: Vec<WorldPos> = Vec::with_capacity(self.bodies.len());

        for body in &self.bodies {
            if body.orbital_period_s <= 0.0 {
                // Star: at the system's galactic position.
                positions.push(self.star_galactic_pos);
            } else {
                let theta = body.initial_phase
                    + (std::f64::consts::TAU * self.game_time_s * TIME_SCALE)
                        / body.orbital_period_s;
                let x_m = body.orbital_radius_m * theta.cos();
                let z_m = body.orbital_radius_m * theta.sin();

                let parent_pos = if body.parent_index < 0 {
                    self.star_galactic_pos
                } else {
                    positions[body.parent_index as usize]
                };

                positions.push(WorldPos::new(
                    parent_pos.x + x_m * meters_to_ly,
                    parent_pos.y,
                    parent_pos.z + z_m * meters_to_ly,
                ));
            }
        }
        positions
    }
}

/// Build and push a planet body into the bodies vec.
fn push_planet_body(
    bodies: &mut Vec<LoadedBody>,
    mesh_store: &mut MeshStore,
    device: &wgpu::Device,
    planet: &Planet,
    index: usize,
) {
    let planet_radius_m = planet.radius_earth as f64 * EARTH_RADIUS_M;
    let subdivisions = 4;

    let mesh = match planet.planet_type {
        PlanetType::Rocky => planet_mesh::build_rocky_planet_mesh(
            subdivisions,
            planet_radius_m as f32,
            planet.sub_type,
            planet.color_seed,
        ),
        PlanetType::GasGiant | PlanetType::IceGiant => planet_mesh::build_gas_giant_mesh(
            subdivisions,
            planet_radius_m as f32,
            planet.sub_type,
            planet.color_seed,
        ),
    };
    let handle = mesh_store.upload(device, &mesh);

    bodies.push(LoadedBody {
        mesh_handle: handle,
        orbital_radius_m: planet.orbital_radius_au as f64 * AU_METERS,
        orbital_period_s: planet.orbital_period_years as f64 * SECONDS_PER_YEAR,
        initial_phase: planet.initial_phase as f64,
        radius_m: planet_radius_m,
        parent_index: -1, // orbits star
        label: format!("Planet {}", index + 1),
    });
}
