//! UI system: egui-based HUD overlay and in-world monitor rendering.

pub mod hud;

use egui_wgpu::ScreenDescriptor;
use sa_ship::InteractableKind;

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
    egui_ctx: egui::Context,
    egui_renderer: egui_wgpu::Renderer,
    screen_width: u32,
    screen_height: u32,
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
        Self {
            egui_ctx,
            egui_renderer,
            screen_width: width,
            screen_height: height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
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
