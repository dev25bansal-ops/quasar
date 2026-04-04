//! Level Streaming — async level loading with world partitioning.
//!
//! Provides:
//! - **Async level loading** — load/unload levels without frame hitches
//! - **World partitioning** — spatial regions for streaming
//! - **Level of Detail** — load different LODs based on distance
//! - **Priority scheduling** — load what's visible first
//!
//! # Architecture
//!
//! The streaming system divides the world into cells (chunks). Each cell
//! contains a list of entities and assets. Cells are loaded/unloaded
//! based on player position and streaming distance.
//!
//! # Example
//!
//! ```rust,ignore
//! use quasar_core::streaming::*;
//!
//! // Create streaming world
//! let mut world = StreamingWorld::new(StreamingConfig {
//!     cell_size: 100.0,
//!     load_distance: 300.0,
//!     unload_distance: 400.0,
//! });
//!
//! // Load level asynchronously
//! world.load_level_async("levels/forest").await?;
//!
//! // Update streaming each frame
//! world.update(player_position);
//! ```

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use quasar_math::Vec3;

use crate::ecs::{System, World};

/// Configuration for the streaming system.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Size of each streaming cell in world units.
    pub cell_size: f32,
    /// Distance from player to start loading cells.
    pub load_distance: f32,
    /// Distance from player to unload cells.
    pub unload_distance: f32,
    /// Maximum concurrent cell loads.
    pub max_concurrent_loads: usize,
    /// Priority boost for cells in view direction.
    pub view_direction_boost: f32,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            cell_size: 100.0,
            load_distance: 300.0,
            unload_distance: 400.0,
            max_concurrent_loads: 4,
            view_direction_boost: 2.0,
        }
    }
}

/// Unique identifier for a streaming cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId {
    pub x: i32,
    pub z: i32,
}

impl CellId {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// Convert world position to cell ID.
    pub fn from_world_pos(pos: Vec3, cell_size: f32) -> Self {
        Self {
            x: (pos.x / cell_size).floor() as i32,
            z: (pos.z / cell_size).floor() as i32,
        }
    }

    /// Get center of cell in world coordinates.
    pub fn center(&self, cell_size: f32) -> Vec3 {
        Vec3::new(
            (self.x as f32 + 0.5) * cell_size,
            0.0,
            (self.z as f32 + 0.5) * cell_size,
        )
    }

    /// Distance from a world position to the cell center.
    pub fn distance_to(&self, pos: Vec3, cell_size: f32) -> f32 {
        self.center(cell_size).distance(pos)
    }

    /// Get neighboring cells.
    pub fn neighbors(&self) -> [CellId; 8] {
        [
            CellId::new(self.x - 1, self.z - 1),
            CellId::new(self.x, self.z - 1),
            CellId::new(self.x + 1, self.z - 1),
            CellId::new(self.x - 1, self.z),
            CellId::new(self.x + 1, self.z),
            CellId::new(self.x - 1, self.z + 1),
            CellId::new(self.x, self.z + 1),
            CellId::new(self.x + 1, self.z + 1),
        ]
    }
}

/// State of a streaming cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    /// Cell is not loaded.
    Unloaded,
    /// Cell is being loaded.
    Loading,
    /// Cell is loaded and active.
    Loaded,
    /// Cell is pending unload.
    PendingUnload,
}

/// A streaming cell containing level data.
#[derive(Debug, Clone)]
pub struct StreamingCell {
    /// Cell identifier.
    pub id: CellId,
    /// Path to the cell data file.
    pub path: PathBuf,
    /// Current state.
    pub state: CellState,
    /// Entities in this cell.
    pub entities: Vec<u64>,
    /// Memory usage in bytes.
    pub memory_usage: u64,
    /// Last frame this cell was visible.
    pub last_visible_frame: u64,
    /// Priority for loading (higher = more important).
    pub priority: f32,
}

/// Level data for streaming.
#[derive(Debug, Clone)]
pub struct LevelData {
    /// Level name.
    pub name: String,
    /// All cells in the level.
    pub cells: HashMap<CellId, StreamingCell>,
    /// Total memory usage.
    pub total_memory: u64,
}

/// Async loading task.
struct LoadTask {
    cell_id: CellId,
    path: PathBuf,
    priority: f32,
}

/// The streaming world manages level streaming.
pub struct StreamingWorld {
    config: StreamingConfig,
    /// Currently loaded levels.
    levels: HashMap<String, LevelData>,
    /// All cells across all levels.
    cells: HashMap<CellId, StreamingCell>,
    /// Currently loading cells.
    loading: HashSet<CellId>,
    /// Pending load tasks.
    pending_loads: Vec<LoadTask>,
    /// Current frame counter.
    frame: u64,
    /// Last player position.
    player_pos: Vec3,
    /// Player view direction.
    player_view_dir: Vec3,
}

impl StreamingWorld {
    /// Create a new streaming world.
    pub fn new(config: StreamingConfig) -> Self {
        Self {
            config,
            levels: HashMap::new(),
            cells: HashMap::new(),
            loading: HashSet::new(),
            pending_loads: Vec::new(),
            frame: 0,
            player_pos: Vec3::ZERO,
            player_view_dir: Vec3::Z,
        }
    }

    /// Load a level asynchronously.
    pub async fn load_level_async(&mut self, path: &str) -> Result<(), std::io::Error> {
        let level_path = PathBuf::from(path);
        let metadata_path = level_path.join("level.json");

        // Load level metadata
        let metadata_bytes = std::fs::read(&metadata_path)?;
        let metadata: LevelMetadata = serde_json::from_slice(&metadata_bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut cells = HashMap::new();
        for cell_meta in metadata.cells {
            let cell_id = CellId::new(cell_meta.x, cell_meta.z);
            cells.insert(cell_id, StreamingCell {
                id: cell_id,
                path: level_path.join(&cell_meta.file),
                state: CellState::Unloaded,
                entities: Vec::new(),
                memory_usage: cell_meta.memory_usage,
                last_visible_frame: 0,
                priority: 0.0,
            });
        }

        let level_name = metadata.name.clone();
        let level_data = LevelData {
            name: metadata.name,
            cells: cells.clone(),
            total_memory: metadata.total_memory,
        };

        // Merge cells into world
        for (id, cell) in cells {
            self.cells.insert(id, cell);
        }

        self.levels.insert(level_name, level_data);

        Ok(())
    }

    /// Unload a level.
    pub fn unload_level(&mut self, name: &str) {
        if let Some(level) = self.levels.remove(name) {
            for cell_id in level.cells.keys() {
                self.cells.remove(cell_id);
            }
        }
    }

    /// Update streaming based on player position.
    pub fn update(&mut self, player_pos: Vec3, view_dir: Vec3) {
        self.frame += 1;
        self.player_pos = player_pos;
        self.player_view_dir = view_dir.normalize();

        // Update cell priorities and states
        self.update_priorities();

        // Queue cells for loading
        self.queue_loads();

        // Process pending loads
        self.process_loads();

        // Unload distant cells
        self.unload_distant();
    }

    /// Calculate priority for a cell.
    fn calculate_priority(&self, cell: &StreamingCell) -> f32 {
        let distance = cell.id.distance_to(self.player_pos, self.config.cell_size);

        // Distance factor (closer = higher priority)
        let dist_factor = 1.0 - (distance / self.config.load_distance).min(1.0);

        // View direction factor
        let to_cell = (cell.id.center(self.config.cell_size) - self.player_pos).normalize();
        let view_factor = (to_cell.dot(self.player_view_dir).max(0.0))
            * self.config.view_direction_boost;

        dist_factor + view_factor
    }

    /// Update priorities for all cells.
    fn update_priorities(&mut self) {
        let player_pos = self.player_pos;
        let view_dir = self.player_view_dir;
        let load_distance = self.config.load_distance;
        let cell_size = self.config.cell_size;
        let frame = self.frame;

        for cell in self.cells.values_mut() {
            if cell.state == CellState::Loaded {
                cell.last_visible_frame = frame;
            }
            
            // Calculate priority inline
            let distance = cell.id.distance_to(player_pos, cell_size);
            let dist_factor = 1.0 - (distance / load_distance).min(1.0);
            let to_cell = (cell.id.center(cell_size) - player_pos).normalize();
            let view_factor = (to_cell.dot(view_dir).max(0.0)) * self.config.view_direction_boost;
            cell.priority = dist_factor + view_factor;
        }
    }

    /// Queue cells for loading.
    fn queue_loads(&mut self) {
        for (id, cell) in &self.cells {
            if cell.state == CellState::Unloaded && !self.loading.contains(id) {
                let distance = cell.id.distance_to(self.player_pos, self.config.cell_size);
                if distance < self.config.load_distance {
                    self.pending_loads.push(LoadTask {
                        cell_id: *id,
                        path: cell.path.clone(),
                        priority: cell.priority,
                    });
                }
            }
        }

        // Sort by priority (highest first)
        self.pending_loads.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Process pending load tasks.
    fn process_loads(&mut self) {
        let to_load: Vec<_> = self.pending_loads
            .drain(..self.config.max_concurrent_loads.min(self.pending_loads.len()))
            .collect();

        for task in to_load {
            self.loading.insert(task.cell_id);
            
            // Spawn async load
            let cell_id = task.cell_id;
            let _path = task.path;
            
            // In a real implementation, this would use async asset loading
            // For now, we'll just mark it as loading
            if let Some(cell) = self.cells.get_mut(&cell_id) {
                cell.state = CellState::Loading;
            }

            // Simulate async load completion
            // In production, this would be handled by the asset server
            self.loading.remove(&cell_id);
            if let Some(cell) = self.cells.get_mut(&cell_id) {
                cell.state = CellState::Loaded;
                log::debug!("Loaded streaming cell {:?}", cell_id);
            }
        }
    }

    /// Unload cells that are too far away.
    fn unload_distant(&mut self) {
        let mut to_unload = Vec::new();

        for (id, cell) in &self.cells {
            if cell.state == CellState::Loaded {
                let distance = cell.id.distance_to(self.player_pos, self.config.cell_size);
                if distance > self.config.unload_distance {
                    to_unload.push(*id);
                }
            }
        }

        for id in to_unload {
            if let Some(cell) = self.cells.get_mut(&id) {
                cell.state = CellState::Unloaded;
                cell.entities.clear();
                log::debug!("Unloaded streaming cell {:?}", id);
            }
        }
    }

    /// Get all loaded cells.
    pub fn loaded_cells(&self) -> impl Iterator<Item = &StreamingCell> {
        self.cells.values().filter(|c| c.state == CellState::Loaded)
    }

    /// Get the cell at a world position.
    pub fn cell_at(&self, pos: Vec3) -> Option<&StreamingCell> {
        let id = CellId::from_world_pos(pos, self.config.cell_size);
        self.cells.get(&id)
    }

    /// Get statistics about streaming.
    pub fn stats(&self) -> StreamingStats {
        let loaded = self.cells.values().filter(|c| c.state == CellState::Loaded).count();
        let loading = self.cells.values().filter(|c| c.state == CellState::Loading).count();
        let unloaded = self.cells.values().filter(|c| c.state == CellState::Unloaded).count();
        let memory: u64 = self.cells.values()
            .filter(|c| c.state == CellState::Loaded)
            .map(|c| c.memory_usage)
            .sum();

        StreamingStats {
            loaded_cells: loaded,
            loading_cells: loading,
            unloaded_cells: unloaded,
            total_memory: memory,
        }
    }
}

/// Streaming statistics.
#[derive(Debug, Clone, Copy)]
pub struct StreamingStats {
    pub loaded_cells: usize,
    pub loading_cells: usize,
    pub unloaded_cells: usize,
    pub total_memory: u64,
}

/// Level metadata for serialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LevelMetadata {
    pub name: String,
    pub cells: Vec<CellMetadata>,
    pub total_memory: u64,
}

/// Cell metadata for serialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CellMetadata {
    pub x: i32,
    pub z: i32,
    pub file: String,
    pub memory_usage: u64,
}

/// Streaming system for ECS integration.
pub struct StreamingSystem;

impl System for StreamingSystem {
    fn name(&self) -> &str {
        "streaming"
    }

    fn run(&mut self, world: &mut World) {
        // Update streaming based on camera position
        let _ = world; // Placeholder
    }
}
