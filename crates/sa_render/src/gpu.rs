use std::sync::Arc;
use winit::window::Window;

pub struct GpuContext {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    pub fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("Failed to find a suitable GPU adapter");

        // Request timestamp query features for GPU profiling (optional — works without).
        let mut features = wgpu::Features::empty();
        let supported = adapter.features();
        if supported.contains(wgpu::Features::TIMESTAMP_QUERY) {
            features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        if supported.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS) {
            features |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        }
        if supported.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES) {
            features |= wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        }

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SpaceAway Device"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            },
            None,
        ))
        .expect("Failed to create GPU device");

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Surface not supported by adapter");
        // Prefer sRGB format for correct gamma — DX12 may default to linear
        // which makes colors appear washed out / brighter than intended.
        let caps = surface.get_capabilities(&adapter);
        if let Some(srgb) = caps.formats.iter().find(|f| f.is_srgb()) {
            config.format = *srgb;
        }
        // AutoVsync: adapts to platform capabilities.
        // On macOS (Metal): behaves like Fifo (VSync).
        // On Windows (DX12): uses Mailbox if available (uncapped, no tearing),
        // falls back to Fifo. User can toggle with V key.
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        Self { surface, device, queue, config }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    /// Toggle between VSync (Fifo) and uncapped (Immediate) for benchmarking.
    /// Returns true if VSync is now ON.
    pub fn toggle_vsync(&mut self) -> bool {
        let vsync_on = match self.config.present_mode {
            wgpu::PresentMode::Fifo => {
                self.config.present_mode = wgpu::PresentMode::Immediate;
                false
            }
            _ => {
                self.config.present_mode = wgpu::PresentMode::Fifo;
                true
            }
        };
        self.surface.configure(&self.device, &self.config);
        vsync_on
    }
}
