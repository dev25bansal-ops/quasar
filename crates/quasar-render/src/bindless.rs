//! Bindless texture atlas and GPU-driven material system.
//!
//! Provides:
//! - **TextureAtlas** — global mapping from texture ID → texture array index,
//!   backed by `TEXTURE_BINDING_ARRAY` when available.
//! - **MaterialDataBuffer** — `StorageBuffer` of material parameters indexed
//!   per draw call, allowing multi-draw-indirect without rebinding.
//! - Integration hooks for the existing GPU cull / indirect draw pipeline
//!   in `occlusion.rs`.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu;

// ── Bindless Texture Atlas ──────────────────────────────────────

pub const MAX_BINDLESS_TEXTURES: u32 = 1024;

/// Maps logical texture IDs to array-layer indices inside a single
/// `texture_2d_array` (or binding-array when the adapter supports it).
pub struct TextureAtlas {
    /// Logical texture ID → array index.
    id_to_index: HashMap<u64, u32>,
    /// Next array index to allocate.
    next_index: u32,
    /// Whether the adapter supports TEXTURE_BINDING_ARRAY.
    pub binding_array_supported: bool,
    /// The bind group layout entry for the texture array.
    pub bind_group_layout: Option<wgpu::BindGroupLayout>,
    /// The bind group referencing the texture array.
    pub bind_group: Option<wgpu::BindGroup>,
    /// Collected texture views for building the bind group.
    views: Vec<wgpu::TextureView>,
}

impl TextureAtlas {
    pub fn new(adapter: &wgpu::Adapter, device: &wgpu::Device) -> Self {
        let features = adapter.features();
        let binding_array_supported = features.contains(
            wgpu::Features::TEXTURE_BINDING_ARRAY
                | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        );

        let bind_group_layout = if binding_array_supported {
            Some(
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("bindless_texture_atlas_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: std::num::NonZeroU32::new(MAX_BINDLESS_TEXTURES),
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                }),
            )
        } else {
            None
        };

        Self {
            id_to_index: HashMap::new(),
            next_index: 0,
            binding_array_supported,
            bind_group_layout,
            bind_group: None,
            views: Vec::new(),
        }
    }

    /// Register a texture and return its array index.
    /// If the texture is already registered, returns the existing index.
    pub fn register(&mut self, texture_id: u64, view: wgpu::TextureView) -> u32 {
        if let Some(&idx) = self.id_to_index.get(&texture_id) {
            return idx;
        }
        let idx = self.next_index;
        self.next_index += 1;
        self.id_to_index.insert(texture_id, idx);
        self.views.push(view);
        idx
    }

    /// Look up the array index for a texture.
    pub fn index_of(&self, texture_id: u64) -> Option<u32> {
        self.id_to_index.get(&texture_id).copied()
    }

    /// Rebuild the bind group after new textures have been registered.
    pub fn rebuild_bind_group(&mut self, device: &wgpu::Device, sampler: &wgpu::Sampler) {
        if !self.binding_array_supported {
            return;
        }
        let Some(layout) = &self.bind_group_layout else {
            return;
        };
        if self.views.is_empty() {
            return;
        }

        let view_refs: Vec<&wgpu::TextureView> = self.views.iter().collect();

        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bindless_texture_atlas_bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&view_refs),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        }));
    }

    /// Number of registered textures.
    pub fn count(&self) -> u32 {
        self.next_index
    }
}

// ── GPU Material Data Buffer ────────────────────────────────────

pub const MAX_MATERIALS: usize = 4096;

/// Per-material data uploaded to a GPU storage buffer.
/// Each draw call references a material by its index into this buffer.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GpuMaterialData {
    pub base_color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive_strength: f32,
    /// Index into the bindless texture atlas for the albedo map (u32::MAX = no texture).
    pub albedo_tex_index: u32,
    /// Index for the normal map.
    pub normal_tex_index: u32,
    /// Index for the metallic-roughness map.
    pub mr_tex_index: u32,
    pub _pad: [u32; 2],
}

impl Default for GpuMaterialData {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            emissive_strength: 0.0,
            albedo_tex_index: u32::MAX,
            normal_tex_index: u32::MAX,
            mr_tex_index: u32::MAX,
            _pad: [0; 2],
        }
    }
}

/// Storage buffer holding all material data for GPU-driven rendering.
pub struct MaterialDataBuffer {
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    materials: Vec<GpuMaterialData>,
}

impl MaterialDataBuffer {
    pub fn new(device: &wgpu::Device) -> Self {
        let buf_size = (MAX_MATERIALS * std::mem::size_of::<GpuMaterialData>()) as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material_data_buffer"),
            size: buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material_data_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material_data_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            buffer,
            bind_group_layout,
            bind_group,
            materials: Vec::new(),
        }
    }

    /// Add a material. Returns its index.
    pub fn push(&mut self, mat: GpuMaterialData) -> u32 {
        let idx = self.materials.len() as u32;
        self.materials.push(mat);
        idx
    }

    /// Upload the material array to the GPU.
    pub fn upload(&self, queue: &wgpu::Queue) {
        if !self.materials.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.materials));
        }
    }

    pub fn count(&self) -> u32 {
        self.materials.len() as u32
    }

    /// Get a mutable reference to a material by index.
    pub fn get_mut(&mut self, index: u32) -> Option<&mut GpuMaterialData> {
        self.materials.get_mut(index as usize)
    }
}
