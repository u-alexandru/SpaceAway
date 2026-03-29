//! UI system: egui-based HUD overlay and in-world monitor rendering.

pub mod helm_screen;
pub mod hud;
pub mod sensors_screen;
pub mod visor;

use egui_wgpu::ScreenDescriptor;
use helm_screen::HelmData;
use sa_ship::InteractableKind;
use sensors_screen::SensorsData;

/// Monitor texture resolution (square).
const MONITOR_SIZE: u32 = 256;

/// State passed to the HUD each frame.
pub struct HudState {
    /// The kind of interactable the player is looking at, if any.
    pub hovered_kind: Option<InteractableKind>,
    /// Screen dimensions in physical pixels.
    pub screen_width: u32,
    pub screen_height: u32,
    /// Current fuel level (0.0 to 1.0).
    #[allow(dead_code)]
    pub fuel: f32,
    /// Current oxygen level (0.0 to 1.0).
    #[allow(dead_code)]
    pub oxygen: f32,
    /// Whether a gatherable deposit is within range.
    pub gather_available: bool,
    /// Suit oxygen (0.0 to 1.0).
    pub suit_o2: f32,
    /// Suit power (0.0 to 1.0).
    pub suit_power: f32,
    /// Whether the cursor is grabbed (gameplay mode).
    pub cursor_grabbed: bool,
    /// Target screen position (None = no target or behind camera).
    pub target_screen_pos: Option<[f32; 2]>,
    /// Off-screen target angle for edge chevron.
    pub target_off_screen_angle: Option<f32>,
    /// Target catalog name.
    pub target_name: Option<String>,
    /// Distance to target in light-years.
    pub target_distance_ly: Option<f64>,
    /// Time in seconds for animations.
    pub time: f32,
}

/// Reference height for UI scaling (designed at 1080p).
const REFERENCE_HEIGHT: f32 = 1080.0;

/// Manages egui context, egui-wgpu renderer, and all UI rendering.
pub struct UiSystem {
    // --- HUD ---
    egui_ctx: egui::Context,
    egui_renderer: egui_wgpu::Renderer,
    screen_width: u32,
    screen_height: u32,

    // --- Helm Monitor (offscreen) ---
    monitor_ctx: egui::Context,
    monitor_renderer: egui_wgpu::Renderer,
    /// Kept alive so the texture_view remains valid.
    #[allow(dead_code)]
    monitor_texture: wgpu::Texture,
    monitor_texture_view: wgpu::TextureView,

    // --- Sensors Monitor (offscreen) ---
    sensors_ctx: egui::Context,
    sensors_renderer: egui_wgpu::Renderer,
    #[allow(dead_code)]
    sensors_texture: wgpu::Texture,
    sensors_texture_view: wgpu::TextureView,
}

impl UiSystem {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let egui_ctx = egui::Context::default();

        // Load Orbitron font for visor HUD
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(font_data) = std::fs::read("resources/fonts/Orbitron-Regular.ttf") {
            fonts.font_data.insert(
                "orbitron".to_string(),
                egui::FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Name("orbitron".into()))
                .or_default()
                .push("orbitron".to_string());
            log::info!("Loaded Orbitron font for visor HUD");
        } else {
            log::warn!("Orbitron font not found — using default");
        }
        egui_ctx.set_fonts(fonts);

        let egui_renderer = egui_wgpu::Renderer::new(
            device,
            surface_format,
            None,  // no depth format
            1,     // msaa samples
            false, // dithering
        );

        // Monitor uses a separate egui context and renderer targeting an offscreen texture.
        let monitor_ctx = egui::Context::default();
        let monitor_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let monitor_renderer = egui_wgpu::Renderer::new(
            device,
            monitor_format,
            None,
            1,
            false,
        );

        let (monitor_texture, monitor_texture_view) = Self::create_monitor_texture(device, "Helm Monitor Texture");

        // Sensors monitor: separate context, renderer, and texture
        let sensors_ctx = egui::Context::default();
        let sensors_renderer = egui_wgpu::Renderer::new(
            device,
            monitor_format,
            None,
            1,
            false,
        );
        let (sensors_texture, sensors_texture_view) = Self::create_monitor_texture(device, "Sensors Monitor Texture");

        Self {
            egui_ctx,
            egui_renderer,
            screen_width: width,
            screen_height: height,
            monitor_ctx,
            monitor_renderer,
            monitor_texture,
            monitor_texture_view,
            sensors_ctx,
            sensors_renderer,
            sensors_texture,
            sensors_texture_view,
        }
    }

    fn create_monitor_texture(device: &wgpu::Device, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: MONITOR_SIZE,
                height: MONITOR_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// UI scale factor for font sizes. Designed at 1080p physical pixels.
    /// On Retina (2560×1440 physical for 1280×720 logical): 1440/1080 = 1.33.
    /// Fonts are multiplied by this, keeping pixels_per_point=1.0 for sharpness.
    pub fn font_scale(&self) -> f32 {
        (self.screen_height as f32 / REFERENCE_HEIGHT).max(0.5)
    }

    /// Get the helm monitor texture view for binding in the screen pipeline.
    pub fn helm_texture_view(&self) -> &wgpu::TextureView {
        &self.monitor_texture_view
    }

    /// Get the sensors monitor texture view for binding in the screen pipeline.
    pub fn sensors_texture_view(&self) -> &wgpu::TextureView {
        &self.sensors_texture_view
    }

    /// Render the helm monitor UI to the offscreen texture.
    pub fn render_helm_monitor(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        helm_data: &HelmData,
    ) {
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [MONITOR_SIZE, MONITOR_SIZE],
            pixels_per_point: 1.0,
        };

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(MONITOR_SIZE as f32, MONITOR_SIZE as f32),
            )),
            ..Default::default()
        };

        let full_output = self.monitor_ctx.run(raw_input, |ctx| {
            helm_screen::draw_helm_screen(ctx, helm_data);
        });

        let paint_jobs = self
            .monitor_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // Upload textures (font atlas etc.)
        for (id, delta) in &full_output.textures_delta.set {
            self.monitor_renderer
                .update_texture(device, queue, *id, delta);
        }

        // Update buffers
        let cmd_buffers = self.monitor_renderer.update_buffers(
            device,
            queue,
            encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Render to offscreen texture
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Helm Monitor Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.monitor_texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            let mut pass = pass.forget_lifetime();
            self.monitor_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        if !cmd_buffers.is_empty() {
            queue.submit(cmd_buffers);
        }

        for id in &full_output.textures_delta.free {
            self.monitor_renderer.free_texture(id);
        }
    }

    /// Render the sensors monitor UI to the offscreen texture.
    pub fn render_sensors_monitor(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        sensors_data: &SensorsData,
    ) {
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [MONITOR_SIZE, MONITOR_SIZE],
            pixels_per_point: 1.0,
        };

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(MONITOR_SIZE as f32, MONITOR_SIZE as f32),
            )),
            ..Default::default()
        };

        let full_output = self.sensors_ctx.run(raw_input, |ctx| {
            sensors_screen::draw_sensors_screen(ctx, sensors_data);
        });

        let paint_jobs = self
            .sensors_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        for (id, delta) in &full_output.textures_delta.set {
            self.sensors_renderer
                .update_texture(device, queue, *id, delta);
        }

        let cmd_buffers = self.sensors_renderer.update_buffers(
            device,
            queue,
            encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Sensors Monitor Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.sensors_texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.03,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            let mut pass = pass.forget_lifetime();
            self.sensors_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        if !cmd_buffers.is_empty() {
            queue.submit(cmd_buffers);
        }

        for id in &full_output.textures_delta.free {
            self.sensors_renderer.free_texture(id);
        }
    }

    /// Run egui layouts and render the HUD overlay into the given render pass.
    pub fn render_hud(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        hud_state: &HudState,
    ) {
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.screen_width, self.screen_height],
            pixels_per_point: 1.0, // render at physical pixels for sharpness
        };

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.screen_width as f32, self.screen_height as f32),
            )),
            ..Default::default()
        };

        let font_scale = self.font_scale();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let visor_state = visor::VisorState {
                screen_width: hud_state.screen_width as f32,
                screen_height: hud_state.screen_height as f32,
                font_scale,
                cursor_grabbed: hud_state.cursor_grabbed,
                hovered_kind: hud_state.hovered_kind.clone(),
                suit_o2: hud_state.suit_o2,
                suit_power: hud_state.suit_power,
                target_screen_pos: hud_state.target_screen_pos,
                target_off_screen_angle: hud_state.target_off_screen_angle,
                target_name: hud_state.target_name.clone(),
                target_distance_ly: hud_state.target_distance_ly,
                time: hud_state.time,
                gather_available: hud_state.gather_available,
            };
            visor::draw_visor(ctx, &visor_state);
        });

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // Upload textures
        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(device, queue, *id, delta);
        }

        // Update buffers
        let cmd_buffers = self.egui_renderer.update_buffers(
            device,
            queue,
            encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // Render egui in a new render pass (overlay, no depth, load existing content)
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui HUD Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            let mut pass = pass.forget_lifetime();
            self.egui_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        // Submit any extra command buffers from update_buffers
        if !cmd_buffers.is_empty() {
            queue.submit(cmd_buffers);
        }

        // Free textures
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }

    /// Render the main menu overlay. Returns true if "Continue" was clicked.
    #[allow(clippy::too_many_arguments)]
    pub fn render_menu(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        menu: &mut crate::menu::MainMenu,
        mouse_pos: Option<[f32; 2]>,
        mouse_clicked: bool,
    ) -> bool {
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.screen_width, self.screen_height],
            pixels_per_point: 1.0,
        };

        let mut raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.screen_width as f32, self.screen_height as f32),
            )),
            ..Default::default()
        };

        // Pass mouse position and clicks to egui for menu interaction
        if let Some([mx, my]) = mouse_pos {
            raw_input.events.push(egui::Event::PointerMoved(egui::pos2(mx, my)));
        }
        if mouse_clicked
            && let Some([mx, my]) = mouse_pos
        {
            raw_input.events.push(egui::Event::PointerButton {
                pos: egui::pos2(mx, my),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
            // Also send release immediately for click detection
            raw_input.events.push(egui::Event::PointerButton {
                pos: egui::pos2(mx, my),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
        }

        let mut start_game = false;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            start_game = menu.render_egui(ctx, self.font_scale());
        });

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(device, queue, *id, delta);
        }

        let cmd_buffers = self.egui_renderer.update_buffers(
            device,
            queue,
            encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui Menu Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            let mut pass = pass.forget_lifetime();
            self.egui_renderer
                .render(&mut pass, &paint_jobs, &screen_descriptor);
        }

        if !cmd_buffers.is_empty() {
            queue.submit(cmd_buffers);
        }

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        start_game
    }
}
