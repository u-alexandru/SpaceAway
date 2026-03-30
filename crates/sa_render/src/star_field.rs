use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StarVertex {
    pub position: [f32; 3],
    pub brightness: f32,
    pub color: [f32; 3],
    pub _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StarUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub screen_height: f32,
    pub screen_width: f32,
    /// Speed as fraction of c (0.0–0.99) for relativistic aberration.
    pub beta: f32,
    /// Star streak length in pixels (0 = point, 300 = full warp).
    pub streak_factor: f32,
    /// Normalized velocity direction (world space).
    pub velocity_dir: [f32; 3],
    /// Additive white flash for transitions (0.0–1.0).
    pub flash_intensity: f32,
}

pub struct StarField {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub star_count: u32,
}

impl StarField {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat, stars: &[StarVertex]) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Star Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/stars.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Star Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Star Uniforms"),
            size: std::mem::size_of::<StarUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Star Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Star Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Star Vertices"),
            contents: bytemuck::cast_slice(stars),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Star Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                // Each star is an instance drawn as 6 vertices (2 triangles).
                // The vertex shader uses vertex_index % 6 to determine the quad corner.
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<StarVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32 },
                        wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Max,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::GreaterEqual, // reversed-Z
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self { pipeline, vertex_buffer, uniform_buffer, bind_group, star_count: stars.len() as u32 }
    }

    /// Replace the star vertex buffer with new data (e.g. when the player moves
    /// to a new sector and the visible star set changes).
    pub fn update_star_buffer(&mut self, device: &wgpu::Device, stars: &[StarVertex]) {
        self.vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Star Vertices"),
            contents: bytemuck::cast_slice(stars),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.star_count = stars.len() as u32;
    }
}

pub fn generate_stars(count: u32, seed: u64) -> Vec<StarVertex> {
    let mut stars = Vec::with_capacity(count as usize);
    let mut state = seed.wrapping_add(1);
    let mut rand_f32 = || -> f32 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state as f32) / (u64::MAX as f32)
    };

    for _ in 0..count {
        let (x, y, z) = loop {
            let x = rand_f32() * 2.0 - 1.0;
            let y = rand_f32() * 2.0 - 1.0;
            let z = rand_f32() * 2.0 - 1.0;
            let len_sq = x * x + y * y + z * z;
            if len_sq > 0.001 && len_sq <= 1.0 {
                let len = len_sq.sqrt();
                break (x / len, y / len, z / len);
            }
        };

        let brightness = rand_f32() * 0.8 + 0.2;
        let temp = rand_f32();
        let color = if temp < 0.3 { [1.0, 0.85, 0.7] }
            else if temp < 0.7 { [1.0, 1.0, 1.0] }
            else { [0.8, 0.9, 1.0] };

        stars.push(StarVertex { position: [x, y, z], brightness, color, _pad: 0.0 });
    }
    stars
}
