mod drive_integration;
mod landing;
mod menu;
mod navigation;
mod ship_colliders;
mod ship_setup;
mod solar_system;
mod star_streaming;
mod terrain_colliders;
mod terrain_integration;
mod ui;

use glam::{Mat4, Vec3};
use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use sa_player::PlayerController;
use sa_render::{
    Camera, DrawCommand, GpuContext, MeshData, NebulaInstance, Renderer, ScreenDrawCommand,
    ScreenQuad, Vertex,
};
use sa_ship::helm::HelmController;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_survival::{ResourceDeposit, ShipResources, SuitResources, generate_deposits};
use sa_universe::{MasterSeed, Universe};
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{CursorGrabMode, Window, WindowId};

#[derive(PartialEq)]
enum GamePhase {
    Menu,
    Playing,
}

/// Convert visible stars from the universe query into GPU-ready vertices.
///
/// Convert universe nebulae to GPU-ready `NebulaInstance` data.
///
/// Nebulae are at galaxy scale (light-years). We project them onto the sky dome
/// like stars: normalize the direction, place at a large distance, and scale
/// the billboard radius by angular size so nearby nebulae appear large.
const SKY_DOME_DIST: f32 = 80_000.0;

fn nebulae_to_instances(
    nebulae: &[sa_universe::Nebula],
    observer: WorldPos,
) -> Vec<NebulaInstance> {
    nebulae
        .iter()
        .filter_map(|n| {
            let dx = (n.x - observer.x) as f32;
            let dy = (n.y - observer.y) as f32;
            let dz = (n.z - observer.z) as f32;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            if !(1.0..=80_000.0).contains(&dist) {
                return None;
            }

            // Angular size: radius / distance (in radians)
            let angular_radius = (n.radius as f32) / dist;
            // Skip if too tiny to see (< 0.5 degree)
            if angular_radius < 0.008 {
                return None;
            }

            // Place on sky dome: normalize direction, multiply by dome distance
            let nx = dx / dist;
            let ny = dy / dist;
            let nz = dz / dist;
            let dome_radius = angular_radius * SKY_DOME_DIST;

            // Opacity falls off with distance
            let dist_opacity = (1.0 - dist / 80_000.0).clamp(0.1, 1.0);

            Some(NebulaInstance {
                center: [nx * SKY_DOME_DIST, ny * SKY_DOME_DIST, nz * SKY_DOME_DIST],
                radius: dome_radius,
                color: n.color,
                opacity: n.opacity * dist_opacity,
                seed: (n.seed % 10_000) as f32,
                _pad0: 0.0,
                _pad1: 0.0,
                _pad2: 0.0,
            })
        })
        .collect()
}

/// Convert distant galaxies to GPU-ready `NebulaInstance` data.
///
/// Each galaxy is placed at a large distance along its direction vector,
/// analogous to how stars are projected onto the sky dome.
fn distant_galaxies_to_instances(
    galaxies: &[sa_universe::DistantGalaxy],
) -> Vec<NebulaInstance> {
    let dist = 80_000.0_f32;
    galaxies
        .iter()
        .map(|g| NebulaInstance {
            center: [
                g.direction[0] * dist,
                g.direction[1] * dist,
                g.direction[2] * dist,
            ],
            radius: g.angular_size * dist,
            color: [
                g.color[0] * g.brightness,
                g.color[1] * g.brightness,
                g.color[2] * g.brightness,
            ],
            opacity: g.brightness,
            seed: (g.rotation * 1000.0) % 10_000.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        })
        .collect()
}

// Distance threshold (in light-years) before we regenerate the star field.
// Roughly one sector width.
// Player is in meters, universe is in light-years. This threshold is in
// whatever units WorldPos uses. Since physics/player uses meters and the
// universe treats coordinates as light-years, walking 100m = 100 "ly" in
// universe space. Set high enough that walking doesn't trigger regen.
// Star streaming constants are in star_streaming.rs

/// Per-frame performance timings in microseconds.
#[derive(Default)]
struct PerfTimings {
    player_us: u64,
    physics_us: u64,
    stars_us: u64,
    render_us: u64,
    total_us: u64,
    fps: f64,
    star_count: u32,
    draw_calls: u32,
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    camera: Camera,
    input: InputState,
    world: GameWorld,
    events: EventBus,
    time: FrameTime,
    schedule: Schedule,
    last_frame: Instant,
    cube_mesh: Option<sa_core::Handle<sa_render::MeshMarker>>,
    cursor_grabbed: bool,
    physics: PhysicsWorld,
    player: Option<PlayerController>,
    #[allow(dead_code)]
    universe: Universe,
    star_streaming: star_streaming::StarStreaming,
    nebulae: Vec<sa_universe::Nebula>,
    distant_galaxies: Vec<sa_universe::DistantGalaxy>,
    perf: PerfTimings,
    perf_update_timer: f64,
    teleport_counter: u64,
    fly_mode: bool,
    fly_speed: f64, // light-years per second
    /// Ship's position in the galaxy (light-years). Separate from camera/physics
    /// positions which are in meters. Used for star field and universe queries.
    /// In walk/ship mode this is effectively frozen (m/s → ly/s ≈ 0).
    /// In fly mode this moves at ly/s. Teleport sets it directly.
    galactic_position: WorldPos,
    /// Current ship part mesh handle (for key 6/7 viewing, also the hull in normal mode).
    ship_part_mesh: Option<sa_core::Handle<sa_render::MeshMarker>>,
    /// Index into all_ship_parts() for cycling (key 6).
    ship_part_index: usize,
    /// View mode: 0=normal scene, 6=single ship part, 7=full ship.
    view_mode: u8,
    /// Ship entity (physics body + propulsion state).
    ship: Option<Ship>,
    /// Interaction system for ship interactables.
    interaction: Option<InteractionSystem>,
    /// IDs of key interactables for game-loop wiring.
    ship_ids: Option<ship_setup::ShipInteractableIds>,
    /// Helm controller for seated flight mode.
    helm: Option<HelmController>,
    /// GPU mesh handles for interactable objects.
    interactable_meshes: Vec<sa_core::Handle<sa_render::MeshMarker>>,
    /// Debug ray mesh (thin line from camera to hit point).
    #[allow(dead_code)]
    debug_ray_mesh: Option<sa_core::Handle<sa_render::MeshMarker>>,
    /// Debug ray endpoint and color for rendering.
    #[allow(clippy::type_complexity, dead_code)]
    debug_ray_data: Option<([f32; 3], [f32; 3], [f32; 3], bool)>, // origin, end, color, visible
    /// UI system (egui HUD overlay and monitors).
    ui_system: Option<ui::UiSystem>,
    /// Screen quad mesh for the helm monitor.
    screen_quad: Option<ScreenQuad>,
    /// Bind group for the helm monitor texture.
    screen_bind_group: Option<wgpu::BindGroup>,
    /// Screen quad mesh for the sensors monitor.
    sensors_quad: Option<ScreenQuad>,
    /// Bind group for the sensors monitor texture.
    sensors_bind_group: Option<wgpu::BindGroup>,
    /// Drive controller (impulse/cruise/warp).
    drive: sa_ship::DriveController,
    /// Drive visual state for smooth shader transitions.
    drive_visuals: drive_integration::DriveVisualState,
    /// Ship survival resources (fuel, oxygen, power).
    ship_resources: ShipResources,
    /// Suit survival resources (emergency O2 and battery).
    suit: SuitResources,
    /// Active solar system (set when the player enters one).
    active_system: Option<solar_system::ActiveSystem>,
    /// Mouse look offset while seated at helm (yaw/pitch in ship-local frame).
    /// Separate from camera.yaw/pitch which belong to walk mode.
    helm_look_yaw: f32,
    helm_look_pitch: f32,
    /// Resource deposits in the game world.
    deposits: Vec<ResourceDeposit>,
    /// IDs of deposits that have been gathered.
    gathered: HashSet<u64>,
    /// Index of the nearest gatherable deposit (within range), if any.
    nearest_gatherable: Option<usize>,
    /// Navigation: nearby stars, lock-on, gravity well detection.
    navigation: navigation::Navigation,
    /// Audio manager (engine, music, SFX, voice).
    audio: sa_audio::AudioManager,
    /// Prevents spamming the low-fuel voice announcement.
    fuel_low_announced: bool,
    /// Prevents repeated proximity warnings during a single warp pass.
    proximity_warned: bool,
    /// Previous frame's distance to locked target (for approach voice trigger).
    prev_target_dist: Option<f64>,
    /// Current game phase (Menu or Playing).
    phase: GamePhase,
    /// Main menu state (only present during Menu phase).
    menu: Option<menu::MainMenu>,
    /// Deferred star lock-on flag — set by Tab key, consumed in RedrawRequested
    /// after camera orientation is fully updated for the current frame.
    wants_star_lock: bool,
    /// Active terrain manager (when near a landable planet).
    terrain: Option<terrain_integration::TerrainManager>,
    /// Gravity state from terrain (planet gravity blending).
    terrain_gravity: Option<sa_terrain::gravity::GravityState>,
    /// Set by menu Quit action, checked in event loop to exit.
    quit_requested: bool,
    /// Landing state machine.
    landing: landing::LandingSystem,
    /// Landing skid collider handles (created when ship is spawned).
    #[allow(dead_code)]
    landing_skids: Option<[rapier3d::prelude::ColliderHandle; 4]>,
    /// Last computed minimum clearance from landing system for altitude HUD display.
    last_clearance: Option<f32>,
    /// Countdown timer for altitude proximity beeps (seconds until next beep).
    altitude_beep_timer: f32,
}

impl App {
    fn new() -> Self {
        // Zero gravity for space
        // Use normal gravity — the ship body has gravity_scale(0.0) so it's unaffected.
        // The player body uses default gravity_scale(1.0) so it falls naturally onto the floor.
        // This is more stable than manually applying force each frame.
        let mut physics = PhysicsWorld::new(); // default gravity (0, -9.81, 0)

        // Create ship and interactables
        let (ship, interaction, ids) =
            ship_setup::create_ship_and_interactables(&mut physics);

        // Create landing skids on the ship body (persist for the lifetime of the session).
        let landing_skids = Some(ship.add_landing_skids(
            &mut physics,
            ship_colliders::SHIP_EXTERIOR,
            ship_colliders::TERRAIN,
        ));

        // Helm controller: viewpoint at pilot seat position (port side) + eye height
        let helm = HelmController::new(glam::Vec3::new(-0.8, 0.3, 2.0));

        // Spawn player behind the helm seats (standing room in v2 cockpit)
        let player = PlayerController::spawn(&mut physics, 0.0, 0.0, 3.5);

        let camera = Camera::new();
        let seed = MasterSeed(42);
        let universe = Universe::new(seed);

        Self {
            window: None,
            gpu: None,
            renderer: None,
            camera,
            input: InputState::new(),
            world: GameWorld::new(),
            events: EventBus::new(),
            time: FrameTime::new(),
            schedule: Schedule::new(),
            last_frame: Instant::now(),
            cube_mesh: None,
            cursor_grabbed: false,
            physics,
            player: Some(player),
            star_streaming: star_streaming::StarStreaming::new(seed),
            universe,
            nebulae: Vec::new(),
            distant_galaxies: Vec::new(),
            perf: PerfTimings::default(),
            perf_update_timer: 0.0,
            teleport_counter: 0,
            fly_mode: false,
            fly_speed: 5.0,
            galactic_position: WorldPos::ORIGIN,
            ship_part_mesh: None,
            ship_part_index: 0,
            view_mode: 0,
            ship: Some(ship),
            interaction: Some(interaction),
            ship_ids: Some(ids),
            helm: Some(helm),
            interactable_meshes: Vec::new(),
            debug_ray_mesh: None,
            debug_ray_data: None,
            ui_system: None,
            screen_quad: None,
            screen_bind_group: None,
            sensors_quad: None,
            sensors_bind_group: None,
            drive: sa_ship::DriveController::new(),
            drive_visuals: drive_integration::DriveVisualState::new(),
            ship_resources: ShipResources::new(),
            suit: SuitResources::new(),
            active_system: None,
            helm_look_yaw: 0.0,
            helm_look_pitch: 0.0,
            deposits: generate_deposits(42),
            gathered: HashSet::new(),
            nearest_gatherable: None,
            navigation: navigation::Navigation::new(seed),
            audio: sa_audio::AudioManager::new("resources/sounds".into()),
            fuel_low_announced: false,
            proximity_warned: false,
            prev_target_dist: None,
            phase: GamePhase::Menu,
            menu: None,
            wants_star_lock: false,
            terrain: None,
            terrain_gravity: None,
            quit_requested: false,
            landing: landing::LandingSystem::new(),
            landing_skids,
            last_clearance: None,
            altitude_beep_timer: 0.0,
        }
    }

    fn setup_scene(&mut self) {
        let renderer = self.renderer.as_mut().unwrap();
        let gpu = self.gpu.as_ref().unwrap();
        let handle = renderer.mesh_store.upload(&gpu.device, &make_cube());
        self.cube_mesh = Some(handle);

        // Generate nebulae and distant galaxies from the master seed.
        let seed = MasterSeed(42);
        self.nebulae = sa_universe::generate_nebulae(seed);
        self.distant_galaxies = sa_universe::generate_distant_galaxies(seed);

        // Force-load initial star field (instant, no fade on first frame)
        self.star_streaming.force_load(self.galactic_position);

        // Upload nebula instances.
        let nebula_instances = nebulae_to_instances(&self.nebulae, WorldPos::ORIGIN);
        renderer
            .nebula_renderer
            .update_instances(&gpu.device, &nebula_instances);
        log::info!("Uploaded {} nebula instances", nebula_instances.len());

        // Upload distant galaxy instances (direction-based, observer-independent).
        let galaxy_instances = distant_galaxies_to_instances(&self.distant_galaxies);
        renderer
            .galaxy_renderer
            .update_instances(&gpu.device, &galaxy_instances);
        log::info!(
            "Uploaded {} distant galaxy instances",
            galaxy_instances.len(),
        );

        // Generate and upload ship hull mesh
        let ship_meshes = ship_setup::generate_ship_meshes();
        let hull_data = meshgen_to_render(&ship_meshes.hull);
        let hull_handle = renderer.mesh_store.upload(&gpu.device, &hull_data);
        self.ship_part_mesh = Some(hull_handle);

        // Upload interactable meshes
        for (mesh, _pos) in &ship_meshes.interactable_meshes {
            let data = meshgen_to_render(mesh);
            let handle = renderer.mesh_store.upload(&gpu.device, &data);
            self.interactable_meshes.push(handle);
        }
        log::info!(
            "Uploaded ship hull + {} interactable meshes",
            self.interactable_meshes.len(),
        );

        // Create screen quad for the helm monitor (0.4 x 0.25, matching Speed Display size)
        let screen_quad = ScreenQuad::new(&gpu.device, 0.6, 0.4);
        self.screen_quad = Some(screen_quad);

        // Create sensors screen quad (same size as helm)
        let sensors_quad = ScreenQuad::new(&gpu.device, 0.6, 0.4);
        self.sensors_quad = Some(sensors_quad);

        // Create initial bind groups for the monitor textures
        if let Some(ui_sys) = &self.ui_system {
            let bind_group = renderer.screen_pipeline.create_texture_bind_group(
                &gpu.device,
                ui_sys.helm_texture_view(),
            );
            self.screen_bind_group = Some(bind_group);

            let sensors_bg = renderer.screen_pipeline.create_texture_bind_group(
                &gpu.device,
                ui_sys.sensors_texture_view(),
            );
            self.sensors_bind_group = Some(sensors_bg);
        }
    }

    /// Rebuild the GPU star buffer from the procedural universe if the observer
    /// has moved more than `STAR_REGEN_THRESHOLD` since the last generation,
    /// or if stars have never been generated yet.
    fn maybe_regenerate_stars(&mut self) {
        let observer = self.galactic_position;
        let dt = self.time.delta_seconds() as f32;

        let sector_changed = self.star_streaming.update(observer, dt);

        // Rebuild every frame when drive is engaged — observer moves continuously
        // and star directions must track it. In impulse, only rebuild on sector change.
        let drive_active = self.drive.mode() != sa_ship::DriveMode::Impulse
            && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged);

        if !sector_changed && !drive_active && !self.fly_mode {
            return;
        }

        let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) else {
            return;
        };

        let vertices = self.star_streaming.build_vertices(observer);
        renderer.star_field.update_star_buffer(&gpu.device, &vertices);

        // Refresh nebulae when moving at galactic scale (fly mode or warp)
        if self.fly_mode || drive_active {
            let nebula_instances = nebulae_to_instances(&self.nebulae, observer);
            renderer.nebula_renderer.update_instances(&gpu.device, &nebula_instances);
        }
    }

    /// Teleport to a position in the galaxy. `viewpoint` selects the type:
    /// 0=mid-disc, 1=above-galaxy, 2=galaxy-edge, 3=near-center, 4=near-nebula
    fn teleport_to(&mut self, viewpoint: u64) {
        self.teleport_counter += 1;
        let mut rng = sa_universe::Rng64::new(self.teleport_counter.wrapping_mul(0xDEAD_BEEF));
        let (x, y, z, label) = match viewpoint {
            0 => {
                // Inside the disc, mid-galaxy (like our Sun)
                let r = rng.range_f64(20000.0, 35000.0);
                let theta = rng.range_f64(0.0, std::f64::consts::TAU);
                (r * theta.cos(), rng.range_f64(-100.0, 100.0), r * theta.sin(), "mid-disc")
            }
            1 => {
                // Above the galaxy — looking down at the disc
                let r = rng.range_f64(0.0, 15000.0);
                let theta = rng.range_f64(0.0, std::f64::consts::TAU);
                (r * theta.cos(), rng.range_f64(15000.0, 40000.0), r * theta.sin(), "above-galaxy")
            }
            2 => {
                // Edge of galaxy — sparse, looking back at the disc
                let r = rng.range_f64(45000.0, 70000.0);
                let theta = rng.range_f64(0.0, std::f64::consts::TAU);
                (r * theta.cos(), rng.range_f64(-1000.0, 1000.0), r * theta.sin(), "galaxy-edge")
            }
            3 => {
                // Near galactic center — dense, warm
                let r = rng.range_f64(1000.0, 5000.0);
                let theta = rng.range_f64(0.0, std::f64::consts::TAU);
                (r * theta.cos(), rng.range_f64(-200.0, 200.0), r * theta.sin(), "near-center")
            }
            _ => {
                // Near a nebula — pick a random nebula and teleport close
                if !self.nebulae.is_empty() {
                    let idx = (rng.next_u64() % self.nebulae.len() as u64) as usize;
                    let neb = &self.nebulae[idx];
                    let offset = neb.radius * 0.5;
                    (neb.x + rng.range_f64(-offset, offset),
                     neb.y + rng.range_f64(-offset, offset),
                     neb.z + rng.range_f64(-offset, offset),
                     "near-nebula")
                } else {
                    (27000.0, 0.0, 0.0, "fallback")
                }
            }
        };

        self.galactic_position = WorldPos::new(x, y, z);
        self.camera.position = self.galactic_position;

        // Move the physics body to the new position so the next frame doesn't
        // overwrite the camera with the old body position.
        if let Some(player) = &self.player
            && let Some(body) = self.physics.get_body_mut(player.body_handle)
        {
            body.set_translation(
                rapier3d::na::Vector3::new(x as f32, y as f32, z as f32),
                true,
            );
            body.set_linvel(rapier3d::na::Vector3::new(0.0, 0.0, 0.0), true);
        }

        let r = (x * x + y * y + z * z).sqrt();
        log::info!(
            "Teleported to ({:.0}, {:.0}, {:.0}) — {:.0} ly from center [{}]",
            x, y, z, r, label,
        );

        // Regenerate stars and nebulae immediately for the new position
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            let start = Instant::now();

            // Force-load all sectors at new position (instant, no fade)
            self.star_streaming.force_load(self.galactic_position);
            let vertices = self.star_streaming.build_vertices(self.galactic_position);
            renderer.star_field.update_star_buffer(&gpu.device, &vertices);

            // Refresh nebula instances
            let nebula_instances = nebulae_to_instances(&self.nebulae, self.galactic_position);
            renderer.nebula_renderer.update_instances(&gpu.device, &nebula_instances);

            log::info!(
                "Regen complete in {:.0}ms ({} stars, {} sectors)",
                start.elapsed().as_secs_f64() * 1000.0,
                vertices.len(),
                self.star_streaming.sector_count(),
            );
        }
    }

    /// Write live debug state to /tmp/spaceaway_debug.json for external inspection.
    #[allow(clippy::collapsible_if)]
    fn write_debug_state(&self) {
        let mut lines = Vec::new();
        lines.push("{".to_string());
        lines.push(format!("  \"frame\": {},", self.time.frame_count()));
        lines.push(format!("  \"view_mode\": \"{}\",", match self.view_mode {
            6 => "PART_PREVIEW",
            7 => "SHIP_PREVIEW",
            _ => if self.helm.as_ref().is_some_and(|h| h.is_seated()) { "HELM" } else if self.fly_mode { "FLY" } else { "WALK" },
        }));
        lines.push(format!("  \"cursor_grabbed\": {},", self.cursor_grabbed));

        // Player state
        if let Some(player) = &self.player {
            if let Some(body) = self.physics.get_body(player.body_handle) {
                let p = body.translation();
                let v = body.linvel();
                lines.push("  \"player\": {".to_string());
                lines.push(format!("    \"pos\": [{:.3}, {:.3}, {:.3}],", p.x, p.y, p.z));
                lines.push(format!("    \"vel\": [{:.3}, {:.3}, {:.3}],", v.x, v.y, v.z));
                lines.push(format!("    \"speed\": {:.3},", v.magnitude()));
                lines.push(format!("    \"sleeping\": {},", body.is_sleeping()));
                lines.push(format!("    \"grounded\": {},", player.grounded));
                lines.push(format!("    \"yaw\": {:.3}, \"pitch\": {:.3}", player.yaw, player.pitch));
                lines.push("  },".to_string());
            }
        }

        // Ship state
        if let Some(ship) = &self.ship {
            if let Some(body) = self.physics.get_body(ship.body_handle) {
                let p = body.translation();
                let v = body.linvel();
                lines.push("  \"ship\": {".to_string());
                lines.push(format!("    \"pos\": [{:.3}, {:.3}, {:.3}],", p.x, p.y, p.z));
                lines.push(format!("    \"vel\": [{:.3}, {:.3}, {:.3}],", v.x, v.y, v.z));
                lines.push(format!("    \"throttle\": {:.3},", ship.throttle));
                lines.push(format!("    \"engine_on\": {},", ship.engine_on));
                lines.push(format!("    \"mass\": {:.1}", body.mass()));
                lines.push("  },".to_string());
            }
        }

        // Interaction state
        if let Some(interaction) = &self.interaction {
            lines.push("  \"interaction\": {".to_string());
            lines.push(format!("    \"hovered\": {:?},", interaction.hovered()));
            lines.push(format!("    \"dragging\": {},", interaction.is_dragging()));
            // Debug ray info
            let dr = interaction.debug_ray();
            lines.push(format!(
                "    \"debug_ray\": {{\"origin\": [{:.3}, {:.3}, {:.3}], \"dir\": [{:.3}, {:.3}, {:.3}], \"hit\": {:?}, \"hit_id\": {:?}}},",
                dr.ray_origin[0], dr.ray_origin[1], dr.ray_origin[2],
                dr.ray_dir[0], dr.ray_dir[1], dr.ray_dir[2],
                dr.hit, dr.hit_id,
            ));
            // Show each interactable's collider world position
            let mut interactable_lines = Vec::new();
            for i in 0..10 { // max 10
                if let Some(inter) = interaction.get(i) {
                    if let Some(col) = self.physics.collider_set.get(inter.collider_handle) {
                        let p = col.position().translation;
                        interactable_lines.push(format!(
                            "      {{\"id\": {}, \"label\": \"{}\", \"world_pos\": [{:.2}, {:.2}, {:.2}]}}",
                            i, inter.label, p.x, p.y, p.z));
                    }
                } else { break; }
            }
            lines.push(format!("    \"interactables\": [{}]", interactable_lines.join(",")));
            lines.push("  },".to_string());
        }

        // Camera
        lines.push("  \"camera\": {".to_string());
        lines.push(format!("    \"pos\": [{:.3}, {:.3}, {:.3}],", self.camera.position.x, self.camera.position.y, self.camera.position.z));
        lines.push(format!("    \"yaw\": {:.3}, \"pitch\": {:.3}", self.camera.yaw, self.camera.pitch));
        lines.push("  },".to_string());

        // Input
        lines.push("  \"input\": {".to_string());
        lines.push(format!("    \"mouse_delta\": [{:.1}, {:.1}],", self.input.mouse.delta().0, self.input.mouse.delta().1));
        lines.push(format!("    \"left_btn\": {}", self.input.mouse.left_pressed()));
        lines.push("  },".to_string());

        // Physics world stats
        // Per-system timing breakdown
        lines.push("  \"timing_ms\": {".to_string());
        lines.push(format!("    \"total\": {:.2},", self.perf.total_us as f64 / 1000.0));
        lines.push(format!("    \"player\": {:.2},", self.perf.player_us as f64 / 1000.0));
        // physics = phys_step_us * 1000 + query_pipeline_us (encoded)
        let phys_step_ms = (self.perf.physics_us / 1000) as f64 / 1000.0;
        let qp_ms = (self.perf.physics_us % 1000) as f64 / 1000.0;
        let move_shape_ms = self.perf.stars_us as f64 / 1000.0;
        lines.push(format!("    \"phys_step\": {:.2},", phys_step_ms));
        lines.push(format!("    \"query_pipeline\": {:.2},", qp_ms));
        lines.push(format!("    \"move_shape\": {:.2},", move_shape_ms));
        lines.push(format!("    \"render\": {:.2},", self.perf.render_us as f64 / 1000.0));
        lines.push(format!("    \"fps\": {:.0}", self.perf.fps));
        lines.push("  },".to_string());

        // Player-to-ship relative position (should be constant when standing still)
        if let (Some(player), Some(ship)) = (&self.player, &self.ship) {
            if let (Some(pb), Some(sb)) = (self.physics.get_body(player.body_handle), self.physics.get_body(ship.body_handle)) {
                let pp = pb.translation();
                let sp = sb.translation();
                lines.push(format!("  \"player_ship_offset\": [{:.3}, {:.3}, {:.3}],",
                    pp.x - sp.x, pp.y - sp.y, pp.z - sp.z));
            }
        }

        lines.push("  \"physics\": {".to_string());
        lines.push(format!("    \"bodies\": {},", self.physics.rigid_body_set.len()));
        lines.push(format!("    \"colliders\": {},", self.physics.collider_set.len()));
        let grav = self.physics.gravity();
        lines.push(format!("    \"gravity\": [{:.1}, {:.1}, {:.1}]", grav.0, grav.1, grav.2));
        lines.push("  }".to_string());

        lines.push("}".to_string());

        let content = lines.join("\n");
        if let Ok(mut f) = std::fs::File::create("/tmp/spaceaway_debug.json") {
            let _ = f.write_all(content.as_bytes());
        }
    }

    fn grab_cursor(&mut self) {
        if let Some(window) = &self.window {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
            self.cursor_grabbed = true;
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("SpaceAway")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            let gpu = GpuContext::new(window.clone());
            let renderer = Renderer::new(&gpu);
            let ui_sys = ui::UiSystem::new(
                &gpu.device,
                gpu.config.format,
                gpu.config.width,
                gpu.config.height,
            );
            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.ui_system = Some(ui_sys);
            self.window = Some(window.clone());
            self.last_frame = Instant::now();
            self.setup_scene();

            // Create main menu scene after GPU is ready
            if self.menu.is_none() && self.phase == GamePhase::Menu {
                if let (Some(gpu_ref), Some(renderer_ref)) = (&self.gpu, &mut self.renderer) {
                    self.menu = Some(menu::MainMenu::new(
                        &mut renderer_ref.mesh_store,
                        &gpu_ref.device,
                    ));
                }
                // Load stars for the menu's galactic position
                if let Some(menu_ref) = &self.menu {
                    let menu_pos = menu_ref.galactic_position();
                    self.star_streaming.force_load(menu_pos);
                    if let (Some(gpu_ref), Some(renderer_ref)) = (&self.gpu, &mut self.renderer) {
                        let verts = self.star_streaming.build_vertices(menu_pos);
                        renderer_ref.star_field.update_star_buffer(&gpu_ref.device, &verts);
                    }
                }
                self.audio.set_music_context(sa_audio::MusicContext::Idle);
                // Make sure cursor is free during menu
                window.set_cursor_visible(true);
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                self.cursor_grabbed = false;
                log::info!("Main menu created");
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                    self.input
                        .keyboard
                        .set_pressed(code, event.state.is_pressed());
                    if code == KeyCode::Escape
                        && event.state.is_pressed()
                        && let Some(window) = &self.window
                    {
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        self.cursor_grabbed = false;
                    }

                    // Menu keyboard navigation
                    if event.state.is_pressed() && self.phase == GamePhase::Menu
                        && let Some(menu) = &mut self.menu
                    {
                        match code {
                            KeyCode::ArrowUp | KeyCode::KeyW => menu.nav_up(),
                            KeyCode::ArrowDown | KeyCode::KeyS => menu.nav_down(),
                            KeyCode::Enter | KeyCode::Space => {
                                match menu.selected {
                                    0 => { // Continue
                                        self.phase = GamePhase::Playing;
                                        self.menu = None;
                                        self.star_streaming.force_load(self.galactic_position);
                                        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                            let verts = self.star_streaming.build_vertices(self.galactic_position);
                                            renderer.star_field.update_star_buffer(&gpu.device, &verts);
                                        }
                                        self.audio.set_power(true);
                                        log::info!("Starting game — entering ship");
                                    }
                                    3 => self.quit_requested = true, // Quit
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }

                    // Debug teleport: 0 = 1km above nearest planet (works seated or standing)
                    if event.state.is_pressed() && code == KeyCode::Digit0 && self.phase == GamePhase::Playing {
                        let ly_to_m = 9.461e15_f64;
                        let m_to_ly = 1.0 / ly_to_m;
                        let planet_info: Option<(WorldPos, f64)> = if let Some(terrain_mgr) = &self.terrain {
                            Some((terrain_mgr.frozen_planet_center_ly(), terrain_mgr.planet_radius_m()))
                        } else if let Some(sys) = &self.active_system {
                            let positions = sys.compute_positions_ly_pub();
                            let mut best: Option<(usize, f64)> = None;
                            for (i, pos) in positions.iter().enumerate() {
                                if sys.body_radius_m(i).is_some() && sys.planet_data(i).is_some() {
                                    let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                                    let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                                    let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                                    let d = (dx*dx + dy*dy + dz*dz).sqrt();
                                    if best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                                        best = Some((i, d));
                                    }
                                }
                            }
                            best.and_then(|(i, _)| {
                                let r = sys.body_radius_m(i)?;
                                Some((*positions.get(i)?, r))
                            })
                        } else {
                            None
                        };
                        if let Some((planet_pos, radius_m)) = planet_info {
                            let dx = (self.galactic_position.x - planet_pos.x) * ly_to_m;
                            let dy = (self.galactic_position.y - planet_pos.y) * ly_to_m;
                            let dz = (self.galactic_position.z - planet_pos.z) * ly_to_m;
                            let dist = (dx*dx + dy*dy + dz*dz).sqrt().max(1.0);
                            let target_dist = radius_m + 1000.0;
                            let scale = target_dist / dist;
                            self.galactic_position = WorldPos::new(
                                planet_pos.x + dx * scale * m_to_ly,
                                planet_pos.y + dy * scale * m_to_ly,
                                planet_pos.z + dz * scale * m_to_ly,
                            );
                            self.camera.position = self.galactic_position;
                            if let Some(ship) = &self.ship
                                && let Some(body) = self.physics.rigid_body_set.get_mut(ship.body_handle)
                            {
                                body.set_linvel(nalgebra::Vector3::zeros(), true);
                                body.set_angvel(nalgebra::Vector3::zeros(), true);
                            }
                            self.drive.request_disengage();
                            log::info!("Teleported 1km above planet surface (radius {:.0}km)", radius_m / 1000.0);
                        } else {
                            log::warn!("No planet found — press 8 to enter a system first");
                        }
                    }

                    // Teleport keys: only when NOT seated at helm (1/2/3 are drive keys when seated)
                    let is_seated = self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false);
                    if event.state.is_pressed() && !is_seated && self.phase == GamePhase::Playing {
                        match code {
                            // Debug galaxy teleports — only in fly mode to avoid
                            // conflicting with drive keys (1/2/3) when walking.
                            KeyCode::Digit1 if self.fly_mode => self.teleport_to(0),
                            KeyCode::Digit2 if self.fly_mode => self.teleport_to(1),
                            KeyCode::Digit3 if self.fly_mode => self.teleport_to(2),
                            KeyCode::Digit4 if self.fly_mode => self.teleport_to(3),
                            KeyCode::Digit5 if self.fly_mode => self.teleport_to(4),
                            KeyCode::KeyF => {
                                self.fly_mode = !self.fly_mode;
                                log::info!("Fly mode: {}", if self.fly_mode { "ON (WASD to fly, scroll to change speed)" } else { "OFF" });
                            }
                            KeyCode::KeyV => {
                                if let Some(gpu) = &mut self.gpu {
                                    let vsync = gpu.toggle_vsync();
                                    log::info!("VSync: {}", if vsync { "ON (60 FPS cap)" } else { "OFF (uncapped — benchmark mode)" });
                                }
                            }
                            KeyCode::Digit6 => {
                                // Cycle through individual ship parts
                                let parts = all_ship_parts();
                                self.ship_part_index = if self.view_mode == 6 {
                                    (self.ship_part_index + 1) % parts.len()
                                } else {
                                    0
                                };
                                self.view_mode = 6;
                                let (name, mesh) = &parts[self.ship_part_index];
                                if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                    let mesh_data = meshgen_to_render(mesh);
                                    let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                                    self.ship_part_mesh = Some(handle);
                                    log::info!("Showing ship part: {} ({} verts, {} tris)",
                                        name, mesh.vertices.len(), mesh.triangle_count());
                                }
                            }
                            KeyCode::Digit7 => {
                                // Assemble and render full ship
                                self.view_mode = 7;
                                let ship_mesh = assemble_ship();
                                if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                    let mesh_data = meshgen_to_render(&ship_mesh);
                                    let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                                    self.ship_part_mesh = Some(handle);
                                    log::info!("Assembled full ship: {} verts, {} tris",
                                        ship_mesh.vertices.len(), ship_mesh.triangle_count());
                                }
                            }
                            KeyCode::Tab => {
                                // Flag for lock-on — actual projection happens in RedrawRequested
                                // where the camera orientation is current for this frame.
                                self.wants_star_lock = true;
                            }
                            KeyCode::Digit8 => {
                                // Debug: cycle through planets in current system (or jump to nearest star)
                                if let Some(system) = &mut self.active_system {
                                    // Already in a system — cycle to next planet
                                    let planets = &system.system.planets;
                                    if !planets.is_empty() {
                                        let idx = self.teleport_counter as usize % planets.len();
                                        self.teleport_counter += 1;
                                        let planet = &planets[idx];
                                        let meters_in_ly = 1.0 / 9.461e15_f64;
                                        let planet_radius_m = planet.radius_earth as f64 * 6_371_000.0;
                                        let view_dist_m = planet_radius_m * 1.8; // inside terrain activation (2.0×)
                                        // Use the planet's ACTUAL orbital position instead of
                                        // assuming it's along +X from the star. Planets orbit
                                        // with TIME_SCALE so their position depends on game_time.
                                        let positions = system.compute_positions_ly_pub();
                                        // Planet body index: star(0) + corona(1) + planets start at 2
                                        // Count preceding bodies to find this planet's body index
                                        let mut body_idx = 2; // skip star + corona
                                        for pi in 0..idx {
                                            body_idx += 1; // planet
                                            let pp = &system.system.planets[pi];
                                            if pp.atmosphere.is_some() { body_idx += 1; }
                                            if pp.has_rings { body_idx += 1; }
                                            body_idx += pp.moons.len();
                                        }
                                        let planet_pos = positions.get(body_idx)
                                            .copied()
                                            .unwrap_or(system.star_galactic_pos);
                                        // Place camera at view_dist along the star→planet direction
                                        let dx = planet_pos.x - system.star_galactic_pos.x;
                                        let dz = planet_pos.z - system.star_galactic_pos.z;
                                        let dist_ly = (dx * dx + dz * dz).sqrt();
                                        let (dir_x, dir_z) = if dist_ly > 1e-20 {
                                            (dx / dist_ly, dz / dist_ly)
                                        } else {
                                            (1.0, 0.0)
                                        };
                                        let above_ly = view_dist_m * 0.4 * meters_in_ly;
                                        self.galactic_position = WorldPos::new(
                                            planet_pos.x + dir_x * view_dist_m * meters_in_ly,
                                            planet_pos.y + above_ly,
                                            planet_pos.z + dir_z * view_dist_m * meters_in_ly,
                                        );
                                        self.camera.position = self.galactic_position;
                                        self.camera.orientation_override = None;
                                        self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                                        self.camera.pitch = -0.2; // look down toward planet
                                        log::info!("→ Planet {} of {}: {:?} {:.0}km at {:.2}AU (viewing from {:.0}km)",
                                            idx + 1, planets.len(), planet.sub_type,
                                            planet.radius_earth * 6371.0, planet.orbital_radius_au,
                                            view_dist_m / 1000.0);
                                    }
                                } else {
                                    // Not in a system — jump to nearest star
                                    self.navigation.update_nearby(self.galactic_position);
                                    if let Some(star) = self.navigation.nearby_stars.first().cloned() {
                                        log::info!("Jumping to: {} ({:.2} ly)", star.catalog_name, star.distance_ly);
                                        let sector_coord = sa_universe::SectorCoord::new(
                                            star.id.sector_x().into(),
                                            star.id.sector_y().into(),
                                            star.id.sector_z().into(),
                                        );
                                        let sector = sa_universe::sector::generate_sector(MasterSeed(42), sector_coord);
                                        if let Some(placed) = sector.stars.iter().find(|s| s.id == star.id) {
                                            let system = sa_universe::generate_system(&placed.star, star.id.0);
                                            let au_in_ly = 1.581e-5_f64;
                                            let meters_in_ly = 1.0 / 9.461e15_f64;
                                            let (offset_ly, desc) = if let Some(p) = system.planets.first() {
                                                let r_m = p.radius_earth as f64 * 6_371_000.0;
                                                let view_m = r_m * 1.8;
                                                let orb_ly = p.orbital_radius_au as f64 * au_in_ly;
                                                (orb_ly + view_m * meters_in_ly, format!(
                                                    "Planet 1: {:?} {:.0}km at {:.2}AU",
                                                    p.sub_type, p.radius_earth * 6371.0, p.orbital_radius_au))
                                            } else {
                                                let sr = placed.star.radius as f64 * 696_000_000.0;
                                                (sr * 3.0 * meters_in_ly, "No planets — near star".into())
                                            };
                                            let above_ly = offset_ly * 0.3;
                                            self.galactic_position = WorldPos::new(
                                                star.galactic_pos.x + offset_ly,
                                                star.galactic_pos.y + above_ly,
                                                star.galactic_pos.z);
                                            self.camera.position = self.galactic_position;
                                            self.camera.orientation_override = None;
                                            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                                            self.camera.pitch = -0.15;
                                            self.drive.request_disengage();
                                            self.teleport_counter = 1; // next press cycles to planet 2
                                            self.star_streaming.force_load(self.galactic_position);
                                            if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                                let verts = self.star_streaming.build_vertices(self.galactic_position);
                                                renderer.star_field.update_star_buffer(&gpu.device, &verts);
                                            }
                                            if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                                let sys = solar_system::ActiveSystem::load(
                                                    star.id, placed, &mut renderer.mesh_store, &gpu.device);
                                                log::info!("System: {} bodies — {}", sys.body_count(), desc);
                                                for line in sys.body_summary() { log::info!("  {}", line); }
                                                // Re-teleport to the planet's actual orbital position
                                                // (the position computed above assumed +X axis, but
                                                // planets orbit at initial_phase angle).
                                                if let Some(p) = system.planets.first() {
                                                    let positions = sys.compute_positions_ly_pub();
                                                    if let Some(&planet_pos) = positions.get(2) {
                                                        let r_m = p.radius_earth as f64 * 6_371_000.0;
                                                        let view_m = r_m * 1.8;
                                                        let dx = planet_pos.x - star.galactic_pos.x;
                                                        let dz = planet_pos.z - star.galactic_pos.z;
                                                        let d = (dx * dx + dz * dz).sqrt();
                                                        let (nx, nz) = if d > 1e-20 { (dx/d, dz/d) } else { (1.0, 0.0) };
                                                        self.galactic_position = WorldPos::new(
                                                            planet_pos.x + nx * view_m * meters_in_ly,
                                                            planet_pos.y + view_m * 0.4 * meters_in_ly,
                                                            planet_pos.z + nz * view_m * meters_in_ly,
                                                        );
                                                        self.camera.position = self.galactic_position;
                                                    }
                                                }
                                                // Clean up terrain before switching system
                                                if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                                                self.terrain = None;
                                                self.terrain_gravity = None;
                                                self.active_system = Some(sys);
                                            }
                                        }
                                        self.navigation.clear_target();
                                    }
                                }
                            }
                            KeyCode::Digit9 => {
                                // Debug: jump to a DIFFERENT star (skip nearest, pick next)
                                if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                                self.terrain = None;
                                self.terrain_gravity = None;
                                self.active_system = None;
                                self.navigation.update_nearby(self.galactic_position);
                                let skip = (self.teleport_counter as usize) % self.navigation.nearby_stars.len().max(1);
                                self.teleport_counter += 1;
                                if let Some(star) = self.navigation.nearby_stars.get(skip).cloned() {
                                    // Teleport near the star (system loads on next key 8 press)
                                    let au_in_ly = 1.581e-5_f64;
                                    self.galactic_position = WorldPos::new(
                                        star.galactic_pos.x + 30.0 * au_in_ly,
                                        star.galactic_pos.y, star.galactic_pos.z);
                                    self.camera.position = self.galactic_position;
                                    self.star_streaming.force_load(self.galactic_position);
                                    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                        let verts = self.star_streaming.build_vertices(self.galactic_position);
                                        renderer.star_field.update_star_buffer(&gpu.device, &verts);
                                    }
                                    log::info!("Jumped near {} ({:.2} ly) — press 8 to enter system",
                                        star.catalog_name, star.distance_ly);
                                }
                            }
                            KeyCode::Digit0 => {
                                // Debug: teleport 1km above the nearest planet surface.
                                // If terrain is already active, use the frozen planet.
                                // Otherwise, find the nearest planet in the system.
                                let ly_to_m = 9.461e15_f64;
                                let m_to_ly = 1.0 / ly_to_m;

                                let planet_info: Option<(WorldPos, f64)> = if let Some(terrain_mgr) = &self.terrain {
                                    Some((terrain_mgr.frozen_planet_center_ly(), terrain_mgr.planet_radius_m()))
                                } else if let Some(sys) = &self.active_system {
                                    let positions = sys.compute_positions_ly_pub();
                                    let mut best: Option<(usize, f64)> = None;
                                    for (i, pos) in positions.iter().enumerate() {
                                        if let Some(_r) = sys.body_radius_m(i)
                                            && sys.planet_data(i).is_some()
                                        {
                                            let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                                            let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                                            let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                                            let d = (dx*dx + dy*dy + dz*dz).sqrt();
                                            if best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                                                best = Some((i, d));
                                            }
                                        }
                                    }
                                    best.and_then(|(i, _)| {
                                        let r = sys.body_radius_m(i)?;
                                        Some((*positions.get(i)?, r))
                                    })
                                } else {
                                    None
                                };

                                if let Some((planet_pos, radius_m)) = planet_info {
                                    // Place 1km above surface, along planet→ship direction
                                    let dx = (self.galactic_position.x - planet_pos.x) * ly_to_m;
                                    let dy = (self.galactic_position.y - planet_pos.y) * ly_to_m;
                                    let dz = (self.galactic_position.z - planet_pos.z) * ly_to_m;
                                    let dist = (dx*dx + dy*dy + dz*dz).sqrt().max(1.0);
                                    let target_dist = radius_m + 1000.0; // 1km above surface
                                    let scale = target_dist / dist;
                                    self.galactic_position = WorldPos::new(
                                        planet_pos.x + dx * scale * m_to_ly,
                                        planet_pos.y + dy * scale * m_to_ly,
                                        planet_pos.z + dz * scale * m_to_ly,
                                    );
                                    self.camera.position = self.galactic_position;
                                    // Zero ship velocity
                                    if let Some(ship) = &self.ship
                                        && let Some(body) = self.physics.rigid_body_set.get_mut(ship.body_handle)
                                    {
                                        body.set_linvel(nalgebra::Vector3::zeros(), true);
                                        body.set_angvel(nalgebra::Vector3::zeros(), true);
                                    }
                                    self.drive.request_disengage();
                                    log::info!("Teleported 1km above planet surface (radius {:.0}km, alt 1km)",
                                        radius_m / 1000.0);
                                } else {
                                    log::warn!("No planet found — press 8 to enter a system first");
                                }
                            }
                            KeyCode::Backquote => {
                                // Debug: find and teleport to a planet with rings or atmosphere
                                log::info!("Searching for interesting planet...");
                                self.navigation.update_nearby(self.galactic_position);
                                let mut found = false;
                                'search: for star_nav in self.navigation.nearby_stars.iter().cloned() {
                                    let sc = sa_universe::SectorCoord::new(
                                        star_nav.id.sector_x().into(),
                                        star_nav.id.sector_y().into(),
                                        star_nav.id.sector_z().into(),
                                    );
                                    let sector = sa_universe::sector::generate_sector(MasterSeed(42), sc);
                                    if let Some(placed) = sector.stars.iter().find(|s| s.id == star_nav.id) {
                                        let sys = sa_universe::generate_system(&placed.star, star_nav.id.0);
                                        for (pi, planet) in sys.planets.iter().enumerate() {
                                            let has_feature = planet.has_rings || planet.atmosphere.is_some();
                                            if !has_feature { continue; }

                                            let au_in_ly = 1.581e-5_f64;
                                            let meters_in_ly = 1.0 / 9.461e15_f64;
                                            let r_m = planet.radius_earth as f64 * 6_371_000.0;
                                            let view_m = r_m * 1.5;
                                            let orb_ly = planet.orbital_radius_au as f64 * au_in_ly;

                                            let above_ly = view_m * 0.4 * meters_in_ly;
                                            self.galactic_position = WorldPos::new(
                                                star_nav.galactic_pos.x + orb_ly + view_m * meters_in_ly,
                                                star_nav.galactic_pos.y + above_ly,
                                                star_nav.galactic_pos.z,
                                            );
                                            self.camera.position = self.galactic_position;
                                            self.camera.orientation_override = None;
                                            self.camera.yaw = -std::f32::consts::FRAC_PI_2;
                                            self.camera.pitch = 0.0;
                                            self.drive.request_disengage();
                                            self.star_streaming.force_load(self.galactic_position);
                                            if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                                let verts = self.star_streaming.build_vertices(self.galactic_position);
                                                renderer.star_field.update_star_buffer(&gpu.device, &verts);
                                                let loaded = solar_system::ActiveSystem::load(
                                                    star_nav.id, placed, &mut renderer.mesh_store, &gpu.device);
                                                log::info!("Found! {} — Planet {}: {:?} {:.0}km rings={} atmo={}",
                                                    star_nav.catalog_name, pi + 1, planet.sub_type,
                                                    planet.radius_earth * 6371.0,
                                                    planet.has_rings, planet.atmosphere.is_some());
                                                log::info!("System: {} bodies total", loaded.body_count());
                                                for line in loaded.body_summary() { log::info!("  {}", line); }
                                                // Clean up terrain before switching system
                                                if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                                                self.terrain = None;
                                                self.terrain_gravity = None;
                                                self.active_system = Some(loaded);
                                            }
                                            self.navigation.clear_target();
                                            found = true;
                                            break 'search;
                                        }
                                    }
                                }
                                if !found { log::warn!("No planets with rings/atmosphere found nearby"); }
                            }
                            KeyCode::Equal => { self.fly_speed *= 2.0; log::info!("Fly speed: {:.0} ly/s", self.fly_speed); }
                            KeyCode::Minus => { self.fly_speed = (self.fly_speed / 2.0).max(1.0); log::info!("Fly speed: {:.0} ly/s", self.fly_speed); }
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse.set_cursor_position(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    self.input.mouse.set_left_pressed(state.is_pressed());
                }
                // Only grab cursor during Playing phase
                if state.is_pressed() && !self.cursor_grabbed && self.phase == GamePhase::Playing {
                    self.grab_cursor();
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size.width, new_size.height);
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(gpu);
                    }
                    if let Some(ui_sys) = &mut self.ui_system {
                        ui_sys.resize(new_size.width, new_size.height);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let frame_start = Instant::now();
                let now = frame_start;
                self.time.advance(now - self.last_frame);
                self.last_frame = now;

                let dt = self.time.delta_seconds() as f32;

                // --- MENU PHASE ---
                if self.phase == GamePhase::Menu {
                    if let Some(menu_ref) = &mut self.menu {
                        menu_ref.update(dt);
                    }
                    self.audio.update(dt);

                    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                        // Update star field for menu position
                        let menu_pos = self.menu.as_ref()
                            .map(|m| m.galactic_position())
                            .unwrap_or(WorldPos::ORIGIN);
                        let needs_rebuild = self.star_streaming.update(menu_pos, dt);
                        if needs_rebuild {
                            let verts = self.star_streaming.build_vertices(menu_pos);
                            renderer.star_field.update_star_buffer(&gpu.device, &verts);
                        }

                        // Get menu draw commands
                        let commands = self.menu.as_ref()
                            .map(|m| m.draw_commands())
                            .unwrap_or_default();
                        let screen_draws: Vec<ScreenDrawCommand<'_>> = vec![];
                        let drive_params = sa_render::DriveRenderParams::default();

                        let default_cam = Camera::new();
                        let menu_camera = self.menu.as_ref()
                            .map(|m| m.camera())
                            .unwrap_or(&default_cam);

                        if let Some(mut frame_ctx) = renderer.render_frame(
                            gpu,
                            menu_camera,
                            &commands,
                            &screen_draws,
                            Vec3::new(0.5, -0.8, -0.3),
                            menu_pos,
                            &drive_params,
                        ) {
                            // Render menu egui overlay
                            if let Some(ui_sys) = &mut self.ui_system {
                                // Pass mouse position (physical pixels) and click state to egui
                                let mouse_pos = self.input.mouse.position()
                                    .map(|(x, y)| [x, y]);
                                let mouse_clicked = self.input.mouse.left_just_pressed();
                                let menu_action = ui_sys.render_menu(
                                    &gpu.device,
                                    &gpu.queue,
                                    &mut frame_ctx.encoder,
                                    &frame_ctx.view,
                                    self.menu.as_mut().unwrap(),
                                    mouse_pos,
                                    mouse_clicked,
                                );
                                match menu_action {
                                    Some(menu::MenuAction::Continue) => {
                                        self.phase = GamePhase::Playing;
                                        self.menu = None;
                                        self.star_streaming.force_load(self.galactic_position);
                                        let verts = self.star_streaming.build_vertices(self.galactic_position);
                                        renderer.star_field.update_star_buffer(&gpu.device, &verts);
                                        self.audio.set_power(true);
                                        log::info!("Starting game -- entering ship");
                                    }
                                    Some(menu::MenuAction::Quit) => {
                                        self.quit_requested = true;
                                    }
                                    None => {}
                                }
                            }

                            Renderer::submit_frame(gpu, frame_ctx);
                        }
                    }

                    self.input.end_frame();
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }

                // --- PLAYING PHASE ---
                self.schedule
                    .run(&mut self.world, &mut self.events, &self.time);

                // --- Player + Physics + Helm ---
                let t0 = Instant::now();

                if self.fly_mode {
                    // Fly mode: move galactic position in light-years, bypass physics.
                    // camera.position tracks galactic_position in fly mode.
                    let (dx, dy) = self.input.mouse.delta();
                    self.camera.rotate(dx * 0.003, -dy * 0.003);

                    let fwd = self.camera.forward();
                    let right = self.camera.right();
                    let speed = self.fly_speed * dt as f64;

                    use winit::keyboard::KeyCode as KC;
                    if self.input.keyboard.is_pressed(KC::KeyW) {
                        self.galactic_position.x += fwd.x as f64 * speed;
                        self.galactic_position.y += fwd.y as f64 * speed;
                        self.galactic_position.z += fwd.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::KeyS) {
                        self.galactic_position.x -= fwd.x as f64 * speed;
                        self.galactic_position.y -= fwd.y as f64 * speed;
                        self.galactic_position.z -= fwd.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::KeyA) {
                        self.galactic_position.x -= right.x as f64 * speed;
                        self.galactic_position.z -= right.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::KeyD) {
                        self.galactic_position.x += right.x as f64 * speed;
                        self.galactic_position.z += right.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::Space) {
                        self.galactic_position.y += speed;
                    }
                    if self.input.keyboard.is_pressed(KC::ShiftLeft) {
                        self.galactic_position.y -= speed;
                    }
                    // Camera follows galactic position in fly mode
                    self.camera.position = self.galactic_position;
                } else if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
                    // --- Seated at helm: WASD rotates ship, mouse free-looks ---
                    if let Some(ship) = &self.ship {
                        // Reset forces each frame so only current input applies
                        ship.reset_forces(&mut self.physics);

                        let wants_stand = if let Some(helm) = &self.helm {
                            helm.update_seated(ship, &mut self.physics, &self.input, dt)
                        } else {
                            false
                        };

                        // Drive mode selection (1/2/3) while seated at helm
                        if self.input.keyboard.just_pressed(KeyCode::Digit1) {
                            self.drive.request_disengage();
                            log::info!("Drive: IMPULSE");
                        }
                        if self.input.keyboard.just_pressed(KeyCode::Digit2) {
                            // Block cruise inside the atmosphere (blend > 0.1).
                            // Above atmosphere with terrain active: cruise is allowed
                            // for approach but auto-disengages at atmosphere boundary.
                            let in_atmosphere = self.terrain_gravity
                                .as_ref()
                                .is_some_and(|g| g.blend > 0.1);
                            if in_atmosphere {
                                log::warn!("Cannot engage cruise: inside atmosphere");
                            } else {
                                let ship_speed = self.ship.as_ref()
                                    .map(|s| s.speed(&self.physics))
                                    .unwrap_or(0.0);
                                if self.drive.request_engage_with_speed(sa_ship::DriveMode::Cruise, ship_speed) {
                                    log::info!("Drive: CRUISE engaged");
                                } else if ship_speed > 100.0 {
                                    log::warn!("Cannot engage cruise: ship moving at {:.0} m/s (slow down first)", ship_speed);
                                }
                            }
                        }

                        // Tab: lock nearest target to crosshair.
                        // In a solar system: lock nearest PLANET. Otherwise: lock nearest STAR.
                        if self.input.keyboard.just_pressed(KeyCode::Tab)
                            && let Some(gpu) = &self.gpu {
                                let sw = gpu.config.width as f32;
                                let sh = gpu.config.height as f32;
                                let cx = sw / 2.0;
                                let cy = sh / 2.0;
                                let aspect = sw / sh;
                                let vp = self.camera.view_projection_matrix(aspect);
                                let cam_fwd = self.camera.forward();

                                let locked = if let Some(sys) = &self.active_system {
                                    // In a solar system — lock nearest planet to crosshair
                                    let positions = sys.compute_positions_ly_pub();
                                    let mut best_dist = f32::MAX;
                                    let mut best: Option<(usize, f32)> = None;

                                    for (i, pos) in positions.iter().enumerate() {
                                        if i == 0 { continue; } // skip star
                                        let Some(radius_m) = sys.body_radius_m(i) else { continue };
                                        let Some((_, _sub_type, _, _, _)) = sys.planet_data(i) else { continue };
                                        // Skip non-planets (atmosphere, rings)
                                        if radius_m < 100_000.0 { continue; }

                                        let dx = (pos.x - self.galactic_position.x) as f32;
                                        let dy = (pos.y - self.galactic_position.y) as f32;
                                        let dz = (pos.z - self.galactic_position.z) as f32;
                                        let len = (dx*dx + dy*dy + dz*dz).sqrt();
                                        if len < 1e-10 { continue; }
                                        let dir_norm = glam::Vec3::new(dx/len, dy/len, dz/len);
                                        // Only consider planets within ~60° of crosshair
                                        if cam_fwd.dot(dir_norm) < 0.5 { continue; }
                                        let dir = dir_norm * 90000.0;
                                        let clip = vp * glam::Vec4::new(dir.x, dir.y, dir.z, 1.0);
                                        if clip.w <= 0.0 { continue; }
                                        let sx = (clip.x / clip.w * 0.5 + 0.5) * sw;
                                        let sy = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * sh;
                                        let sd = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                                        if sd < best_dist {
                                            best_dist = sd;
                                            best = Some((i, sd));
                                        }
                                    }

                                    if let Some((idx, px_dist)) = best {
                                        let pos = positions[idx];
                                        let dist_ly = self.galactic_position.distance_to(pos);
                                        let radius_m = sys.body_radius_m(idx).unwrap_or(0.0);
                                        let (_, sub_type, _, _, _) = sys.planet_data(idx).unwrap();
                                        let name = format!("Planet {} ({:?}, {:.0}km)",
                                            idx, sub_type, radius_m / 1000.0);
                                        let nav = navigation::NavStar {
                                            id: sa_universe::ObjectId(0),
                                            galactic_pos: pos,
                                            catalog_name: name.clone(),
                                            distance_ly: dist_ly,
                                            color: [1.0, 1.0, 1.0],
                                            spectral_class: sa_universe::SpectralClass::G,
                                            luminosity: 1.0,
                                        };
                                        self.navigation.lock_star(nav);
                                        log::info!("LOCKED PLANET: {} ({:.6} ly, {:.0}px from center)",
                                            name, dist_ly, px_dist);
                                        self.audio.play_sfx(sa_audio::SfxId::Confirm, None);
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                // Fallback: lock nearest star if no planet was locked
                                if !locked {
                                    self.navigation.update_nearby(self.galactic_position);
                                    let mut best_screen_dist = f32::MAX;
                                    let mut best_idx: Option<usize> = None;
                                    for (i, star) in self.navigation.nearby_stars.iter().enumerate() {
                                        let dx = (star.galactic_pos.x - self.galactic_position.x) as f32;
                                        let dy = (star.galactic_pos.y - self.galactic_position.y) as f32;
                                        let dz = (star.galactic_pos.z - self.galactic_position.z) as f32;
                                        let len = (dx*dx + dy*dy + dz*dz).sqrt();
                                        if len < 0.001 { continue; }
                                        let dir_norm = glam::Vec3::new(dx/len, dy/len, dz/len);
                                        if cam_fwd.dot(dir_norm) < 0.866 { continue; }
                                        let dir = dir_norm * 90000.0;
                                        let clip = vp * glam::Vec4::new(dir.x, dir.y, dir.z, 1.0);
                                        if clip.w <= 0.0 { continue; }
                                        let sx = (clip.x / clip.w * 0.5 + 0.5) * sw;
                                        let sy = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * sh;
                                        let screen_dist = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                                        if screen_dist < best_screen_dist {
                                            best_screen_dist = screen_dist;
                                            best_idx = Some(i);
                                        }
                                    }
                                    if let Some(idx) = best_idx {
                                        self.navigation.lock_target(idx);
                                        if let Some(target) = &self.navigation.locked_target {
                                            log::info!("LOCKED STAR: {} ({:.2} ly, {:.0}px from center)",
                                                target.catalog_name, target.distance_ly, best_screen_dist);
                                            self.audio.play_sfx(sa_audio::SfxId::Confirm, None);
                                        }
                                    } else {
                                        log::warn!("No target visible on screen");
                                    }
                                }
                        }
                        if self.input.keyboard.just_pressed(KeyCode::Digit3) {
                            // Block warp near planets (always — warp is for star-to-star).
                            let in_atmosphere = self.terrain_gravity
                                .as_ref()
                                .is_some_and(|g| g.blend > 0.5);
                            if self.terrain.is_some() || in_atmosphere {
                                log::warn!("Cannot engage warp: too close to planet");
                            } else if self.ship_resources.exotic_fuel > 0.0 {
                                let ship_speed = self.ship.as_ref()
                                    .map(|s| s.speed(&self.physics))
                                    .unwrap_or(0.0);
                                if self.drive.request_engage_with_speed(sa_ship::DriveMode::Warp, ship_speed) {
                                    log::info!("Drive: WARP spooling...");
                                    self.audio.announce(sa_audio::VoiceId::EngagingWarp);
                                    // Unload active system when entering warp
                                    if self.active_system.is_some() {
                                        if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                                        self.terrain = None;
                                        self.terrain_gravity = None;
                                        self.active_system = None;
                                        log::info!("Left solar system — entering warp");
                                    }
                                } else if ship_speed > 10.0 {
                                    log::warn!("Cannot engage warp: ship moving at {:.0} m/s (stop first)", ship_speed);
                                }
                            } else {
                                log::warn!("Cannot engage warp: no exotic fuel");
                            }
                        }

                        // Update drive spool progress
                        self.drive.update(dt);

                        // Map throttle lever to drive speed when in cruise/warp
                        if self.drive.mode() != sa_ship::DriveMode::Impulse
                            && let Some(ship) = &self.ship
                        {
                            self.drive.set_speed_fraction(ship.throttle);
                        }

                        // Apply continuous thrust from throttle lever + engine button
                        ship.apply_thrust(&mut self.physics);

                        // Vertical RCS: Space = thrust up, Shift = thrust down (ship-local)
                        let vert_up = self.input.keyboard.is_pressed(KeyCode::Space);
                        let vert_down = self.input.keyboard.is_pressed(KeyCode::ShiftLeft);
                        let vertical: f32 = if vert_up { 1.0 } else if vert_down { -1.0 } else { 0.0 };
                        if vertical.abs() > 0.01 {
                            ship.apply_rcs(&mut self.physics, 0.0, vertical, 0.0);
                        }

                        if wants_stand
                            && let Some(helm) = &mut self.helm
                        {
                            self.drive.request_disengage();
                            // Set player yaw to ship's WORLD heading so they face the
                            // ship's nose direction. Player.yaw is WORLD space.
                            if let Some(body) = self.physics.get_body(ship.body_handle) {
                                let rot = body.rotation();
                                let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                                if let Some(player) = &mut self.player {
                                    player.yaw = fwd.x.atan2(-fwd.z);
                                    player.pitch = 0.0;
                                }
                            }
                            helm.stand_up();
                            // Re-enable player and teleport to ship's current position
                            // (ship may have moved while seated)
                            if let Some(player) = &self.player {
                                // Get ship state before mutable borrow
                                let ship_pos = ship.position(&self.physics);
                                let ship_vel = ship.speed_vector(&self.physics);
                                // Compute rotated stand-up offset before mutable borrow
                                let ship_rot = self.physics.get_body(ship.body_handle)
                                    .map(|b| *b.rotation())
                                    .unwrap_or(nalgebra::UnitQuaternion::identity());
                                let local_offset = nalgebra::Vector3::new(0.0, -0.1, 2.8);
                                let world_offset = ship_rot * local_offset;

                                if let (Some((sx, sy, sz)), Some(body)) = (
                                    ship_pos,
                                    self.physics.get_body_mut(player.body_handle),
                                ) {
                                    body.set_enabled(true);
                                    body.set_translation(
                                        nalgebra::Vector3::new(sx + world_offset.x, sy + world_offset.y, sz + world_offset.z),
                                        true,
                                    );
                                    // Match ship velocity so player doesn't slide on stand-up
                                    body.set_linvel(nalgebra::Vector3::new(ship_vel.0, ship_vel.1, ship_vel.2), true);
                                }
                            }
                            log::info!("Left helm seated mode — player teleported to ship");
                        }
                    }

                    // Apply terrain gravity + atmospheric drag to ship body before step.
                    // Skip when LANDED — ship is frozen on the surface.
                    if self.landing.state() != landing::LandingState::Landed
                        && let Some(ref grav) = self.terrain_gravity
                        && grav.blend > 0.01
                        && let Some(ship) = &self.ship
                        && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                    {
                        let helm_dt = dt.min(1.0 / 30.0);
                        let grav_accel = nalgebra::Vector3::new(
                            grav.direction[0] * grav.magnitude,
                            grav.direction[1] * grav.magnitude,
                            grav.direction[2] * grav.magnitude,
                        );
                        let vel = body.linvel() + grav_accel * helm_dt;
                        // Atmospheric drag: terminal velocity ~200 m/s at surface
                        let atmo_drag = 0.05 * grav.blend;
                        let vel = vel * (1.0 - atmo_drag * helm_dt).max(0.0);
                        body.set_linvel(vel, true);
                    }

                    // Record ship position before physics step (for galactic tracking).
                    let ship_pos_pre_helm = self.ship.as_ref()
                        .and_then(|s| self.physics.get_body(s.body_handle))
                        .map(|b| b.translation().clone_owned());

                    // Physics step
                    let physics_dt = dt.min(1.0 / 30.0);
                    if physics_dt > 0.0 {
                        self.physics.step(physics_dt);
                    }

                    // Track ship physics into galactic_position (impulse only).
                    // When terrain is active, compute exactly from anchor + ship rapier pos.
                    // When terrain is NOT active, use delta-based tracking.
                    if self.drive.mode() == sa_ship::DriveMode::Impulse {
                        if let Some(terrain_mgr) = &self.terrain
                            && let Some(ship) = &self.ship
                            && let Some(body) = self.physics.get_body(ship.body_handle)
                        {
                            // Exact galactic position: frozen_planet_center + (anchor + ship_rapier) / LY_TO_M
                            let anchor = terrain_mgr.anchor_f64();
                            let post = body.translation();
                            let planet = terrain_mgr.frozen_planet_center_ly();
                            let ly_to_m = 9.461e15_f64;
                            self.galactic_position.x = planet.x + (anchor[0] + post.x as f64) / ly_to_m;
                            self.galactic_position.y = planet.y + (anchor[1] + post.y as f64) / ly_to_m;
                            self.galactic_position.z = planet.z + (anchor[2] + post.z as f64) / ly_to_m;
                        } else if let Some(pre) = ship_pos_pre_helm
                            && let Some(ship) = &self.ship
                            && let Some(body) = self.physics.get_body(ship.body_handle)
                        {
                            let post = body.translation();
                            let m_to_ly = 1.0 / 9.461e15_f64;
                            self.galactic_position.x += (post.x as f64 - pre.x as f64) * m_to_ly;
                            self.galactic_position.y += (post.y as f64 - pre.y as f64) * m_to_ly;
                            self.galactic_position.z += (post.z as f64 - pre.z as f64) * m_to_ly;
                        }
                    }

                    // Auto-orient ship toward locked target in cruise/warp.
                    // Smoothly slerps the ship body rotation so its forward (-Z)
                    // aligns with the direction to the target.
                    if self.drive.mode() != sa_ship::DriveMode::Impulse
                        && let Some(target) = &self.navigation.locked_target
                        && let Some(ship) = &self.ship
                        && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                    {
                        // Compute direction in f64 to handle both star targets (ly scale)
                        // and planet targets (AU scale = ~1e-5 ly). The old f32 threshold
                        // of 0.001 ly filtered out all in-system planets.
                        let dx_f64 = target.galactic_pos.x - self.galactic_position.x;
                        let dy_f64 = target.galactic_pos.y - self.galactic_position.y;
                        let dz_f64 = target.galactic_pos.z - self.galactic_position.z;
                        let len_f64 = (dx_f64 * dx_f64 + dy_f64 * dy_f64 + dz_f64 * dz_f64).sqrt();
                        if len_f64 > 1e-15 { // ~9.5 meters in ly — practically any nonzero distance
                            let to_target = glam::Vec3::new(
                                (dx_f64 / len_f64) as f32,
                                (dy_f64 / len_f64) as f32,
                                (dz_f64 / len_f64) as f32,
                            );
                            // Ship forward is -Z, so desired rotation takes -Z to to_target
                            let desired = glam::Quat::from_rotation_arc(
                                glam::Vec3::new(0.0, 0.0, -1.0),
                                to_target,
                            );
                            let current_rot = body.rotation();
                            let current = glam::Quat::from_xyzw(
                                current_rot.i, current_rot.j, current_rot.k, current_rot.w,
                            );
                            // Fast slerp: 95% of the way per second (exponential smoothing)
                            let t = (1.0 - (-3.0 * dt).exp()).clamp(0.0, 1.0);
                            let new_rot = current.slerp(desired, t).normalize();
                            let q = nalgebra::UnitQuaternion::from_quaternion(
                                nalgebra::Quaternion::new(new_rot.w, new_rot.x, new_rot.y, new_rot.z),
                            );
                            body.set_rotation(q, true);
                            // Zero angular velocity so manual steering doesn't fight
                            body.set_angvel(nalgebra::Vector3::zeros(), true);
                        }
                    }

                    // Update galactic position based on drive speed (with deceleration)
                    if self.drive.mode() != sa_ship::DriveMode::Impulse {
                        let direction = if let Some(ship) = &self.ship {
                            if let Some(body) = self.physics.get_body(ship.body_handle) {
                                let rot = body.rotation();
                                let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                                [fwd.x as f64, fwd.y as f64, fwd.z as f64]
                            } else {
                                [0.0, 0.0, -1.0]
                            }
                        } else {
                            [0.0, 0.0, -1.0]
                        };

                        // Save position before warp movement
                        let pos_before = self.galactic_position;

                        // Compute target distance for deceleration
                        let target_dist = self.navigation.locked_target.as_ref()
                            .map(|t| self.galactic_position.distance_to(t.galactic_pos));

                        // Apply warp movement with deceleration
                        let (delta, effective_speed) = drive_integration::galactic_position_delta_decel(
                            &self.drive,
                            direction,
                            dt as f64,
                            target_dist,
                        );
                        self.galactic_position.x += delta[0];
                        self.galactic_position.y += delta[1];
                        self.galactic_position.z += delta[2];

                        // When terrain is active and in cruise/warp, sync the
                        // ship's rapier position to match galactic_position.
                        // This keeps the rebase system consistent and ensures
                        // the sphere barrier is at the correct relative position.
                        if let Some(terrain_mgr) = &self.terrain
                            && let Some(ship) = &self.ship
                            && let Some(body) = self.physics.rigid_body_set.get_mut(ship.body_handle)
                        {
                            let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                            let anchor = terrain_mgr.anchor_f64();
                            let new_rapier = [
                                (cam_rel[0] - anchor[0]) as f32,
                                (cam_rel[1] - anchor[1]) as f32,
                                (cam_rel[2] - anchor[2]) as f32,
                            ];
                            body.set_translation(
                                nalgebra::Vector3::new(new_rapier[0], new_rapier[1], new_rapier[2]),
                                true,
                            );
                        }

                        let pos_after = self.galactic_position;

                        // Auto-disengage cruise at atmosphere boundary when
                        // terrain is active. Cruise doesn't move the rapier
                        // body so the sphere barrier can't stop the ship.
                        if self.drive.mode() == sa_ship::DriveMode::Cruise
                            && let Some(terrain_mgr) = &self.terrain
                        {
                            let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                            let dist_m = (cam_rel[0] * cam_rel[0]
                                + cam_rel[1] * cam_rel[1]
                                + cam_rel[2] * cam_rel[2]).sqrt();
                            // Disengage at 1.05× radius (~385km for Earth-sized planet).
                            // Close enough that the surface is visible and impulse
                            // feels responsive, but high enough to decelerate safely.
                            let atmo_boundary = terrain_mgr.planet_radius_m() * 1.05;
                            if dist_m < atmo_boundary {
                                // Clamp position to atmosphere boundary.
                                let safe_dist = atmo_boundary / dist_m;
                                let planet_ly = terrain_mgr.frozen_planet_center_ly();
                                let ly_to_m = 9.461e15_f64;
                                self.galactic_position.x = planet_ly.x + cam_rel[0] * safe_dist / ly_to_m;
                                self.galactic_position.y = planet_ly.y + cam_rel[1] * safe_dist / ly_to_m;
                                self.galactic_position.z = planet_ly.z + cam_rel[2] * safe_dist / ly_to_m;
                                self.drive.request_disengage();
                                log::info!("Cruise auto-disengage: entered atmosphere at {:.0}km altitude",
                                    (dist_m - terrain_mgr.planet_radius_m()) / 1000.0);
                            }
                        }

                        // Approach voice + cascade auto-disengage
                        if let Some(dist) = target_dist {
                            let prev = self.prev_target_dist;

                            // Voice: "Approaching destination" when deceleration begins (~1 ly)
                            if dist < 1.0 && prev.is_none_or(|d| d >= 1.0)
                                && self.drive.mode() == sa_ship::DriveMode::Warp
                            {
                                self.audio.announce(sa_audio::VoiceId::ApproachingDestination);
                                log::info!("Approaching destination — warp deceleration engaged");
                            }

                            // Warp → Impulse: auto-disengage at 0.01 ly
                            // Player can manually engage cruise to continue approach.
                            if self.drive.mode() == sa_ship::DriveMode::Warp
                                && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                                && dist < drive_integration::WARP_DISENGAGE_LY
                            {
                                self.drive.request_disengage();
                                self.audio.announce(sa_audio::VoiceId::AllSystemsReady);
                                log::info!("Warp disengaged at {:.4} ly ({:.0} AU) from target",
                                    dist, dist / 1.581e-5);
                            }

                            // Cruise → Impulse: auto-disengage at ~50 AU
                            if self.drive.mode() == sa_ship::DriveMode::Cruise
                                && dist < drive_integration::CRUISE_DISENGAGE_LY
                            {
                                self.drive.request_disengage();
                                log::info!("Cruise disengaged at {:.6} ly ({:.0} AU) from target",
                                    dist, dist / 1.581e-5);
                            }

                            self.prev_target_dist = Some(dist);
                        }

                        // Free warp proximity warning (3 seconds lookahead)
                        if self.drive.mode() == sa_ship::DriveMode::Warp
                            && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                            && self.navigation.locked_target.is_none()
                            && !self.proximity_warned
                        {
                            let lookahead = effective_speed * 3.0;
                            if self.navigation.check_proximity_warning(
                                self.galactic_position, direction, lookahead,
                            ).is_some() {
                                self.audio.announce(sa_audio::VoiceId::Alert);
                                self.proximity_warned = true;
                                log::info!("PROXIMITY ALERT — star ahead");
                            }
                        }

                        // Predictive gravity well auto-drop (ray-segment, catches flythroughs)
                        if self.drive.mode() == sa_ship::DriveMode::Warp
                            && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                            && self.active_system.is_none()
                            && let Some((nav_star, drop_pos)) =
                                self.navigation.check_gravity_well_predictive(pos_before, pos_after)
                        {
                                // Place ship at well boundary, not inside
                                self.galactic_position = drop_pos;
                                self.drive.request_disengage();
                                self.proximity_warned = false;
                                self.prev_target_dist = None;
                                log::info!("GRAVITY WELL — entering system: {}", nav_star.catalog_name);

                                // Load the solar system
                                let nav_id = nav_star.id;
                                let nav_name = nav_star.catalog_name.clone();
                                if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                    let sector_coord = sa_universe::SectorCoord::new(
                                        nav_id.sector_x().into(),
                                        nav_id.sector_y().into(),
                                        nav_id.sector_z().into(),
                                    );
                                    let sector = sa_universe::sector::generate_sector(
                                        MasterSeed(42),
                                        sector_coord,
                                    );
                                    if let Some(placed) = sector.stars.iter().find(|s| s.id == nav_id) {
                                        let system = solar_system::ActiveSystem::load(
                                            nav_id,
                                            placed,
                                            &mut renderer.mesh_store,
                                            &gpu.device,
                                        );
                                        log::info!("System loaded: {} bodies ({})", system.body_count(), nav_name);
                                        self.audio.announce(sa_audio::VoiceId::AllSystemsReady);
                                        // Clean up terrain before switching system
                                        if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                                        self.terrain = None;
                                        self.terrain_gravity = None;
                                        self.active_system = Some(system);
                                    }
                                }

                                // Keep the navigation lock — marker stays visible
                                // after arrival. Gravity well won't re-trigger
                                // because it's guarded by active_system.is_none().
                        }
                    } else {
                        // Not in cruise/warp — reset proximity warning and approach tracking
                        self.proximity_warned = false;
                        self.prev_target_dist = None;
                    }

                    // Sync interior collider rotation to match ship (same as walk mode).
                    // Needed so colliders are correct when the player stands up.
                    if let (Some(ship_ref), Some(ih)) = (&self.ship, crate::ship_colliders::interior_body_handle()) {
                        let ship_rot = self.physics.get_body(ship_ref.body_handle)
                            .map(|b| *b.rotation());
                        if let (Some(rot), Some(ib)) = (ship_rot, self.physics.get_body_mut(ih)) {
                            ib.set_position(
                                nalgebra::Isometry3::from_parts(
                                    nalgebra::Translation3::identity(),
                                    rot,
                                ),
                                true,
                            );
                        }
                    }

                    // --- Landing system update (helm mode) ---
                    if let Some(ship) = &self.ship {
                        let ship_iso = self.physics.get_body(ship.body_handle)
                            .map(|b| *b.position())
                            .unwrap_or_else(nalgebra::Isometry3::identity);
                        let ship_speed_ms = ship.speed(&self.physics);
                        let ship_vel_raw = ship.speed_vector(&self.physics);
                        let ship_velocity = nalgebra::Vector3::new(
                            ship_vel_raw.0, ship_vel_raw.1, ship_vel_raw.2,
                        );
                        let gravity_raw = self.terrain_gravity.as_ref()
                            .map(|g| g.direction)
                            .unwrap_or([0.0, -1.0, 0.0]);
                        let gravity_dir = nalgebra::Unit::new_normalize(
                            nalgebra::Vector3::new(gravity_raw[0], gravity_raw[1], gravity_raw[2]),
                        );
                        let planet_gravity = self.terrain_gravity.as_ref()
                            .map(|g| g.magnitude)
                            .unwrap_or(0.0);
                        let terrain_active = self.terrain.is_some();
                        let landing_result = self.landing.update(landing::LandingParams {
                            physics: &self.physics,
                            ship_iso: &ship_iso,
                            ship_speed_ms,
                            ship_velocity,
                            gravity_dir,
                            planet_gravity,
                            terrain_active,
                            engine_on: ship.engine_on,
                            throttle: ship.throttle,
                        });
                        self.last_clearance = landing_result.min_clearance;

                        // Diagnostic logging when terrain is active (throttled).
                        if terrain_active && self.time.frame_count().is_multiple_of(60) {
                            log::info!(
                                "LANDING_DIAG: state={:?}, speed={:.1}m/s, vertical={:.1}m/s, clearance={:.1}m, skid_contact={}",
                                landing_result.state,
                                ship_speed_ms,
                                ship_velocity.dot(&gravity_dir).abs(),
                                landing_result.min_clearance.unwrap_or(100.0),
                                landing_result.skid_contact,
                            );
                        }

                        // Freeze ship when LANDED; restore on unlock.
                        if landing_result.state == landing::LandingState::Landed
                            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                        {
                            body.set_linvel(nalgebra::Vector3::zeros(), true);
                            body.set_angvel(nalgebra::Vector3::zeros(), true);
                            body.set_gravity_scale(0.0, true);
                        }
                        // Restore gravity_scale when transitioning OUT of Landed.
                        if landing_result.previous_state == landing::LandingState::Landed
                            && landing_result.state != landing::LandingState::Landed
                            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                        {
                            // Ship gravity_scale is always 0 (gravity applied manually).
                            body.set_gravity_scale(0.0, true);
                            body.set_linvel(nalgebra::Vector3::zeros(), true);
                        }
                        if let Some(ref impact) = landing_result.impact {
                            log::info!(
                                "IMPACT: {:?} at {:.1} m/s",
                                impact.category,
                                impact.impact_speed_ms
                            );
                            match impact.category {
                                landing::ImpactCategory::Clean => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactSoft, None);
                                }
                                landing::ImpactCategory::Minor => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactHeavy, None);
                                }
                                landing::ImpactCategory::Major => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactCrash, None);
                                    self.audio.play_alarm(sa_audio::AlarmId::StructuralDamage);
                                }
                                landing::ImpactCategory::Destroyed => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactExplosion, None);
                                }
                            }
                            self.events.emit(impact.clone());
                        }
                    }

                    // Camera: ship quaternion + helm mouse look offset.
                    // ONLY runs while still seated — must not run on the stand-up frame
                    // or orientation_override gets re-set after being cleared.
                    if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
                        let (dx, dy) = self.input.mouse.delta();
                        self.helm_look_yaw += dx * 0.003;
                        self.helm_look_pitch -= dy * 0.003;
                        let max_p = std::f32::consts::FRAC_PI_2 - 0.01;
                        self.helm_look_pitch = self.helm_look_pitch.clamp(-max_p, max_p);

                        if let Some(ship) = &self.ship
                            && let Some(body) = self.physics.get_body(ship.body_handle)
                        {
                            let r = body.rotation();
                            let ship_quat = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);

                            let look_offset = glam::Quat::from_rotation_y(-self.helm_look_yaw)
                                * glam::Quat::from_rotation_x(self.helm_look_pitch);

                            self.camera.orientation_override = Some(ship_quat * look_offset);
                        }
                    }

                    // Camera position fixed at helm viewpoint (moves with ship)
                    if let (Some(helm), Some(ship)) = (&self.helm, &self.ship)
                        && let Some((cx, cy, cz)) = helm.camera_position(&self.physics, ship)
                    {
                        self.camera.position = WorldPos::new(cx as f64, cy as f64, cz as f64);
                    }
                } else {
                    // --- Walk mode: kinematic character controller ---
                    //
                    // CORRECT ORDER (prevents drift and teleporting):
                    // 1. Apply forces to ship
                    // 2. Run physics step (ship moves to final position)
                    // 3. Update query pipeline (colliders at final position)
                    // 4. Read ship's POST-STEP velocity
                    // 5. Move player using post-step velocity + move_shape
                    //
                    // This ensures the player moves relative to where the ship
                    // ACTUALLY IS after the step, not where it WAS before.

                    // MANUAL ship integration — no physics.step() needed.
                    // physics.step() is extremely expensive with many colliders
                    // (47ms at 345 m/s). Since the player is kinematic (uses
                    // move_shape, not contacts) and the ship is the only dynamic
                    // body, we integrate the ship manually: v += a*dt, p += v*dt.
                    let physics_dt = dt.min(1.0 / 30.0);
                    let t_phys = Instant::now();

                    // Record ship transform BEFORE integration (needed to carry player)
                    let (ship_pos_before, ship_rot_before) = self.ship.as_ref()
                        .and_then(|s| self.physics.get_body(s.body_handle))
                        .map(|b| (b.translation().clone_owned(), *b.rotation()))
                        .unwrap_or((nalgebra::Vector3::zeros(), nalgebra::UnitQuaternion::identity()));

                    // Manually integrate ship: apply thrust as velocity change.
                    // Skip all integration when landed — ship is frozen on the surface.
                    if self.landing.state() == landing::LandingState::Landed {
                        // Ship is locked on the ground — zero velocity and hold position.
                        if let Some(ship) = &self.ship
                            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                        {
                            body.set_linvel(nalgebra::Vector3::zeros(), true);
                            body.set_angvel(nalgebra::Vector3::zeros(), true);
                            body.set_gravity_scale(0.0, true);
                        }
                    } else if let Some(ship) = &self.ship
                        && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                    {
                            // Apply thrust: F = throttle * max_thrust * engine_on
                            if ship.engine_on && ship.throttle > 0.0 {
                                let rot = *body.rotation();
                                let forward = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                                let accel = forward * (ship.throttle * ship.max_thrust / body.mass());
                                let vel = body.linvel() + accel * physics_dt;
                                body.set_linvel(vel, true);
                            }
                            // Linear damping always applies (not just when engine on).
                            // Without this, gravity accumulates velocity without bound.
                            {
                                let vel = body.linvel() * (1.0 - 0.001 * physics_dt).max(0.0);
                                body.set_linvel(vel, true);
                            }
                            // Apply terrain gravity (planet surface pull).
                            if let Some(ref grav) = self.terrain_gravity
                                && grav.blend > 0.01
                            {
                                let grav_accel = nalgebra::Vector3::new(
                                    grav.direction[0] * grav.magnitude,
                                    grav.direction[1] * grav.magnitude,
                                    grav.direction[2] * grav.magnitude,
                                );
                                let vel = body.linvel() + grav_accel * physics_dt;
                                body.set_linvel(vel, true);

                                // Atmospheric drag: increases with gravity blend.
                                // At full blend (surface), drag = 0.05/s → terminal velocity
                                // ~200 m/s under Earth gravity (9.81/0.05 = 196 m/s).
                                // At atmosphere edge (blend ~0), negligible drag.
                                let atmo_drag = 0.05 * grav.blend;
                                let vel = body.linvel() * (1.0 - atmo_drag * physics_dt).max(0.0);
                                body.set_linvel(vel, true);
                            }
                            // Ground contact response (walk-mode substitute for
                            // rapier contact solving, since physics.step() is skipped).
                            // When SLIDING, cancel downward velocity and apply friction.
                            if self.landing.state() == landing::LandingState::Sliding
                                && let Some(ref grav) = self.terrain_gravity
                            {
                                let gdir = nalgebra::Vector3::new(
                                    grav.direction[0], grav.direction[1], grav.direction[2],
                                );
                                let vel = *body.linvel();
                                let down_speed = vel.dot(&gdir);
                                if down_speed > 0.0 {
                                    // Cancel velocity into ground (normal reaction).
                                    let corrected = vel - gdir * down_speed;
                                    body.set_linvel(corrected, true);
                                }
                                // Ground friction: combined friction 0.6 * 0.8 = 0.48.
                                // Deceleration = friction * gravity.
                                let friction_decel = 0.48 * grav.magnitude;
                                let tangent_vel = *body.linvel();
                                let tangent_speed = tangent_vel.magnitude();
                                if tangent_speed > 0.01 {
                                    let decel = (friction_decel * physics_dt).min(tangent_speed);
                                    let braked = tangent_vel * (1.0 - decel / tangent_speed);
                                    body.set_linvel(braked, true);
                                }
                            }
                            // Angular damping ALWAYS applies (not just when engine on).
                            // Without this, Q/E roll torques create permanent spin.
                            let angvel = body.angvel() * (1.0 - 5.0 * physics_dt).max(0.0);
                            body.set_angvel(angvel, true);
                            // Integrate position: p += v * dt
                            let vel = *body.linvel();
                            let pos = body.translation() + vel * physics_dt;
                            let angvel = *body.angvel();
                            let rot = *body.rotation();
                            let drot = nalgebra::UnitQuaternion::new(angvel * physics_dt * 0.5);
                            let new_rot = drot * rot;
                            body.set_position(
                                nalgebra::Isometry3::from_parts(
                                    nalgebra::Translation3::from(pos),
                                    new_rot,
                                ),
                                true,
                            );
                    }
                    let phys_step_us = t_phys.elapsed().as_micros();

                    // Step 3: Get ship transform AFTER integration
                    let (ship_pos_after, ship_rot_after) = self.ship.as_ref()
                        .and_then(|s| self.physics.get_body(s.body_handle))
                        .map(|b| (b.translation().clone_owned(), *b.rotation()))
                        .unwrap_or((nalgebra::Vector3::zeros(), nalgebra::UnitQuaternion::identity()));

                    // Track ship physics into galactic_position (impulse only).
                    // When terrain is active, compute exactly from anchor + ship rapier pos.
                    // When terrain is NOT active, use delta-based tracking.
                    if self.drive.mode() == sa_ship::DriveMode::Impulse
                        && self.ship.is_some()
                    {
                        if let Some(terrain_mgr) = &self.terrain {
                            let anchor = terrain_mgr.anchor_f64();
                            let planet = terrain_mgr.frozen_planet_center_ly();
                            let ly_to_m = 9.461e15_f64;
                            self.galactic_position.x = planet.x + (anchor[0] + ship_pos_after.x as f64) / ly_to_m;
                            self.galactic_position.y = planet.y + (anchor[1] + ship_pos_after.y as f64) / ly_to_m;
                            self.galactic_position.z = planet.z + (anchor[2] + ship_pos_after.z as f64) / ly_to_m;
                        } else {
                            let m_to_ly = 1.0 / 9.461e15_f64;
                            self.galactic_position.x += (ship_pos_after.x as f64 - ship_pos_before.x as f64) * m_to_ly;
                            self.galactic_position.y += (ship_pos_after.y as f64 - ship_pos_before.y as f64) * m_to_ly;
                            self.galactic_position.z += (ship_pos_after.z as f64 - ship_pos_before.z as f64) * m_to_ly;
                        }
                    }

                    // --- Landing system update (walk mode) ---
                    if let Some(ship) = &self.ship {
                        let ship_iso = self.physics.get_body(ship.body_handle)
                            .map(|b| *b.position())
                            .unwrap_or_else(nalgebra::Isometry3::identity);
                        let ship_speed_ms = ship.speed(&self.physics);
                        let ship_vel_raw = ship.speed_vector(&self.physics);
                        let ship_velocity = nalgebra::Vector3::new(
                            ship_vel_raw.0, ship_vel_raw.1, ship_vel_raw.2,
                        );
                        let gravity_raw = self.terrain_gravity.as_ref()
                            .map(|g| g.direction)
                            .unwrap_or([0.0, -1.0, 0.0]);
                        let gravity_dir = nalgebra::Unit::new_normalize(
                            nalgebra::Vector3::new(gravity_raw[0], gravity_raw[1], gravity_raw[2]),
                        );
                        let planet_gravity = self.terrain_gravity.as_ref()
                            .map(|g| g.magnitude)
                            .unwrap_or(0.0);
                        let terrain_active = self.terrain.is_some();
                        let landing_result = self.landing.update(landing::LandingParams {
                            physics: &self.physics,
                            ship_iso: &ship_iso,
                            ship_speed_ms,
                            ship_velocity,
                            gravity_dir,
                            planet_gravity,
                            terrain_active,
                            engine_on: ship.engine_on,
                            throttle: ship.throttle,
                        });
                        self.last_clearance = landing_result.min_clearance;

                        // Diagnostic logging when terrain is active (throttled).
                        if terrain_active && self.time.frame_count().is_multiple_of(60) {
                            log::info!(
                                "LANDING_DIAG: state={:?}, speed={:.1}m/s, vertical={:.1}m/s, clearance={:.1}m, skid_contact={}",
                                landing_result.state,
                                ship_speed_ms,
                                ship_velocity.dot(&gravity_dir).abs(),
                                landing_result.min_clearance.unwrap_or(100.0),
                                landing_result.skid_contact,
                            );
                        }

                        // Freeze ship when LANDED; restore on unlock.
                        if landing_result.state == landing::LandingState::Landed
                            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                        {
                            body.set_linvel(nalgebra::Vector3::zeros(), true);
                            body.set_angvel(nalgebra::Vector3::zeros(), true);
                            body.set_gravity_scale(0.0, true);
                        }
                        // Restore gravity_scale when transitioning OUT of Landed.
                        if landing_result.previous_state == landing::LandingState::Landed
                            && landing_result.state != landing::LandingState::Landed
                            && let Some(body) = self.physics.get_body_mut(ship.body_handle)
                        {
                            body.set_gravity_scale(0.0, true);
                            body.set_linvel(nalgebra::Vector3::zeros(), true);
                        }
                        if let Some(ref impact) = landing_result.impact {
                            log::info!(
                                "IMPACT: {:?} at {:.1} m/s",
                                impact.category,
                                impact.impact_speed_ms
                            );
                            match impact.category {
                                landing::ImpactCategory::Clean => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactSoft, None);
                                }
                                landing::ImpactCategory::Minor => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactHeavy, None);
                                }
                                landing::ImpactCategory::Major => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactCrash, None);
                                    self.audio.play_alarm(sa_audio::AlarmId::StructuralDamage);
                                }
                                landing::ImpactCategory::Destroyed => {
                                    self.audio.play_sfx(sa_audio::SfxId::ImpactExplosion, None);
                                }
                            }
                            self.events.emit(impact.clone());
                        }
                    }

                    // Step 4: Carry player with ship.
                    // The player's world position is relative to the OLD ship position.
                    // Transform to local space (using old ship), then back to world (using new ship).
                    // This is an instant teleport — no collision sweep, no AABB cost.
                    // For a non-rotating ship: equivalent to adding ship displacement.
                    // For a rotating ship: also rotates the player around the ship origin.
                    let ship_rot_before_inv = ship_rot_before.inverse();
                    if let Some(player) = &self.player
                        && let Some(body) = self.physics.get_body_mut(player.body_handle)
                    {
                        let p = body.translation().clone_owned();
                        let local = ship_rot_before_inv * (p - ship_pos_before);
                        let carried = ship_pos_after + ship_rot_after * local;
                        body.set_translation(carried, true);
                    }

                    // Step 5: Sync interior collider ROTATION to match ship.
                    // Interior colliders are on a fixed body at world origin.
                    // We sync rotation (not position) so collision geometry matches
                    // the ship's visual orientation after roll/pitch/yaw.
                    // Position stays at origin — high-speed translation is handled by
                    // the player controller's origin-offset transform.
                    if let (Some(ship), Some(ih)) = (&self.ship, crate::ship_colliders::interior_body_handle()) {
                        // Read ship rotation first (immutable borrow)
                        let ship_rot = self.physics.get_body(ship.body_handle)
                            .map(|b| *b.rotation());
                        // Then set interior rotation (mutable borrow)
                        if let (Some(rot), Some(ib)) = (ship_rot, self.physics.get_body_mut(ih)) {
                            ib.set_position(
                                nalgebra::Isometry3::from_parts(
                                    nalgebra::Translation3::identity(),
                                    rot,
                                ),
                                true,
                            );
                        }
                    }

                    // Sync child collider world positions + update query pipeline.
                    self.physics.sync_collider_positions();
                    let t_qp = Instant::now();
                    self.physics.update_query_pipeline();
                    let qp_us = t_qp.elapsed().as_micros();

                    log::trace!("phys_step: {}us, query_pipeline: {}us", phys_step_us, qp_us);

                    // Step 6: Move player in ship-local space.
                    // All collision detection happens in the ship's local coordinate
                    // system where colliders are stationary. Performance is O(1)
                    // regardless of ship speed.
                    let t_player = Instant::now();
                    if let Some(player) = &mut self.player {
                        player.update(
                            &mut self.physics,
                            &self.input,
                            physics_dt,
                            ship_pos_after,
                            ship_rot_after,
                        );
                    }
                    let player_move_us = t_player.elapsed().as_micros();
                    // Encode breakdown: physics_us = phys_step*1000000 + qp*1000 + move_shape
                    // Read as: phys_step=X.XXms, qp=X.XXms, move=X.XXms
                    self.perf.physics_us = phys_step_us as u64 * 1000 + qp_us as u64;
                    self.perf.stars_us = player_move_us as u64;

                    if let Some(player) = &self.player {
                        // Eye position: offset along ship's up (correct after roll/pitch)
                        self.camera.position = player.position_ship_up(&self.physics, ship_rot_after);
                        // Camera: ship quaternion * (player look offset from ship heading).
                        // Player.yaw is WORLD space. Subtract ship's world yaw to get
                        // the ship-relative look offset. This way the camera follows
                        // the ship's roll while showing the correct look direction.
                        if let Some(ship) = &self.ship {
                            if let Some(body) = self.physics.get_body(ship.body_handle) {
                                let r = body.rotation();
                                let ship_quat = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                                let fwd = r * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                                let ship_yaw = fwd.x.atan2(-fwd.z);
                                // Offset from ship heading
                                let yaw_offset = player.yaw - ship_yaw;
                                let look = glam::Quat::from_rotation_y(-yaw_offset)
                                    * glam::Quat::from_rotation_x(player.pitch);
                                self.camera.orientation_override = Some(ship_quat * look);
                            }
                        } else {
                            self.camera.yaw = player.yaw;
                            self.camera.pitch = player.pitch;
                            self.camera.orientation_override = None;
                        }
                    }
                }

                // --- Deferred star lock-on (Tab) ---
                // Runs AFTER camera orientation is fully set for this frame.
                // Searches ALL rendered stars (not just 15 nearest), so the
                // player can lock any visible star on screen.
                if self.wants_star_lock {
                    self.wants_star_lock = false;
                    if let Some(gpu) = &self.gpu {
                        let sw = gpu.config.width as f32;
                        let sh = gpu.config.height as f32;
                        let cx = sw / 2.0;
                        let cy = sh / 2.0;
                        let aspect = sw / sh;
                        let vp = self.camera.view_projection_matrix(aspect);

                        let visible = self.star_streaming.visible_stars(self.galactic_position);
                        let cam_fwd = self.camera.forward();
                        let mut best_screen_dist = f32::MAX;
                        let mut best: Option<(usize, f32)> = None; // (index into visible, px)

                        for (i, (placed, _dist_ly)) in visible.iter().enumerate() {
                            let dx = (placed.position.x - self.galactic_position.x) as f32;
                            let dy = (placed.position.y - self.galactic_position.y) as f32;
                            let dz = (placed.position.z - self.galactic_position.z) as f32;
                            let len = (dx * dx + dy * dy + dz * dz).sqrt();
                            if len < 0.001 { continue; }
                            let dir_norm = glam::Vec3::new(dx / len, dy / len, dz / len);
                            // Reject stars outside ~30° cone (dot < 0.866)
                            if cam_fwd.dot(dir_norm) < 0.866 { continue; }
                            let dir = dir_norm * 90000.0;
                            let clip = vp * glam::Vec4::new(dir.x, dir.y, dir.z, 1.0);
                            if clip.w <= 0.0 { continue; }
                            let sx = (clip.x / clip.w * 0.5 + 0.5) * sw;
                            let sy = (1.0 - (clip.y / clip.w * 0.5 + 0.5)) * sh;
                            let screen_dist = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
                            if screen_dist < best_screen_dist {
                                best_screen_dist = screen_dist;
                                best = Some((i, screen_dist));
                            }
                        }

                        if let Some((idx, px_dist)) = best {
                            let (placed, dist_ly) = &visible[idx];
                            let nav_star = navigation::NavStar {
                                id: placed.id,
                                galactic_pos: placed.position,
                                catalog_name: navigation::catalog_name_from_id(placed.id),
                                distance_ly: *dist_ly,
                                color: placed.star.color,
                                spectral_class: placed.star.spectral_class,
                                luminosity: placed.star.luminosity,
                            };
                            log::info!("LOCKED: {} ({:.2} ly, {:.0}px from center, {} candidates)",
                                nav_star.catalog_name, nav_star.distance_ly, px_dist, visible.len());
                            self.navigation.lock_star(nav_star);
                            self.audio.play_sfx(sa_audio::SfxId::Confirm, None);
                        } else {
                            log::warn!("No star visible on screen");
                        }
                    }
                }

                // Write live debug state in ALL modes (walk, helm, fly)
                if self.time.frame_count().is_multiple_of(30) {
                    self.write_debug_state();
                }

                // Query pipeline already updated in walk branch. Only update in
                // non-walk modes (helm/fly don't update it).
                if self.fly_mode || self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
                    self.physics.update_query_pipeline();
                }

                let t_interaction = Instant::now();
                // --- Interaction (runs in both standing and seated modes) ---
                let is_seated = self.helm.as_ref()
                    .map(|h| h.is_seated())
                    .unwrap_or(false);

                if let (Some(interaction), Some(player)) = (&mut self.interaction, &self.player)
                    && !self.fly_mode
                {
                    // Interactable colliders are children of the ship body (NOT
                    // the interior body). Use camera.position as ray origin —
                    // it's in the same coordinate space as the ship body.
                    //
                    // The query pipeline must be refreshed first: between
                    // physics.step() and this point, the ship body may have
                    // moved (gravity, cruise sync, rebase). The AABB tree
                    // becomes stale and the broadphase can't find the colliders
                    // even though ray and colliders moved by the same offset.
                    self.physics.sync_collider_positions();
                    self.physics.update_query_pipeline();

                    let (ray_origin, ray_dir) = if is_seated {
                        let pos = self.camera.position;
                        let fwd = self.camera.forward();
                        (
                            [pos.x as f32, pos.y as f32, pos.z as f32],
                            [fwd.x, fwd.y, fwd.z],
                        )
                    } else {
                        let eye_pos = player.position(&self.physics);
                        let fwd = self.camera.forward();
                        (
                            [eye_pos.x as f32, eye_pos.y as f32, eye_pos.z as f32],
                            [fwd.x, fwd.y, fwd.z],
                        )
                    };

                    let (_, mouse_dy) = self.input.mouse.delta();

                    // Collision groups handle filtering: exclude_solids()
                    // limits to sensors, and only interactable sensors are
                    // registered in the collider_to_id map.
                    // Debug: log ray origin + ship position every 120 frames
                    if self.time.frame_count().is_multiple_of(120) {
                        let st = self.ship.as_ref()
                            .and_then(|s| self.physics.get_body(s.body_handle))
                            .map(|b| *b.translation())
                            .unwrap_or(nalgebra::Vector3::zeros());
                        log::info!(
                            "INTERACT_DIAG: ray_origin=({:.2},{:.2},{:.2}), ship_pos=({:.1},{:.1},{:.1}), cam=({:.1},{:.1},{:.1}), seated={}",
                            ray_origin[0], ray_origin[1], ray_origin[2],
                            st.x, st.y, st.z,
                            self.camera.position.x, self.camera.position.y, self.camera.position.z,
                            is_seated,
                        );
                    }

                    let helm_clicked = interaction.update(
                        ray_origin,
                        ray_dir,
                        mouse_dy,
                        self.input.mouse.left_just_pressed(),
                        self.input.mouse.left_pressed(),
                        self.input.mouse.left_just_released(),
                        &self.physics,
                    );

                    // SFX: lever drag start
                    if self.input.mouse.left_just_pressed()
                        && let Some(ids) = &self.ship_ids
                        && interaction.hovered() == Some(ids.throttle_lever)
                    {
                        self.audio.play_sfx(sa_audio::SfxId::LeverMove, None);
                    }

                    // Update debug ray visualization
                    let debug = interaction.debug_ray();
                    let max_range = 2.0_f32;
                    let end_dist = debug.hit
                        .map(|(_, toi)| toi.min(max_range))
                        .unwrap_or(max_range);
                    let _end = [
                        ray_origin[0] + ray_dir[0] * end_dist,
                        ray_origin[1] + ray_dir[1] * end_dist,
                        ray_origin[2] + ray_dir[2] * end_dist,
                    ];
                    let _color = if debug.hit_id.is_some() {
                        [0.0, 1.0, 0.0] // green = hit interactable
                    } else if debug.hit.is_some() {
                        [1.0, 1.0, 0.0] // yellow = hit something but not interactable
                    } else {
                        [1.0, 0.0, 0.0] // red = miss
                    };
                    // Debug ray visualization disabled — was generating mesh + uploading
                    // to GPU every frame (24ms!). Enable only when actively debugging.

                    // If helm seat was clicked while standing, enter seated mode
                    if !is_seated && helm_clicked.is_some()
                        && let Some(helm) = &mut self.helm
                    {
                        helm.sit_down();
                        // Reset helm look offset (face ship forward direction)
                        self.helm_look_yaw = 0.0;
                        self.helm_look_pitch = 0.0;
                        // Disable player physics body while seated
                        if let Some(body) = self.physics.get_body_mut(player.body_handle) {
                            body.set_enabled(false);
                        }
                        log::info!("Entered helm seated mode — camera facing forward");
                    }
                }


                // Sync interactable state -> ship state
                if let (Some(interaction), Some(ship), Some(ids)) =
                    (&self.interaction, &mut self.ship, &self.ship_ids)
                {
                    // Throttle lever -> ship.throttle
                    if let Some(lever) = interaction.get(ids.throttle_lever) {
                        ship.throttle = lever.lever_position().unwrap_or(0.0);
                    }
                    // Engine button -> ship.engine_on
                    if let Some(button) = interaction.get(ids.engine_button) {
                        ship.engine_on = button.is_button_pressed().unwrap_or(false);
                    }
                }
                // Update interactable meshes ONLY when state changes (not every frame).
                // Regenerating meshes + uploading to GPU every frame is extremely expensive.
                if let (Some(interaction), Some(gpu), Some(renderer), Some(ids)) =
                    (&self.interaction, &self.gpu, &mut self.renderer, &self.ship_ids)
                {
                    // Update lever mesh only if position changed
                    if let Some(lever) = interaction.get(ids.throttle_lever) {
                        let pos = lever.lever_position().unwrap_or(0.0);
                        if interaction.is_dragging() {
                            let mesh = sa_meshgen::interactables::lever_mesh(pos);
                            let mesh_data = meshgen_to_render(&mesh);
                            let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                            if let Some(slot) = self.interactable_meshes.get_mut(ids.throttle_lever) {
                                *slot = handle;
                            }
                        }
                    }

                    // Update button mesh only on click (toggle frame)
                    if let Some(button) = interaction.get(ids.engine_button)
                        && self.input.mouse.left_just_released() && interaction.hovered() == Some(ids.engine_button) {
                            let pressed = button.is_button_pressed().unwrap_or(false);
                            self.audio.play_sfx(sa_audio::SfxId::ButtonClick, None);
                            if pressed {
                                self.audio.announce(sa_audio::VoiceId::EnginesIgniting);
                            }
                            let mesh = sa_meshgen::interactables::button_mesh(pressed);
                            let mesh_data = meshgen_to_render(&mesh);
                            let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                            if let Some(slot) = self.interactable_meshes.get_mut(ids.engine_button) {
                                *slot = handle;
                            }
                    }
                }

                // Landing lock button: toggle landing lock when clicked.
                if let (Some(interaction), Some(ids)) = (&self.interaction, &self.ship_ids)
                    && self.input.mouse.left_just_released()
                    && interaction.hovered() == Some(ids.landing_lock_button)
                {
                    self.landing.request_lock_toggle();
                    self.audio.play_sfx(sa_audio::SfxId::ButtonClick, None);
                    log::debug!("Landing lock toggle requested");
                }

                // --- Survival resources update ---
                {
                    let throttle = self.ship.as_ref().map(|s| s.throttle).unwrap_or(0.0);
                    let engine_on = self.ship.as_ref().map(|s| s.engine_on).unwrap_or(false);
                    let is_seated = self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false);
                    if is_seated {
                        self.ship_resources.update_with_drive(
                            dt,
                            throttle,
                            engine_on,
                            self.drive.mode(),
                            self.drive.speed_fraction(),
                        );
                    } else {
                        self.ship_resources.update_with_drive(
                            dt,
                            throttle,
                            engine_on,
                            sa_ship::DriveMode::Impulse,
                            0.0,
                        );
                    }

                    // Emergency warp drop on empty exotic fuel
                    if self.drive.mode() == sa_ship::DriveMode::Warp
                        && self.ship_resources.exotic_fuel <= 0.0
                    {
                        self.drive.request_disengage();
                        log::warn!("WARP DRIVE FAILED — exotic fuel exhausted!");
                        self.audio.announce(sa_audio::VoiceId::Danger);
                    }

                    // Low fuel consequence: reduce max thrust linearly below 20%
                    if let Some(ship) = &mut self.ship {
                        if self.ship_resources.fuel < 0.2 {
                            ship.max_thrust = self.ship_resources.fuel * 5.0 * Ship::DEFAULT_MAX_THRUST;
                        } else {
                            ship.max_thrust = Ship::DEFAULT_MAX_THRUST;
                        }
                    }
                }

                // --- Suit resources: drain when ship fails, recharge when ship works ---
                self.suit.update(
                    dt,
                    self.ship_resources.oxygen > 0.1,
                    self.ship_resources.power > 0.0,
                );

                // --- Gathering: check proximity to deposits ---
                {
                    let ship_pos = self.ship.as_ref()
                        .and_then(|s| s.position(&self.physics))
                        .unwrap_or((0.0, 0.0, 0.0));

                    let mut nearest: Option<(usize, f32)> = None;
                    for (i, dep) in self.deposits.iter().enumerate() {
                        if self.gathered.contains(&dep.id) {
                            continue;
                        }
                        let dx = dep.position[0] - ship_pos.0;
                        let dy = dep.position[1] - ship_pos.1;
                        let dz = dep.position[2] - ship_pos.2;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                        if dist < 500.0
                            && (nearest.is_none() || dist < nearest.unwrap().1)
                        {
                            nearest = Some((i, dist));
                        }
                    }
                    self.nearest_gatherable = nearest.map(|(i, _)| i);

                    // Gather on click when in range (and not interacting with ship controls)
                    if let Some(idx) = self.nearest_gatherable
                        && self.input.mouse.left_just_pressed()
                    {
                        let dep = &self.deposits[idx];
                        match dep.kind {
                            sa_survival::ResourceKind::FuelAsteroid => {
                                self.ship_resources.add_fuel(dep.amount);
                            }
                            sa_survival::ResourceKind::SupplyCache => {
                                self.ship_resources.add_oxygen(dep.amount);
                            }
                            sa_survival::ResourceKind::Derelict => {
                                self.ship_resources.add_fuel(dep.amount);
                                self.ship_resources.add_oxygen(dep.amount);
                            }
                        }
                        log::info!(
                            "Gathered {} (+{:.0}%) at [{:.0}, {:.0}, {:.0}]",
                            dep.kind.label(),
                            dep.amount * 100.0,
                            dep.position[0], dep.position[1], dep.position[2],
                        );
                        self.gathered.insert(dep.id);
                        self.nearest_gatherable = None;
                    }
                }

                // Update speed screen text
                if let (Some(interaction), Some(ship)) = (&mut self.interaction, &self.ship)
                    && let Some(ids) = &self.ship_ids
                {
                    let speed = ship.speed(&self.physics);
                    if let Some(screen) = interaction.get_mut(ids.speed_screen) {
                        let speed_line = match self.drive.mode() {
                            sa_ship::DriveMode::Warp if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                                let ly_s = self.drive.current_speed_c() * 3.169e-8;
                                format!("Speed: {:.3} ly/s", ly_s)
                            }
                            sa_ship::DriveMode::Cruise if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                                format!("Speed: {:.0}c", self.drive.current_speed_c())
                            }
                            _ => format!("Speed: {:.1} m/s", speed),
                        };
                        screen.set_screen_text(vec![
                            speed_line,
                            format!("Throttle: {:.0}%", ship.throttle * 100.0),
                            format!("Engine: {}", if ship.engine_on { "ON" } else { "OFF" }),
                        ]);
                    }
                }

                let interaction_us = t_interaction.elapsed().as_micros() as u64;
                self.perf.player_us = t0.elapsed().as_micros() as u64;
                self.perf.stars_us = interaction_us; // repurpose stars_us for interaction timing

                // --- Star regen ---
                let t2 = Instant::now();
                self.maybe_regenerate_stars();
                self.perf.stars_us = t2.elapsed().as_micros() as u64;

                // Update navigation (nearby stars for window markers + gravity well)
                self.navigation.update_nearby(self.galactic_position);

                // --- Drive visuals update (smooth shader parameter transitions) ---
                let drive_dir = if let Some(ship) = &self.ship {
                    if let Some(body) = self.physics.get_body(ship.body_handle) {
                        let rot = body.rotation();
                        let fwd = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                        [fwd.x, fwd.y, fwd.z]
                    } else {
                        [0.0, 0.0, -1.0]
                    }
                } else {
                    [0.0, 0.0, -1.0]
                };
                self.drive_visuals.update(&self.drive, drive_dir, dt);

                // --- Audio update ---
                {
                    let fwd = self.camera.forward();
                    let right = self.camera.right();
                    let up = fwd.cross(right).normalize_or_zero();
                    let cam_pos = self.camera.position;
                    self.audio.set_listener(sa_audio::Listener {
                        position: glam::Vec3::new(
                            cam_pos.x as f32,
                            cam_pos.y as f32,
                            cam_pos.z as f32,
                        ),
                        forward: fwd,
                        up: if up.length() > 0.5 { up } else { glam::Vec3::Y },
                    });

                    // Engine state
                    let engine_state = if let Some(ship) = &self.ship {
                        if !ship.engine_on {
                            sa_audio::EngineState::Off
                        } else if self.drive.mode() == sa_ship::DriveMode::Warp {
                            if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) {
                                sa_audio::EngineState::WarpEngaged
                            } else {
                                sa_audio::EngineState::WarpSpool
                            }
                        } else if self.drive.mode() == sa_ship::DriveMode::Cruise {
                            sa_audio::EngineState::Cruise
                        } else if ship.throttle > 0.01 {
                            sa_audio::EngineState::Impulse
                        } else {
                            sa_audio::EngineState::Idle
                        }
                    } else {
                        sa_audio::EngineState::Off
                    };
                    self.audio.set_engine_state(engine_state);

                    // Music context
                    let music_ctx = if self.drive.mode() == sa_ship::DriveMode::Warp
                        && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                    {
                        sa_audio::MusicContext::Warp
                    } else if self.ship_resources.fuel < 0.2
                        || self.ship_resources.exotic_fuel < 0.1
                    {
                        sa_audio::MusicContext::Tension
                    } else if self.active_system.is_some() {
                        sa_audio::MusicContext::Exploration
                    } else {
                        sa_audio::MusicContext::Idle
                    };
                    self.audio.set_music_context(music_ctx);

                    // Power state
                    self.audio.set_power(self.ship_resources.power > 0.0);

                    // Fuel warnings
                    if self.ship_resources.fuel < 0.2 && !self.fuel_low_announced {
                        self.audio.announce(sa_audio::VoiceId::EnergyLow);
                        self.fuel_low_announced = true;
                    }
                    if self.ship_resources.fuel >= 0.3 {
                        self.fuel_low_announced = false; // reset when fuel recovers
                    }

                    // Altitude proximity beep: fires when descending within 100 m.
                    // Beep rate scales with closeness to ground.
                    self.altitude_beep_timer -= dt;
                    if let Some(clearance) = self.last_clearance {
                        if clearance < 100.0 && self.altitude_beep_timer <= 0.0 {
                            self.audio.play_sfx(sa_audio::SfxId::AltitudeBeep, None);
                            // Reset timer: interval shrinks as altitude drops.
                            self.altitude_beep_timer = if clearance < 5.0 {
                                0.05 // near-continuous (20/sec) — avoids 60 overlapping samples
                            } else if clearance < 10.0 {
                                1.0 / 8.0
                            } else if clearance < 20.0 {
                                1.0 / 4.0
                            } else if clearance < 50.0 {
                                1.0 / 2.0
                            } else {
                                1.0 // 1 beep/sec at 50–100 m
                            };
                        }
                    } else {
                        // No clearance data — reset timer so beep starts promptly on entry.
                        self.altitude_beep_timer = 0.0;
                    }

                    self.audio.update(dt);
                }

                // --- Render ---
                let t3 = Instant::now();

                // --- Terrain streaming (before immutable renderer borrow) ---
                // Deactivation check
                // Check deactivation separately to avoid borrow issues
                let should_deactivate_terrain = self.terrain.as_ref()
                    .is_some_and(|t| t.should_deactivate(self.galactic_position));
                if should_deactivate_terrain {
                    if let Some(sys) = &mut self.active_system {
                        sys.hidden_body_index = None;
                    }
                    if let Some(mut t) = self.terrain.take() {
                        t.cleanup(&mut self.physics);
                    }
                    self.terrain_gravity = None;
                    log::info!("Terrain deactivated");
                }

                // Diagnostic: log planet distances every 60 frames to debug terrain activation
                if let Some(sys) = &self.active_system
                    && self.time.frame_count().is_multiple_of(60)
                {
                    let positions = sys.compute_positions_ly_pub();
                    let ly_to_m = 9.461e15_f64;
                    for (i, pos) in positions.iter().enumerate() {
                        if let Some(r_m) = sys.body_radius_m(i)
                            && sys.planet_data(i).is_some()
                        {
                            let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                            let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                            let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                            let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();
                            let ratio = dist_m / r_m;
                            log::info!(
                                "DIAG body[{}]: dist={:.0}km, radius={:.0}km, ratio={:.2}x, \
                                 terrain={}, hidden={:?}, galactic=({:.12},{:.12},{:.12}), \
                                 planet=({:.12},{:.12},{:.12})",
                                i, dist_m / 1000.0, r_m / 1000.0, ratio,
                                self.terrain.is_some(), sys.hidden_body_index,
                                self.galactic_position.x, self.galactic_position.y, self.galactic_position.z,
                                pos.x, pos.y, pos.z,
                            );
                        }
                    }
                }

                // Activation check (only if no terrain active and in a solar system)
                if self.terrain.is_none()
                    && let Some(sys) = &self.active_system
                    && let Some((body_idx, planet_pos, config, surface_grav)) =
                        terrain_integration::find_terrain_planet(sys, self.galactic_position)
                {
                    log::info!(
                        "Terrain activated for body {} (radius {:.0} km, g={:.2} m/s2, type={:?}, seed={})",
                        body_idx,
                        config.radius_m / 1000.0,
                        surface_grav,
                        config.sub_type,
                        config.noise_seed,
                    );
                    self.terrain = Some(terrain_integration::TerrainManager::new(
                        config, planet_pos, body_idx, surface_grav,
                    ));

                    // Unified physics origin rebase: on terrain activation,
                    // shift all physics bodies so the ship is at the rapier origin.
                    // This ensures terrain colliders (within ~600m) and the sphere
                    // barrier are at reasonable f32 distances from the ship.
                    if let Some(ship) = &self.ship
                        && let Some(body) = self.physics.rigid_body_set.get(ship.body_handle)
                    {
                        let ship_pos = body.translation().clone_owned();
                        let shift = -ship_pos;
                        // Shift ship body
                        if let Some(b) = self.physics.rigid_body_set.get_mut(ship.body_handle) {
                            let t = b.translation();
                            b.set_translation(
                                nalgebra::Vector3::new(t.x + shift.x, t.y + shift.y, t.z + shift.z),
                                true,
                            );
                        }
                        // Shift player body
                        if let Some(player) = &self.player
                            && let Some(b) = self.physics.rigid_body_set.get_mut(player.body_handle)
                        {
                            let t = b.translation();
                            b.set_translation(
                                nalgebra::Vector3::new(t.x + shift.x, t.y + shift.y, t.z + shift.z),
                                true,
                            );
                        }
                        // Interior body stays at origin (player controller handles offset).
                        log::info!("Physics origin rebase on terrain activation: shifted by ({:.0},{:.0},{:.0})",
                            shift.x, shift.y, shift.z);
                    }

                    // Auto-disengage cruise/warp on terrain activation.
                    // Without this, cruise speed (~55,000 km/s) overshoots the
                    // planet surface in a fraction of a second, landing the camera
                    // deep inside where only LOD 0 chunks are visible.
                    if self.drive.mode() != sa_ship::DriveMode::Impulse {
                        log::info!("Auto-disengage drive: terrain activation");
                        self.drive.request_disengage();
                    }
                }

                // Deactivate terrain if no solar system is active
                if self.active_system.is_none()
                    && let Some(terrain_mgr) = &mut self.terrain
                {
                    terrain_mgr.cleanup(&mut self.physics);
                    self.terrain = None;
                    self.terrain_gravity = None;
                    log::info!("Terrain deactivated (no solar system)");
                }

                // Update terrain and collect draw commands (needs &mut renderer + &mut physics)
                // Ship "down" direction for gravity blending.
                let ship_down = self.ship.as_ref()
                    .and_then(|s| self.physics.get_body(s.body_handle))
                    .map(|body| {
                        let down = body.rotation() * nalgebra::Vector3::new(0.0, -1.0, 0.0);
                        [down.x, down.y, down.z]
                    })
                    .unwrap_or([0.0, -1.0, 0.0]);

                let terrain_commands: Vec<DrawCommand> = if let Some(terrain_mgr) = &mut self.terrain {
                    if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                        let planet_pos = self.active_system.as_ref()
                            .and_then(|sys| {
                                let positions = sys.compute_positions_ly_pub();
                                positions.get(terrain_mgr.body_index()).copied()
                            })
                            .unwrap_or(WorldPos::ORIGIN);
                        let rebase_bodies = terrain_colliders::RebaseBodies {
                            ship: self.ship.as_ref().map(|s| s.body_handle),
                            player: self.player.as_ref().map(|p| p.body_handle),
                        };
                        let result = terrain_mgr.update(
                            self.galactic_position,
                            planet_pos,
                            &mut renderer.mesh_store,
                            &gpu.device,
                            &mut self.physics,
                            ship_down,
                            &rebase_bodies,
                        );
                        if let Some(sys) = &mut self.active_system {
                            if sys.hidden_body_index != result.hidden_body_index {
                                log::info!(
                                    "Hidden body index: {:?}, system has {} bodies",
                                    result.hidden_body_index,
                                    sys.body_count(),
                                );
                            }
                            sys.hidden_body_index = result.hidden_body_index;
                        }
                        self.terrain_gravity = result.gravity;
                        result.draw_commands
                    } else {
                        vec![]
                    }
                } else {
                    self.terrain_gravity = None;
                    vec![]
                };

                // After terrain update (which may rebase physics origin), re-sync
                // the camera position from the ship's current rapier position.
                // Without this, the camera reads the pre-rebase position while the
                // ship mesh uses the post-rebase position → one-frame flicker.
                if self.terrain.is_some() {
                    if self.helm.as_ref().is_some_and(|h| h.is_seated()) {
                        if let (Some(helm), Some(ship)) = (&self.helm, &self.ship)
                            && let Some((cx, cy, cz)) = helm.camera_position(&self.physics, ship)
                        {
                            self.camera.position = WorldPos::new(cx as f64, cy as f64, cz as f64);
                        }
                    } else if let Some(player) = &self.player
                        && let Some(ship) = &self.ship
                        && let Some(body) = self.physics.get_body(ship.body_handle)
                    {
                        let ship_rot = *body.rotation();
                        self.camera.position = player.position_ship_up(&self.physics, ship_rot);
                    }
                }

                // APPROACH_DIAG: consolidated approach diagnostic (every 60 frames)
                if self.active_system.is_some()
                    && self.time.frame_count().is_multiple_of(60)
                {
                    let drive_mode = match self.drive.mode() {
                        sa_ship::DriveMode::Impulse => "impulse",
                        sa_ship::DriveMode::Cruise => "cruise",
                        sa_ship::DriveMode::Warp => "warp",
                    };
                    let terrain_active = self.terrain.is_some();
                    let gravity_blend = self.terrain_gravity
                        .as_ref()
                        .map(|g| g.blend)
                        .unwrap_or(0.0);
                    let vertical_speed = self.ship.as_ref()
                        .and_then(|s| self.physics.get_body(s.body_handle))
                        .map(|b| {
                            let gdir = self.terrain_gravity
                                .as_ref()
                                .map(|g| nalgebra::Vector3::new(g.direction[0], g.direction[1], g.direction[2]))
                                .unwrap_or(nalgebra::Vector3::new(0.0, -1.0, 0.0));
                            b.linvel().dot(&gdir)
                        })
                        .unwrap_or(0.0);
                    // Find nearest planet distance.
                    // When terrain is active, use the frozen planet position
                    // (matches what the terrain system actually uses) instead of
                    // the orbiting position which diverges rapidly at TIME_SCALE=30.
                    if let Some(terrain_mgr) = &self.terrain {
                        let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                        let dist_m = (cam_rel[0] * cam_rel[0]
                            + cam_rel[1] * cam_rel[1]
                            + cam_rel[2] * cam_rel[2]).sqrt();
                        let ratio = dist_m / terrain_mgr.planet_radius_m();
                        log::info!(
                            "APPROACH_DIAG: dist={:.0}km, ratio={:.2}x, drive={}, terrain={}, gravity_blend={:.2}, vertical_speed={:.1}m/s",
                            dist_m / 1000.0, ratio, drive_mode, terrain_active, gravity_blend, vertical_speed,
                        );
                    } else if let Some(sys) = &self.active_system {
                        let positions = sys.compute_positions_ly_pub();
                        let ly_to_m = 9.461e15_f64;
                        let mut best_dist_km = f64::MAX;
                        let mut best_ratio = f64::MAX;
                        for (i, pos) in positions.iter().enumerate() {
                            if let Some(r_m) = sys.body_radius_m(i)
                                && sys.planet_data(i).is_some()
                            {
                                let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                                let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                                let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                                let dist_m = (dx * dx + dy * dy + dz * dz).sqrt();
                                let ratio = dist_m / r_m;
                                if ratio < best_ratio {
                                    best_ratio = ratio;
                                    best_dist_km = dist_m / 1000.0;
                                }
                            }
                        }
                        if best_dist_km < f64::MAX {
                            log::info!(
                                "APPROACH_DIAG: dist={:.0}km, ratio={:.2}x, drive={}, terrain={}, gravity_blend={:.2}, vertical_speed={:.1}m/s",
                                best_dist_km, best_ratio, drive_mode, terrain_active, gravity_blend, vertical_speed,
                            );
                        }
                    }
                }

                if let (Some(gpu), Some(renderer)) = (&self.gpu, &self.renderer) {
                    // 1. Render BOTH monitors to offscreen textures in ONE encoder
                    // (avoids multiple queue.submit() calls per frame)
                    if let Some(ui_sys) = &mut self.ui_system {
                        let system_info = self.active_system.as_ref()
                            .map(|sys| sys.body_summary());
                        let target_info = self.navigation.locked_target.as_ref().map(|t| {
                            let speed_ly_s = self.drive.current_speed_ly_s();
                            let (dist, eta) = self.navigation
                                .target_eta(self.galactic_position, speed_ly_s)
                                .unwrap_or((0.0, f64::INFINITY));
                            (t.catalog_name.clone(), dist, eta)
                        });
                        let helm_data = ui::helm_screen::HelmData {
                            speed: self.ship.as_ref()
                                .map(|s| s.speed(&self.physics))
                                .unwrap_or(0.0),
                            throttle: self.ship.as_ref()
                                .map(|s| s.throttle)
                                .unwrap_or(0.0),
                            engine_on: self.ship.as_ref()
                                .map(|s| s.engine_on)
                                .unwrap_or(false),
                            fuel: self.ship_resources.fuel,
                            drive_mode: self.drive.mode(),
                            drive_status: self.drive.status(),
                            drive_speed_c: self.drive.current_speed_c(),
                            exotic_fuel: self.ship_resources.exotic_fuel,
                            system_info,
                            target_info,
                            altitude_m: self.last_clearance,
                        };

                        // Placeholder: deposits are positioned in rapier space at
                        // game start. After origin rebase they become stale. Skip
                        // contacts when terrain is active (deposits are in space,
                        // not on planet surfaces). TODO: replace with galactic-
                        // coordinate deposits in the resource gathering system.
                        let contacts: Vec<ui::sensors_screen::SensorContact> = if self.terrain.is_some() {
                            Vec::new()
                        } else {
                            let ship_pos = self.ship.as_ref()
                                .and_then(|s| s.position(&self.physics))
                                .unwrap_or((0.0, 0.0, 0.0));
                            self.deposits.iter()
                                .map(|dep| {
                                    let dx = dep.position[0] - ship_pos.0;
                                    let dy = dep.position[1] - ship_pos.1;
                                    let dz = dep.position[2] - ship_pos.2;
                                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                                    ui::sensors_screen::SensorContact {
                                        label: dep.kind.label().to_string(),
                                        icon: dep.kind.icon().to_string(),
                                        distance: dist,
                                        gathered: self.gathered.contains(&dep.id),
                                    }
                                })
                                .collect()
                        };
                        let sensors_data = ui::sensors_screen::SensorsData {
                            contacts,
                            ship_fuel: self.ship_resources.fuel,
                        };

                        // Single encoder for both monitors
                        let mut monitor_encoder = gpu.device.create_command_encoder(
                            &wgpu::CommandEncoderDescriptor { label: Some("Monitor Encoder") },
                        );
                        ui_sys.render_helm_monitor(&gpu.device, &gpu.queue, &mut monitor_encoder, &helm_data);
                        ui_sys.render_sensors_monitor(&gpu.device, &gpu.queue, &mut monitor_encoder, &sensors_data);
                        gpu.queue.submit(std::iter::once(monitor_encoder.finish()));
                    }

                    // 2. Build draw commands
                    let mut commands = if self.view_mode == 6 || self.view_mode == 7 {
                        // Debug: ship part viewing at origin
                        if let Some(ship_mesh) = self.ship_part_mesh {
                            vec![DrawCommand {
                                mesh: ship_mesh,
                                model_matrix: Mat4::IDENTITY,
                                pre_rebased: false,
                            }]
                        } else {
                            vec![]
                        }
                    } else {
                        // Normal gameplay: render ship hull + interactables
                        let mut cmds = Vec::new();

                        // Ship hull
                        // Ship transform from physics body (position + rotation)
                        let ship_transform = if let Some(ship) = &self.ship {
                            if let Some(body) = self.physics.get_body(ship.body_handle) {
                                let p = body.translation();
                                let r = body.rotation();
                                let rot = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                                Mat4::from_rotation_translation(rot, Vec3::new(p.x, p.y, p.z))
                            } else { Mat4::IDENTITY }
                        } else { Mat4::IDENTITY };

                        // Hull mesh at ship's world position/rotation
                        if let Some(hull_handle) = self.ship_part_mesh {
                            cmds.push(DrawCommand {
                                mesh: hull_handle,
                                model_matrix: ship_transform,
                                pre_rebased: false,
                            });
                        }

                        // Interactable meshes in ship-local space (transformed by ship).
                        // Skip the Speed Display mesh (id=3) — replaced by the egui monitor quad.
                        let layout = sa_ship::station::cockpit_layout();
                        for (i, handle) in self.interactable_meshes.iter().enumerate() {
                            if let Some(ids) = &self.ship_ids
                                && i == ids.speed_screen
                            {
                                continue;
                            }
                            if let Some(placement) = layout.interactables.get(i) {
                                let pos = placement.position;
                                cmds.push(DrawCommand {
                                    mesh: *handle,
                                    model_matrix: ship_transform * Mat4::from_translation(pos),
                                    pre_rebased: false,
                                });
                            }
                        }

                        cmds
                    };

                    // Append solar system bodies (planets, moons, star) if in a system
                    if let Some(system) = &mut self.active_system {
                        let hidden = system.hidden_body_index;
                        let total_bodies = system.body_count();
                        let system_commands = system.update(
                            dt as f64,
                            self.galactic_position,
                        );
                        // Log every 60 frames when terrain is active
                        if self.terrain.is_some() && self.time.frame_count().is_multiple_of(60) {
                            log::info!("RENDER: {} solar cmds (hidden={:?}, bodies={}), {} terrain cmds",
                                system_commands.len(), hidden, total_bodies, terrain_commands.len());
                        }
                        commands.extend(system_commands);
                    }

                    // Append terrain draw commands
                    commands.extend(terrain_commands);

                    // DIAGNOSTIC: dump every draw command's position once per second
                    if self.terrain.is_some() && self.time.frame_count().is_multiple_of(60) {
                        for (i, cmd) in commands.iter().enumerate() {
                            let t = cmd.model_matrix.col(3);
                            let dist = ((t.x * t.x + t.y * t.y + t.z * t.z) as f64).sqrt();
                            let mesh_info = if let (Some(_gpu), Some(renderer)) = (&self.gpu, &self.renderer) {
                                renderer.mesh_store.get(cmd.mesh)
                                    .map(|m| m.index_count)
                                    .unwrap_or(0)
                            } else { 0 };
                            log::info!("CMD[{}]: pos=({:.0},{:.0},{:.0}) dist={:.0}m tris={} pre_rebased={}",
                                i, t.x, t.y, t.z, dist, mesh_info / 3, cmd.pre_rebased);
                        }
                    }

                    // Unload system if player has cruised far away (> 100 AU from star)
                    if let Some(system) = &self.active_system {
                        let dist = self.galactic_position.distance_to(system.star_galactic_pos);
                        let au_in_ly = 1.581e-5_f64;
                        if dist > 100.0 * au_in_ly {
                            log::info!("Left system boundary — unloading");
                            if let Some(t) = &mut self.terrain { t.cleanup(&mut self.physics); }
                            self.terrain = None;
                            self.terrain_gravity = None;
                            self.active_system = None;
                        }
                    }

                    // 3. Build screen draw commands for in-world monitors
                    let screen_draws: Vec<ScreenDrawCommand<'_>> =
                        if self.view_mode == 0 {
                            // In normal gameplay, place the screen at the Speed Display position
                            let ship_transform = if let Some(ship) = &self.ship {
                                if let Some(body) = self.physics.get_body(ship.body_handle) {
                                    let p = body.translation();
                                    let r = body.rotation();
                                    let rot = glam::Quat::from_xyzw(r.i, r.j, r.k, r.w);
                                    Mat4::from_rotation_translation(rot, Vec3::new(p.x, p.y, p.z))
                                } else { Mat4::IDENTITY }
                            } else { Mat4::IDENTITY };

                            let mut draws = Vec::new();
                            // Helm monitor at Speed Display position
                            if let (Some(quad), Some(bind_group)) =
                                (&self.screen_quad, &self.screen_bind_group)
                            {
                                let screen_pos = Vec3::new(0.0, 0.3, 0.8);
                                draws.push(ScreenDrawCommand {
                                    quad,
                                    model_matrix: ship_transform * Mat4::from_translation(screen_pos),
                                    texture_bind_group: bind_group,
                                });
                            }
                            // Sensors monitor to the right of the helm
                            if let (Some(quad), Some(bind_group)) =
                                (&self.sensors_quad, &self.sensors_bind_group)
                            {
                                let screen_pos = Vec3::new(0.8, 0.3, 0.8);
                                draws.push(ScreenDrawCommand {
                                    quad,
                                    model_matrix: ship_transform * Mat4::from_translation(screen_pos),
                                    texture_bind_group: bind_group,
                                });
                            }
                            draws
                        } else {
                            vec![]
                        };

                    self.perf.draw_calls = commands.len() as u32 + screen_draws.len() as u32;
                    self.perf.star_count = renderer.star_field.star_count;

                    // 4. Render 3D scene + screen quads
                    let drive_render = sa_render::DriveRenderParams {
                        velocity_dir: self.drive_visuals.visuals.velocity_dir,
                        beta: self.drive_visuals.visuals.beta,
                        streak_factor: self.drive_visuals.visuals.streak_factor,
                        warp_intensity: self.drive_visuals.visuals.warp_intensity,
                        flash_intensity: self.drive_visuals.visuals.flash_intensity,
                    };
                    if let Some(mut frame_ctx) = renderer.render_frame(
                        gpu,
                        &self.camera,
                        &commands,
                        &screen_draws,
                        Vec3::new(0.5, -0.8, -0.3),
                        self.galactic_position,
                        &drive_render,
                    ) {
                        // 5. Render HUD overlay via egui
                        if let Some(ui_sys) = &mut self.ui_system {
                            let hovered_kind = self.interaction.as_ref()
                                .and_then(|inter| {
                                    let id = inter.hovered()?;
                                    Some(inter.get(id)?.kind.clone())
                                });
                            // Project locked target to screen space
                            let sw = gpu.config.width as f32;
                            let sh = gpu.config.height as f32;
                            let aspect = sw / sh;
                            let vp = self.camera.view_projection_matrix(aspect);
                            let (target_screen, target_angle, target_name, target_dist) =
                                if let Some(target) = &self.navigation.locked_target {
                                    // Live distance (not stale lock-time value)
                                    let live_dist = self.galactic_position.distance_to(target.galactic_pos);

                                    // Convert galactic ly position to camera-relative meters
                                    let ly_to_m: f64 = 9.461e15;
                                    let dx = (target.galactic_pos.x - self.galactic_position.x) * ly_to_m;
                                    let dy = (target.galactic_pos.y - self.galactic_position.y) * ly_to_m;
                                    let dz = (target.galactic_pos.z - self.galactic_position.z) * ly_to_m;
                                    let cam_rel = glam::Vec3::new(dx as f32, dy as f32, dz as f32)
                                        - glam::Vec3::new(
                                            self.camera.position.x as f32,
                                            self.camera.position.y as f32,
                                            self.camera.position.z as f32,
                                        );
                                    let clip = vp * glam::Vec4::new(cam_rel.x, cam_rel.y, cam_rel.z, 1.0);
                                    if clip.w > 0.0 {
                                        let ndc_x = clip.x / clip.w;
                                        let ndc_y = clip.y / clip.w;
                                        let sx = (ndc_x * 0.5 + 0.5) * sw;
                                        let sy = (1.0 - (ndc_y * 0.5 + 0.5)) * sh;
                                        if sx >= 0.0 && sx <= sw && sy >= 0.0 && sy <= sh {
                                            (Some([sx, sy]), None, Some(target.catalog_name.clone()), Some(live_dist))
                                        } else {
                                            let angle = (sy - sh / 2.0).atan2(sx - sw / 2.0);
                                            (None, Some(angle), Some(target.catalog_name.clone()), Some(live_dist))
                                        }
                                    } else {
                                        let angle = (-clip.y).atan2(-clip.x);
                                        (None, Some(angle), Some(target.catalog_name.clone()), Some(live_dist))
                                    }
                                } else {
                                    (None, None, None, None)
                                };
                            let hud_state = ui::HudState {
                                hovered_kind,
                                screen_width: gpu.config.width,
                                screen_height: gpu.config.height,
                                fuel: self.ship_resources.fuel,
                                oxygen: self.ship_resources.oxygen,
                                gather_available: self.nearest_gatherable.is_some(),
                                suit_o2: self.suit.oxygen,
                                suit_power: self.suit.power,
                                cursor_grabbed: self.cursor_grabbed,
                                target_screen_pos: target_screen,
                                target_off_screen_angle: target_angle,
                                target_name,
                                target_distance_ly: target_dist,
                                time: self.time.total_seconds() as f32,
                            };
                            ui_sys.render_hud(
                                &gpu.device,
                                &gpu.queue,
                                &mut frame_ctx.encoder,
                                &frame_ctx.view,
                                &hud_state,
                            );
                        }
                        // 6. Submit and present
                        Renderer::submit_frame(gpu, frame_ctx);
                    }
                }
                self.perf.render_us = t3.elapsed().as_micros() as u64;
                self.perf.total_us = frame_start.elapsed().as_micros() as u64;

                let dt_secs = self.time.delta_seconds();
                if dt_secs > 0.0 {
                    self.perf.fps = 1.0 / dt_secs;
                }

                // Update window title with perf stats every 0.5s
                self.perf_update_timer += dt_secs;
                if self.perf_update_timer >= 0.5 {
                    self.perf_update_timer = 0.0;
                    if let Some(window) = &self.window {
                        let helm_status = if self.fly_mode {
                            "FLY".to_string()
                        } else if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
                            let speed = self.ship.as_ref()
                                .map(|s| s.speed(&self.physics))
                                .unwrap_or(0.0);
                            match self.drive.mode() {
                                sa_ship::DriveMode::Warp if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                                    let ly_s = self.drive.current_speed_c() * 3.169e-8;
                                    format!("WARP {:.3} ly/s", ly_s)
                                }
                                sa_ship::DriveMode::Cruise if matches!(self.drive.status(), sa_ship::DriveStatus::Engaged) => {
                                    format!("CRUISE {:.0}c", self.drive.current_speed_c())
                                }
                                _ => format!("HELM {:.1}m/s", speed),
                            }
                        } else {
                            "WALK".to_string()
                        };
                        // Interaction feedback in title
                        let hover_info = if let Some(interaction) = &self.interaction {
                            if let Some(id) = interaction.hovered() {
                                if let Some(inter) = interaction.get(id) {
                                    format!(" | [{}]", inter.label)
                                } else { String::new() }
                            } else { String::new() }
                        } else { String::new() };
                        let engine_info = if let Some(ship) = &self.ship {
                            format!(" | Engine:{} Throttle:{:.0}%",
                                if ship.engine_on { "ON" } else { "OFF" },
                                ship.throttle * 100.0)
                        } else { String::new() };

                        window.set_title(&format!(
                            "SpaceAway | {:.0} FPS | {}{}{} | draws {}",
                            self.perf.fps,
                            helm_status,
                            hover_info,
                            engine_info,
                            self.perf.draw_calls,
                        ));
                    }
                }

                self.events.flush();
                self.input.end_frame();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }

        if self.quit_requested {
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.cursor_grabbed
        {
            self.input
                .mouse
                .accumulate_delta(delta.0 as f32, delta.1 as f32);
        }
    }
}

/// Convert sa_meshgen::Mesh -> sa_render::MeshData.
/// Simple field-by-field copy; no GPU dependencies in sa_meshgen.
fn meshgen_to_render(mesh: &sa_meshgen::Mesh) -> MeshData {
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
fn all_ship_parts() -> Vec<(&'static str, sa_meshgen::Mesh)> {
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

/// Build the full ship using the modular assembly system.
///
/// Layout:
/// cockpit(4.0) -> corridor(4.0) -> transition(4.0->5.0) -> nav_room(5.0)
/// -> transition(5.0->4.0) -> corridor(4.0) -> transition(4.0->5.0)
/// -> eng_room(5.0) -> transition(5.0->3.5) -> engine(3.5)
/// v1 ship assembly (preserved for reference).
#[allow(dead_code)]
fn assemble_ship_v1() -> sa_meshgen::Mesh {
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

/// v2 ship assembly — larger, windowed cockpit, thick bulkheads.
fn assemble_ship() -> sa_meshgen::Mesh {
    sa_meshgen::ship_parts_v2::assemble_ship_v2()
}

fn make_cube() -> MeshData {
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

fn main() {
    env_logger::init();
    log::info!("SpaceAway starting...");
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
