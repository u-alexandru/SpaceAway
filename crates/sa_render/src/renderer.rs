use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::mesh::{MeshMarker, MeshStore};
use crate::pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
use crate::star_field::{StarField, StarUniforms};
use glam::{Mat4, Vec3};
use sa_core::Handle;
use wgpu::util::DeviceExt;

pub struct DrawCommand {
    pub mesh: Handle<MeshMarker>,
    pub model_matrix: Mat4,
}

pub struct Renderer {
    pub geometry_pipeline: GeometryPipeline,
    pub star_field: StarField,
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
        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);
        Self {
            geometry_pipeline,
            star_field,
            mesh_store: MeshStore::new(),
        }
    }

    pub fn resize(&mut self, gpu: &GpuContext) {
        self.geometry_pipeline
            .resize(&gpu.device, gpu.config.width, gpu.config.height);
    }

    pub fn render_frame(
        &self,
        gpu: &GpuContext,
        camera: &Camera,
        draw_commands: &[DrawCommand],
        light_dir: Vec3,
    ) {
        let aspect = gpu.aspect_ratio();
        let view_proj = camera.view_projection_matrix(aspect);

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

        let star_view = camera.view_matrix();
        let star_vp = camera.projection_matrix(aspect) * star_view;
        let star_uniforms = StarUniforms {
            view_proj: star_vp.to_cols_array_2d(),
        };
        gpu.queue.write_buffer(
            &self.star_field.uniform_buffer,
            0,
            bytemuck::bytes_of(&star_uniforms),
        );

        let frame = match gpu.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
                return;
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

            // Draw geometry
            if !draw_commands.is_empty() {
                pass.set_pipeline(&self.geometry_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                for cmd in draw_commands {
                    if let Some(mesh) = self.mesh_store.get(cmd.mesh) {
                        let instance = InstanceRaw {
                            model: cmd.model_matrix.to_cols_array_2d(),
                        };
                        let instance_buffer =
                            gpu.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Instance Buffer"),
                                    contents: bytemuck::bytes_of(&instance),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, instance_buffer.slice(..));
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                    }
                }
            }

            // Draw stars
            pass.set_pipeline(&self.star_field.pipeline);
            pass.set_bind_group(0, &self.star_field.bind_group, &[]);
            pass.set_vertex_buffer(0, self.star_field.vertex_buffer.slice(..));
            pass.draw(0..self.star_field.star_count, 0..1);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
