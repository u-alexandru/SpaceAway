//! Main menu: random celestial background, atmospheric text, music.

use sa_core::Handle;
use sa_math::WorldPos;
use sa_render::{planet_mesh, Camera, DrawCommand, MeshMarker, MeshStore};
use sa_universe::{generate_star, generate_system, PlanetType, Rng64};

use glam::{Mat4, Vec3};

const QUOTES: &[&str] = &[
    "The universe is vast and indifferent.",
    "Every star is a sun. Every sun, a world.",
    "In space, no one can hear you wonder.",
    "The cold between stars is patient.",
    "You are small. The universe does not care.",
    "Mystery is the only compass.",
    "The silence out here has weight.",
    "Every light in the sky is a place you could go.",
    "Fear and wonder are the same thing, out here.",
    "The void doesn't end. Neither does curiosity.",
];

/// What celestial object is shown as the menu background.
enum MenuScene {
    /// Galaxy panorama -- just sky + stars, camera in galactic disc
    GalaxyPanorama { galactic_pos: WorldPos },
    /// Deep space -- far from disc, minimal stars
    DeepSpace { galactic_pos: WorldPos },
    /// Planet close-up
    Planet {
        galactic_pos: WorldPos,
        mesh: Handle<MeshMarker>,
        offset_m: [f32; 3],
    },
    /// Star close-up
    Star {
        galactic_pos: WorldPos,
        mesh: Handle<MeshMarker>,
        offset_m: [f32; 3],
    },
}

pub struct MainMenu {
    scene: MenuScene,
    camera: Camera,
    quote: &'static str,
    time: f32,
}

impl MainMenu {
    /// Generate a random menu scene from a time-based seed.
    pub fn new(mesh_store: &mut MeshStore, device: &wgpu::Device) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut rng = Rng64::new(seed);

        let quote_idx = (rng.next_u64() as usize) % QUOTES.len();
        let quote = QUOTES[quote_idx];

        let roll = rng.next_f32();
        let (scene, camera) = if roll < 0.20 {
            Self::make_galaxy_scene(&mut rng)
        } else if roll < 0.30 {
            Self::make_deep_space_scene(&mut rng)
        } else if roll < 0.75 {
            Self::make_planet_scene(&mut rng, mesh_store, device)
        } else if roll < 0.85 {
            Self::make_star_scene(&mut rng, mesh_store, device)
        } else {
            Self::make_galaxy_scene(&mut rng)
        };

        Self {
            scene,
            camera,
            quote,
            time: 0.0,
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

        if let Some(planet) = system.planets.first() {
            let radius_m = planet.radius_earth as f64 * 6_371_000.0;
            let mesh = match planet.planet_type {
                PlanetType::Rocky => planet_mesh::build_rocky_planet_mesh(
                    4,
                    radius_m as f32,
                    planet.sub_type,
                    planet.color_seed,
                ),
                _ => planet_mesh::build_gas_giant_mesh(
                    4,
                    radius_m as f32,
                    planet.sub_type,
                    planet.color_seed,
                ),
            };
            let handle = mesh_store.upload(device, &mesh);

            let view_dist = radius_m * 2.0;
            let offset = [
                -(view_dist as f32),
                (radius_m * 0.3) as f32,
                0.0,
            ];

            let mut cam = Camera::new();
            cam.position = WorldPos::ORIGIN;
            cam.yaw = -std::f32::consts::FRAC_PI_2;
            cam.pitch = -0.15;

            (
                MenuScene::Planet {
                    galactic_pos: WorldPos::ORIGIN,
                    mesh: handle,
                    offset_m: offset,
                },
                cam,
            )
        } else {
            Self::make_galaxy_scene(rng)
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

        let mesh = planet_mesh::build_star_mesh(
            3,
            radius_m as f32,
            star.color,
            star_seed,
        );
        let handle = mesh_store.upload(device, &mesh);

        let view_dist = radius_m * 3.0;
        let offset = [-(view_dist as f32), (radius_m * 0.2) as f32, 0.0];

        let mut cam = Camera::new();
        cam.position = WorldPos::ORIGIN;
        cam.yaw = -std::f32::consts::FRAC_PI_2;
        cam.pitch = -0.05;

        (
            MenuScene::Star {
                galactic_pos: WorldPos::ORIGIN,
                mesh: handle,
                offset_m: offset,
            },
            cam,
        )
    }

    /// Update camera drift.
    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.camera.yaw += 0.002 * dt;
    }

    /// Get the camera for rendering.
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Get the galactic position for sky/star rendering.
    pub fn galactic_position(&self) -> WorldPos {
        match &self.scene {
            MenuScene::GalaxyPanorama { galactic_pos }
            | MenuScene::DeepSpace { galactic_pos }
            | MenuScene::Planet { galactic_pos, .. }
            | MenuScene::Star { galactic_pos, .. } => *galactic_pos,
        }
    }

    /// Get draw commands for any 3D objects in the scene.
    pub fn draw_commands(&self) -> Vec<DrawCommand> {
        match &self.scene {
            MenuScene::Planet { mesh, offset_m, .. }
            | MenuScene::Star { mesh, offset_m, .. } => {
                vec![DrawCommand {
                    mesh: *mesh,
                    model_matrix: Mat4::from_translation(Vec3::from(*offset_m)),
                    pre_rebased: true,
                }]
            }
            _ => vec![],
        }
    }

    /// Render the menu overlay via egui. Returns true if "Continue" clicked.
    pub fn render_egui(&self, ctx: &egui::Context, font_scale: f32) -> bool {
        let mut start_game = false;
        let a = |frac: f32| -> u8 { ((self.time * 0.5).min(1.0) * 255.0 * frac) as u8 };
        let s = font_scale; // scale all sizes

        // Semi-transparent dark overlay
        egui::Area::new(egui::Id::new("menu_bg"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    ctx.screen_rect(), 0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 5, 80),
                );
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();

                // Title + menu in center
                ui.vertical_centered(|ui| {
                    ui.add_space(screen.height() * 0.28);
                    ui.label(egui::RichText::new("S  P  A  C  E  A  W  A  Y")
                        .color(egui::Color32::from_rgba_unmultiplied(200, 205, 215, a(1.0)))
                        .size(56.0 * s).strong());
                    ui.add_space(80.0 * s);

                    let btn = egui::RichText::new("Continue")
                        .color(egui::Color32::from_rgba_unmultiplied(180, 185, 200, a(1.0)))
                        .size(30.0 * s);
                    if ui.add(egui::Label::new(btn).sense(egui::Sense::click())).clicked() {
                        start_game = true;
                    }
                    ui.add_space(20.0 * s);
                    ui.label(egui::RichText::new("New Game")
                        .color(egui::Color32::from_rgba_unmultiplied(80, 80, 90, a(0.35)))
                        .size(30.0 * s));
                    ui.add_space(20.0 * s);
                    ui.label(egui::RichText::new("Settings")
                        .color(egui::Color32::from_rgba_unmultiplied(80, 80, 90, a(0.35)))
                        .size(30.0 * s));
                });

                // Quote pinned to bottom center
                egui::Area::new(egui::Id::new("menu_quote"))
                    .fixed_pos(egui::pos2(screen.center().x - 200.0 * s, screen.bottom() - 50.0 * s))
                    .show(ctx, |ui| {
                        ui.label(egui::RichText::new(self.quote)
                            .color(egui::Color32::from_rgba_unmultiplied(120, 125, 140, a(0.6)))
                            .size(16.0 * s).italics());
                    });
            });
        start_game
    }
}
