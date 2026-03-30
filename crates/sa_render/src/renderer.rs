use crate::camera::Camera;
use crate::gpu::GpuContext;
use crate::mesh::{MeshMarker, MeshStore};
use crate::nebula::{NebulaRenderer, NebulaUniforms};
use crate::pipeline::{GeometryPipeline, InstanceRaw, Uniforms};
use crate::screen_pipeline::{ScreenInstanceRaw, ScreenPipeline, ScreenQuad};
use crate::slab_allocator::TerrainSlab;
use crate::sky::{SkyRenderer, SkyUniforms};
use crate::star_field::{StarField, StarUniforms};
use crate::terrain_pipeline::{TerrainInstanceRaw, TerrainPipeline};
use glam::{Mat4, Vec3};
use sa_core::Handle;
use wgpu::util::DeviceExt;
use wgpu_profiler::{GpuProfiler, GpuProfilerSettings};

pub struct DrawCommand {
    pub mesh: Handle<MeshMarker>,
    pub model_matrix: Mat4,
    /// If true, model_matrix is already in camera-relative coordinates.
    /// The renderer skips origin rebasing for this command.
    /// Used by the solar system manager (planet positions pre-computed in f64).
    pub pre_rebased: bool,
}

/// A draw command for a terrain chunk rendered via the slab allocator.
pub struct TerrainDrawCommand {
    pub slab_slot: u32,
    pub model_matrix: Mat4,
    pub morph_factor: f32,
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

/// Drive visual parameters for shader effects. Passed as plain floats
/// so the renderer has no dependency on the drive system.
#[derive(Default)]
pub struct DriveRenderParams {
    pub velocity_dir: [f32; 3],
    pub beta: f32,
    pub streak_factor: f32,
    pub warp_intensity: f32,
    pub flash_intensity: f32,
}

/// Initial capacity for the persistent instance buffer (number of instances).
const INITIAL_INSTANCE_CAPACITY: u64 = 1024;

pub struct Renderer {
    pub geometry_pipeline: GeometryPipeline,
    pub screen_pipeline: ScreenPipeline,
    pub sky_renderer: SkyRenderer,
    pub star_field: StarField,
    pub nebula_renderer: NebulaRenderer,
    pub galaxy_renderer: NebulaRenderer,
    pub mesh_store: MeshStore,
    /// Persistent instance buffer, reused each frame to avoid per-draw allocations.
    instance_buffer: wgpu::Buffer,
    /// Current capacity of the instance buffer in number of instances.
    instance_buffer_capacity: u64,
    /// GPU profiler for render pass timing (optional — needs TIMESTAMP_QUERY).
    pub gpu_profiler: GpuProfiler,
    /// Latest GPU timing results (label, duration_ms) from the previous frame.
    pub gpu_timings: Vec<(String, f64)>,
    /// Terrain-specific render pipeline with morph support.
    pub terrain_pipeline: TerrainPipeline,
    /// Budget-driven vertex buffer pool for terrain chunks.
    pub terrain_slab: TerrainSlab,
    /// Shared index buffer for all terrain chunks (identical topology).
    terrain_index_buffer: wgpu::Buffer,
    /// Number of indices in the shared terrain index buffer.
    terrain_index_count: u32,
    /// Per-instance data buffer for terrain draw calls.
    terrain_instance_buffer: wgpu::Buffer,
    /// Current capacity of the terrain instance buffer (in instances).
    terrain_instance_capacity: u64,
}

impl Renderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let geometry_pipeline = GeometryPipeline::new(
            &gpu.device,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
        );
        let sky_renderer = SkyRenderer::new(
            &gpu.device,
            gpu.config.format,
            gpu.config.width,
            gpu.config.height,
        );
        let stars = crate::star_field::generate_stars(4000, 42);
        let star_field = StarField::new(&gpu.device, gpu.config.format, &stars);
        let screen_pipeline = ScreenPipeline::new(
            &gpu.device,
            gpu.config.format,
            &geometry_pipeline.bind_group_layout,
        );
        let nebula_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        let galaxy_renderer = NebulaRenderer::new(&gpu.device, gpu.config.format);
        let instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Persistent Instance Buffer"),
            size: INITIAL_INSTANCE_CAPACITY * std::mem::size_of::<InstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Only enable timer queries if the GPU supports them (Metal may not).
        // Timer queries disabled at startup — they cause flickering on Metal
        // by inserting GPU timestamp writes inside render passes that interfere
        // with buffer synchronization. Enable on demand via toggle_gpu_profiler().
        // Debug groups are safe and useful for Metal GPU Capture.
        let gpu_profiler = GpuProfiler::new(&gpu.device, GpuProfilerSettings {
            enable_timer_queries: false,
            enable_debug_groups: true,
            max_num_pending_frames: 4,
        })
        .expect("Failed to create GPU profiler");

        // Terrain pipeline and slab allocator
        let terrain_pipeline = TerrainPipeline::new(
            &gpu.device,
            gpu.config.format,
            &geometry_pipeline.bind_group_layout,
        );

        let terrain_indices = sa_terrain::chunk::shared_indices();
        let terrain_index_buffer = gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Terrain Shared Index Buffer"),
                contents: bytemuck::cast_slice(terrain_indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        );
        let terrain_index_count = terrain_indices.len() as u32;

        let slot_size = sa_terrain::config::VERTS_PER_HEIGHTMAP_CHUNK * 48;
        let terrain_slab = TerrainSlab::new(
            &gpu.device,
            sa_terrain::config::HEIGHTMAP_BUDGET_BYTES,
            slot_size,
        );

        const INITIAL_TERRAIN_INSTANCE_CAP: u64 = 512;
        let terrain_instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Terrain Instance Buffer"),
            size: INITIAL_TERRAIN_INSTANCE_CAP
                * std::mem::size_of::<TerrainInstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            geometry_pipeline,
            screen_pipeline,
            sky_renderer,
            star_field,
            nebula_renderer,
            galaxy_renderer,
            mesh_store: MeshStore::new(),
            instance_buffer,
            instance_buffer_capacity: INITIAL_INSTANCE_CAPACITY,
            gpu_profiler,
            gpu_timings: Vec::new(),
            terrain_pipeline,
            terrain_slab,
            terrain_index_buffer,
            terrain_index_count,
            terrain_instance_buffer,
            terrain_instance_capacity: INITIAL_TERRAIN_INSTANCE_CAP,
        }
    }

    pub fn resize(&mut self, gpu: &GpuContext) {
        self.geometry_pipeline
            .resize(&gpu.device, gpu.config.width, gpu.config.height);
        self.sky_renderer
            .resize(&gpu.device, gpu.config.width, gpu.config.height);
    }

    /// Submit a frame after all render passes are complete.
    pub fn submit_frame(&mut self, gpu: &GpuContext, ctx: FrameContext) {
        gpu.queue.submit(std::iter::once(ctx.encoder.finish()));
        ctx.frame.present();

        // End the GPU profiler frame and collect results.
        if let Err(e) = self.gpu_profiler.end_frame() {
            log::warn!("GPU profiler end_frame error: {e:?}");
        }
        let ts_period = gpu.queue.get_timestamp_period();
        if let Some(results) = self.gpu_profiler.process_finished_frame(ts_period) {
            self.gpu_timings.clear();
            fn flatten(
                out: &mut Vec<(String, f64)>,
                results: &[wgpu_profiler::GpuTimerQueryResult],
            ) {
                for r in results {
                    if let Some(ref time) = r.time {
                        let ms = (time.end - time.start) * 1000.0;
                        out.push((r.label.to_string(), ms));
                    }
                    flatten(out, &r.nested_queries);
                }
            }
            flatten(&mut self.gpu_timings, &results);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_frame(
        &mut self,
        gpu: &GpuContext,
        camera: &Camera,
        draw_commands: &[DrawCommand],
        terrain_draws: &[TerrainDrawCommand],
        screen_draws: &[ScreenDrawCommand<'_>],
        light_dir: Vec3,
        galactic_pos: sa_math::WorldPos,
        drive_params: &DriveRenderParams,
    ) -> Option<FrameContext> {
        let aspect = gpu.aspect_ratio();
        let view_proj = camera.view_projection_matrix(aspect);

        // Camera world position for origin rebasing (physics meters)
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

        // Sky uniforms: inverse view-projection for reconstructing view direction.
        // Uses galactic_pos (light-years) for galaxy density ray-marching,
        // NOT cam_pos (which is in physics meters).
        let inv_view_proj = view_proj.inverse();
        let gc_dir = Vec3::new(
            -(galactic_pos.x as f32),
            -(galactic_pos.y as f32),
            -(galactic_pos.z as f32),
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
            observer_pos: [galactic_pos.x as f32, galactic_pos.y as f32, galactic_pos.z as f32],
            warp_intensity: drive_params.warp_intensity,
            warp_dir: drive_params.velocity_dir,
            flash_intensity: drive_params.flash_intensity,
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
            beta: drive_params.beta,
            streak_factor: drive_params.streak_factor,
            velocity_dir: drive_params.velocity_dir,
            flash_intensity: drive_params.flash_intensity,
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
            streak_factor: drive_params.streak_factor,
            velocity_dir: drive_params.velocity_dir,
            warp_intensity: drive_params.warp_intensity,
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
        // Pre-compute batched instances and write to GPU BEFORE any render pass.
        // Writing to a buffer during an active render pass can cause flickering
        // on some backends (Metal) because the GPU may read stale data.
        struct Batch {
            mesh_id: u64,
            start: usize,
            count: usize,
        }
        let mut batches: Vec<Batch> = Vec::new();
        if !draw_commands.is_empty() {
            let mut entries: Vec<(u64, InstanceRaw)> = draw_commands
                .iter()
                .filter(|cmd| self.mesh_store.get(cmd.mesh).is_some())
                .map(|cmd| {
                    let rebased_model = if cmd.pre_rebased {
                        cmd.model_matrix
                    } else {
                        let col3 = cmd.model_matrix.col(3);
                        let rebased_translation = Vec3::new(
                            (col3.x as f64 - cam_pos.x) as f32,
                            (col3.y as f64 - cam_pos.y) as f32,
                            (col3.z as f64 - cam_pos.z) as f32,
                        );
                        let mut m = cmd.model_matrix;
                        m.col_mut(3).x = rebased_translation.x;
                        m.col_mut(3).y = rebased_translation.y;
                        m.col_mut(3).z = rebased_translation.z;
                        m
                    };
                    (cmd.mesh.id(), InstanceRaw {
                        model: rebased_model.to_cols_array_2d(),
                    })
                })
                .collect();
            entries.sort_by_key(|(mesh_id, _)| *mesh_id);

            // Grow instance buffer if needed.
            let needed = entries.len() as u64;
            if needed > self.instance_buffer_capacity {
                let new_cap = needed.next_power_of_two();
                self.instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Persistent Instance Buffer"),
                    size: new_cap * std::mem::size_of::<InstanceRaw>() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.instance_buffer_capacity = new_cap;
            }

            // Write all instances in one call BEFORE render passes.
            let instance_data: Vec<InstanceRaw> = entries.iter()
                .map(|(_, inst)| *inst)
                .collect();
            gpu.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&instance_data),
            );

            // Build batch descriptors for the draw loop.
            let mut batch_start = 0usize;
            while batch_start < entries.len() {
                let mesh_id = entries[batch_start].0;
                let mut batch_end = batch_start + 1;
                while batch_end < entries.len() && entries[batch_end].0 == mesh_id {
                    batch_end += 1;
                }
                batches.push(Batch {
                    mesh_id,
                    start: batch_start,
                    count: batch_end - batch_start,
                });
                batch_start = batch_end;
            }
        }

        // Write terrain instances BEFORE render passes (same reason as geometry).
        if !terrain_draws.is_empty() {
            let terrain_instances: Vec<TerrainInstanceRaw> = terrain_draws
                .iter()
                .map(|cmd| TerrainInstanceRaw {
                    model: cmd.model_matrix.to_cols_array_2d(),
                    morph_factor: cmd.morph_factor,
                    _pad: [0.0; 3],
                })
                .collect();

            let needed = terrain_instances.len() as u64;
            if needed > self.terrain_instance_capacity {
                let new_cap = needed.next_power_of_two();
                self.terrain_instance_buffer =
                    gpu.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Terrain Instance Buffer"),
                        size: new_cap
                            * std::mem::size_of::<TerrainInstanceRaw>() as u64,
                        usage: wgpu::BufferUsages::VERTEX
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                self.terrain_instance_capacity = new_cap;
            }
            gpu.queue.write_buffer(
                &self.terrain_instance_buffer,
                0,
                bytemuck::cast_slice(&terrain_instances),
            );
        }

        let mut encoder =
            gpu.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Frame Encoder"),
                });

        // Pass 1: Render sky to half-res offscreen texture
        {
            let sky_query = self.gpu_profiler.begin_query("sky_pass", &mut encoder);
            self.sky_renderer.render_to_texture(&mut encoder);
            self.gpu_profiler.end_query(&mut encoder, sky_query);
        }

        // Pass 2: Main scene pass — blit sky + geometry + stars + nebulae + screens
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.geometry_pipeline.depth_texture,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0), // reversed-Z: 0 = infinity
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            // Blit half-res sky to main framebuffer (additive blend, no depth write)
            self.sky_renderer.blit_to_main(&mut pass);

            // Draw sky-dome elements BEFORE geometry so that opaque objects
            // (ship, planets) naturally overwrite them. Stars/nebulae paint
            // the full sky background; geometry paints on top.
            // (With reversed-Z, far objects have depth near 0. Opaque geometry
            // at any distance has depth > 0 and overwrites via GreaterEqual.)

            // Stars
            {
                let q = self.gpu_profiler.begin_query("star_field", &mut pass);
                pass.set_pipeline(&self.star_field.pipeline);
                pass.set_bind_group(0, &self.star_field.bind_group, &[]);
                pass.set_vertex_buffer(0, self.star_field.vertex_buffer.slice(..));
                pass.draw(0..6, 0..self.star_field.star_count);
                self.gpu_profiler.end_query(&mut pass, q);
            }

            // Nebulae (alpha blended, no depth write)
            {
                let q = self.gpu_profiler.begin_query("nebula", &mut pass);
                self.nebula_renderer.render(&mut pass);
                self.galaxy_renderer.render(&mut pass);
                self.gpu_profiler.end_query(&mut pass, q);
            }

            // Draw geometry — instances were pre-computed and written to the
            // instance buffer BEFORE the render pass to avoid flickering.
            let geom_query = self.gpu_profiler.begin_query("geometry_pass", &mut pass);
            if !batches.is_empty() {
                pass.set_pipeline(&self.geometry_pipeline.pipeline);
                pass.set_bind_group(0, &self.geometry_pipeline.uniform_bind_group, &[]);

                let instance_stride = std::mem::size_of::<InstanceRaw>() as u64;
                for batch in &batches {
                    let mesh_handle = draw_commands.iter()
                        .find(|c| c.mesh.id() == batch.mesh_id)
                        .unwrap()
                        .mesh;
                    if let Some(mesh) = self.mesh_store.get(mesh_handle) {
                        let offset = batch.start as u64 * instance_stride;
                        let size = batch.count as u64 * instance_stride;
                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(
                            1,
                            self.instance_buffer.slice(offset..offset + size),
                        );
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed(
                            0..mesh.index_count,
                            0,
                            0..batch.count as u32,
                        );
                    }
                }
            }

            // Draw terrain chunks via the slab allocator (after geometry).
            if !terrain_draws.is_empty() {
                let tq = self.gpu_profiler.begin_query("terrain_pass", &mut pass);
                pass.set_pipeline(&self.terrain_pipeline.pipeline);
                pass.set_bind_group(
                    0,
                    &self.geometry_pipeline.uniform_bind_group,
                    &[],
                );

                if let Some(slab_buf) = self.terrain_slab.vertex_buffer() {
                    pass.set_vertex_buffer(0, slab_buf.slice(..));
                    pass.set_index_buffer(
                        self.terrain_index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );

                    let inst_stride =
                        std::mem::size_of::<TerrainInstanceRaw>() as u64;
                    for (i, cmd) in terrain_draws.iter().enumerate() {
                        let offset = i as u64 * inst_stride;
                        pass.set_vertex_buffer(
                            1,
                            self.terrain_instance_buffer
                                .slice(offset..offset + inst_stride),
                        );
                        pass.draw_indexed(
                            0..self.terrain_index_count,
                            self.terrain_slab.base_vertex(cmd.slab_slot) as i32,
                            0..1,
                        );
                    }
                }
                self.gpu_profiler.end_query(&mut pass, tq);
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

            // Stars, nebulae, and galaxies were rendered before geometry (above).
            self.gpu_profiler.end_query(&mut pass, geom_query);
        }

        // Resolve GPU profiler queries before submitting.
        self.gpu_profiler.resolve_queries(&mut encoder);

        Some(FrameContext {
            encoder,
            frame,
            view,
        })
    }
}
