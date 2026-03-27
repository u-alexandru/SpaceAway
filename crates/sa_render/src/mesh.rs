use crate::vertex::Vertex;
use sa_core::{Handle, HandleGenerator};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

pub struct MeshMarker;

pub struct MeshStore {
    meshes: HashMap<Handle<MeshMarker>, GpuMesh>,
    handle_gen: HandleGenerator,
}

impl MeshStore {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            handle_gen: HandleGenerator::new(),
        }
    }

    pub fn upload(&mut self, device: &wgpu::Device, data: &MeshData) -> Handle<MeshMarker> {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Vertex Buffer"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Index Buffer"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let handle = self.handle_gen.next();
        self.meshes.insert(
            handle,
            GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: data.indices.len() as u32,
            },
        );
        handle
    }

    pub fn get(&self, handle: Handle<MeshMarker>) -> Option<&GpuMesh> {
        self.meshes.get(&handle)
    }
}

impl Default for MeshStore {
    fn default() -> Self {
        Self::new()
    }
}
