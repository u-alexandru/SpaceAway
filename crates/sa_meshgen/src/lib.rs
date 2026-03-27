pub mod mesh;
pub mod primitives;
pub mod primitives_ext;
pub mod csg;
pub mod assembly;
pub mod ship_parts;

pub use mesh::{Mesh, MeshVertex};

/// Color palette --- flat vertex colors, no textures.
pub mod colors {
    pub const HULL_EXTERIOR: [f32; 3] = [0.3, 0.3, 0.35];
    pub const INTERIOR_WALL: [f32; 3] = [0.5, 0.5, 0.55];
    pub const FLOOR: [f32; 3] = [0.25, 0.25, 0.28];
    pub const CONSOLE_SCREEN: [f32; 3] = [0.2, 0.3, 0.5];
    pub const ACCENT_HELM: [f32; 3] = [0.2, 0.4, 0.7];
    pub const ACCENT_ENGINEERING: [f32; 3] = [0.7, 0.5, 0.2];
    pub const ACCENT_SENSORS: [f32; 3] = [0.5, 0.2, 0.6];
    pub const ACCENT_NAVIGATION: [f32; 3] = [0.2, 0.6, 0.4];
    pub const ACCENT_ENGINE: [f32; 3] = [0.7, 0.2, 0.2];
    pub const AIRLOCK_WARNING: [f32; 3] = [0.7, 0.6, 0.1];
}
