use bytemuck::{Pod, Zeroable};

/// Uniforms for the sky shader (core glow + cubemap).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SkyUniforms {
    pub inv_view_proj: [[f32; 4]; 4],
    pub galactic_center_dir: [f32; 3],
    pub core_brightness: f32,
    pub cubemap_enabled: u32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Precomputed Milky Way cubemap data (CPU-generated, uploaded to GPU).
pub struct MilkyWayCubemap {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

/// Resolution of each cubemap face.
const CUBEMAP_SIZE: u32 = 256;

/// Face directions for cubemap generation.
/// Each entry is (forward, up, right) for the face.
/// Order: +X, -X, +Y, -Y, +Z, -Z
const FACE_DIRS: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, -1.0]), // +X
    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),  // -X
    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]),  // +Y
    ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),  // -Y
    ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),   // +Z
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]), // -Z
];

/// Generate the cubemap pixel data on CPU.
/// `density_fn(x, y, z) -> f64` is the galaxy density function.
/// `observer` is the observer position in light-years.
/// Returns 6 faces of RGBA u8 data, each `CUBEMAP_SIZE` x `CUBEMAP_SIZE`.
pub fn generate_cubemap_data(
    density_fn: &dyn Fn(f64, f64, f64) -> f64,
    observer: [f64; 3],
) -> Vec<Vec<u8>> {
    let size = CUBEMAP_SIZE as usize;
    let num_samples = 32;
    let max_dist = 50000.0_f64;
    let step = max_dist / num_samples as f64;

    let mut faces = Vec::with_capacity(6);

    for &(forward, up, right) in &FACE_DIRS {
        let mut pixels = vec![0u8; size * size * 4];

        for py in 0..size {
            for px in 0..size {
                // Map pixel to [-1, 1] on the face
                let u = (px as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                let v = -((py as f32 + 0.5) / size as f32 * 2.0 - 1.0);

                // Ray direction
                let dx = forward[0] + right[0] * u + up[0] * v;
                let dy = forward[1] + right[1] * u + up[1] * v;
                let dz = forward[2] + right[2] * u + up[2] * v;
                let len = (dx * dx + dy * dy + dz * dz).sqrt();
                let (dx, dy, dz) = (dx / len, dy / len, dz / len);

                // Integrate density along ray
                let mut accumulated = 0.0_f64;
                let mut warm_factor = 0.0_f64;

                for s in 0..num_samples {
                    let t = (s as f64 + 0.5) * step;
                    let sx = observer[0] + dx as f64 * t;
                    let sy = observer[1] + dy as f64 * t;
                    let sz = observer[2] + dz as f64 * t;

                    let d = density_fn(sx, sy, sz);
                    accumulated += d * step / max_dist;

                    // Track warmth: closer to center = warmer color
                    let r_center = (sx * sx + sy * sy + sz * sz).sqrt();
                    if r_center < 15000.0 {
                        warm_factor += d * step / max_dist;
                    }
                }

                // Map to brightness
                let brightness = (accumulated * 3.0).min(1.0);
                let warmth = (warm_factor / accumulated.max(0.001)).min(1.0);

                // Color: blend from blue-white (arm) to warm gold (center)
                let cool = [0.7_f32, 0.75, 0.9];
                let warm = [0.95_f32, 0.85, 0.65];
                let w = warmth as f32;
                let r = (cool[0] * (1.0 - w) + warm[0] * w) * brightness as f32;
                let g = (cool[1] * (1.0 - w) + warm[1] * w) * brightness as f32;
                let b = (cool[2] * (1.0 - w) + warm[2] * w) * brightness as f32;

                let idx = (py * size + px) * 4;
                pixels[idx] = (r * 255.0).min(255.0) as u8;
                pixels[idx + 1] = (g * 255.0).min(255.0) as u8;
                pixels[idx + 2] = (b * 255.0).min(255.0) as u8;
                pixels[idx + 3] = 255;
            }
        }
        faces.push(pixels);
    }
    faces
}

impl MilkyWayCubemap {
    /// Create from precomputed face data (6 faces of RGBA u8).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, faces: &[Vec<u8>]) -> Self {
        let size = CUBEMAP_SIZE;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Milky Way Cubemap"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (i, face_data) in faces.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                face_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        Self { texture, view }
    }

    /// Create a placeholder 1x1 black cubemap (used before real data is available).
    pub fn placeholder(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let black_face = vec![0u8, 0, 0, 255];
        let faces: Vec<Vec<u8>> = (0..6).map(|_| black_face.clone()).collect();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Cubemap"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (i, face_data) in faces.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                face_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        Self { texture, view }
    }
}

/// Renderer for the sky layers (Milky Way cubemap + galactic core glow).
pub struct SkyRenderer {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub sampler: wgpu::Sampler,
}

impl SkyRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        cubemap: &MilkyWayCubemap,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sky Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sky.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cubemap Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sky Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky Uniforms"),
            size: std::mem::size_of::<SkyUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cubemap.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
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
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            sampler,
        }
    }

    /// Rebuild the bind group after a cubemap regeneration.
    pub fn rebuild_bind_group(&mut self, device: &wgpu::Device, cubemap: &MilkyWayCubemap) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cubemap.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
    }
}
