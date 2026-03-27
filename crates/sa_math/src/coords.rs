use std::ops::{Add, Sub};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldPos {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl WorldPos {
    pub const ORIGIN: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn distance_to(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

impl Add for WorldPos {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl Sub for WorldPos {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalPos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl LocalPos {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_pos_creation() {
        let pos = WorldPos::new(1.0, 2.0, 3.0);
        assert_eq!(pos.x, 1.0);
    }

    #[test]
    fn world_pos_addition() {
        let c = WorldPos::new(1.0, 2.0, 3.0) + WorldPos::new(4.0, 5.0, 6.0);
        assert_eq!(c.x, 5.0);
        assert_eq!(c.y, 7.0);
    }

    #[test]
    fn world_pos_distance() {
        let a = WorldPos::new(0.0, 0.0, 0.0);
        let b = WorldPos::new(3.0, 4.0, 0.0);
        assert!((a.distance_to(b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn world_pos_origin() {
        assert_eq!(WorldPos::ORIGIN.x, 0.0);
    }
}
