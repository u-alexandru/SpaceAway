//! GPU vertex format for terrain chunks, with morph target for CDLOD morphing.

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub normal: [f32; 3],
    pub morph_target: [f32; 3],
}

impl TerrainVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        static ATTRIBUTES: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x3 },
            wgpu::VertexAttribute { offset: 36, shader_location: 7, format: wgpu::VertexFormat::Float32x3 },
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRIBUTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_vertex_size_is_48_bytes() {
        assert_eq!(std::mem::size_of::<TerrainVertex>(), 48);
    }

    #[test]
    fn terrain_vertex_layout_has_four_attributes() {
        assert_eq!(TerrainVertex::layout().attributes.len(), 4);
    }

    #[test]
    fn morph_target_at_location_7() {
        let attrs = TerrainVertex::layout().attributes;
        assert_eq!(attrs[3].shader_location, 7);
        assert_eq!(attrs[3].offset, 36);
    }
}
