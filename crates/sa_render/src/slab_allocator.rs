//! Budget-driven vertex buffer pool for terrain chunks.
//!
//! All heightmap chunks share identical topology (33×33 grid + skirts),
//! so every slot has the same vertex count. Slots are managed via a free-list.

use std::collections::{HashMap, HashSet};

use sa_terrain::ChunkKey;

/// Fixed-size slot allocator backed by a single large GPU vertex buffer.
///
/// Every terrain chunk gets one slot of identical size. A free-list tracks
/// available slots, and distance-based eviction reclaims slots when the
/// budget is exhausted.
pub struct TerrainSlab {
    /// GPU vertex buffer (None in CPU-only test mode).
    vertex_buffer: Option<wgpu::Buffer>,
    /// Available slot indices.
    free_list: Vec<u32>,
    /// Slot → chunk key mapping.
    slot_to_chunk: HashMap<u32, ChunkKey>,
    /// Chunk key → slot index mapping.
    chunk_to_slot: HashMap<ChunkKey, u32>,
    /// Chunk key → f64 center (for eviction distance calculation).
    chunk_centers: HashMap<ChunkKey, [f64; 3]>,
    /// Vertices per slot.
    pub slot_vertex_count: u32,
    /// Bytes per slot.
    pub slot_size_bytes: u32,
    /// Total number of slots.
    pub total_slots: u32,
    /// Total budget in bytes.
    budget_bytes: u64,
}

impl TerrainSlab {
    /// Create a slab with GPU buffer.
    pub fn new(
        device: &wgpu::Device,
        budget_bytes: u64,
        slot_size_bytes: u32,
    ) -> Self {
        let total_slots = (budget_bytes / slot_size_bytes as u64) as u32;
        let buffer_size = total_slots as u64 * slot_size_bytes as u64;

        let vertex_buffer =
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Terrain Slab Vertex Buffer"),
                size: buffer_size,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

        let free_list = (0..total_slots).rev().collect();

        Self {
            vertex_buffer: Some(vertex_buffer),
            free_list,
            slot_to_chunk: HashMap::with_capacity(total_slots as usize),
            chunk_to_slot: HashMap::with_capacity(total_slots as usize),
            chunk_centers: HashMap::with_capacity(total_slots as usize),
            slot_vertex_count: slot_size_bytes / 48,
            slot_size_bytes,
            total_slots,
            budget_bytes,
        }
    }

    /// Create a CPU-only slab for testing (no GPU buffer).
    #[cfg(test)]
    pub fn new_cpu(budget_bytes: u64, slot_size_bytes: u32) -> Self {
        let total_slots = (budget_bytes / slot_size_bytes as u64) as u32;
        let free_list = (0..total_slots).rev().collect();
        Self {
            vertex_buffer: None,
            free_list,
            slot_to_chunk: HashMap::new(),
            chunk_to_slot: HashMap::new(),
            chunk_centers: HashMap::new(),
            slot_vertex_count: slot_size_bytes / 48,
            slot_size_bytes,
            total_slots,
            budget_bytes,
        }
    }

    /// Allocate a slot for the given chunk key. Returns the slot index,
    /// or `None` if the slab is full.
    pub fn allocate(&mut self, key: ChunkKey) -> Option<u32> {
        if self.chunk_to_slot.contains_key(&key) {
            return self.chunk_to_slot.get(&key).copied();
        }
        let slot = self.free_list.pop()?;
        self.slot_to_chunk.insert(slot, key);
        self.chunk_to_slot.insert(key, slot);
        Some(slot)
    }

    /// Free the slot occupied by the given chunk key.
    pub fn free(&mut self, key: &ChunkKey) {
        if let Some(slot) = self.chunk_to_slot.remove(key) {
            self.slot_to_chunk.remove(&slot);
            self.chunk_centers.remove(key);
            self.free_list.push(slot);
        }
    }

    /// Upload vertex data into the slot's region of the GPU buffer.
    pub fn upload(&self, slot: u32, data: &[u8], queue: &wgpu::Queue) {
        if let Some(ref buf) = self.vertex_buffer {
            let offset = slot as u64 * self.slot_size_bytes as u64;
            queue.write_buffer(buf, offset, data);
        }
    }

    /// Reference to the underlying GPU vertex buffer.
    pub fn vertex_buffer(&self) -> Option<&wgpu::Buffer> {
        self.vertex_buffer.as_ref()
    }

    /// Record the world-space center of a chunk (used for eviction).
    pub fn set_center(&mut self, key: ChunkKey, center: [f64; 3]) {
        self.chunk_centers.insert(key, center);
    }

    /// Evict the chunk farthest from the camera, skipping protected keys.
    /// Returns the evicted key, or `None` if nothing can be evicted.
    pub fn evict_farthest(
        &mut self,
        camera: [f64; 3],
        protected: &HashSet<ChunkKey>,
    ) -> Option<ChunkKey> {
        let mut worst_key: Option<ChunkKey> = None;
        let mut worst_score: f64 = f64::NEG_INFINITY;

        for (&key, &center) in &self.chunk_centers {
            if protected.contains(&key) {
                continue;
            }
            let dx = camera[0] - center[0];
            let dy = camera[1] - center[1];
            let dz = camera[2] - center[2];
            let dist_sq = dx * dx + dy * dy + dz * dz;
            let score = dist_sq + (key.lod as f64 * 1000.0);
            if score > worst_score {
                worst_score = score;
                worst_key = Some(key);
            }
        }

        if let Some(key) = worst_key {
            self.free(&key);
        }
        worst_key
    }

    /// First vertex index for the given slot (for draw calls).
    pub fn base_vertex(&self, slot: u32) -> u32 {
        slot * self.slot_vertex_count
    }

    /// Whether the chunk key has an allocated slot.
    pub fn contains(&self, key: &ChunkKey) -> bool {
        self.chunk_to_slot.contains_key(key)
    }

    /// Get the slot index for a chunk key.
    pub fn get_slot(&self, key: &ChunkKey) -> Option<u32> {
        self.chunk_to_slot.get(key).copied()
    }

    /// Number of free slots remaining.
    pub fn free_slots(&self) -> u32 {
        self.free_list.len() as u32
    }

    /// Number of occupied slots.
    pub fn occupied_slots(&self) -> u32 {
        self.total_slots - self.free_slots()
    }

    /// Total budget in bytes.
    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Get the stored center position for a chunk key.
    pub fn get_center(&self, key: &ChunkKey) -> Option<[f64; 3]> {
        self.chunk_centers.get(key).copied()
    }

    /// Free all occupied slots (used on teleport / terrain deactivation).
    pub fn clear(&mut self) {
        let keys: Vec<ChunkKey> = self.chunk_to_slot.keys().copied().collect();
        for key in keys {
            self.free(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_slab_has_correct_slot_count() {
        let slab = TerrainSlab::new_cpu(30_000_000, 58_416);
        assert!(slab.total_slots >= 500);
        assert!(slab.total_slots <= 520);
    }

    #[test]
    fn allocate_returns_slot_and_tracks_key() {
        let mut slab = TerrainSlab::new_cpu(1_000_000, 1000);
        let key = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        let slot = slab.allocate(key);
        assert!(slot.is_some());
        assert!(slab.contains(&key));
    }

    #[test]
    fn free_returns_slot_to_pool() {
        let mut slab = TerrainSlab::new_cpu(1_000_000, 1000);
        let key = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        slab.allocate(key);
        slab.free(&key);
        assert!(!slab.contains(&key));
    }

    #[test]
    fn allocate_fails_when_full() {
        let mut slab = TerrainSlab::new_cpu(3000, 1000);
        let k1 = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        let k2 = ChunkKey { face: 1, lod: 0, x: 0, y: 0 };
        let k3 = ChunkKey { face: 2, lod: 0, x: 0, y: 0 };
        let k4 = ChunkKey { face: 3, lod: 0, x: 0, y: 0 };
        assert!(slab.allocate(k1).is_some());
        assert!(slab.allocate(k2).is_some());
        assert!(slab.allocate(k3).is_some());
        assert!(slab.allocate(k4).is_none());
    }

    #[test]
    fn base_vertex_offset_is_correct() {
        let slab = TerrainSlab::new_cpu(1_000_000, 48000);
        assert_eq!(slab.base_vertex(0), 0);
        assert_eq!(slab.base_vertex(1), slab.slot_vertex_count);
        assert_eq!(slab.base_vertex(2), slab.slot_vertex_count * 2);
    }

    #[test]
    fn evict_farthest_removes_correct_chunk() {
        let mut slab = TerrainSlab::new_cpu(3000, 1000);
        let k1 = ChunkKey { face: 0, lod: 5, x: 0, y: 0 };
        let k2 = ChunkKey { face: 1, lod: 5, x: 0, y: 0 };
        let k3 = ChunkKey { face: 2, lod: 5, x: 0, y: 0 };
        slab.allocate(k1);
        slab.allocate(k2);
        slab.allocate(k3);
        slab.set_center(k1, [100.0, 0.0, 0.0]);
        slab.set_center(k2, [1000.0, 0.0, 0.0]);
        slab.set_center(k3, [200.0, 0.0, 0.0]);

        let protected = HashSet::new();
        let evicted = slab.evict_farthest([0.0, 0.0, 0.0], &protected);
        assert_eq!(evicted, Some(k2));
        assert!(!slab.contains(&k2));
    }

    #[test]
    fn evict_skips_protected_chunks() {
        let mut slab = TerrainSlab::new_cpu(3000, 1000);
        let k1 = ChunkKey { face: 0, lod: 0, x: 0, y: 0 };
        let k2 = ChunkKey { face: 1, lod: 5, x: 0, y: 0 };
        let k3 = ChunkKey { face: 2, lod: 5, x: 0, y: 0 };
        slab.allocate(k1);
        slab.allocate(k2);
        slab.allocate(k3);
        slab.set_center(k1, [5000.0, 0.0, 0.0]);
        slab.set_center(k2, [1000.0, 0.0, 0.0]);
        slab.set_center(k3, [200.0, 0.0, 0.0]);

        let mut protected = HashSet::new();
        protected.insert(k1);
        let evicted = slab.evict_farthest([0.0, 0.0, 0.0], &protected);
        assert_eq!(evicted, Some(k2));
        assert!(slab.contains(&k1));
    }
}
