//! GPU streaming pool — async asset streaming with memory budget and LRU eviction.
//!
//! Manages a fixed GPU memory budget for textures and meshes. Assets are
//! loaded asynchronously in background threads and uploaded when ready.
//! When the budget is exceeded the least-recently-used assets are evicted
//! and replaced with low-resolution fallbacks.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Default texture memory budget (512 MiB).
pub const DEFAULT_TEXTURE_BUDGET: u64 = 512 * 1024 * 1024;
/// Default mesh memory budget (256 MiB).
pub const DEFAULT_MESH_BUDGET: u64 = 256 * 1024 * 1024;

/// Priority for a streaming request. Higher priority assets are loaded first
/// and evicted last.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamingPriority {
    /// Screen-space projected size (larger = higher priority).
    pub screen_size: f32,
    /// Inverse camera distance (closer = higher priority).
    pub inv_distance: f32,
}

impl StreamingPriority {
    pub fn score(&self) -> f32 {
        self.screen_size * self.inv_distance
    }
}

impl Eq for StreamingPriority {}

impl PartialOrd for StreamingPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StreamingPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score()
            .partial_cmp(&other.score())
            .unwrap_or(Ordering::Equal)
    }
}

/// A request to stream an asset from disk to GPU.
#[derive(Debug, Clone)]
pub struct StreamingRequest {
    pub path: PathBuf,
    pub priority: StreamingPriority,
    pub kind: StreamingAssetKind,
}

impl PartialEq for StreamingRequest {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for StreamingRequest {}

impl PartialOrd for StreamingRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StreamingRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingAssetKind {
    Texture,
    Mesh,
}

/// State of an asset in the streaming pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyState {
    /// Not loaded — using a fallback placeholder.
    NotResident,
    /// Background thread is loading data from disk.
    Loading,
    /// Fully uploaded to GPU memory.
    Resident,
    /// Scheduled for eviction at end of frame.
    PendingEviction,
}

/// Bookkeeping entry for a single streaming asset.
struct StreamingEntry {
    path: PathBuf,
    kind: StreamingAssetKind,
    state: ResidencyState,
    /// Approximate GPU memory consumed (bytes).
    gpu_bytes: u64,
    /// Frame counter at which this asset was last accessed.
    last_used_frame: u64,
}

/// Result of a completed background load (bytes ready for GPU upload).
struct LoadedAssetData {
    path: PathBuf,
    kind: StreamingAssetKind,
    data: Vec<u8>,
    gpu_bytes: u64,
}

/// Manages GPU memory budgets, async loading, and LRU eviction.
pub struct StreamingPool {
    texture_budget: u64,
    mesh_budget: u64,
    texture_used: u64,
    mesh_used: u64,
    entries: HashMap<PathBuf, StreamingEntry>,
    /// Pending load results from background threads.
    completed: Arc<Mutex<Vec<LoadedAssetData>>>,
    /// Current frame counter (bumped each `begin_frame`).
    frame: u64,
    /// Maximum number of in-flight loads to prevent thread-pool saturation.
    max_in_flight: usize,
    in_flight: usize,
}

impl StreamingPool {
    pub fn new() -> Self {
        Self::with_budgets(DEFAULT_TEXTURE_BUDGET, DEFAULT_MESH_BUDGET)
    }

    pub fn with_budgets(texture_budget: u64, mesh_budget: u64) -> Self {
        Self {
            texture_budget,
            mesh_budget,
            texture_used: 0,
            mesh_used: 0,
            entries: HashMap::new(),
            completed: Arc::new(Mutex::new(Vec::new())),
            frame: 0,
            max_in_flight: 8,
            in_flight: 0,
        }
    }

    /// Call at the start of each frame to bump the internal clock.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Request that `path` be streamed in. If already resident this simply
    /// marks it as recently used. Otherwise a background load is queued.
    pub fn request(&mut self, req: &StreamingRequest) {
        if let Some(entry) = self.entries.get_mut(&req.path) {
            entry.last_used_frame = self.frame;
            if entry.state == ResidencyState::Resident || entry.state == ResidencyState::Loading {
                return;
            }
        }

        // Budget check: evict LRU assets if needed.
        let budget = match req.kind {
            StreamingAssetKind::Texture => self.texture_budget,
            StreamingAssetKind::Mesh => self.mesh_budget,
        };
        let used = match req.kind {
            StreamingAssetKind::Texture => self.texture_used,
            StreamingAssetKind::Mesh => self.mesh_used,
        };
        if used >= budget {
            self.evict_lru(req.kind);
        }

        if self.in_flight >= self.max_in_flight {
            return; // throttle
        }

        // Spawn background load.
        let path = req.path.clone();
        let kind = req.kind;
        let completed = Arc::clone(&self.completed);

        rayon::spawn(move || {
            if let Ok(data) = std::fs::read(&path) {
                let gpu_bytes = data.len() as u64;
                let loaded = LoadedAssetData {
                    path,
                    kind,
                    data,
                    gpu_bytes,
                };
                if let Ok(mut list) = completed.lock() {
                    list.push(loaded);
                }
            } else {
                log::warn!("Streaming: failed to read {:?}", path);
            }
        });

        self.in_flight += 1;

        self.entries.insert(
            req.path.clone(),
            StreamingEntry {
                path: req.path.clone(),
                kind: req.kind,
                state: ResidencyState::Loading,
                gpu_bytes: 0,
                last_used_frame: self.frame,
            },
        );
    }

    /// Drain completed loads and return the raw data for the caller to
    /// upload to the GPU. Each returned item should be passed to
    /// `device.create_texture` / `device.create_buffer` by the renderer.
    pub fn drain_completed(&mut self) -> Vec<(PathBuf, StreamingAssetKind, Vec<u8>)> {
        let mut loaded = self
            .completed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect::<Vec<_>>();

        let mut results = Vec::with_capacity(loaded.len());
        for item in loaded.drain(..) {
            if let Some(entry) = self.entries.get_mut(&item.path) {
                entry.state = ResidencyState::Resident;
                entry.gpu_bytes = item.gpu_bytes;
                match entry.kind {
                    StreamingAssetKind::Texture => self.texture_used += item.gpu_bytes,
                    StreamingAssetKind::Mesh => self.mesh_used += item.gpu_bytes,
                }
            }
            self.in_flight = self.in_flight.saturating_sub(1);
            results.push((item.path, item.kind, item.data));
        }

        results
    }

    /// Query the residency state of an asset.
    pub fn state(&self, path: &Path) -> ResidencyState {
        self.entries
            .get(path)
            .map(|e| e.state)
            .unwrap_or(ResidencyState::NotResident)
    }

    /// Memory used / budget for textures (bytes).
    pub fn texture_usage(&self) -> (u64, u64) {
        (self.texture_used, self.texture_budget)
    }

    /// Memory used / budget for meshes (bytes).
    pub fn mesh_usage(&self) -> (u64, u64) {
        (self.mesh_used, self.mesh_budget)
    }

    // -----------------------------------------------------------------------
    // LRU eviction
    // -----------------------------------------------------------------------

    fn evict_lru(&mut self, kind: StreamingAssetKind) {
        // Find the oldest resident entry of the given kind.
        let victim = self
            .entries
            .values()
            .filter(|e| e.kind == kind && e.state == ResidencyState::Resident)
            .min_by_key(|e| e.last_used_frame)
            .map(|e| e.path.clone());

        if let Some(path) = victim {
            if let Some(entry) = self.entries.get_mut(&path) {
                match entry.kind {
                    StreamingAssetKind::Texture => {
                        self.texture_used = self.texture_used.saturating_sub(entry.gpu_bytes);
                    }
                    StreamingAssetKind::Mesh => {
                        self.mesh_used = self.mesh_used.saturating_sub(entry.gpu_bytes);
                    }
                }
                entry.state = ResidencyState::PendingEviction;
                entry.gpu_bytes = 0;
                log::debug!("Streaming: evicting LRU asset {:?}", path);
            }
        }
    }
}

impl Default for StreamingPool {
    fn default() -> Self {
        Self::new()
    }
}
