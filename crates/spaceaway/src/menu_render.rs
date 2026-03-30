use super::App;
use crate::menu;
use crate::GamePhase;
use glam::Vec3;
use sa_math::WorldPos;
use sa_render::{Camera, ScreenDrawCommand};

impl App {
    /// Render the menu phase frame. Returns true if the menu consumed the frame.
    pub(super) fn render_menu_frame(&mut self, dt: f32) -> bool {
        if self.phase != GamePhase::Menu {
            return false;
        }

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
                &[],  // no terrain draws in menu
                &screen_draws,
                Vec3::new(0.5, -0.8, -0.3),
                menu_pos,
                &drive_params,
            ) {
                // Render menu egui overlay
                if let Some(ui_sys) = &mut self.ui_system {
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
        true
    }
}
