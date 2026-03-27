mod ship_setup;

use glam::{Mat4, Vec3};
use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use sa_player::PlayerController;
use sa_render::{
    Camera, DrawCommand, GpuContext, MeshData, NebulaInstance, Renderer, StarVertex, Vertex,
};
use sa_ship::helm::HelmController;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_universe::{MasterSeed, Universe, VisibleStar};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::KeyCode;
use winit::window::{CursorGrabMode, Window, WindowId};

/// Convert visible stars from the universe query into GPU-ready vertices.
///
/// `VisibleStar::relative_pos` is in light-years relative to the observer.
/// We normalise each direction onto the unit sphere so that stars render as a
/// sky-dome, and use `brightness` / `color` directly from the universe data.
fn visible_stars_to_vertices(stars: &[VisibleStar]) -> Vec<StarVertex> {
    stars
        .iter()
        .map(|vs| {
            let [dx, dy, dz] = vs.relative_pos;
            let len = (dx * dx + dy * dy + dz * dz).sqrt();
            let (nx, ny, nz) = if len > 1e-6 {
                (dx / len, dy / len, dz / len)
            } else {
                (0.0, 1.0, 0.0)
            };
            StarVertex {
                position: [nx, ny, nz],
                brightness: vs.brightness,
                color: vs.color,
                _pad: 0.0,
            }
        })
        .collect()
}

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
const STAR_REGEN_THRESHOLD: f64 = 500.0;

/// Number of sectors to query in each direction around the observer.
const STAR_QUERY_RADIUS: i32 = 4;
/// Minimum star brightness to render. Culls dim stars that are visually
/// indistinguishable, reducing vertex count by ~60%.
const STAR_MIN_BRIGHTNESS: f32 = 0.32;

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
    last_star_gen_pos: WorldPos,
    stars_initialised: bool,
    nebulae: Vec<sa_universe::Nebula>,
    distant_galaxies: Vec<sa_universe::DistantGalaxy>,
    perf: PerfTimings,
    perf_update_timer: f64,
    teleport_counter: u64,
    fly_mode: bool,
    fly_speed: f64, // light-years per second
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
}

impl App {
    fn new() -> Self {
        // Zero gravity for space
        let mut physics = PhysicsWorld::with_gravity(0.0, 0.0, 0.0);

        // Create ship and interactables
        let (ship, interaction, ids) =
            ship_setup::create_ship_and_interactables(&mut physics);

        // Helm controller: viewpoint is cockpit seat position + eye height
        let helm = HelmController::new(glam::Vec3::new(0.0, 0.3, 1.5));

        // Spawn player inside the cockpit (near the helm seat)
        // Player gets simulated gravity via force each frame
        let player = PlayerController::spawn(&mut physics, 0.0, 0.0, 2.5);

        let camera = Camera::new();
        let universe = Universe::new(MasterSeed(42));

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
            universe,
            last_star_gen_pos: WorldPos::ORIGIN,
            stars_initialised: false,
            nebulae: Vec::new(),
            distant_galaxies: Vec::new(),
            perf: PerfTimings::default(),
            perf_update_timer: 0.0,
            teleport_counter: 0,
            fly_mode: false,
            fly_speed: 5.0,
            ship_part_mesh: None,
            ship_part_index: 0,
            view_mode: 0,
            ship: Some(ship),
            interaction: Some(interaction),
            ship_ids: Some(ids),
            helm: Some(helm),
            interactable_meshes: Vec::new(),
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
    }

    /// Rebuild the GPU star buffer from the procedural universe if the observer
    /// has moved more than `STAR_REGEN_THRESHOLD` since the last generation,
    /// or if stars have never been generated yet.
    fn maybe_regenerate_stars(&mut self) {
        let observer = self.camera.position;
        let dist = observer.distance_to(self.last_star_gen_pos);

        // In fly mode use a lower threshold so stars update as you fly
        let threshold = if self.fly_mode { 100.0 } else { STAR_REGEN_THRESHOLD };
        let needs_regen = !self.stars_initialised || dist > threshold;
        if !needs_regen {
            return;
        }

        let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) else {
            return;
        };

        let visible = self.universe.visible_stars_filtered(
            observer,
            STAR_QUERY_RADIUS,
            STAR_MIN_BRIGHTNESS,
        );
        let vertices = visible_stars_to_vertices(&visible);
        renderer.star_field.update_star_buffer(&gpu.device, &vertices);

        // Also refresh nebulae in fly mode (positions change at galaxy scale)
        if self.fly_mode {
            let nebula_instances = nebulae_to_instances(&self.nebulae, observer);
            renderer.nebula_renderer.update_instances(&gpu.device, &nebula_instances);
        }

        self.last_star_gen_pos = observer;
        self.stars_initialised = true;

        log::debug!(
            "Regenerated star field: {} stars at ({:.1}, {:.1}, {:.1})",
            visible.len(),
            observer.x,
            observer.y,
            observer.z,
        );
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

        self.camera.position = WorldPos::new(x, y, z);

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

        // Force star regeneration
        self.stars_initialised = false;

        // Regenerate stars and nebulae immediately for the new position
        if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
            let start = Instant::now();

            // Regen stars
            let visible = self.universe.visible_stars_filtered(
                self.camera.position,
                STAR_QUERY_RADIUS,
                STAR_MIN_BRIGHTNESS,
            );
            let vertices = visible_stars_to_vertices(&visible);
            renderer.star_field.update_star_buffer(&gpu.device, &vertices);

            // Refresh nebula instances (only on teleport, not on star regen)
            let nebula_instances = nebulae_to_instances(&self.nebulae, self.camera.position);
            renderer.nebula_renderer.update_instances(&gpu.device, &nebula_instances);

            self.last_star_gen_pos = self.camera.position;
            self.stars_initialised = true;

            log::info!(
                "Regen complete in {:.0}ms ({} stars)",
                start.elapsed().as_secs_f64() * 1000.0,
                visible.len(),
            );
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
            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
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

                    // Teleport keys: each forces a specific viewpoint type
                    if event.state.is_pressed() {
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
                    // Fly mode: move camera directly in light-years, bypass physics
                    let (dx, dy) = self.input.mouse.delta();
                    self.camera.rotate(dx * 0.003, -dy * 0.003);

                    let fwd = self.camera.forward();
                    let right = self.camera.right();
                    let speed = self.fly_speed * dt as f64;

                    use winit::keyboard::KeyCode as KC;
                    if self.input.keyboard.is_pressed(KC::KeyW) {
                        self.camera.position.x += fwd.x as f64 * speed;
                        self.camera.position.y += fwd.y as f64 * speed;
                        self.camera.position.z += fwd.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::KeyS) {
                        self.camera.position.x -= fwd.x as f64 * speed;
                        self.camera.position.y -= fwd.y as f64 * speed;
                        self.camera.position.z -= fwd.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::KeyA) {
                        self.camera.position.x -= right.x as f64 * speed;
                        self.camera.position.z -= right.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::KeyD) {
                        self.camera.position.x += right.x as f64 * speed;
                        self.camera.position.z += right.z as f64 * speed;
                    }
                    if self.input.keyboard.is_pressed(KC::Space) {
                        self.camera.position.y += speed;
                    }
                    if self.input.keyboard.is_pressed(KC::ShiftLeft) {
                        self.camera.position.y -= speed;
                    }
                } else if self.helm.as_ref().map(|h| h.is_seated()).unwrap_or(false) {
                    // --- Seated at helm: control ship ---
                    if let Some(ship) = &self.ship {
                        // Reset forces each frame so only current input applies
                        ship.reset_forces(&mut self.physics);

                        let wants_stand = if let Some(helm) = &self.helm {
                            helm.update_seated(ship, &mut self.physics, &self.input, dt)
                        } else {
                            false
                        };

                        if wants_stand
                            && let Some(helm) = &mut self.helm
                        {
                            helm.stand_up();
                            // Re-enable player physics body
                            if let Some(player) = &self.player
                                && let Some(body) = self.physics.get_body_mut(player.body_handle)
                            {
                                body.set_enabled(true);
                            }
                            log::info!("Left helm seated mode");
                        }
                    }

                    // Physics step
                    let physics_dt = dt.min(1.0 / 30.0);
                    if physics_dt > 0.0 {
                        self.physics.step(physics_dt);
                    }

                    // Camera follows ship orientation
                    if let (Some(helm), Some(ship)) = (&self.helm, &self.ship) {
                        if let Some((cx, cy, cz)) = helm.camera_position(&self.physics, ship) {
                            self.camera.position = WorldPos::new(cx as f64, cy as f64, cz as f64);
                        }
                        if let Some((yaw, pitch)) = helm.camera_orientation(&self.physics, ship) {
                            self.camera.yaw = yaw;
                            self.camera.pitch = pitch;
                        }
                    }
                } else {
                    // --- Walk mode: physics-driven ---
                    if let Some(player) = &mut self.player {
                        player.update(&mut self.physics, &self.input, dt);
                    }

                    // Simulated mag-boot gravity: ~1g on a ~80kg player = ~785 N downward
                    if let Some(player) = &self.player
                        && let Some(body) = self.physics.get_body_mut(player.body_handle)
                    {
                        body.add_force(nalgebra::Vector3::new(0.0, -785.0, 0.0), true);
                    }

                    let physics_dt = dt.min(1.0 / 30.0);
                    if physics_dt > 0.0 {
                        self.physics.step(physics_dt);
                    }

                    if let Some(player) = &self.player {
                        self.camera.position = player.position(&self.physics);
                        self.camera.yaw = player.yaw;
                        self.camera.pitch = player.pitch;

                        // Debug: log player physics state every 60 frames
                        if self.time.frame_count() % 60 == 0 {
                            if let Some(body) = self.physics.get_body(player.body_handle) {
                                let pos = body.translation();
                                let vel = body.linvel();
                                log::info!(
                                    "[DEBUG] Player pos=({:.2},{:.2},{:.2}) vel=({:.2},{:.2},{:.2}) sleeping={}",
                                    pos.x, pos.y, pos.z, vel.x, vel.y, vel.z, body.is_sleeping()
                                );
                            }
                            if let Some(ship) = &self.ship {
                                if let Some(body) = self.physics.get_body(ship.body_handle) {
                                    let pos = body.translation();
                                    log::info!(
                                        "[DEBUG] Ship pos=({:.2},{:.2},{:.2}) colliders={}",
                                        pos.x, pos.y, pos.z,
                                        self.physics.collider_set.len()
                                    );
                                }
                            }
                        }
                    }
                }

                // Always update query pipeline after physics step (needed for raycasting)
                self.physics.update_query_pipeline();

                // --- Interaction ---
                if let (Some(interaction), Some(player)) = (&mut self.interaction, &self.player) {
                    let is_standing = self.helm.as_ref()
                        .map(|h| !h.is_seated())
                        .unwrap_or(true);

                    if is_standing && !self.fly_mode {
                        let eye_pos = player.position(&self.physics);
                        let fwd = player.forward();
                        let ray_origin = [eye_pos.x as f32, eye_pos.y as f32, eye_pos.z as f32];
                        let ray_dir = [fwd.x, fwd.y, fwd.z];

                        let (_, mouse_dy) = self.input.mouse.delta();

                        let helm_clicked = interaction.update(
                            ray_origin,
                            ray_dir,
                            mouse_dy,
                            self.input.mouse.left_just_pressed(),
                            self.input.mouse.left_pressed(),
                            self.input.mouse.left_just_released(),
                            &self.physics,
                        );

                        // If helm seat was clicked, enter seated mode
                        if helm_clicked.is_some()
                            && let Some(helm) = &mut self.helm
                        {
                            helm.sit_down();
                            // Disable player physics body while seated
                            if let Some(body) = self.physics.get_body_mut(player.body_handle) {
                                body.set_enabled(false);
                            }
                            log::info!("Entered helm seated mode");
                        }
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

                self.perf.player_us = t0.elapsed().as_micros() as u64;
                self.perf.physics_us = 0;

                // --- Star regen ---
                let t2 = Instant::now();
                self.maybe_regenerate_stars();
                self.perf.stars_us = t2.elapsed().as_micros() as u64;

                // --- Render ---
                let t3 = Instant::now();
                if let (Some(gpu), Some(renderer)) = (&self.gpu, &self.renderer) {
                    let commands = if self.view_mode == 6 || self.view_mode == 7 {
                        // Debug: ship part viewing at origin
                        if let Some(ship_mesh) = self.ship_part_mesh {
                            vec![DrawCommand {
                                mesh: ship_mesh,
                                model_matrix: Mat4::IDENTITY,
                            }]
                        } else {
                            vec![]
                        }
                    } else {
                        // Normal gameplay: render ship hull + interactables
                        let mut cmds = Vec::new();

                        // Ship hull
                        if let Some(hull_handle) = self.ship_part_mesh {
                            cmds.push(DrawCommand {
                                mesh: hull_handle,
                                model_matrix: Mat4::IDENTITY,
                            });
                        }

                        // Interactable meshes at their positions
                        let layout = sa_ship::station::cockpit_layout();
                        for (i, handle) in self.interactable_meshes.iter().enumerate() {
                            if let Some(placement) = layout.interactables.get(i) {
                                let pos = placement.position;
                                cmds.push(DrawCommand {
                                    mesh: *handle,
                                    model_matrix: Mat4::from_translation(pos),
                                });
                            }
                        }

                        cmds
                    };
                    self.perf.draw_calls = commands.len() as u32;
                    self.perf.star_count = renderer.star_field.star_count;
                    renderer.render_frame(
                        gpu,
                        &self.camera,
                        &commands,
                        Vec3::new(0.5, -0.8, -0.3),
                    );
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
                        window.set_title(&format!(
                            "SpaceAway | {:.0} FPS | {} | frame {:.1}ms | player {:.1}ms | stars {:.1}ms ({}) | render {:.1}ms | draws {}",
                            self.perf.fps,
                            helm_status,
                            self.perf.total_us as f64 / 1000.0,
                            self.perf.player_us as f64 / 1000.0,
                            self.perf.stars_us as f64 / 1000.0,
                            self.perf.star_count,
                            self.perf.render_us as f64 / 1000.0,
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

/// All ship parts for visual cycling.
fn all_ship_parts() -> Vec<(&'static str, sa_meshgen::Mesh)> {
    use sa_meshgen::ship_parts::*;
    vec![
        ("cockpit", hull_cockpit().mesh),
        ("corridor", hull_corridor(3.0).mesh),
        ("transition_4_5", hull_transition(4.0, 5.0, 1.0).mesh),
        ("nav_room", hull_room("nav", sa_meshgen::colors::ACCENT_NAVIGATION, &[]).mesh),
        ("eng_room", hull_room("eng", sa_meshgen::colors::ACCENT_ENGINEERING, &[]).mesh),
        ("transition_5_35", hull_transition(5.0, 3.5, 1.0).mesh),
        ("engine_section", hull_engine_section().mesh),
        ("airlock", hull_airlock().mesh),
    ]
}

/// Build the full ship using the modular assembly system.
///
/// Layout:
/// cockpit(4.0) -> corridor(4.0) -> transition(4.0->5.0) -> nav_room(5.0)
/// -> transition(5.0->4.0) -> corridor(4.0) -> transition(4.0->5.0)
/// -> eng_room(5.0) -> transition(5.0->3.5) -> engine(3.5)
fn assemble_ship() -> sa_meshgen::Mesh {
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
