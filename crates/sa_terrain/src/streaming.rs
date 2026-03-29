//! Async chunk streaming: background generation with LRU cache.
//!
//! Four worker threads receive `ChunkKey` requests via crossbeam channels,
//! generate chunk meshes, and send results back. `ChunkStreaming::update()`
//! is called each frame with the current visible node list.

use std::collections::{HashMap, HashSet, VecDeque};
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::chunk::generate_chunk;
use crate::{ChunkData, ChunkKey, TerrainConfig};
use crate::quadtree::VisibleNode;

/// Number of background worker threads for chunk generation.
const WORKER_COUNT: usize = 4;

/// Maximum number of chunks held in the LRU cache.
const LRU_CAPACITY: usize = 500;

// ---------------------------------------------------------------------------
// LRU cache
// ---------------------------------------------------------------------------

/// Simple LRU cache backed by a `VecDeque` for recency ordering and a
/// `HashMap` for O(1) key lookup.
pub struct LruCache {
    capacity: usize,
    /// Most-recently-used at the back, least-recently-used at the front.
    order: VecDeque<ChunkKey>,
    map: HashMap<ChunkKey, ChunkData>,
    /// Keys evicted by capacity overflow since last drain.
    evicted: Vec<ChunkKey>,
}

impl LruCache {
    /// Create an empty cache with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity + 1),
            map: HashMap::with_capacity(capacity + 1),
            evicted: Vec::new(),
        }
    }

    /// Drain the list of keys evicted since the last call.
    pub fn drain_evicted(&mut self) -> Vec<ChunkKey> {
        std::mem::take(&mut self.evicted)
    }

    /// Insert a chunk. If the key already exists it is refreshed (promoted to
    /// MRU). Evicted keys (capacity overflow) are collected in `self.evicted`
    /// and retrieved via `drain_evicted()`.
    pub fn insert(&mut self, data: ChunkData) {
        let key = data.key;

        // If already present, remove from order so we can push to back.
        if self.map.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }

        self.map.insert(key, data);
        self.order.push_back(key);

        // Evict LRU if over capacity.
        if self.order.len() > self.capacity
            && let Some(evicted_key) = self.order.pop_front() {
                self.map.remove(&evicted_key);
                self.evicted.push(evicted_key);
            }
    }

    /// Retrieve a chunk by key, promoting it to MRU. Returns `None` if not
    /// present.
    pub fn get(&mut self, key: &ChunkKey) -> Option<&ChunkData> {
        if self.map.contains_key(key) {
            // Promote to MRU.
            self.order.retain(|k| k != key);
            self.order.push_back(*key);
            self.map.get(key)
        } else {
            None
        }
    }

    /// True if the cache contains the given key.
    pub fn contains(&self, key: &ChunkKey) -> bool {
        self.map.contains_key(key)
    }

    /// Current number of cached chunks.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when the cache holds no chunks.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Remove a key from the cache, returning the data if present.
    pub fn remove(&mut self, key: &ChunkKey) -> Option<ChunkData> {
        let data = self.map.remove(key);
        if data.is_some() {
            self.order.retain(|k| k != key);
        }
        data
    }
}

// ---------------------------------------------------------------------------
// ChunkStreaming
// ---------------------------------------------------------------------------

/// Manages background chunk generation and the LRU cache.
///
/// Call `update()` each frame with the visible node list. It dispatches
/// generation requests for missing chunks and returns newly-arrived chunks
/// plus keys that are no longer needed.
pub struct ChunkStreaming {
    /// Channel to send keys to worker threads.
    request_tx: Sender<ChunkKey>,
    /// Channel to receive completed chunks from worker threads.
    result_rx: Receiver<ChunkData>,
    /// LRU cache of generated chunks.
    cache: LruCache,
    /// Keys currently in-flight (requested but not yet received).
    in_flight: HashSet<ChunkKey>,
}

impl ChunkStreaming {
    /// Create a new streaming manager and spawn `WORKER_COUNT` worker threads.
    pub fn new(config: TerrainConfig) -> Self {
        let (request_tx, request_rx) = unbounded::<ChunkKey>();
        let (result_tx, result_rx) = unbounded::<ChunkData>();

        for _ in 0..WORKER_COUNT {
            let rx = request_rx.clone();
            let tx = result_tx.clone();
            let cfg = config.clone();

            thread::spawn(move || {
                // Block on incoming requests until the sender is dropped.
                while let Ok(key) = rx.recv() {
                    // Catch panics in chunk generation so one bad chunk
                    // doesn't kill the worker thread permanently.
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        generate_chunk(key, &cfg)
                    })) {
                        Ok(data) => {
                            let _ = tx.send(data);
                        }
                        Err(_) => {
                            log::error!(
                                "Terrain worker panic generating chunk face={} lod={} x={} y={}",
                                key.face, key.lod, key.x, key.y
                            );
                            // Worker continues — it does not die.
                        }
                    }
                }
            });
        }

        Self {
            request_tx,
            result_rx,
            cache: LruCache::new(LRU_CAPACITY),
            in_flight: HashSet::new(),
        }
    }

    /// Update streaming state for the current frame.
    ///
    /// * Drains completed chunks from the result channel into the cache
    ///   (up to `MAX_UPLOADS_PER_FRAME` to avoid GPU upload stalls).
    /// * Requests generation for any visible node that is neither cached nor
    ///   in-flight.
    /// * Computes which cached keys are no longer visible.
    ///
    /// Returns `(new_chunks, removed_keys)`:
    /// - `new_chunks`: freshly generated chunks (NOT cached re-deliveries).
    ///   The caller tracks which chunks it has already uploaded to the GPU.
    /// - `removed_keys`: keys evicted from the LRU cache by capacity overflow.
    pub fn update(
        &mut self,
        visible_nodes: &[VisibleNode],
        _config: &TerrainConfig,
    ) -> (Vec<ChunkData>, Vec<ChunkKey>) {
        /// Max chunks returned per frame to cap GPU upload cost.
        const MAX_UPLOADS_PER_FRAME: usize = 8;

        // Build the set of keys needed this frame.
        let needed: HashSet<ChunkKey> = visible_nodes
            .iter()
            .map(|n| ChunkKey {
                face: n.face as u8,
                lod: n.lod,
                x: n.x,
                y: n.y,
            })
            .collect();

        // ---------------------------------------------------------------
        // 1. Drain completed chunks from the result channel (capped).
        //    Only freshly generated chunks are returned — the caller's
        //    gpu_meshes HashMap already tracks what has been uploaded.
        // ---------------------------------------------------------------
        let mut new_chunks: Vec<ChunkData> = Vec::new();

        while new_chunks.len() < MAX_UPLOADS_PER_FRAME {
            match self.result_rx.try_recv() {
                Ok(data) => {
                    self.in_flight.remove(&data.key);
                    let data_copy = data.clone();
                    self.cache.insert(data);
                    new_chunks.push(data_copy);
                }
                Err(_) => break,
            }
        }

        // ---------------------------------------------------------------
        // 2. Request generation for needed chunks not yet cached or
        //    in-flight. No re-delivery of cached chunks — the caller
        //    retains GPU meshes independently.
        // ---------------------------------------------------------------
        for key in &needed {
            if !self.cache.contains(key) && !self.in_flight.contains(key) {
                self.in_flight.insert(*key);
                let _ = self.request_tx.send(*key);
            }
        }

        // ---------------------------------------------------------------
        // 3. Evicted keys: chunks removed by LRU capacity overflow.
        //    These are truly gone and the caller should free GPU resources.
        //    We do NOT report cached-but-invisible chunks as removed —
        //    they stay in cache for fast re-use.
        // ---------------------------------------------------------------
        let removed_keys: Vec<ChunkKey> = self.cache.drain_evicted();

        (new_chunks, removed_keys)
    }

    /// Number of chunks currently in the cache.
    pub fn cached_count(&self) -> usize {
        self.cache.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_sphere::CubeFace;
    use sa_universe::PlanetSubType;

    fn test_config() -> TerrainConfig {
        TerrainConfig {
            radius_m: 6_371_000.0,
            noise_seed: 42,
            sub_type: PlanetSubType::Temperate,
            displacement_fraction: 0.02,
        }
    }

    // -----------------------------------------------------------------------
    // LRU tests
    // -----------------------------------------------------------------------

    fn make_chunk(face: u8, lod: u8, x: u32, y: u32) -> ChunkData {
        ChunkData {
            key: ChunkKey { face, lod, x, y },
            center_f64: [0.0; 3],
            vertices: Vec::new(),
            indices: Vec::new(),
            heights: Vec::new(),
            min_height: 0.0,
            max_height: 1.0,
        }
    }

    #[test]
    fn lru_insert_and_retrieve() {
        let mut cache = LruCache::new(10);
        let chunk = make_chunk(0, 0, 0, 0);
        let key = chunk.key;
        cache.insert(chunk);
        assert!(cache.contains(&key));
        assert!(cache.get(&key).is_some());
    }

    #[test]
    fn lru_eviction() {
        let mut cache = LruCache::new(3);

        // Fill cache to capacity.
        let k0 = make_chunk(0, 0, 0, 0).key;
        let k1 = make_chunk(1, 0, 0, 0).key;
        let k2 = make_chunk(2, 0, 0, 0).key;
        cache.insert(make_chunk(0, 0, 0, 0));
        cache.insert(make_chunk(1, 0, 0, 0));
        cache.insert(make_chunk(2, 0, 0, 0));
        assert_eq!(cache.len(), 3);

        // Access k0 so k1 becomes the LRU.
        cache.get(&k0);

        // Insert a 4th entry — k1 should be evicted.
        cache.insert(make_chunk(3, 0, 0, 0));
        let evicted = cache.drain_evicted();
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], k1);
        assert!(!cache.contains(&k1));
        assert!(cache.contains(&k0));
        assert!(cache.contains(&k2));
        assert_eq!(cache.len(), 3);
    }

    // -----------------------------------------------------------------------
    // Streaming test
    // -----------------------------------------------------------------------

    #[test]
    fn streaming_receives_chunks() {
        let config = test_config();
        let mut streaming = ChunkStreaming::new(config.clone());

        // Build a single VisibleNode at a coarse LOD so generation is fast.
        let node = VisibleNode {
            face: CubeFace::PosZ,
            lod: 0,
            x: 0,
            y: 0,
            center: [0.0, 0.0, config.radius_m],
            morph_factor: 0.0,
        };
        let visible = vec![node];

        // Poll update() until a chunk arrives or we time out.
        let mut received = false;
        for _ in 0..200 {
            let (new_chunks, _removed) = streaming.update(&visible, &config);
            if !new_chunks.is_empty() {
                received = true;
                let chunk = &new_chunks[0];
                assert_eq!(chunk.key.face, CubeFace::PosZ as u8);
                assert_eq!(chunk.key.lod, 0);
                assert_eq!(chunk.key.x, 0);
                assert_eq!(chunk.key.y, 0);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(received, "no chunk received within timeout");
    }
}
