pub mod approach;
pub mod constants;
mod debug_state;
mod drive_integration;
mod frame_update;
mod game_helpers;
mod game_systems;
#[allow(clippy::too_many_lines)]
mod helm_mode;
mod input_handler;
mod landing;
mod menu;
mod menu_render;
mod mesh_utils;
mod navigation;
#[allow(clippy::too_many_lines)]
mod render_frame;
mod walk_mode;
mod ship_setup;
mod sky;
mod solar_system;
mod star_streaming;
mod terrain_integration;
mod ui;

use spaceaway::ship_colliders;
use spaceaway::terrain_colliders;

use sa_core::{EventBus, FrameTime};
use sa_ecs::{GameWorld, Schedule};
use sa_input::InputState;
use sa_math::WorldPos;
use sa_physics::PhysicsWorld;
use sa_player::PlayerController;
use sa_render::{
    Camera, GpuContext, Renderer,
    ScreenQuad,
};
use sa_ship::helm::HelmController;
use sa_ship::interaction::InteractionSystem;
use sa_ship::ship::Ship;
use sa_survival::{ResourceDeposit, ShipResources, SuitResources};
use sa_universe::Universe;
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
    /// Unified planet approach state machine.
    approach: crate::approach::ApproachManager,
    /// Last computed approach state (shared with helm_mode).
    pub approach_state: Option<crate::approach::ApproachState>,
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init_window(event_loop);
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

                if self.render_menu_frame(dt) {
                    return;
                }

                // --- PLAYING PHASE ---
                profiling::scope!("playing_frame");

                self.schedule
                    .run(&mut self.world, &mut self.events, &self.time);

                // --- Player + Physics + Helm + Game Systems ---
                let t0 = Instant::now();
                self.update_approach_state();
                self.update_player_physics(dt);
                self.update_game_systems(dt);
                self.perf.player_us = t0.elapsed().as_micros() as u64;

                self.render_playing_frame(dt, frame_start);


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
