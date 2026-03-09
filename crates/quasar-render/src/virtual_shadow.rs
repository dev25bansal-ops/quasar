//! Virtual Shadow Maps — clipmap-based paged shadow mapping.
//!
//! Instead of rendering the entire shadow map every frame, the scene is
//! divided into a clipmap of pages that are allocated on demand and cached
//! across frames.  Only pages that are visible to the camera and
//! invalidated by moving geometry are re-rendered.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

/// Size of a single virtual shadow page in texels.
pub const VSM_PAGE_SIZE: u32 = 128;
/// Number of clipmap levels.
pub const VSM_CLIPMAP_LEVELS: u32 = 6;
/// Tiles per axis at the finest level.
pub const VSM_TILES_PER_LEVEL: u32 = 16;
/// Maximum number of physical pages in the page pool.
pub const VSM_MAX_PHYSICAL_PAGES: u32 = 256;

// ── Types ────────────────────────────────────────────────────────────

/// Identifies a single page in the virtual shadow map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId {
    pub level: u32,
    pub x: u32,
    pub y: u32,
}

/// A physical page slot in the shadow atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPage {
    pub index: u32,
    /// Frame number at which this page was last rendered.
    pub last_rendered: u64,
}

/// GPU-side per-page info so shaders know which physical slot to read.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PageTableEntry {
    /// Physical page index (u32::MAX = not resident).
    pub physical_index: u32,
    /// Clipmap level.
    pub level: u32,
    pub tile_x: u32,
    pub tile_y: u32,
}

/// GPU uniform for the VSM system.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VsmUniforms {
    /// Light view-projection at each clipmap level (column-major mat4 × 6).
    pub level_view_proj: [[f32; 16]; VSM_CLIPMAP_LEVELS as usize],
    /// Number of valid levels.
    pub level_count: u32,
    pub page_size: u32,
    pub tiles_per_level: u32,
    pub _pad: u32,
}

// ── Page cache ───────────────────────────────────────────────────────

/// Manages the mapping between virtual pages and physical atlas slots.
pub struct PageCache {
    /// Virtual → physical mapping.
    mapping: HashMap<PageId, PhysicalPage>,
    /// Next free physical slot.
    next_free: u32,
    /// LRU list of physical pages for eviction (oldest first).
    lru: Vec<(PageId, u64)>,
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            mapping: HashMap::new(),
            next_free: 0,
            lru: Vec::new(),
        }
    }

    /// Request a physical page for the given `PageId`.  Returns the physical
    /// index and whether the page is newly allocated (needs rendering).
    pub fn request(&mut self, id: PageId, frame: u64) -> (u32, bool) {
        if let Some(pp) = self.mapping.get_mut(&id) {
            pp.last_rendered = frame;
            let idx = pp.index;
            self.touch_lru(id, frame);
            return (idx, false);
        }

        let phys = if self.next_free < VSM_MAX_PHYSICAL_PAGES {
            let idx = self.next_free;
            self.next_free += 1;
            idx
        } else {
            self.evict()
        };

        self.mapping.insert(
            id,
            PhysicalPage {
                index: phys,
                last_rendered: frame,
            },
        );
        self.lru.push((id, frame));
        (phys, true)
    }

    /// Mark all pages as potentially stale so they can be rechecked.
    pub fn invalidate_all(&mut self) {
        self.mapping.clear();
        self.lru.clear();
        self.next_free = 0;
    }

    /// Build a page table buffer suitable for GPU upload.
    pub fn build_page_table(&self) -> Vec<PageTableEntry> {
        let total = (VSM_CLIPMAP_LEVELS * VSM_TILES_PER_LEVEL * VSM_TILES_PER_LEVEL) as usize;
        let mut table = vec![
            PageTableEntry {
                physical_index: u32::MAX,
                level: 0,
                tile_x: 0,
                tile_y: 0,
            };
            total
        ];
        for (&pid, pp) in &self.mapping {
            let idx = (pid.level * VSM_TILES_PER_LEVEL * VSM_TILES_PER_LEVEL
                + pid.y * VSM_TILES_PER_LEVEL
                + pid.x) as usize;
            if idx < total {
                table[idx] = PageTableEntry {
                    physical_index: pp.index,
                    level: pid.level,
                    tile_x: pid.x,
                    tile_y: pid.y,
                };
            }
        }
        table
    }

    // ── internals ────────────────────────────────────────────────────

    fn touch_lru(&mut self, id: PageId, frame: u64) {
        if let Some(entry) = self.lru.iter_mut().find(|(p, _)| *p == id) {
            entry.1 = frame;
        }
    }

    fn evict(&mut self) -> u32 {
        // Evict the least-recently-used page.
        self.lru.sort_by_key(|(_, f)| *f);
        let (evicted_id, _) = self.lru.remove(0);
        let pp = self.mapping.remove(&evicted_id).expect("LRU/mapping mismatch");
        pp.index
    }
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Virtual Shadow Map manager ───────────────────────────────────────

/// The main VSM manager.  Owns the physical atlas texture and page cache.
pub struct VirtualShadowMap {
    pub page_cache: PageCache,
    /// Physical atlas: a 2D texture atlas of `sqrt(MAX_PAGES) × sqrt(MAX_PAGES)`
    /// pages, each `PAGE_SIZE × PAGE_SIZE`.
    pub atlas_texture: wgpu::Texture,
    pub atlas_view: wgpu::TextureView,
    pub page_table_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: Option<wgpu::BindGroup>,
    pub frame: u64,
}

impl VirtualShadowMap {
    pub fn new(device: &wgpu::Device) -> Self {
        let atlas_dim = 16u32; // 16×16 pages = 256 pages
        let atlas_px = atlas_dim * VSM_PAGE_SIZE;
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("VSM Atlas"),
            size: wgpu::Extent3d {
                width: atlas_px,
                height: atlas_px,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let page_table_size = (VSM_CLIPMAP_LEVELS * VSM_TILES_PER_LEVEL * VSM_TILES_PER_LEVEL) as u64
            * std::mem::size_of::<PageTableEntry>() as u64;
        let page_table_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VSM Page Table"),
            size: page_table_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VSM Uniforms"),
            size: std::mem::size_of::<VsmUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("VSM BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Depth,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(
                            wgpu::SamplerBindingType::Comparison,
                        ),
                        count: None,
                    },
                ],
            });

        Self {
            page_cache: PageCache::new(),
            atlas_texture,
            atlas_view,
            page_table_buffer,
            uniform_buffer,
            bind_group_layout,
            bind_group: None,
            frame: 0,
        }
    }

    /// Determine which pages are needed this frame and return the list
    /// of pages that need to be (re-)rendered.
    pub fn update_pages(
        &mut self,
        visible_pages: &[PageId],
    ) -> Vec<(PageId, u32)> {
        self.frame += 1;
        let mut to_render = Vec::new();
        for &pid in visible_pages {
            let (phys, needs_render) = self.page_cache.request(pid, self.frame);
            if needs_render {
                to_render.push((pid, phys));
            }
        }
        to_render
    }

    /// Upload the current page table to the GPU.
    pub fn upload_page_table(&self, queue: &wgpu::Queue) {
        let table = self.page_cache.build_page_table();
        queue.write_buffer(&self.page_table_buffer, 0, bytemuck::cast_slice(&table));
    }

    /// Rebuild the bind group (call after atlas or buffer changes).
    pub fn rebuild_bind_group(
        &mut self,
        device: &wgpu::Device,
        comparison_sampler: &wgpu::Sampler,
    ) {
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("VSM Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.page_table_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(comparison_sampler),
                },
            ],
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_cache_allocates_and_reuses() {
        let mut cache = PageCache::new();
        let pid = PageId { level: 0, x: 0, y: 0 };

        let (phys, fresh) = cache.request(pid, 1);
        assert!(fresh);
        assert_eq!(phys, 0);

        let (phys2, fresh2) = cache.request(pid, 2);
        assert!(!fresh2);
        assert_eq!(phys2, 0);
    }

    #[test]
    fn page_cache_evicts_lru() {
        let mut cache = PageCache::new();
        // Fill all physical pages.
        for i in 0..VSM_MAX_PHYSICAL_PAGES {
            let pid = PageId { level: 0, x: i, y: 0 };
            let (phys, fresh) = cache.request(pid, i as u64);
            assert!(fresh);
            assert_eq!(phys, i);
        }
        // Next allocation should evict the oldest (frame 0).
        let new_pid = PageId { level: 1, x: 0, y: 0 };
        let (phys, fresh) = cache.request(new_pid, VSM_MAX_PHYSICAL_PAGES as u64);
        assert!(fresh);
        assert_eq!(phys, 0); // reused slot from evicted page
    }

    #[test]
    fn page_table_roundtrip() {
        let mut cache = PageCache::new();
        let pid = PageId { level: 2, x: 3, y: 5 };
        cache.request(pid, 1);
        let table = cache.build_page_table();
        let idx = (2 * VSM_TILES_PER_LEVEL * VSM_TILES_PER_LEVEL
            + 5 * VSM_TILES_PER_LEVEL
            + 3) as usize;
        assert_eq!(table[idx].physical_index, 0);
        assert_eq!(table[idx].level, 2);
    }
}
