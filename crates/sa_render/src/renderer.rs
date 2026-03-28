use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::mesh::{MeshMarker, MeshStore};
use crate::nebula::{NebulaRenderer, NebulaUniforms};
use crate::pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
use crate::screen_pipeline::{ScreenInstanceRaw, ScreenPipeline, ScreenQuad};
use crate::sky::{SkyRenderer, SkyUniforms};
use crate::star_field::{StarField, StarUniforms};
use glam::{Mat4, Vec3};
use sa_core::Handle;
use wgpu::util::DeviceExt;

pub struct DrawCommand {
    pub mesh: Handle<MeshMarker>,
    pub model_matrix: Mat4,
}

/// A draw command for a textured screen quad.
pub struct ScreenDrawCommand<'a> {
    pub quad: &'a ScreenQuad,
    pub model_matrix: Mat4,
    pub texture_bind_group: &'a wgpu::BindGroup,
}

/// Holds the in-progress frame state between render_frame and submit_frame.
/// This allows the caller to run additional render passes (e.g. egui HUD)
/// before submitting.
pub struct FrameContext {
    pub encoder: wgpu::CommandEncoder,
    pub frame: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

pub struct Renderer {
    pub geometry_pipeline: GeometryPipeline,
    pub screen_pipeline: ScreenPipeline,
    pub sky_renderer: SkyRenderer,
    pub star_field: StarField,
    pub nebula_renderer: NebulaRenderer,
    pub galaxy_renderer: NebulaRenderer,
    pub mesh_store: MeshStore,
}

impl Renderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let geometry_pipeline = GeometryPipeline::new(
            &gpu.device,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
        );
        let sky_renderer = SkyRenderer::new(&gpu.device, gpu.config.format);
        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);
        let screen_pipeline = ScreenPipeline::new(
            &gpu.device,
            gpu.config.format,
            &geometry_pipeline.bind_group_layout,
        );
        let nebula_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        let galaxy_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        Self {
            geometry_pipeline,
            screen_pipeline,
            sky_renderer,
            star_field,
            nebula_renderer,
            galaxy_renderer,
            mesh_store: MeshStore::new(),
        }
    }

    pub fn resize(&mut self, gpu: &GpuContext) {
        self.geometry_pipeline
            .resize(&gpu.device, gpu.config.width, gpu.config.height);
    }

    /// Submit a frame after all render passes are complete.
    pub fn submit_frame(gpu: &GpuContext, ctx: FrameContext) {
        gpu.queue.submit(std::iter::once(ctx.encoder.finish()));
        ctx.frame.present();
    }

    pub fn render_frame(
        &self,
        gpu: &GpuContext,
        camera: &Camera,
        draw_commands: &[DrawCommand],
        screen_draws: &[ScreenDrawCommand<'_>],
        light_dir: Vec3,
    ) -> Option<FrameContext> {
        let aspect = gpu.aspect_ratio();
        let view_proj = camera.view_projection_matrix(aspect);

        // Camera world position for origin rebasing
        let cam_pos = camera.position;

        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: light_dir.normalize().to_array(),
            _pad: 0.0,
            light_color: [1.0, 0.95, 0.9],
            _pad2: 0.0,
            ambient: [0.02, 0.02, 0.03],
            _pad3: 0.0,
        };
        gpu.queue.write_buffer(
            &self.geometry_pipeline.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        // Sky uniforms: inverse view-projection for reconstructing view direction
        let inv_view_proj = view_proj.inverse();
        // Galactic center direction from camera: center is at origin, so direction is -cam_pos
        let gc_dir = Vec3::new(
            -(cam_pos.x as f32),
            -(cam_pos.y as f32),
            -(cam_pos.z as f32),
        );
        let gc_dir = if gc_dir.length_squared() > 0.0 {
            gc_dir.normalize()
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };
        let sky_uniforms = SkyUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            galactic_center_dir: gc_dir.to_array(),
            core_brightness: 0.35,
            observer_pos: [cam_pos.x as f32, cam_pos.y as f32, cam_pos.z as f32],
            _pad: 0.0,
        };
        gpu.queue.write_buffer(
            &self.sky_renderer.uniform_buffer,
            0,
            bytemuck::bytes_of(&sky_uniforms),
        );

        let star_view = camera.view_matrix();
        let star_vp = camera.projection_matrix(aspect) * star_view;
        let star_uniforms = StarUniforms {
            view_proj: star_vp.to_cols_array_2d(),
            screen_height: gpu.config.height as f32,
            screen_width: gpu.config.width as f32,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        gpu.queue.write_buffer(
            &self.star_field.uniform_buffer,
            0,
            bytemuck::bytes_of(&star_uniforms),
        );

        // Nebula uniforms: use the star view_proj (rotation-only, no translation)
        // because nebulae are at galaxy scale (light-years), effectively at infinity.
        // The game binary places nebula instances as direction * large_distance.
        let view_mat = camera.view_matrix();
        let camera_right = Vec3::new(view_mat.col(0).x, view_mat.col(1).x, view_mat.col(2).x);
        let camera_up = Vec3::new(view_mat.col(0).y, view_mat.col(1).y, view_mat.col(2).y);
        let nebula_uniforms = NebulaUniforms {
            view_proj: star_vp.to_cols_array_2d(),
            camera_right: camera_right.to_array(),
            _pad0: 0.0,
            camera_up: camera_up.to_array(),
            _pad1: 0.0,
        };
        gpu.queue.write_buffer(
            &self.nebula_renderer.uniform_buffer,
            0,
            bytemuck::bytes_of(&nebula_uniforms),
        );
        gpu.queue.write_buffer(
            &self.galaxy_renderer.uniform_buffer,
            0,
            bytemuck::bytes_of(&nebula_uniforms),
        );

        let frame = match gpu.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return None;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return None;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            gpu.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Frame Encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.005,
                            g: 0.005,
                            b: 0.015,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.geometry_pipeline.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // Draw sky (before geometry, no depth)
            pass.set_pipeline(&self.sky_renderer.pipeline);
            pass.set_bind_group(0, &self.sky_renderer.bind_group, &[]);
            pass.draw(0..6, 0..1);

            // Draw geometry — collect instance buffers up-front so they
            // live as long as the render pass (the pass borrows their slices).
            let instance_buffers: Vec<wgpu::Buffer> = draw_commands
                .iter()
                .filter_map(|cmd| {
                    self.mesh_store.get(cmd.mesh)?;
                    let col3 = cmd.model_matrix.col(3);
                    let rebased_translation = Vec3::new(
                        (col3.x as f64 - cam_pos.x) as f32,
                        (col3.y as f64 - cam_pos.y) as f32,
                        (col3.z as f64 - cam_pos.z) as f32,
                    );
                    let mut rebased_model = cmd.model_matrix;
                    rebased_model.col_mut(3).x = rebased_translation.x;
                    rebased_model.col_mut(3).y = rebased_translation.y;
                    rebased_model.col_mut(3).z = rebased_translation.z;

                    let instance = InstanceRaw {
                        model: rebased_model.to_cols_array_2d(),
                    };
                    Some(
                        gpu.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Instance Buffer"),
                                contents: bytemuck::bytes_of(&instance),
                                usage: wgpu::BufferUsages::VERTEX,
                            }),
                    )
                })
                .collect();

            if !draw_commands.is_empty() {
                pass.set_pipeline(&self.geometry_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                let mut buf_idx = 0;
                for cmd in draw_commands {
                    if let Some(mesh) = self.mesh_store.get(cmd.mesh) {
                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, instance_buffers[buf_idx].slice(..));
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                        buf_idx += 1;
                    }
                }
            }

            // Draw screen quads (textured monitors, after geometry, same depth buffer)
            // Pre-create instance buffers so they outlive the render pass.
            let screen_instance_buffers: Vec<wgpu::Buffer> = screen_draws
                .iter()
                .map(|screen_cmd| {
                    let col3 = screen_cmd.model_matrix.col(3);
                    let rebased_translation = Vec3::new(
                        (col3.x as f64 - cam_pos.x) as f32,
                        (col3.y as f64 - cam_pos.y) as f32,
                        (col3.z as f64 - cam_pos.z) as f32,
                    );
                    let mut rebased_model = screen_cmd.model_matrix;
                    rebased_model.col_mut(3).x = rebased_translation.x;
                    rebased_model.col_mut(3).y = rebased_translation.y;
                    rebased_model.col_mut(3).z = rebased_translation.z;

                    let instance = ScreenInstanceRaw {
                        model: rebased_model.to_cols_array_2d(),
                    };
                    gpu.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Screen Instance Buffer"),
                            contents: bytemuck::bytes_of(&instance),
                            usage: wgpu::BufferUsages::VERTEX,
                        })
                })
                .collect();

            if !screen_draws.is_empty() {
                pass.set_pipeline(&self.screen_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                for (i, screen_cmd) in screen_draws.iter().enumerate() {
                    pass.set_bind_group(1, screen_cmd.texture_bind_group, &[]);
                    pass.set_vertex_buffer(0, screen_cmd.quad.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, screen_instance_buffers[i].slice(..));
                    pass.set_index_buffer(
                        screen_cmd.quad.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed(0..screen_cmd.quad.index_count, 0, 0..1);
                }
            }

            // Draw stars
            pass.set_pipeline(&self.star_field.pipeline);
            pass.set_bind_group(0, &self.star_field.bind_group, &[]);
            pass.set_vertex_buffer(0, self.star_field.vertex_buffer.slice(..));
            // 6 vertices per star (2 triangles = billboard quad), instanced per star
            pass.draw(0..6, 0..self.star_field.star_count);

            // Draw nebulae (after stars, alpha blended, no depth write)
            self.nebula_renderer.render(&mut pass);

            // Draw distant galaxies (same pipeline as nebulae, smaller/dimmer)
            self.galaxy_renderer.render(&mut pass);
        }

        Some(FrameContext {
            encoder,
            frame,
            view,
        })
    }
}
