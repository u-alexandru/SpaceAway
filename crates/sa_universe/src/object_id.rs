use serde::{Deserialize, Serialize};

/// Packed 64-bit identifier for any object in the universe.
///
/// Bit layout (MSB to LSB):
///   [63..48] sector_x  (16 bits, signed as i16)
///   [47..32] sector_y  (16 bits, signed as i16)
///   [31..16] sector_z  (16 bits, signed as i16)
///   [15..13] layer     (3 bits, 0-7)
///   [12..5]  system    (8 bits, 0-255)
///   [4..0]   body      (5 bits, 0-31)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(pub u64);

impl ObjectId {
    pub fn pack(
        sector_x: i16,
        sector_y: i16,
        sector_z: i16,
        layer: u8,
        system: u8,
        body: u8,
    ) -> Self {
        let sx = (sector_x as u16) as u64;
        let sy = (sector_y as u16) as u64;
        let sz = (sector_z as u16) as u64;
        let l = (layer & 0x07) as u64;
        let s = system as u64;
        let b = (body & 0x1F) as u64;

        Self((sx << 48) | (sy << 32) | (sz << 16) | (l << 13) | (s << 5) | b)
    }

    pub fn sector_x(self) -> i16 {
        ((self.0 >> 48) & 0xFFFF) as u16 as i16
    }

    pub fn sector_y(self) -> i16 {
        ((self.0 >> 32) & 0xFFFF) as u16 as i16
    }

    pub fn sector_z(self) -> i16 {
        ((self.0 >> 16) & 0xFFFF) as u16 as i16
    }

    pub fn layer(self) -> u8 {
        ((self.0 >> 13) & 0x07) as u8
    }

    pub fn system(self) -> u8 {
        ((self.0 >> 5) & 0xFF) as u8
    }

    pub fn body(self) -> u8 {
        (self.0 & 0x1F) as u8
    }

    /// Returns an ObjectId addressing just the sector (layer/system/body = 0).
    pub fn sector_id(sector_x: i16, sector_y: i16, sector_z: i16) -> Self {
        Self::pack(sector_x, sector_y, sector_z, 0, 0, 0)
    }

    /// Returns an ObjectId addressing a star system within a sector.
    pub fn star_id(sector_x: i16, sector_y: i16, sector_z: i16, system: u8) -> Self {
        Self::pack(sector_x, sector_y, sector_z, 0, system, 0)
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({},{},{}):L{}:S{}:B{}",
            self.sector_x(), self.sector_y(), self.sector_z(),
            self.layer(), self.system(), self.body(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let id = ObjectId::pack(100, -50, 32767, 5, 200, 31);
        assert_eq!(id.sector_x(), 100);
        assert_eq!(id.sector_y(), -50);
        assert_eq!(id.sector_z(), 32767);
        assert_eq!(id.layer(), 5);
        assert_eq!(id.system(), 200);
        assert_eq!(id.body(), 31);
    }

    #[test]
    fn pack_unpack_zeros() {
        let id = ObjectId::pack(0, 0, 0, 0, 0, 0);
        assert_eq!(id.0, 0);
        assert_eq!(id.sector_x(), 0);
        assert_eq!(id.sector_y(), 0);
        assert_eq!(id.sector_z(), 0);
        assert_eq!(id.layer(), 0);
        assert_eq!(id.system(), 0);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn pack_unpack_negative_coords() {
        let id = ObjectId::pack(-1, -32768, -100, 7, 255, 0);
        assert_eq!(id.sector_x(), -1);
        assert_eq!(id.sector_y(), -32768);
        assert_eq!(id.sector_z(), -100);
        assert_eq!(id.layer(), 7);
        assert_eq!(id.system(), 255);
    }

    #[test]
    fn pack_unpack_max_values() {
        let id = ObjectId::pack(32767, 32767, 32767, 7, 255, 31);
        assert_eq!(id.sector_x(), 32767);
        assert_eq!(id.sector_y(), 32767);
        assert_eq!(id.sector_z(), 32767);
        assert_eq!(id.layer(), 7);
        assert_eq!(id.system(), 255);
        assert_eq!(id.body(), 31);
    }

    #[test]
    fn sector_id_helper() {
        let id = ObjectId::sector_id(10, 20, 30);
        assert_eq!(id.sector_x(), 10);
        assert_eq!(id.sector_y(), 20);
        assert_eq!(id.sector_z(), 30);
        assert_eq!(id.layer(), 0);
        assert_eq!(id.system(), 0);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn star_id_helper() {
        let id = ObjectId::star_id(5, -5, 0, 42);
        assert_eq!(id.sector_x(), 5);
        assert_eq!(id.sector_y(), -5);
        assert_eq!(id.system(), 42);
        assert_eq!(id.body(), 0);
    }

    #[test]
    fn display_format() {
        let id = ObjectId::pack(1, 2, 3, 0, 10, 5);
        let s = format!("{id}");
        assert_eq!(s, "(1,2,3):L0:S10:B5");
    }

    #[test]
    fn different_fields_produce_different_ids() {
        let a = ObjectId::pack(0, 0, 0, 0, 0, 0);
        let b = ObjectId::pack(0, 0, 0, 0, 0, 1);
        let c = ObjectId::pack(0, 0, 0, 0, 1, 0);
        let d = ObjectId::pack(0, 0, 0, 1, 0, 0);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
