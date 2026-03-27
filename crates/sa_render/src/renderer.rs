use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::mesh::{MeshMarker, MeshStore};
use crate::nebula::{NebulaRenderer, NebulaUniforms};
use crate::pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
use crate::sky::{MilkyWayCubemap, SkyRenderer, SkyUniforms};
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
    pub sky_renderer: SkyRenderer,
    pub milky_way_cubemap: MilkyWayCubemap,
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
        let milky_way_cubemap = MilkyWayCubemap::placeholder(&gpu.device, &gpu.queue);
        let sky_renderer = SkyRenderer::new(&gpu.device, gpu.config.format, &milky_way_cubemap);
        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);
        let nebula_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        let galaxy_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        Self {
            geometry_pipeline,
            sky_renderer,
            milky_way_cubemap,
            star_field,
            nebula_renderer,
            galaxy_renderer,
            mesh_store: MeshStore::new(),
        }
    }

    /// Upload a new Milky Way cubemap and rebuild the sky bind group.
    pub fn update_milky_way_cubemap(&mut self, gpu: &GpuContext, faces: &[Vec<u8>]) {
        self.milky_way_cubemap = MilkyWayCubemap::new(&gpu.device, &gpu.queue, faces);
        self.sky_renderer
            .rebuild_bind_group(&gpu.device, &self.milky_way_cubemap);
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
            core_brightness: 0.8,
            cubemap_enabled: 1,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
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
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        gpu.queue.write_buffer(
            &self.star_field.uniform_buffer,
            0,
            bytemuck::bytes_of(&star_uniforms),
        );

        // Nebula uniforms: view_proj + camera right/up for billboarding
        let view_mat = camera.view_matrix();
        let camera_right = Vec3::new(view_mat.col(0).x, view_mat.col(1).x, view_mat.col(2).x);
        let camera_up = Vec3::new(view_mat.col(0).y, view_mat.col(1).y, view_mat.col(2).y);
        let nebula_uniforms = NebulaUniforms {
            view_proj: view_proj.to_cols_array_2d(),
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

            // Draw sky (before geometry, no depth)
            pass.set_pipeline(&self.sky_renderer.pipeline);
            pass.set_bind_group(0, &self.sky_renderer.bind_group, &[]);
            pass.draw(0..6, 0..1);

            // Draw geometry
            if !draw_commands.is_empty() {
                pass.set_pipeline(&self.geometry_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                for cmd in draw_commands {
                    if let Some(mesh) = self.mesh_store.get(cmd.mesh) {
                        // Origin rebasing: subtract camera world position from
                        // model translation so geometry is camera-relative (f32).
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
            // 6 vertices per star (2 triangles = billboard quad), instanced per star
            pass.draw(0..6, 0..self.star_field.star_count);

            // Draw nebulae (after stars, alpha blended, no depth write)
            self.nebula_renderer.render(&mut pass);

            // Draw distant galaxies (same pipeline as nebulae, smaller/dimmer)
            self.galaxy_renderer.render(&mut pass);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
