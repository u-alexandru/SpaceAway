//! Main menu: random celestial background, atmospheric text, music.

mod menu_scenes;

use sa_math::WorldPos;
use sa_render::{Camera, DrawCommand, MeshStore};

use glam::{Mat4, Vec3};

pub use menu_scenes::MenuScene;
use menu_scenes::generate_scene;

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

/// Camera drift speed in radians/second (0.5 degrees/s per spec).
const CAMERA_DRIFT_SPEED: f32 = 0.00873;

const MENU_CONTINUE: usize = 0;
const MENU_QUIT: usize = 3;
const MENU_ITEM_COUNT: usize = 4;

pub struct MainMenu {
    scene: MenuScene,
    camera: Camera,
    quote: &'static str,
    time: f32,
    pub selected: usize,
}

/// What the menu wants the game to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Continue,
    Quit,
}

impl MainMenu {
    pub fn new(mesh_store: &mut MeshStore, device: &wgpu::Device) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut rng = sa_universe::Rng64::new(seed);

        let quote_idx = (rng.next_u64() as usize) % QUOTES.len();
        let (scene, camera) = generate_scene(&mut rng, mesh_store, device);

        Self {
            scene,
            camera,
            quote: QUOTES[quote_idx],
            time: 0.0,
            selected: 0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.camera.yaw += CAMERA_DRIFT_SPEED * dt;
    }

    pub fn nav_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn nav_down(&mut self) {
        if self.selected < MENU_ITEM_COUNT - 1 {
            self.selected += 1;
        }
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn galactic_position(&self) -> WorldPos {
        self.scene.galactic_position()
    }

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
            MenuScene::GasGiant {
                meshes, offsets, ..
            }
            | MenuScene::BinaryStar {
                meshes, offsets, ..
            } => meshes
                .iter()
                .zip(offsets.iter())
                .map(|(mesh, off)| DrawCommand {
                    mesh: *mesh,
                    model_matrix: Mat4::from_translation(Vec3::from(*off)),
                    pre_rebased: true,
                })
                .collect(),
            _ => vec![],
        }
    }

    /// Render the menu overlay. Returns action when a menu item is activated.
    pub fn render_egui(&mut self, ctx: &egui::Context, font_scale: f32) -> Option<MenuAction> {
        let mut action = None;
        let fade = (self.time * 0.5).min(1.0);
        let s = font_scale;

        // Semi-transparent dark overlay
        egui::Area::new(egui::Id::new("menu_bg"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.painter().rect_filled(
                    ctx.screen_rect(),
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(0, 0, 5, (80.0 * fade) as u8),
                );
            });

        let items: &[(&str, bool)] = &[
            ("Continue", true),
            ("New Game", false),
            ("Settings", false),
            ("Quit", true),
        ];

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();

                ui.vertical_centered(|ui| {
                    ui.add_space(screen.height() * 0.25);

                    // Title with Orbitron font
                    ui.label(
                        egui::RichText::new("S  P  A  C  E  A  W  A  Y")
                            .family(egui::FontFamily::Name("orbitron".into()))
                            .color(egui::Color32::from_rgba_unmultiplied(
                                210, 215, 225, (255.0 * fade) as u8,
                            ))
                            .size(72.0 * s)
                            .strong(),
                    );
                    ui.add_space(100.0 * s);

                    for (i, (label, enabled)) in items.iter().enumerate() {
                        let selected = i == self.selected;
                        let response = self.draw_menu_item(ui, label, *enabled, selected, fade, s);

                        if response.hovered() && *enabled {
                            self.selected = i;
                        }
                        if response.clicked() && *enabled {
                            match i {
                                MENU_CONTINUE => action = Some(MenuAction::Continue),
                                MENU_QUIT => action = Some(MenuAction::Quit),
                                _ => {}
                            }
                        }
                        ui.add_space(24.0 * s);
                    }
                });

                // Quote at bottom — delayed fade-in, properly centered
                let quote_fade = ((self.time - 1.0).max(0.0) * 0.4).min(1.0);
                egui::TopBottomPanel::bottom("menu_quote_panel")
                    .frame(egui::Frame::NONE)
                    .show_inside(ui, |ui| {
                        ui.add_space(8.0 * s);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(self.quote)
                                    .color(egui::Color32::from_rgba_unmultiplied(
                                        120,
                                        125,
                                        140,
                                        (155.0 * quote_fade) as u8,
                                    ))
                                    .size(18.0 * s)
                                    .italics(),
                            );
                        });
                        ui.add_space(40.0 * s);
                    });
            });

        action
    }

    fn draw_menu_item(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        enabled: bool,
        selected: bool,
        fade: f32,
        s: f32,
    ) -> egui::Response {
        let text_color = if !enabled {
            egui::Color32::from_rgba_unmultiplied(60, 60, 70, (90.0 * fade) as u8)
        } else if selected {
            egui::Color32::from_rgba_unmultiplied(240, 245, 255, (255.0 * fade) as u8)
        } else {
            egui::Color32::from_rgba_unmultiplied(150, 155, 170, (230.0 * fade) as u8)
        };

        let rt = egui::RichText::new(label).color(text_color).size(36.0 * s);
        let response = ui.add(egui::Label::new(rt).sense(egui::Sense::click()));

        // Subtle highlight background behind selected/hovered item
        if selected && enabled {
            let rect = response.rect.expand2(egui::vec2(40.0 * s, 6.0 * s));
            ui.painter().rect_filled(
                rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, (18.0 * fade) as u8),
            );
        }

        response
    }
}
