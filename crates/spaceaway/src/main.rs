mod drive_integration;
mod navigation;
mod ship_colliders;
mod ship_setup;
mod solar_system;
mod star_streaming;
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
    ScreenQuad, StarVertex, Vertex,
};
use sa_ship::helm::HelmController;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_survival::{ResourceDeposit, ShipResources, generate_deposits};
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

/// Distance threshold (in light-years) before we regenerate the star field.
/// Roughly one sector width.
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
    debug_ray_mesh: Option<sa_core::Handle<sa_render::MeshMarker>>,
    /// Debug ray endpoint and color for rendering.
    #[allow(clippy::type_complexity)]
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
    /// Active solar system (set when the player enters one).
    active_system: Option<solar_system::ActiveSystem>,
    /// Resource deposits in the game world.
    deposits: Vec<ResourceDeposit>,
    /// IDs of deposits that have been gathered.
    gathered: HashSet<u64>,
    /// Index of the nearest gatherable deposit (within range), if any.
    nearest_gatherable: Option<usize>,
    /// Navigation: nearby stars, lock-on, gravity well detection.
    navigation: navigation::Navigation,
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
            active_system: None,
            deposits: generate_deposits(42),
            gathered: HashSet::new(),
            nearest_gatherable: None,
            navigation: navigation::Navigation::new(seed),
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
            self.window = Some(window);
            self.last_frame = Instant::now();
            self.setup_scene();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
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

                    // Teleport keys: only when NOT seated at helm (1/2/3 are drive keys when seated)
                    let is_seated = self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false);
                    if event.state.is_pressed() && !is_seated {
                        match code {
                            KeyCode::Digit1 => self.teleport_to(0), // mid-disc
                            KeyCode::Digit2 => self.teleport_to(1), // above galaxy
                            KeyCode::Digit3 => self.teleport_to(2), // galaxy edge
                            KeyCode::Digit4 => self.teleport_to(3), // near center
                            KeyCode::Digit5 => self.teleport_to(4), // near nebula
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
                            KeyCode::Digit0 => {
                                // Return to normal scene view
                                self.view_mode = 0;
                                self.ship_part_mesh = None;
                                log::info!("Returned to normal scene view");
                            }
                            KeyCode::Tab => {
                                // Lock nearest star for navigation
                                self.navigation.update_nearby(self.galactic_position);
                                if !self.navigation.nearby_stars.is_empty() {
                                    self.navigation.lock_target(0);
                                    if let Some(target) = &self.navigation.locked_target {
                                        log::info!("LOCKED: {} ({:.2} ly away)", target.catalog_name, target.distance_ly);
                                    }
                                } else {
                                    log::warn!("No nearby stars found");
                                }
                            }
                            KeyCode::Digit8 => {
                                // Debug: jump to nearest star, teleport next to first planet
                                self.navigation.update_nearby(self.galactic_position);
                                if let Some(star) = self.navigation.nearby_stars.first().cloned() {
                                    log::info!("Jumping to nearest star: {} ({:.2} ly)", star.catalog_name, star.distance_ly);

                                    // Generate the system to find the first planet's position
                                    let sector_coord = sa_universe::SectorCoord::new(
                                        star.id.sector_x().into(),
                                        star.id.sector_y().into(),
                                        star.id.sector_z().into(),
                                    );
                                    let sector = sa_universe::sector::generate_sector(
                                        MasterSeed(42),
                                        sector_coord,
                                    );

                                    if let Some(placed) = sector.stars.iter().find(|s| s.id == star.id) {
                                        let system = sa_universe::generate_system(&placed.star, star.id.0);

                                        // Teleport near the first planet (or near the star if no planets)
                                        let au_in_ly = 1.581e-5_f64;
                                        let meters_in_ly = 1.0 / 9.461e15_f64;
                                        let (offset_ly, description) = if let Some(planet) = system.planets.first() {
                                            // Place camera 3x planet radius from the planet surface
                                            let planet_radius_m = planet.radius_earth as f64 * 6_371_000.0;
                                            let view_dist_m = planet_radius_m * 3.0;
                                            let planet_orbital_ly = planet.orbital_radius_au as f64 * au_in_ly;
                                            // Offset from star: planet orbital distance + viewing distance
                                            let offset = planet_orbital_ly + view_dist_m * meters_in_ly;
                                            (offset, format!(
                                                "Planet 1: {:?} {:.0}km at {:.2}AU — viewing from {:.0}km",
                                                planet.sub_type,
                                                planet.radius_earth * 6371.0,
                                                planet.orbital_radius_au,
                                                view_dist_m / 1000.0,
                                            ))
                                        } else {
                                            // No planets — teleport near star surface
                                            let star_radius_m = placed.star.radius as f64 * 696_000_000.0;
                                            let offset = star_radius_m * 5.0 * meters_in_ly;
                                            (offset, "No planets — near star".to_string())
                                        };

                                        self.galactic_position = WorldPos::new(
                                            star.galactic_pos.x + offset_ly,
                                            star.galactic_pos.y,
                                            star.galactic_pos.z,
                                        );
                                        self.camera.position = self.galactic_position;
                                        self.drive.request_disengage();

                                        // Force star regen
                                        self.star_streaming.force_load(self.galactic_position);
                                        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                            let verts = self.star_streaming.build_vertices(self.galactic_position);
                                            renderer.star_field.update_star_buffer(&gpu.device, &verts);
                                        }

                                        // Load the system
                                        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
                                            let sys = solar_system::ActiveSystem::load(
                                                star.id,
                                                placed,
                                                &mut renderer.mesh_store,
                                                &gpu.device,
                                            );
                                            log::info!("System loaded: {} bodies — {}", sys.body_count(), description);
                                            for line in sys.body_summary() {
                                                log::info!("  {}", line);
                                            }
                                            self.active_system = Some(sys);
                                        }
                                    }
                                    self.navigation.clear_target();
                                } else {
                                    log::warn!("No nearby stars to jump to");
                                }
                            }
                            KeyCode::Equal => { self.fly_speed *= 2.0; log::info!("Fly speed: {:.0} ly/s", self.fly_speed); }
                            KeyCode::Minus => { self.fly_speed = (self.fly_speed / 2.0).max(1.0); log::info!("Fly speed: {:.0} ly/s", self.fly_speed); }
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == winit::event::MouseButton::Left {
                    self.input.mouse.set_left_pressed(state.is_pressed());
                }
                if state.is_pressed() && !self.cursor_grabbed {
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
                self.schedule
                    .run(&mut self.world, &mut self.events, &self.time);

                // --- Player + Physics + Helm ---
                let t0 = Instant::now();
                let dt = self.time.delta_seconds() as f32;

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
                            if self.drive.request_engage(sa_ship::DriveMode::Cruise) {
                                log::info!("Drive: CRUISE engaged");
                            }
                        }
                        // Tab: lock nearest star for navigation
                        if self.input.keyboard.just_pressed(KeyCode::Tab) {
                            if !self.navigation.nearby_stars.is_empty() {
                                self.navigation.lock_target(0);
                                if let Some(target) = &self.navigation.locked_target {
                                    log::info!("LOCKED: {} ({:.2} ly)", target.catalog_name, target.distance_ly);
                                }
                            } else {
                                log::warn!("No nearby stars found");
                            }
                        }
                        if self.input.keyboard.just_pressed(KeyCode::Digit3) {
                            if self.ship_resources.exotic_fuel > 0.0 {
                                if self.drive.request_engage(sa_ship::DriveMode::Warp) {
                                    log::info!("Drive: WARP spooling...");
                                    // Unload active system when entering warp
                                    if self.active_system.is_some() {
                                        self.active_system = None;
                                        log::info!("Left solar system — entering warp");
                                    }
                                }
                            } else {
                                log::warn!("Cannot engage warp: no exotic fuel");
                            }
                        }

                        // Update drive spool progress
                        self.drive.update(dt);

                        // Map throttle lever to drive speed when in cruise/warp
                        if self.drive.mode() != sa_ship::DriveMode::Impulse {
                            if let Some(ship) = &self.ship {
                                self.drive.set_speed_fraction(ship.throttle);
                            }
                        }

                        // Apply continuous thrust from throttle lever + engine button
                        ship.apply_thrust(&mut self.physics);

                        if wants_stand
                            && let Some(helm) = &mut self.helm
                        {
                            self.drive.request_disengage();
                            helm.stand_up();
                            // Re-enable player and teleport to ship's current position
                            // (ship may have moved while seated)
                            if let Some(player) = &self.player {
                                // Get ship state before mutable borrow
                                let ship_pos = ship.position(&self.physics);
                                let ship_vel = ship.speed_vector(&self.physics);
                                if let (Some((sx, sy, sz)), Some(body)) = (
                                    ship_pos,
                                    self.physics.get_body_mut(player.body_handle),
                                ) {
                                    body.set_enabled(true);
                                    // Stand up behind the helm seat in v2 cockpit
                                    // Close enough to interactables (max_range 2.5m)
                                    body.set_translation(
                                        nalgebra::Vector3::new(sx, sy - 0.1, sz + 2.8),
                                        true,
                                    );
                                    // Match ship velocity so player doesn't slide on stand-up
                                    body.set_linvel(nalgebra::Vector3::new(ship_vel.0, ship_vel.1, ship_vel.2), true);
                                }
                            }
                            log::info!("Left helm seated mode — player teleported to ship");
                        }
                    }

                    // Physics step
                    let physics_dt = dt.min(1.0 / 30.0);
                    if physics_dt > 0.0 {
                        self.physics.step(physics_dt);
                    }

                    // Update galactic position based on drive speed
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
                        let delta = drive_integration::galactic_position_delta(
                            &self.drive,
                            direction,
                            dt as f64,
                        );
                        self.galactic_position.x += delta[0];
                        self.galactic_position.y += delta[1];
                        self.galactic_position.z += delta[2];
                    }

                    // Gravity well auto-drop: only check during warp when no system is loaded
                    if self.drive.mode() == sa_ship::DriveMode::Warp
                        && matches!(self.drive.status(), sa_ship::DriveStatus::Engaged)
                        && self.active_system.is_none()
                    {
                        if let Some(nav_star) = self.navigation.check_gravity_well(self.galactic_position) {
                            // Drop out of warp
                            self.drive.request_disengage();
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
                                    self.active_system = Some(system);
                                }
                            }

                            // Clear navigation lock (prevent re-triggering)
                            self.navigation.clear_target();
                        }
                    }

                    // Interior colliders stay at LOCAL origin — ship-local collision
                    // handles the coordinate transform in PlayerController::update().

                    // Mouse -> free-look camera (independent of ship orientation)
                    let (dx, dy) = self.input.mouse.delta();
                    self.camera.rotate(dx * 0.003, -dy * 0.003);

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

                    // Manually integrate ship: apply thrust as velocity change
                    if let Some(ship) = &self.ship {
                        if let Some(body) = self.physics.get_body_mut(ship.body_handle) {
                            // Apply thrust: F = throttle * max_thrust * engine_on
                            if ship.engine_on && ship.throttle > 0.0 {
                                let rot = *body.rotation();
                                let forward = rot * nalgebra::Vector3::new(0.0, 0.0, -1.0);
                                let accel = forward * (ship.throttle * ship.max_thrust / body.mass());
                                let vel = body.linvel() + accel * physics_dt;
                                // Apply angular damping
                                let angvel = body.angvel() * (1.0 - 5.0 * physics_dt).max(0.0);
                                // Apply linear damping
                                let vel = vel * (1.0 - 0.01 * physics_dt).max(0.0);
                                body.set_linvel(vel, true);
                                body.set_angvel(angvel, true);
                            }
                            // Integrate position: p += v * dt
                            let vel = *body.linvel();
                            let pos = body.translation() + vel * physics_dt;
                            let angvel = *body.angvel();
                            let rot = *body.rotation();
                            // Integrate rotation (small angle approximation)
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
                    }
                    let phys_step_us = t_phys.elapsed().as_micros();

                    // Step 3: Get ship transform AFTER integration
                    let (ship_pos_after, ship_rot_after) = self.ship.as_ref()
                        .and_then(|s| self.physics.get_body(s.body_handle))
                        .map(|b| (b.translation().clone_owned(), *b.rotation()))
                        .unwrap_or((nalgebra::Vector3::zeros(), nalgebra::UnitQuaternion::identity()));

                    // Step 4: Carry player with ship.
                    // The player's world position is relative to the OLD ship position.
                    // Transform to local space (using old ship), then back to world (using new ship).
                    // This is an instant teleport — no collision sweep, no AABB cost.
                    // For a non-rotating ship: equivalent to adding ship displacement.
                    // For a rotating ship: also rotates the player around the ship origin.
                    let ship_rot_before_inv = ship_rot_before.inverse();
                    if let Some(player) = &self.player {
                        if let Some(body) = self.physics.get_body_mut(player.body_handle) {
                            let p = body.translation().clone_owned();
                            let local = ship_rot_before_inv * (p - ship_pos_before);
                            let carried = ship_pos_after + ship_rot_after * local;
                            body.set_translation(carried, true);
                        }
                    }

                    // Step 5: Sync collider positions + update query pipeline.
                    // Manual integration uses set_position() which doesn't sync
                    // child colliders (interactables). Must call this explicitly.
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
                        self.camera.position = player.position(&self.physics);
                        self.camera.yaw = player.yaw;
                        self.camera.pitch = player.pitch;
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
                    // When seated, use camera position/direction for raycasting
                    // (player can look around cockpit and click interactables).
                    // When standing, use player eye position/direction.
                    let (ray_origin, ray_dir) = if is_seated {
                        let pos = self.camera.position;
                        let fwd = self.camera.forward();
                        (
                            [pos.x as f32, pos.y as f32, pos.z as f32],
                            [fwd.x, fwd.y, fwd.z],
                        )
                    } else {
                        let eye_pos = player.position(&self.physics);
                        let fwd = player.forward();
                        (
                            [eye_pos.x as f32, eye_pos.y as f32, eye_pos.z as f32],
                            [fwd.x, fwd.y, fwd.z],
                        )
                    };

                    let (_, mouse_dy) = self.input.mouse.delta();

                    // Collision groups handle filtering: exclude_solids()
                    // limits to sensors, and only interactable sensors are
                    // registered in the collider_to_id map.
                    let helm_clicked = interaction.update(
                        ray_origin,
                        ray_dir,
                        mouse_dy,
                        self.input.mouse.left_just_pressed(),
                        self.input.mouse.left_pressed(),
                        self.input.mouse.left_just_released(),
                        &self.physics,
                    );

                    // Update debug ray visualization
                    let debug = interaction.debug_ray();
                    let max_range = 2.0_f32;
                    let end_dist = debug.hit
                        .map(|(_, toi)| toi.min(max_range))
                        .unwrap_or(max_range);
                    let end = [
                        ray_origin[0] + ray_dir[0] * end_dist,
                        ray_origin[1] + ray_dir[1] * end_dist,
                        ray_origin[2] + ray_dir[2] * end_dist,
                    ];
                    let color = if debug.hit_id.is_some() {
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
                        // Reset camera to face forward (-Z = toward cockpit nose)
                        self.camera.yaw = 0.0;
                        self.camera.pitch = 0.0;
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
                    if let Some(button) = interaction.get(ids.engine_button) {
                        if self.input.mouse.left_just_released() && interaction.hovered() == Some(ids.engine_button) {
                            let pressed = button.is_button_pressed().unwrap_or(false);
                            let mesh = sa_meshgen::interactables::button_mesh(pressed);
                            let mesh_data = meshgen_to_render(&mesh);
                            let handle = renderer.mesh_store.upload(&gpu.device, &mesh_data);
                            if let Some(slot) = self.interactable_meshes.get_mut(ids.engine_button) {
                                *slot = handle;
                            }
                        }
                    }
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
                        screen.set_screen_text(vec![
                            format!("Speed: {:.1} m/s", speed),
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

                // --- Render ---
                let t3 = Instant::now();
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
                        };

                        let ship_pos = self.ship.as_ref()
                            .and_then(|s| s.position(&self.physics))
                            .unwrap_or((0.0, 0.0, 0.0));
                        let contacts: Vec<ui::sensors_screen::SensorContact> = self.deposits.iter()
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
                            .collect();
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
                        let system_commands = system.update(
                            dt as f64,
                            self.galactic_position,
                        );
                        commands.extend(system_commands);
                    }

                    // Unload system if player has cruised far away (> 100 AU from star)
                    if let Some(system) = &self.active_system {
                        let dist = self.galactic_position.distance_to(system.star_galactic_pos);
                        let au_in_ly = 1.581e-5_f64;
                        if dist > 100.0 * au_in_ly {
                            log::info!("Left system boundary — unloading");
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
                            let hud_state = ui::HudState {
                                hovered_kind,
                                screen_width: gpu.config.width,
                                screen_height: gpu.config.height,
                                fuel: self.ship_resources.fuel,
                                oxygen: self.ship_resources.oxygen,
                                gather_available: self.nearest_gatherable.is_some(),
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
                            format!("HELM {:.1}m/s", speed)
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
