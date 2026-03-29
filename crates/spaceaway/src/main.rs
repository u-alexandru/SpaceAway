mod debug_state;
mod drive_integration;
mod frame_update;
mod input_handler;
mod landing;
mod menu;
mod mesh_utils;
mod navigation;
mod ship_setup;
mod sky;
mod solar_system;
mod star_streaming;
mod terrain_integration;
mod ui;

use spaceaway::ship_colliders;
use spaceaway::terrain_colliders;

use glam::{Mat4, Vec3};
use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use sa_player::PlayerController;
use sa_render::{
    Camera, DrawCommand, GpuContext, Renderer, ScreenDrawCommand,
    ScreenQuad,
};
use sa_ship::helm::HelmController;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_survival::{ResourceDeposit, ShipResources, SuitResources, generate_deposits};
use sa_universe::{MasterSeed, Universe};
use std::collections::HashSet;
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

use mesh_utils::{make_cube, meshgen_to_render};
use sky::{nebulae_to_instances, distant_galaxies_to_instances};

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
    /// Whether puffin profiling is active (toggled with F3).
    show_profiler: bool,
    /// Puffin HTTP server (streams profiler data to puffin_viewer on port 8585).
    #[allow(dead_code)]
    puffin_server: Option<puffin_http::Server>,
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
            show_profiler: false,
            puffin_server: None,
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

    fn write_debug_state(&self) {
        debug_state::write_debug_state(&debug_state::DebugSnapshot {
            frame_count: self.time.frame_count(),
            view_mode: self.view_mode,
            helm: self.helm.as_ref(),
            fly_mode: self.fly_mode,
            cursor_grabbed: self.cursor_grabbed,
            player: self.player.as_ref(),
            ship: self.ship.as_ref(),
            interaction: self.interaction.as_ref(),
            camera: &self.camera,
            input: &self.input,
            physics: &self.physics,
            perf_total_us: self.perf.total_us,
            perf_player_us: self.perf.player_us,
            perf_physics_us: self.perf.physics_us,
            perf_stars_us: self.perf.stars_us,
            perf_render_us: self.perf.render_us,
            perf_fps: self.perf.fps,
        });
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

                    self.handle_keyboard(code, event.state.is_pressed());
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
                puffin::GlobalProfiler::lock().new_frame();

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

                            renderer.submit_frame(gpu, frame_ctx);
                        }
                    }

                    self.input.end_frame();
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }

                // --- PLAYING PHASE ---
                profiling::scope!("playing_frame");

                self.schedule
                    .run(&mut self.world, &mut self.events, &self.time);

                // --- Player + Physics + Helm ---
                let t0 = Instant::now();

                self.update_player_physics(dt);


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
                profiling::scope!("render");
                let t3 = Instant::now();

                // --- Terrain streaming (before immutable renderer borrow) ---
                profiling::scope!("terrain_update");
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

                if let (Some(gpu), Some(renderer)) = (&self.gpu, &mut self.renderer) {
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
                            // Only show altitude when raycasts actually hit
                            // terrain (< 100m). At 100m (MAX_RAY_DIST) the rays
                            // missed — show SURF distance instead.
                            // Show raycast altitude only when actually hitting terrain
                            // (not the default 100m max). The surface barrier can be
                            // hit at distances close to 100m, so filter generously.
                            altitude_m: self.last_clearance.filter(|&c| c < 95.0),
                            planet_dist_km: {
                                // Show distance to nearest planet surface when < 1000km
                                // and heading toward it (dot product check).
                                let ly_to_m = 9.461e15_f64;
                                let mut best: Option<f32> = None;
                                // Always show when terrain is active (any altitude).
                                // In a system without terrain, show within 50,000km.
                                if let Some(terrain_mgr) = &self.terrain {
                                    let cam_rel = terrain_mgr.cam_rel_m(self.galactic_position);
                                    let dist_m = (cam_rel[0]*cam_rel[0] + cam_rel[1]*cam_rel[1] + cam_rel[2]*cam_rel[2]).sqrt();
                                    let alt_km = ((dist_m - terrain_mgr.planet_radius_m()) / 1000.0) as f32;
                                    best = Some(alt_km.max(0.0));
                                } else if let Some(sys) = &self.active_system {
                                    let positions = sys.compute_positions_ly_pub();
                                    for (i, pos) in positions.iter().enumerate() {
                                        if let Some(r_m) = sys.body_radius_m(i)
                                            && sys.planet_data(i).is_some()
                                        {
                                            let dx = (self.galactic_position.x - pos.x) * ly_to_m;
                                            let dy = (self.galactic_position.y - pos.y) * ly_to_m;
                                            let dz = (self.galactic_position.z - pos.z) * ly_to_m;
                                            let dist_m = (dx*dx + dy*dy + dz*dz).sqrt();
                                            let alt_km = ((dist_m - r_m) / 1000.0) as f32;
                                            if (0.0..50_000.0).contains(&alt_km) && best.is_none_or(|b| alt_km < b) {
                                                best = Some(alt_km);
                                            }
                                        }
                                    }
                                }
                                best
                            },
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
                            let mesh_info = renderer.mesh_store.get(cmd.mesh)
                                .map(|m| m.index_count)
                                .unwrap_or(0);
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
                        renderer.submit_frame(gpu, frame_ctx);
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

fn main() {
    env_logger::init();
    log::info!("SpaceAway starting...");
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
