use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

/// Master seed for the entire universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterSeed(pub u64);

/// Hash sector coordinates into a deterministic 64-bit seed.
/// Uses xxh3 (64-bit) for fast, high-quality hashing.
pub fn sector_hash(master: MasterSeed, sx: i32, sy: i32, sz: i32) -> u64 {
    let mut buf = [0u8; 20];
    buf[0..8].copy_from_slice(&master.0.to_le_bytes());
    buf[8..12].copy_from_slice(&sx.to_le_bytes());
    buf[12..16].copy_from_slice(&sy.to_le_bytes());
    buf[16..20].copy_from_slice(&sz.to_le_bytes());
    xxh3_64(&buf)
}

/// Minimal xorshift64 PRNG seeded from a u64.
/// Not cryptographic. Deterministic, fast, good distribution.
pub struct Rng64 {
    state: u64,
}

impl Rng64 {
    pub fn new(seed: u64) -> Self {
        // Ensure state is never zero (xorshift requires nonzero state).
        // Mix the seed through a splitmix step to get a good initial state.
        let mut s = seed.wrapping_add(0x9E3779B97F4A7C15);
        s = (s ^ (s >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        s = (s ^ (s >> 27)).wrapping_mul(0x94D049BB133111EB);
        s ^= s >> 31;
        if s == 0 { s = 1; }
        Self { state: s }
    }

    /// Returns a u64.
    pub fn next_u64(&mut self) -> u64 {
        let mut s = self.state;
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        self.state = s;
        s
    }

    /// Returns a f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Returns a f32 in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / ((1u64 << 24) as f32)
    }

    /// Returns a f64 in [min, max).
    pub fn range_f64(&mut self, min: f64, max: f64) -> f64 {
        min + self.next_f64() * (max - min)
    }

    /// Returns a f32 in [min, max).
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_hash_deterministic() {
        let seed = MasterSeed(12345);
        let h1 = sector_hash(seed, 10, -20, 30);
        let h2 = sector_hash(seed, 10, -20, 30);
        assert_eq!(h1, h2, "Same inputs must produce same hash");
    }

    #[test]
    fn sector_hash_different_coords_differ() {
        let seed = MasterSeed(12345);
        let h1 = sector_hash(seed, 0, 0, 0);
        let h2 = sector_hash(seed, 1, 0, 0);
        assert_ne!(h1, h2, "Different coords must produce different hashes");
    }

    #[test]
    fn sector_hash_different_seeds_differ() {
        let h1 = sector_hash(MasterSeed(1), 0, 0, 0);
        let h2 = sector_hash(MasterSeed(2), 0, 0, 0);
        assert_ne!(h1, h2, "Different master seeds must produce different hashes");
    }

    #[test]
    fn sector_hash_negative_coords() {
        let seed = MasterSeed(42);
        let h1 = sector_hash(seed, -1, -1, -1);
        let h2 = sector_hash(seed, 1, 1, 1);
        assert_ne!(h1, h2, "Negative and positive coords must differ");
    }

    #[test]
    fn rng_deterministic() {
        let mut a = Rng64::new(42);
        let mut b = Rng64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_different_seeds_differ() {
        let mut a = Rng64::new(1);
        let mut b = Rng64::new(2);
        let differs = (0..10).any(|_| a.next_u64() != b.next_u64());
        assert!(differs);
    }

    #[test]
    fn rng_f64_in_range() {
        let mut rng = Rng64::new(99);
        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "f64 out of [0,1): {v}");
        }
    }

    #[test]
    fn rng_f32_in_range() {
        let mut rng = Rng64::new(99);
        for _ in 0..1000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "f32 out of [0,1): {v}");
        }
    }

    #[test]
    fn rng_range_f64_bounds() {
        let mut rng = Rng64::new(123);
        for _ in 0..1000 {
            let v = rng.range_f64(5.0, 10.0);
            assert!((5.0..10.0).contains(&v), "range_f64 out of [5,10): {v}");
        }
    }

    #[test]
    fn rng_nonzero_state() {
        let mut rng = Rng64::new(0);
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b, "RNG should not get stuck with seed 0");
    }
}
