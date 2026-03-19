//! Virtual Shadow Maps - clipmap-based paged shadow mapping.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

pub const VSM_PAGE_SIZE: u32 = 128;
pub const VSM_CLIPMAP_LEVELS: u32 = 6;
pub const VSM_TILES_PER_LEVEL: u32 = 16;
pub const VSM_MAX_PHYSICAL_PAGES: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId {
    pub level: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalPage {
    pub index: u32,
    pub last_rendered: u64,
    pub last_invalidated: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct PageTableEntry {
    pub physical_index: u32,
    pub level: u32,
    pub tile_x: u32,
    pub tile_y: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VsmUniforms {
    pub level_view_proj: [[f32; 16]; VSM_CLIPMAP_LEVELS as usize],
    pub level_count: u32,
    pub page_size: u32,
    pub tiles_per_level: u32,
    pub _pad: u32,
}

pub struct PageCache {
    mapping: HashMap<PageId, PhysicalPage>,
    next_free: u32,
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

    pub fn request(&mut self, id: PageId, frame: u64) -> (u32, bool) {
        if let Some(pp) = self.mapping.get_mut(&id) {
            pp.last_rendered = frame;
            return (pp.index, false);
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
                last_invalidated: frame,
            },
        );
        self.lru.push((id, frame));

        (phys, true)
    }

    fn evict(&mut self) -> u32 {
        if let Some((id, _)) = self.lru.first().copied() {
            self.lru.remove(0);
            if let Some(pp) = self.mapping.remove(&id) {
                return pp.index;
            }
        }
        0
    }

    pub fn to_gpu_table(&self) -> Vec<PageTableEntry> {
        self.mapping
            .iter()
            .map(|(id, pp)| PageTableEntry {
                physical_index: pp.index,
                level: id.level,
                tile_x: id.x,
                tile_y: id.y,
            })
            .collect()
    }
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new()
    }
}

pub struct VirtualShadowSystem {
    cache: PageCache,
    page_table_buffer: Option<wgpu::Buffer>,
    shadow_atlas: Option<wgpu::Texture>,
    #[allow(dead_code)]
    uniforms: VsmUniforms,
}

impl VirtualShadowSystem {
    pub fn new() -> Self {
        Self {
            cache: PageCache::new(),
            page_table_buffer: None,
            shadow_atlas: None,
            uniforms: VsmUniforms {
                level_view_proj: [[0.0; 16]; VSM_CLIPMAP_LEVELS as usize],
                level_count: VSM_CLIPMAP_LEVELS,
                page_size: VSM_PAGE_SIZE,
                tiles_per_level: VSM_TILES_PER_LEVEL,
                _pad: 0,
            },
        }
    }

    pub fn create_resources(&mut self, device: &wgpu::Device) {
        let atlas_size = (VSM_MAX_PHYSICAL_PAGES as f32).sqrt() as u32 * VSM_PAGE_SIZE;

        self.shadow_atlas = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("VSM Shadow Atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));

        self.page_table_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VSM Page Table"),
            size: (VSM_CLIPMAP_LEVELS * VSM_TILES_PER_LEVEL * VSM_TILES_PER_LEVEL) as u64
                * std::mem::size_of::<PageTableEntry>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }

    pub fn update(&mut self, _device: &wgpu::Device, queue: &wgpu::Queue, _frame: u64) {
        if let Some(buf) = &self.page_table_buffer {
            let table = self.cache.to_gpu_table();
            let bytes: Vec<u8> = table
                .iter()
                .flat_map(|e| bytemuck::bytes_of(e).to_vec())
                .collect();
            queue.write_buffer(buf, 0, &bytes);
        }
    }
}

impl Default for VirtualShadowSystem {
    fn default() -> Self {
        Self::new()
    }
}
