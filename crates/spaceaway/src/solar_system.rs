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
    /// Body radius in meters (for future angular-size LOD and terrain integration).
    radius_m: f64,
    /// Index of parent body in `bodies` vec (-1 for star/planets that orbit the star).
    parent_index: i32,
    /// Human-readable label.
    #[allow(dead_code)]
    pub label: String,
}

/// Active solar system state.
pub struct ActiveSystem {
    #[allow(dead_code)]
    pub star: Star,
    #[allow(dead_code)]
    pub star_id: ObjectId,
    pub system: PlanetarySystem,
    /// Star's position in galactic coordinates (light-years).
    pub star_galactic_pos: WorldPos,
    /// Loaded 3D bodies (star at index 0, then planets + moons).
    bodies: Vec<LoadedBody>,
    /// Accumulated game time in seconds for orbital calculation.
    game_time_s: f64,
    /// Catalog name for display.
    #[allow(dead_code)]
    pub catalog_name: String,
    /// Index of body that has terrain active (icosphere scaled to 0.999× radius).
    pub terrain_body_index: Option<usize>,
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
            star_id.sector_x().unsigned_abs(),
            star_id.sector_z().unsigned_abs(),
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

        // Star corona (large glow disc, rendered at same position as star)
        let corona_mesh = planet_mesh::build_corona_mesh(star_radius_m as f32, star.color);
        let corona_handle = mesh_store.upload(device, &corona_mesh);
        bodies.push(LoadedBody {
            mesh_handle: corona_handle,
            orbital_radius_m: 0.0,
            orbital_period_s: 0.0,
            initial_phase: 0.0,
            radius_m: star_radius_m * 4.0, // corona extends 4x star radius
            parent_index: -1,
            label: "Corona".to_string(),
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

        // Diagnostic: log every body's orbital parameters to verify parent_index
        for (i, b) in bodies.iter().enumerate() {
            log::info!(
                "BODY[{}] '{}': parent={}, orbit_r={:.0}m, period={:.0}s, radius={:.0}km",
                i, b.label, b.parent_index, b.orbital_radius_m, b.orbital_period_s, b.radius_m / 1000.0,
            );
        }

        Self {
            star: star.clone(),
            star_id,
            system,
            star_galactic_pos: placed_star.position,
            bodies,
            game_time_s: 0.0,
            catalog_name,
            terrain_body_index: None,
        }
    }

    /// Advance time and return pre-rebased DrawCommands (camera-relative meters).
    ///
    /// `dt_real`: wall-clock seconds since last frame.
    /// `camera_galactic_pos`: the camera's galactic position (light-years).
    ///
    /// The returned DrawCommands have `pre_rebased: true` — model_matrix
    /// translations are in camera-relative meters (computed in f64).
    pub fn update(
        &mut self,
        dt_real: f64,
        camera_galactic_pos: WorldPos,
    ) -> Vec<DrawCommand> {
        self.game_time_s += dt_real;

        let world_positions = self.compute_positions_ly();

        // Minimum screen size: bodies always render at least MIN_PIXELS across.
        // Distant planets appear as bright colored dots that grow into spheres.
        const MIN_PIXELS: f32 = 4.0;
        const SCREEN_HEIGHT: f32 = 1080.0; // approximate
        const FOV: f32 = std::f32::consts::FRAC_PI_4; // 45° vertical FOV

        let mut commands = Vec::with_capacity(self.bodies.len());
        for (i, body) in self.bodies.iter().enumerate() {
            let pos = world_positions[i];
            let dx_m = ((pos.x - camera_galactic_pos.x) * LY_TO_METERS) as f32;
            let dy_m = ((pos.y - camera_galactic_pos.y) * LY_TO_METERS) as f32;
            let dz_m = ((pos.z - camera_galactic_pos.z) * LY_TO_METERS) as f32;

            let distance_m = (dx_m as f64 * dx_m as f64
                + dy_m as f64 * dy_m as f64
                + dz_m as f64 * dz_m as f64)
                .sqrt();

            // Angular size in pixels
            let angular_pixels = if distance_m > 1.0 {
                2.0 * (body.radius_m / distance_m).atan() as f32
                    * SCREEN_HEIGHT / FOV
            } else {
                1000.0 // very close, no scaling needed
            };

            // Scale up if too small to see (minimum 4 pixels on screen)
            let mut scale = if angular_pixels < MIN_PIXELS && angular_pixels > 0.001 {
                MIN_PIXELS / angular_pixels
            } else {
                1.0
            };

            // When terrain is active for this body, shrink the icosphere to
            // 0.999× radius so terrain chunks (at or above true radius) always
            // win the depth test. This lets the icosphere serve as a fallback
            // backdrop while terrain streams in.
            if self.terrain_body_index == Some(i) {
                scale *= sa_terrain::config::ICOSPHERE_RADIUS_FACTOR as f32;
            }

            let model = Mat4::from_scale_rotation_translation(
                Vec3::splat(scale),
                glam::Quat::IDENTITY,
                Vec3::new(dx_m, dy_m, dz_m),
            );
            commands.push(DrawCommand {
                mesh: body.mesh_handle,
                model_matrix: model,
                pre_rebased: true,
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

    /// Get planet radius in meters for a body index.
    pub fn body_radius_m(&self, index: usize) -> Option<f64> {
        self.bodies.get(index).map(|b| b.radius_m)
    }

    /// Get planet data needed for terrain config.
    /// Returns (color_seed, sub_type, displacement_fraction, mass_earth, radius_earth).
    pub fn planet_data(&self, index: usize) -> Option<(u64, sa_universe::PlanetSubType, f32, f32, f32)> {
        let body = self.bodies.get(index)?;
        for planet in &self.system.planets {
            // Gas/ice giants: currently excluded from terrain activation.
            // Rapier panics at 80,000+ km radius (f32 AABB overflow).
            // FUTURE: atmospheric dive with upgraded ship hull. Needs
            // double-precision physics or origin-rebased chunk placement.
            // See CLAUDE.md "Upcoming Features" item 2.
            if matches!(planet.sub_type,
                sa_universe::PlanetSubType::HotGiant
                | sa_universe::PlanetSubType::WarmGiant
                | sa_universe::PlanetSubType::ColdGiant
                | sa_universe::PlanetSubType::CyanIce
                | sa_universe::PlanetSubType::TealIce
            ) {
                continue;
            }
            let planet_radius_m = planet.radius_earth as f64 * 6_371_000.0;
            if (body.radius_m - planet_radius_m).abs() < 1000.0 {
                // Displacement as fraction of radius, capped at 20km max.
                // Without the cap, a 7706km desert planet gets 308km(!) mountains
                // that extend above the camera during approach.
                let frac: f32 = match planet.sub_type {
                    sa_universe::PlanetSubType::Molten => 0.01,
                    sa_universe::PlanetSubType::Barren | sa_universe::PlanetSubType::Frozen => 0.02,
                    sa_universe::PlanetSubType::Temperate | sa_universe::PlanetSubType::Ocean => 0.015,
                    sa_universe::PlanetSubType::Desert => 0.025,
                    _ => 0.015,
                };
                let planet_radius_km = planet.radius_earth * 6371.0;
                let max_disp_km = 20.0; // absolute cap: tallest feature ~20km
                let amplitude = frac.min(max_disp_km / planet_radius_km);
                return Some((planet.color_seed, planet.sub_type, amplitude, planet.mass_earth, planet.radius_earth));
            }
        }
        None
    }

    /// Public access to body positions in light-years (for terrain integration).
    pub fn compute_positions_ly_pub(&self) -> Vec<sa_math::WorldPos> {
        self.compute_positions_ly()
    }

    /// Compute galactic positions (light-years) for every body.
    fn compute_positions_ly(&self) -> Vec<WorldPos> {
        let meters_to_ly = 1.0 / LY_TO_METERS;
        let mut positions: Vec<WorldPos> = Vec::with_capacity(self.bodies.len());

        for body in &self.bodies {
            if body.parent_index < 0 {
                // Star, corona, or other top-level body: at the star's galactic position.
                positions.push(self.star_galactic_pos);
            } else if body.orbital_period_s <= 0.0 {
                // Co-located child (atmosphere, ring): at parent's position.
                positions.push(positions[body.parent_index as usize]);
            } else {
                // Orbiting child (planet or moon): compute orbital position.
                let theta = body.initial_phase
                    + (std::f64::consts::TAU * self.game_time_s * TIME_SCALE)
                        / body.orbital_period_s;
                let x_m = body.orbital_radius_m * theta.cos();
                let z_m = body.orbital_radius_m * theta.sin();

                let parent_pos = positions[body.parent_index as usize];

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

    let planet_body_idx = bodies.len() as i32;
    bodies.push(LoadedBody {
        mesh_handle: handle,
        orbital_radius_m: planet.orbital_radius_au as f64 * AU_METERS,
        orbital_period_s: planet.orbital_period_years as f64 * SECONDS_PER_YEAR,
        initial_phase: planet.initial_phase as f64,
        radius_m: planet_radius_m,
        parent_index: 0, // orbits star (index 0 in bodies vec)
        label: format!("Planet {}", index + 1),
    });

    // Atmosphere shell (if planet has one)
    log::debug!("  Planet {}: atmo={}, rings={}", index + 1, planet.atmosphere.is_some(), planet.has_rings);
    if let Some(ref atmo) = planet.atmosphere {
        log::info!("  + Atmosphere for Planet {} (color [{:.1},{:.1},{:.1}] opacity {:.1})",
            index + 1, atmo.color[0], atmo.color[1], atmo.color[2], atmo.opacity);
        let atmo_mesh = planet_mesh::build_atmosphere_mesh(
            subdivisions,
            planet_radius_m as f32,
            atmo,
        );
        let atmo_handle = mesh_store.upload(device, &atmo_mesh);
        // Atmosphere orbits at same position as planet (zero orbital radius = co-located)
        bodies.push(LoadedBody {
            mesh_handle: atmo_handle,
            orbital_radius_m: 0.0,
            orbital_period_s: 0.0,
            initial_phase: 0.0,
            radius_m: planet_radius_m * 1.08,
            parent_index: planet_body_idx,
            label: format!("Atmo {}", index + 1),
        });
    }

    // Ring system (if planet has one)
    if planet.has_rings
        && let Some(ref ring) = planet.ring_params {
            log::info!("  + Rings for Planet {} (inner {:.1}x, outer {:.1}x)",
                index + 1, ring.inner_ratio, ring.outer_ratio);
            let ring_mesh = planet_mesh::build_ring_mesh(
                planet_radius_m as f32,
                ring,
                planet.axial_tilt_deg,
                planet.color_seed,
            );
            let ring_handle = mesh_store.upload(device, &ring_mesh);
            bodies.push(LoadedBody {
                mesh_handle: ring_handle,
                orbital_radius_m: 0.0,
                orbital_period_s: 0.0,
                initial_phase: 0.0,
                radius_m: planet_radius_m * ring.outer_ratio as f64,
                parent_index: planet_body_idx,
                label: format!("Ring {}", index + 1),
            });
    }
}
