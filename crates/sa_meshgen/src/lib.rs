pub mod mesh;
pub mod primitives;
pub mod primitives_ext;
pub mod csg;
pub mod assembly;
pub mod hull;
pub mod ship_parts;

pub use mesh::{Mesh, MeshVertex};

/// Color palette --- flat vertex colors, no textures.
pub mod colors {
    pub const HULL_EXTERIOR: [f32; 3] = [0.35, 0.35, 0.38];
    pub const HULL_ACCENT: [f32; 3] = [0.28, 0.30, 0.35];
    pub const INTERIOR_WALL: [f32; 3] = [0.52, 0.54, 0.56];
    pub const FLOOR: [f32; 3] = [0.30, 0.30, 0.32];
    pub const CEILING: [f32; 3] = [0.45, 0.45, 0.48];
    pub const CONSOLE_SCREEN: [f32; 3] = [0.10, 0.25, 0.40];
    pub const WINDOW_GLASS: [f32; 3] = [0.15, 0.20, 0.30];
    pub const ACCENT_HELM: [f32; 3] = [0.15, 0.35, 0.65];
    pub const ACCENT_NAVIGATION: [f32; 3] = [0.15, 0.55, 0.35];
    pub const ACCENT_SENSORS: [f32; 3] = [0.45, 0.18, 0.55];
    pub const ACCENT_ENGINEERING: [f32; 3] = [0.65, 0.45, 0.15];
    pub const ACCENT_ENGINE: [f32; 3] = [0.60, 0.18, 0.18];
    pub const AIRLOCK_WARNING: [f32; 3] = [0.65, 0.55, 0.10];
    pub const STRUCTURAL_RIB: [f32; 3] = [0.28, 0.28, 0.30];
    pub const RADIATOR_FIN: [f32; 3] = [0.22, 0.22, 0.25];
    pub const ANTENNA: [f32; 3] = [0.50, 0.50, 0.52];
    pub const LIGHT_RED: [f32; 3] = [0.90, 0.10, 0.10];
    pub const LIGHT_GREEN: [f32; 3] = [0.10, 0.90, 0.10];
    pub const LIGHT_WHITE: [f32; 3] = [0.95, 0.95, 0.90];
}
