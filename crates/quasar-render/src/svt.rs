//! Sparse Virtual Textures (SVT)
//!
//! Divides textures into 128×128 tiles. A feedback pass identifies visible
//! tiles, a background thread streams them from disk to a VRAM tile pool,
//! and a page table GPU buffer maps virtual → physical tile addresses.

#![allow(clippy::expect_used)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use wgpu;

/// Tile size in texels (each axis).
pub const SVT_TILE_SIZE: u32 = 128;

/// Maximum number of physical tiles resident in the GPU pool at once.
const DEFAULT_MAX_PHYSICAL_TILES: u32 = 1024;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A virtual tile address: (mip_level, tile_x, tile_y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VirtualTileId {
    pub mip: u32,
    pub x: u32,
    pub y: u32,
}

/// A physical slot index into the tile pool texture array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalSlot(pub u32);

/// Feedback entry produced by the GPU feedback pass — one per visible tile.
#[derive(Debug, Clone, Copy)]
pub struct FeedbackEntry {
    pub tile: VirtualTileId,
}

/// Maps virtual tiles to physical pool slots.
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct PageTableEntry {
    /// Physical tile X in the pool atlas.
    pub phys_x: u32,
    /// Physical tile Y in the pool atlas.
    pub phys_y: u32,
    /// 1 if resident, 0 otherwise.
    pub resident: u32,
    pub _pad: u32,
}

// ---------------------------------------------------------------------------
// Tile Pool (GPU-side physical tile cache)
// ---------------------------------------------------------------------------

/// A pool of physical tile slots backed by a large GPU texture.
pub struct TilePool {
    /// Pool dimensions in tiles (pool is `tiles_per_row × tiles_per_row` tiles).
    pub tiles_per_row: u32,
    pub max_tiles: u32,
    /// Free slot indices.
    free_slots: VecDeque<u32>,
    /// LRU tracking: maps virtual tile → (physical_slot, last_used_frame).
    resident: HashMap<VirtualTileId, (PhysicalSlot, u64)>,
}

impl TilePool {
    pub fn new(max_tiles: u32) -> Self {
        let tiles_per_row = (max_tiles as f32).sqrt().ceil() as u32;
        let total = tiles_per_row * tiles_per_row;
        let free_slots = (0..total).collect();
        Self {
            tiles_per_row,
            max_tiles: total,
            free_slots,
            resident: HashMap::new(),
        }
    }

    /// Try to allocate a physical slot for a virtual tile, evicting LRU if full.
    pub fn allocate(&mut self, tile: VirtualTileId, frame: u64) -> PhysicalSlot {
        // Already resident — just touch.
        if let Some((slot, last)) = self.resident.get_mut(&tile) {
            *last = frame;
            return *slot;
        }

        let slot_idx = if let Some(idx) = self.free_slots.pop_front() {
            idx
        } else {
            // Evict LRU tile.
            let (&evict_tile, _) = self
                .resident
                .iter()
                .min_by_key(|(_, (_, last))| *last)
                .expect("pool must have at least one entry");
            let (evicted_slot, _) = self
                .resident
                .remove(&evict_tile)
                .expect("evict_tile must exist");
            evicted_slot.0
        };

        let slot = PhysicalSlot(slot_idx);
        self.resident.insert(tile, (slot, frame));
        slot
    }

    /// Look up the physical slot for a virtual tile (if resident).
    pub fn lookup(&self, tile: &VirtualTileId) -> Option<PhysicalSlot> {
        self.resident.get(tile).map(|(s, _)| *s)
    }

    /// Convert a linear slot index to (tile_x, tile_y) in the pool atlas.
    pub fn slot_to_xy(&self, slot: PhysicalSlot) -> (u32, u32) {
        (slot.0 % self.tiles_per_row, slot.0 / self.tiles_per_row)
    }
}

// ---------------------------------------------------------------------------
// Background tile streamer
// ---------------------------------------------------------------------------

/// A completed tile load from the background thread.
struct TileLoadResult {
    tile: VirtualTileId,
    data: Vec<u8>,
}

/// Manages background I/O for tile streaming.
pub struct TileStreamer {
    /// Tiles that have been requested but not yet loaded.
    pending: Arc<Mutex<HashSet<VirtualTileId>>>,
    /// Completed loads ready to be uploaded.
    completed: Arc<Mutex<Vec<TileLoadResult>>>,
    /// Base directory containing the virtual texture tile files.
    base_path: PathBuf,
}

impl TileStreamer {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashSet::new())),
            completed: Arc::new(Mutex::new(Vec::new())),
            base_path,
        }
    }

    /// Request a tile to be loaded in the background.
    pub fn request(&self, tile: VirtualTileId) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending.contains(&tile) {
            return;
        }
        pending.insert(tile);

        let base = self.base_path.clone();
        let completed = Arc::clone(&self.completed);
        let pending_clone = Arc::clone(&self.pending);

        rayon::spawn(move || {
            let path = base.join(format!("tile_{}_{}_m{}.raw", tile.x, tile.y, tile.mip));
            let data = std::fs::read(&path).unwrap_or_else(|_| {
                // Fallback: generate a solid-colour placeholder tile.
                vec![128u8; (SVT_TILE_SIZE * SVT_TILE_SIZE * 4) as usize]
            });

            completed
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(TileLoadResult { tile, data });
            pending_clone
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&tile);
        });
    }

    /// Drain completed tile loads.
    pub fn drain_completed(&self) -> Vec<(VirtualTileId, Vec<u8>)> {
        let mut completed = self.completed.lock().unwrap_or_else(|e| e.into_inner());
        completed.drain(..).map(|r| (r.tile, r.data)).collect()
    }
}

// ---------------------------------------------------------------------------
// SVT System (orchestrator)
// ---------------------------------------------------------------------------

/// Central sparse virtual texture manager.
///
/// Workflow each frame:
/// 1. User calls `process_feedback` with visible tiles from the GPU read-back.
/// 2. The streamer loads missing tiles in the background.
/// 3. User calls `upload_ready_tiles` to push completed tiles to the GPU pool.
/// 4. User calls `build_page_table` to get an updated page table buffer.
pub struct SvtSystem {
    pub pool: TilePool,
    pub streamer: TileStreamer,
    /// Current frame number (for LRU tracking).
    pub frame: u64,
    /// Virtual texture dimensions in tiles per axis at mip 0.
    pub virtual_tiles_x: u32,
    pub virtual_tiles_y: u32,
}

impl SvtSystem {
    pub fn new(
        virtual_width: u32,
        virtual_height: u32,
        tile_base_path: PathBuf,
        max_physical_tiles: Option<u32>,
    ) -> Self {
        let max_tiles = max_physical_tiles.unwrap_or(DEFAULT_MAX_PHYSICAL_TILES);
        Self {
            pool: TilePool::new(max_tiles),
            streamer: TileStreamer::new(tile_base_path),
            frame: 0,
            virtual_tiles_x: virtual_width.div_ceil(SVT_TILE_SIZE),
            virtual_tiles_y: virtual_height.div_ceil(SVT_TILE_SIZE),
        }
    }

    /// Process GPU feedback — request loading for any non-resident visible tiles.
    pub fn process_feedback(&mut self, feedback: &[FeedbackEntry]) {
        for entry in feedback {
            if self.pool.lookup(&entry.tile).is_none() {
                self.streamer.request(entry.tile);
            } else {
                // Touch for LRU.
                self.pool.allocate(entry.tile, self.frame);
            }
        }
    }

    /// Upload tiles that the background thread has finished loading.
    /// Returns the list of (virtual_tile, physical_slot, rgba_data) ready for
    /// the caller to issue `queue.write_texture` into the pool atlas.
    pub fn upload_ready_tiles(&mut self) -> Vec<(VirtualTileId, PhysicalSlot, Vec<u8>)> {
        let completed = self.streamer.drain_completed();
        let mut uploads = Vec::with_capacity(completed.len());
        for (tile, data) in completed {
            let slot = self.pool.allocate(tile, self.frame);
            uploads.push((tile, slot, data));
        }
        uploads
    }

    /// Build the page table buffer (one entry per virtual tile at mip 0).
    pub fn build_page_table(&self) -> Vec<PageTableEntry> {
        let count = (self.virtual_tiles_x * self.virtual_tiles_y) as usize;
        let mut table = vec![PageTableEntry::default(); count];
        for (&vtile, &(slot, _)) in &self.pool.resident {
            if vtile.mip != 0 {
                continue; // page table is mip-0 only in this implementation
            }
            let idx = (vtile.y * self.virtual_tiles_x + vtile.x) as usize;
            if idx < table.len() {
                let (px, py) = self.pool.slot_to_xy(slot);
                table[idx] = PageTableEntry {
                    phys_x: px,
                    phys_y: py,
                    resident: 1,
                    _pad: 0,
                };
            }
        }
        table
    }

    /// Advance the frame counter (call once per frame).
    pub fn advance_frame(&mut self) {
        self.frame += 1;
    }
}

// ---------------------------------------------------------------------------
// GPU Feedback Pass – renders tile IDs to a small render target for readback
// ---------------------------------------------------------------------------

/// Size of the feedback render target (much smaller than the full frame).
pub const FEEDBACK_RT_SIZE: u32 = 128;

/// GPU-side feedback buffer entry (matches the R32G32B32A32Uint format).
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub struct GpuFeedbackTexel {
    pub tile_x: u32,
    pub tile_y: u32,
    pub mip: u32,
    pub _pad: u32,
}

/// A GPU feedback pass that renders virtual-tile coordinates to a small
/// render target, then reads it back to the CPU for tile request processing.
pub struct GpuFeedbackPass {
    /// Low-res render target storing tile IDs per pixel.
    pub feedback_texture: wgpu::Texture,
    pub feedback_view: wgpu::TextureView,
    /// Staging buffer for CPU read-back of the feedback texture.
    pub readback_buffer: wgpu::Buffer,
    pub width: u32,
    pub height: u32,
}

impl GpuFeedbackPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let width = FEEDBACK_RT_SIZE;
        let height = FEEDBACK_RT_SIZE;
        let feedback_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("svt_feedback_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let feedback_view = feedback_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bytes_per_row = width * 16; // 4 × u32 per texel
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("svt_feedback_readback"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            feedback_texture,
            feedback_view,
            readback_buffer,
            width,
            height,
        }
    }

    /// Encode a copy from the feedback render target to the readback buffer.
    pub fn encode_readback(&self, encoder: &mut wgpu::CommandEncoder) {
        let bytes_per_row = self.width * 16;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.feedback_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Parse the readback buffer (after mapping) into unique feedback entries.
    /// The caller must map the buffer before calling this.
    pub fn parse_feedback(&self, mapped_data: &[u8]) -> Vec<FeedbackEntry> {
        let texels: &[GpuFeedbackTexel] = bytemuck::cast_slice(mapped_data);

        let mut seen = HashSet::new();
        let mut entries = Vec::new();

        for texel in texels {
            // Skip zero / empty entries.
            if texel.tile_x == 0 && texel.tile_y == 0 && texel.mip == 0 {
                continue;
            }
            let tile = VirtualTileId {
                mip: texel.mip,
                x: texel.tile_x,
                y: texel.tile_y,
            };
            if seen.insert(tile) {
                entries.push(FeedbackEntry { tile });
            }
        }
        entries
    }
}

// ---------------------------------------------------------------------------
// VirtualTexture2D – user-facing asset type
// ---------------------------------------------------------------------------

/// A virtual texture asset that can be loaded through the asset server.
///
/// Wraps the virtual-tile metadata and delegates actual tile data to the
/// [`SvtSystem`] which manages the physical tile pool.
pub struct VirtualTexture2D {
    /// Unique asset id.
    pub id: u64,
    /// Full dimensions of the texture (mip 0) in texels.
    pub width: u32,
    pub height: u32,
    /// Number of mip levels covered.
    pub mip_levels: u32,
    /// Base directory containing the pre-split tile files.
    pub tile_base_path: PathBuf,
}

impl VirtualTexture2D {
    pub fn new(id: u64, width: u32, height: u32, tile_base_path: PathBuf) -> Self {
        let mip_levels = (width.max(height) as f32).log2().floor() as u32 + 1;
        Self {
            id,
            width,
            height,
            mip_levels,
            tile_base_path,
        }
    }

    /// Virtual tile grid dimensions at a given mip level.
    pub fn tiles_at_mip(&self, mip: u32) -> (u32, u32) {
        let w = (self.width >> mip).max(1);
        let h = (self.height >> mip).max(1);
        (w.div_ceil(SVT_TILE_SIZE), h.div_ceil(SVT_TILE_SIZE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_pool_allocate_and_lookup() {
        let mut pool = TilePool::new(4);
        let tile = VirtualTileId { mip: 0, x: 0, y: 0 };
        let slot = pool.allocate(tile, 0);
        assert_eq!(pool.lookup(&tile), Some(slot));
    }

    #[test]
    fn tile_pool_evicts_lru() {
        let mut pool = TilePool::new(4);
        let total = pool.max_tiles;
        // Fill all slots.
        for i in 0..total {
            pool.allocate(VirtualTileId { mip: 0, x: i, y: 0 }, i as u64);
        }
        assert!(pool.free_slots.is_empty());
        // Allocate one more — should evict the oldest (frame 0).
        let new_tile = VirtualTileId {
            mip: 0,
            x: 999,
            y: 0,
        };
        pool.allocate(new_tile, total as u64);
        assert!(pool.lookup(&new_tile).is_some());
        // The tile at frame 0 should be evicted.
        assert!(pool.lookup(&VirtualTileId { mip: 0, x: 0, y: 0 }).is_none());
    }

    #[test]
    fn page_table_build() {
        let mut sys = SvtSystem {
            pool: TilePool::new(16),
            streamer: TileStreamer::new(PathBuf::from("/tmp")),
            frame: 0,
            virtual_tiles_x: 4,
            virtual_tiles_y: 4,
        };
        let tile = VirtualTileId { mip: 0, x: 1, y: 2 };
        sys.pool.allocate(tile, 0);
        let table = sys.build_page_table();
        let idx = (2 * 4 + 1) as usize;
        assert_eq!(table[idx].resident, 1);
    }
}
