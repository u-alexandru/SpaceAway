//! UI system: egui-based HUD overlay and in-world monitor rendering.

pub mod helm_screen;
pub mod hud;

use egui_wgpu::ScreenDescriptor;
use helm_screen::HelmData;
use sa_ship::InteractableKind;

/// Monitor texture resolution (square).
const MONITOR_SIZE: u32 = 256;

/// State passed to the HUD each frame.
pub struct HudState {
    /// The kind of interactable the player is looking at, if any.
    pub hovered_kind: Option<InteractableKind>,
    /// Screen dimensions in physical pixels.
    pub screen_width: u32,
    pub screen_height: u32,
}

/// Manages egui context, egui-wgpu renderer, and all UI rendering.
pub struct UiSystem {
    // --- HUD ---
    egui_ctx: egui::Context,
    egui_renderer: egui_wgpu::Renderer,
    screen_width: u32,
    screen_height: u32,

    // --- Monitor (offscreen) ---
    monitor_ctx: egui::Context,
    monitor_renderer: egui_wgpu::Renderer,
    /// Kept alive so the texture_view remains valid.
    #[allow(dead_code)]
    monitor_texture: wgpu::Texture,
    monitor_texture_view: wgpu::TextureView,
}

impl UiSystem {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let egui_ctx = egui::Context::default();
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

        let (monitor_texture, monitor_texture_view) = Self::create_monitor_texture(device);

        Self {
            egui_ctx,
            egui_renderer,
            screen_width: width,
            screen_height: height,
            monitor_ctx,
            monitor_renderer,
            monitor_texture,
            monitor_texture_view,
        }
    }

    fn create_monitor_texture(device: &wgpu::Device) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Helm Monitor Texture"),
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

    /// Get the monitor texture view for binding in the screen pipeline.
    pub fn helm_texture_view(&self) -> &wgpu::TextureView {
        &self.monitor_texture_view
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
            pixels_per_point: 1.0,
        };

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.screen_width as f32, self.screen_height as f32),
            )),
            ..Default::default()
        };

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            hud::draw_hud(ctx, hud_state);
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
}
