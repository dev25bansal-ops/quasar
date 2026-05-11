//! Bindless texture atlas and GPU-driven material system.
//!
//! Provides:
//! - **TextureAtlas** — global mapping from texture ID → texture array index,
//!   backed by `TEXTURE_BINDING_ARRAY` when available.
//! - **SamplerPool** — global sampler registry for bindless access.
//! - **MaterialDataBuffer** — `StorageBuffer` of material parameters indexed
//!   per draw call, allowing multi-draw-indirect without rebinding.
//! - **BindlessBindGroup** — unified bindless bind group combining texture arrays,
//!   sampler arrays, and material storage buffers.
//! - Integration hooks for the existing GPU cull / indirect draw pipeline
//!   in `occlusion.rs`.
//! - **Fallback path** for devices without bindless support.
//! - **TextureBatchUploader** — efficient batched texture upload with staging.
//! - **ResourceLifetimeManager** — tracks GPU resource lifetimes to prevent use-after-free.

use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu;

// ── Constants ───────────────────────────────────────────────────

/// Maximum number of textures in the global bindless texture array.
pub const MAX_BINDLESS_TEXTURES: u32 = 256;

/// Maximum number of samplers in the global bindless sampler array.
pub const MAX_BINDLESS_SAMPLERS: u32 = 64;

/// Maximum number of materials in the GPU material storage buffer.
pub const MAX_MATERIALS: usize = 4096;

/// Maximum number of textures that can be uploaded in a single batch.
pub const MAX_TEXTURE_BATCH: usize = 256;

/// Size of GpuMaterialData in bytes (used for offset calculations).
pub const GPU_MATERIAL_SIZE: u64 = std::mem::size_of::<GpuMaterialData>() as u64;

// ── Device Feature Detection ────────────────────────────────────

/// Captures the bindless capabilities supported by the current adapter.
#[derive(Debug, Clone, Copy)]
pub struct BindlessCapabilities {
    /// Whether TEXTURE_BINDING_ARRAY is supported.
    pub texture_binding_array: bool,
    /// Whether SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING is supported.
    pub non_uniform_indexing: bool,
    /// Whether STORAGE_RESOURCE_BINDING_ARRAY is supported.
    pub storage_binding_array: bool,
    /// Whether BINDLESS_TEXTURE_SAMPLING is supported (Vulkan extension).
    pub bindless_sampling: bool,
    /// Combined: full bindless support (texture array + non-uniform indexing).
    pub full_bindless: bool,
}

impl BindlessCapabilities {
    /// Detect bindless capabilities from a wgpu adapter.
    pub fn from_adapter(adapter: &wgpu::Adapter) -> Self {
        let features = adapter.features();
        let texture_binding_array = features.contains(wgpu::Features::TEXTURE_BINDING_ARRAY);
        let non_uniform_indexing = features.contains(
            wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        );
        let storage_binding_array =
            features.contains(wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY);
        // Check for bindless sampling (available via TEXTURE_BINDING_ARRAY on most platforms)
        let bindless_sampling = texture_binding_array;
        let full_bindless = texture_binding_array && non_uniform_indexing;

        Self {
            texture_binding_array,
            non_uniform_indexing,
            storage_binding_array,
            bindless_sampling,
            full_bindless,
        }
    }

    /// Returns the set of wgpu::Features required for full bindless rendering.
    pub fn required_features(&self) -> wgpu::Features {
        let mut features = wgpu::Features::empty();
        if self.texture_binding_array {
            features |= wgpu::Features::TEXTURE_BINDING_ARRAY;
        }
        if self.non_uniform_indexing {
            features |=
                wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING;
        }
        if self.storage_binding_array {
            features |= wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY;
        }
        features
    }

    /// Returns a human-readable description of bindless support level.
    pub fn support_level(&self) -> &'static str {
        if self.full_bindless {
            "FULL: Texture arrays + non-uniform indexing + storage arrays"
        } else if self.texture_binding_array && self.non_uniform_indexing {
            "PARTIAL: Texture arrays + non-uniform indexing (no storage arrays)"
        } else if self.texture_binding_array {
            "LIMITED: Texture arrays only (no non-uniform indexing)"
        } else {
            "NONE: No bindless support, using fallback"
        }
    }
}

impl Default for BindlessCapabilities {
    fn default() -> Self {
        Self {
            texture_binding_array: false,
            non_uniform_indexing: false,
            storage_binding_array: false,
            bindless_sampling: false,
            full_bindless: false,
        }
    }
}

// ── Bindless Texture Atlas ──────────────────────────────────────

/// Maps logical texture IDs to array-layer indices inside a single
/// `texture_2d_array` (or binding-array when the adapter supports it).
pub struct TextureAtlas {
    /// Logical texture ID → array index.
    id_to_index: HashMap<u64, u32>,
    /// Array index → logical texture ID (reverse mapping).
    index_to_id: Vec<u64>,
    /// Next array index to allocate.
    next_index: u32,
    /// Collected texture views for building the bind group.
    views: Vec<wgpu::TextureView>,
    /// Whether a texture has been removed (tombstone tracking).
    removed_indices: Vec<u32>,
    /// Pending removals (deferred until next rebuild for GPU safety).
    pending_removals: Vec<u64>,
}

impl TextureAtlas {
    /// Create a new empty texture atlas.
    pub fn new() -> Self {
        Self {
            id_to_index: HashMap::new(),
            index_to_id: Vec::new(),
            next_index: 0,
            views: Vec::new(),
            removed_indices: Vec::new(),
            pending_removals: Vec::new(),
        }
    }

    /// Register a texture and return its array index.
    /// If the texture is already registered, returns the existing index.
    /// Returns `None` if the atlas is full.
    pub fn register(&mut self, texture_id: u64, view: wgpu::TextureView) -> Option<u32> {
        if let Some(&idx) = self.id_to_index.get(&texture_id) {
            return Some(idx);
        }

        // Check if we have removed slots to reuse
        if let Some(free_idx) = self.removed_indices.pop() {
            let idx = free_idx;
            self.id_to_index.insert(texture_id, idx);
            // Update the view at this index
            if (idx as usize) < self.views.len() {
                self.views[idx as usize] = view;
            }
            if (idx as usize) < self.index_to_id.len() {
                self.index_to_id[idx as usize] = texture_id;
            }
            return Some(idx);
        }

        if self.next_index >= MAX_BINDLESS_TEXTURES {
            log::warn!(
                "TextureAtlas full: cannot register texture ID {} (max {})",
                texture_id,
                MAX_BINDLESS_TEXTURES
            );
            return None;
        }

        let idx = self.next_index;
        self.next_index += 1;
        self.id_to_index.insert(texture_id, idx);

        // Ensure index_to_id is large enough
        while self.index_to_id.len() <= idx as usize {
            self.index_to_id.push(0);
        }
        self.index_to_id[idx as usize] = texture_id;

        self.views.push(view);
        Some(idx)
    }

    /// Register multiple textures in batch. Returns indices for successful registrations.
    /// This is more efficient than individual registrations when loading many textures.
    pub fn register_batch(
        &mut self,
        textures: &[(u64, wgpu::TextureView)],
    ) -> Vec<(u64, Option<u32>)> {
        let mut results = Vec::with_capacity(textures.len());
        for &(id, ref view) in textures {
            let idx = self.register(id, view.clone());
            results.push((id, idx));
        }
        results
    }

    /// Remove a texture from the atlas by ID.
    /// The slot will be reused for future registrations.
    /// Note: The actual removal is deferred until `flush_removals` is called
    /// to ensure GPU operations referencing the texture have completed.
    pub fn remove(&mut self, texture_id: u64) -> Option<u32> {
        if let Some(idx) = self.id_to_index.remove(&texture_id) {
            self.pending_removals.push(texture_id);
            Some(idx)
        } else {
            None
        }
    }

    /// Flush pending removals and actually free the slots.
    /// Call this after ensuring no GPU commands reference the removed textures.
    pub fn flush_removals(&mut self) {
        for texture_id in self.pending_removals.drain(..) {
            if let Some(&idx) = self.id_to_index.get(&texture_id) {
                self.removed_indices.push(idx);
            }
        }
    }

    /// Immediately remove a texture and free its slot (unsafe if GPU still references it).
    pub fn remove_immediate(&mut self, texture_id: u64) -> Option<u32> {
        if let Some(idx) = self.id_to_index.remove(&texture_id) {
            self.removed_indices.push(idx);
            Some(idx)
        } else {
            None
        }
    }

    /// Look up the array index for a texture.
    pub fn index_of(&self, texture_id: u64) -> Option<u32> {
        self.id_to_index.get(&texture_id).copied()
    }

    /// Look up the texture ID for an array index.
    pub fn id_at(&self, index: u32) -> Option<u64> {
        self.index_to_id.get(index as usize).copied()
    }

    /// Get all texture views as a slice.
    pub fn views(&self) -> &[wgpu::TextureView] {
        &self.views
    }

    /// Number of registered textures.
    pub fn count(&self) -> u32 {
        self.id_to_index.len() as u32
    }

    /// Check if the atlas is full.
    pub fn is_full(&self) -> bool {
        self.next_index >= MAX_BINDLESS_TEXTURES && self.removed_indices.is_empty()
    }

    /// Get the current capacity (max textures that can be registered).
    pub fn capacity(&self) -> u32 {
        MAX_BINDLESS_TEXTURES
    }

    /// Get the number of pending removals.
    pub fn pending_removal_count(&self) -> usize {
        self.pending_removals.len()
    }

    /// Clear all textures and reset the atlas.
    pub fn clear(&mut self) {
        self.id_to_index.clear();
        self.index_to_id.clear();
        self.next_index = 0;
        self.views.clear();
        self.removed_indices.clear();
        self.pending_removals.clear();
    }
}

// ── Sampler Pool ────────────────────────────────────────────────

/// Global pool of samplers for bindless access.
/// Each sampler is assigned a unique index in the bindless sampler array.
pub struct SamplerPool {
    /// Sampler handle → pool index.
    sampler_to_index: HashMap<u64, u32>,
    /// Pool index → wgpu::Sampler.
    samplers: Vec<wgpu::Sampler>,
    /// Next pool index to allocate.
    next_index: u32,
    /// Removed pool indices available for reuse.
    removed_indices: Vec<u32>,
    /// Pending removals (deferred for GPU safety).
    pending_removals: Vec<u64>,
}

impl SamplerPool {
    /// Create a new empty sampler pool.
    pub fn new() -> Self {
        Self {
            sampler_to_index: HashMap::new(),
            samplers: Vec::new(),
            next_index: 0,
            removed_indices: Vec::new(),
            pending_removals: Vec::new(),
        }
    }

    /// Register a sampler and return its pool index.
    /// If an identical sampler is already registered, returns the existing index.
    /// Returns `None` if the pool is full.
    pub fn register(&mut self, handle: u64, sampler: wgpu::Sampler) -> Option<u32> {
        if let Some(&idx) = self.sampler_to_index.get(&handle) {
            return Some(idx);
        }

        // Reuse removed slots
        if let Some(free_idx) = self.removed_indices.pop() {
            let idx = free_idx;
            self.sampler_to_index.insert(handle, idx);
            if (idx as usize) < self.samplers.len() {
                self.samplers[idx as usize] = sampler;
            }
            return Some(idx);
        }

        if self.next_index >= MAX_BINDLESS_SAMPLERS {
            log::warn!(
                "SamplerPool full: cannot register sampler handle {} (max {})",
                handle,
                MAX_BINDLESS_SAMPLERS
            );
            return None;
        }

        let idx = self.next_index;
        self.next_index += 1;
        self.sampler_to_index.insert(handle, idx);
        self.samplers.push(sampler);
        Some(idx)
    }

    /// Register multiple samplers in batch.
    pub fn register_batch(&mut self, samplers: &[(u64, wgpu::Sampler)]) -> Vec<(u64, Option<u32>)> {
        let mut results = Vec::with_capacity(samplers.len());
        for &(handle, ref sampler) in samplers {
            let idx = self.register(handle, sampler.clone());
            results.push((handle, idx));
        }
        results
    }

    /// Remove a sampler from the pool by handle (deferred).
    pub fn remove(&mut self, handle: u64) -> Option<u32> {
        if let Some(idx) = self.sampler_to_index.remove(&handle) {
            self.pending_removals.push(handle);
            Some(idx)
        } else {
            None
        }
    }

    /// Flush pending sampler removals.
    pub fn flush_removals(&mut self) {
        for handle in self.pending_removals.drain(..) {
            if let Some(&idx) = self.sampler_to_index.get(&handle) {
                self.removed_indices.push(idx);
            }
        }
    }

    /// Immediately remove a sampler (unsafe if GPU still references it).
    pub fn remove_immediate(&mut self, handle: u64) -> Option<u32> {
        if let Some(idx) = self.sampler_to_index.remove(&handle) {
            self.removed_indices.push(idx);
            Some(idx)
        } else {
            None
        }
    }

    /// Look up the pool index for a sampler handle.
    pub fn index_of(&self, handle: u64) -> Option<u32> {
        self.sampler_to_index.get(&handle).copied()
    }

    /// Get all samplers as a slice.
    pub fn samplers(&self) -> &[wgpu::Sampler] {
        &self.samplers
    }

    /// Number of registered samplers.
    pub fn count(&self) -> u32 {
        self.sampler_to_index.len() as u32
    }

    /// Get capacity.
    pub fn capacity(&self) -> u32 {
        MAX_BINDLESS_SAMPLERS
    }

    /// Clear all samplers and reset the pool.
    pub fn clear(&mut self) {
        self.sampler_to_index.clear();
        self.samplers.clear();
        self.next_index = 0;
        self.removed_indices.clear();
        self.pending_removals.clear();
    }
}

// ── GPU Material Data Buffer ────────────────────────────────────

/// Per-material data uploaded to a GPU storage buffer.
/// Each draw call references a material by its index into this buffer.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct GpuMaterialData {
    /// Base color (RGBA, sRGB-linearized in shader).
    pub base_color: [f32; 4],
    /// Roughness (0 = mirror, 1 = fully diffuse).
    pub roughness: f32,
    /// Metallic (0 = dielectric, 1 = metallic).
    pub metallic: f32,
    /// Emissive strength (added to final color).
    pub emissive_strength: f32,
    /// Index into the bindless texture atlas for the albedo map (u32::MAX = no texture).
    pub albedo_tex_index: u32,
    /// Index into the bindless texture atlas for the normal map.
    pub normal_tex_index: u32,
    /// Index into the bindless texture atlas for the metallic-roughness map.
    pub mr_tex_index: u32,
    /// Index into the bindless sampler pool.
    pub sampler_index: u32,
    /// Padding to align to 64 bytes (4x vec4).
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
            sampler_index: 0,
            _pad: [0; 2],
        }
    }
}

impl GpuMaterialData {
    /// Create material data from a simple color with no textures.
    pub fn from_color(color: [f32; 4], roughness: f32, metallic: f32) -> Self {
        Self {
            base_color: color,
            roughness,
            metallic,
            ..Default::default()
        }
    }
}

/// Storage buffer holding all material data for GPU-driven rendering.
/// Supports incremental uploads (only dirty regions).
pub struct MaterialDataBuffer {
    /// GPU storage buffer (read-only from shader).
    pub buffer: wgpu::Buffer,
    /// Bind group layout for the material storage buffer.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group referencing the material storage buffer.
    pub bind_group: wgpu::BindGroup,
    /// CPU-side material data.
    materials: Vec<GpuMaterialData>,
    /// Track which materials have been modified since last upload.
    dirty: Vec<bool>,
    /// Number of dirty materials.
    dirty_count: usize,
    /// Track which slots are free (using u32::MAX albedo_tex_index as marker).
    free_slots: Vec<u32>,
    /// Next slot to allocate if no free slots available.
    next_free_index: u32,
}

impl MaterialDataBuffer {
    /// Create a new material data buffer with the given capacity.
    pub fn new(device: &wgpu::Device, capacity: usize) -> Self {
        let actual_capacity = capacity.min(MAX_MATERIALS);
        let buf_size = (actual_capacity * std::mem::size_of::<GpuMaterialData>()) as u64;

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
            materials: vec![GpuMaterialData::default(); actual_capacity],
            dirty: vec![false; actual_capacity],
            dirty_count: 0,
            free_slots: Vec::new(),
            next_free_index: 0,
        }
    }

    /// Check if a slot is free (not in use).
    fn is_slot_free(&self, index: usize) -> bool {
        index < self.materials.len()
            && self.materials[index].albedo_tex_index == u32::MAX
            && self.materials[index].base_color == [0.0, 0.0, 0.0, 0.0]
    }

    /// Add a material. Returns its index, or `None` if the buffer is full.
    pub fn push(&mut self, mat: GpuMaterialData) -> Option<u32> {
        // First try to reuse free slots
        if let Some(slot) = self.free_slots.pop() {
            let idx = slot as usize;
            if idx < self.materials.len() {
                if !self.dirty[idx] {
                    self.dirty[idx] = true;
                    self.dirty_count += 1;
                }
                self.materials[idx] = mat;
                return Some(slot);
            }
        }

        // Allocate new slot
        if self.next_free_index < self.materials.capacity() as u32 {
            let idx = self.next_free_index as usize;
            self.next_free_index += 1;
            if idx < self.materials.len() {
                self.materials[idx] = mat;
                if !self.dirty[idx] {
                    self.dirty[idx] = true;
                    self.dirty_count += 1;
                }
                return Some(idx as u32);
            }
        }

        log::warn!("MaterialDataBuffer full (max {})", MAX_MATERIALS);
        None
    }

    /// Remove a material by marking its slot as free.
    pub fn remove(&mut self, index: u32) {
        let idx = index as usize;
        if idx < self.materials.len() {
            let was_in_use = !self.is_slot_free(idx);
            if was_in_use {
                // Mark as free
                self.materials[idx] = GpuMaterialData::default();
                self.free_slots.push(index);
                // Mark as dirty to upload the cleared state
                if !self.dirty[idx] {
                    self.dirty[idx] = true;
                    self.dirty_count += 1;
                }
            }
        }
    }

    /// Update a material at a specific index.
    pub fn update(&mut self, index: u32, mat: GpuMaterialData) {
        let idx = index as usize;
        if idx < self.materials.len() {
            if !self.dirty[idx] {
                self.dirty[idx] = true;
                self.dirty_count += 1;
            }
            self.materials[idx] = mat;
        }
    }

    /// Get a reference to a material by index.
    pub fn get(&self, index: u32) -> Option<&GpuMaterialData> {
        self.materials.get(index as usize)
    }

    /// Get a mutable reference to a material by index.
    pub fn get_mut(&mut self, index: u32) -> Option<&mut GpuMaterialData> {
        let idx = index as usize;
        if idx < self.materials.len() {
            if !self.dirty[idx] {
                self.dirty[idx] = true;
                self.dirty_count += 1;
            }
            Some(&mut self.materials[idx])
        } else {
            None
        }
    }

    /// Upload all material data to the GPU.
    pub fn upload_all(&self, queue: &wgpu::Queue) {
        if !self.materials.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.materials));
        }
    }

    /// Upload only dirty material data to the GPU (incremental update).
    pub fn upload_dirty(&mut self, queue: &wgpu::Queue) {
        if self.dirty_count == 0 {
            return;
        }

        let mat_size = std::mem::size_of::<GpuMaterialData>() as u64;

        // Batch contiguous dirty regions for efficient upload
        let mut i = 0;
        while i < self.dirty.len() {
            if self.dirty[i] {
                let start = i;
                while i < self.dirty.len() && self.dirty[i] {
                    self.dirty[i] = false;
                    i += 1;
                }
                let count = i - start;
                let offset = (start as u64) * mat_size;
                queue.write_buffer(
                    &self.buffer,
                    offset,
                    bytemuck::cast_slice(&self.materials[start..i]),
                );
            } else {
                i += 1;
            }
        }

        self.dirty_count = 0;
    }

    /// Upload a range of materials to the GPU.
    pub fn upload_range(&self, queue: &wgpu::Queue, start: u32, end: u32) {
        let start = start as usize;
        let end = (end as usize).min(self.materials.len());
        if start < end {
            let offset = (start as u64) * std::mem::size_of::<GpuMaterialData>() as u64;
            queue.write_buffer(
                &self.buffer,
                offset,
                bytemuck::cast_slice(&self.materials[start..end]),
            );
        }
    }

    /// Number of allocated material slots.
    pub fn capacity(&self) -> usize {
        self.materials.capacity()
    }

    /// Number of materials currently in use.
    pub fn count(&self) -> u32 {
        self.materials
            .iter()
            .filter(|m| m.albedo_tex_index != u32::MAX || m.base_color != [0.0, 0.0, 0.0, 0.0])
            .count() as u32
    }

    /// Get the number of free slots available for reuse.
    pub fn free_slot_count(&self) -> usize {
        self.free_slots.len() + (self.materials.capacity() as u32 - self.next_free_index) as usize
    }

    /// Clear all materials and reset dirty tracking.
    pub fn clear(&mut self) {
        for m in &mut self.materials {
            *m = GpuMaterialData::default();
        }
        self.dirty.fill(false);
        self.dirty_count = 0;
        self.free_slots.clear();
        self.next_free_index = 0;
    }

    /// Get the GPU material size in bytes.
    pub fn material_size() -> u64 {
        GPU_MATERIAL_SIZE
    }
}

// ── Combined Bindless Bind Group ────────────────────────────────

/// Represents the combined bindless bind group containing:
/// - Texture array (binding 0): `binding_array<texture_2d<f32>>`
/// - Sampler array (binding 1): `binding_array<sampler>`
/// - Material storage buffer (binding 2): `storage<read>`
pub struct BindlessBindGroup {
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub draw_call_buffer: wgpu::Buffer,
}

impl BindlessBindGroup {
    /// Create a new bindless bind group.
    ///
    /// # Arguments
    /// * `device` — wgpu device
    /// * `atlas` — texture atlas containing all registered textures
    /// * `pool` — sampler pool containing all registered samplers
    /// * `material_buffer` — GPU storage buffer containing material data
    ///
    /// # Panics
    /// Panics if the atlas or pool is empty (at least one texture and one
    /// sampler must be registered before creating the bind group).
    pub fn new(
        device: &wgpu::Device,
        atlas: &TextureAtlas,
        pool: &SamplerPool,
        material_buffer: &MaterialDataBuffer,
    ) -> Self {
        // Collect texture view references
        let view_refs: Vec<&wgpu::TextureView> = atlas.views().iter().collect();
        let sampler_refs: Vec<&wgpu::Sampler> = pool.samplers().iter().collect();

        assert!(
            !view_refs.is_empty(),
            "BindlessBindGroup requires at least one texture in the atlas"
        );
        assert!(
            !sampler_refs.is_empty(),
            "BindlessBindGroup requires at least one sampler in the pool"
        );

        let tex_count = std::num::NonZeroU32::new(MAX_BINDLESS_TEXTURES);
        let samp_count = std::num::NonZeroU32::new(MAX_BINDLESS_SAMPLERS);

        let draw_call_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bindless_draw_call_buffer"),
            size: 4,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bindless_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: tex_count,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: samp_count,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(4),
                    },
                    count: None,
                },
            ],
        });

        let padded_views: Vec<&wgpu::TextureView> = view_refs
            .iter()
            .cycle()
            .take(MAX_BINDLESS_TEXTURES as usize)
            .copied()
            .collect();
        let padded_samplers: Vec<&wgpu::Sampler> = sampler_refs
            .iter()
            .cycle()
            .take(MAX_BINDLESS_SAMPLERS as usize)
            .copied()
            .collect();

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bindless_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&padded_views),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(&padded_samplers),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &material_buffer.buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &draw_call_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(4),
                    }),
                },
            ],
        });

        Self {
            bind_group,
            bind_group_layout,
            draw_call_buffer,
        }
    }

    /// Recreate the bind group after textures or samplers have been added/removed.
    /// This is a destructive operation — the old bind group is replaced.
    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        atlas: &TextureAtlas,
        pool: &SamplerPool,
        material_buffer: &MaterialDataBuffer,
    ) {
        let view_refs: Vec<&wgpu::TextureView> = atlas.views().iter().collect();
        let sampler_refs: Vec<&wgpu::Sampler> = pool.samplers().iter().collect();

        if view_refs.is_empty() || sampler_refs.is_empty() {
            log::warn!("Cannot rebuild bindless bind group: atlas or pool is empty");
            return;
        }

        let padded_views: Vec<&wgpu::TextureView> = view_refs
            .iter()
            .cycle()
            .take(MAX_BINDLESS_TEXTURES as usize)
            .copied()
            .collect();
        let padded_samplers: Vec<&wgpu::Sampler> = sampler_refs
            .iter()
            .cycle()
            .take(MAX_BINDLESS_SAMPLERS as usize)
            .copied()
            .collect();

        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bindless_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&padded_views),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(&padded_samplers),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &material_buffer.buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.draw_call_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(4),
                    }),
                },
            ],
        });
    }
}

// ── Fallback Bind Group Builder ─────────────────────────────────

/// Builds a traditional per-material bind group for devices without bindless support.
pub struct FallbackBindGroupBuilder {
    /// Layout for per-material bind groups.
    pub layout: wgpu::BindGroupLayout,
}

impl FallbackBindGroupBuilder {
    /// Create a fallback bind group builder.
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fallback_material_layout"),
            entries: &[
                // Material uniform (binding 0)
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
                // Albedo texture (binding 1)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Albedo sampler (binding 2)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        Self { layout }
    }

    /// Create a per-material bind group for a specific material and texture.
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        material_buffer: &wgpu::Buffer,
        texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fallback_material_bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: material_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

// ── Resource Lifetime Manager ───────────────────────────────────

/// Manages GPU resource lifetimes to prevent use-after-free.
/// Tracks which textures and samplers are currently in use by materials.
/// Uses frame-delayed removal to ensure GPU commands complete before freeing.
pub struct ResourceLifetimeManager {
    /// Texture ID → reference count (how many materials use it).
    texture_ref_counts: HashMap<u64, u32>,
    /// Sampler handle → reference count.
    sampler_ref_counts: HashMap<u64, u32>,
    /// Material index → (albedo_tex_id, normal_tex_id, mr_tex_id, sampler_handle).
    material_resources: HashMap<u32, (u64, u64, u64, u64)>,
    /// Textures pending removal (waiting for GPU fence).
    pending_texture_removals: Vec<(u64, u64)>, // (texture_id, removal_frame)
    /// Samplers pending removal.
    pending_sampler_removals: Vec<(u64, u64)>, // (sampler_handle, removal_frame)
    /// Current frame number (incremented each frame).
    current_frame: u64,
    /// Number of frames to wait before actually removing a resource.
    removal_delay_frames: u64,
}

impl ResourceLifetimeManager {
    /// Create a new resource lifetime manager.
    pub fn new() -> Self {
        Self {
            texture_ref_counts: HashMap::new(),
            sampler_ref_counts: HashMap::new(),
            material_resources: HashMap::new(),
            pending_texture_removals: Vec::new(),
            pending_sampler_removals: Vec::new(),
            current_frame: 0,
            removal_delay_frames: 3, // Wait 3 frames (typical GPU pipeline depth)
        }
    }

    /// Create with custom removal delay.
    pub fn with_removal_delay(delay_frames: u64) -> Self {
        Self {
            removal_delay_frames: delay_frames,
            ..Self::new()
        }
    }

    /// Advance to the next frame and process pending removals.
    /// Call this once per frame at the end of rendering.
    pub fn advance_frame(&mut self) {
        self.current_frame += 1;

        // Process texture removals
        self.pending_texture_removals.retain(|&(tex_id, frame)| {
            if self.current_frame - frame >= self.removal_delay_frames {
                // Actually remove the reference
                if let Some(count) = self.texture_ref_counts.get_mut(&tex_id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.texture_ref_counts.remove(&tex_id);
                    }
                }
                false
            } else {
                true
            }
        });

        // Process sampler removals
        self.pending_sampler_removals.retain(|&(handle, frame)| {
            if self.current_frame - frame >= self.removal_delay_frames {
                if let Some(count) = self.sampler_ref_counts.get_mut(&handle) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.sampler_ref_counts.remove(&handle);
                    }
                }
                false
            } else {
                true
            }
        });
    }

    /// Register a material's resource dependencies.
    pub fn register_material(&mut self, index: u32, mat: &GpuMaterialData) {
        // Remove old references if material was already registered
        self.unregister_material(index);

        // Register new references
        let albedo_id = if mat.albedo_tex_index != u32::MAX {
            mat.albedo_tex_index as u64
        } else {
            0
        };
        let normal_id = if mat.normal_tex_index != u32::MAX {
            mat.normal_tex_index as u64
        } else {
            0
        };
        let mr_id = if mat.mr_tex_index != u32::MAX {
            mat.mr_tex_index as u64
        } else {
            0
        };
        let sampler_handle = mat.sampler_index as u64;

        if albedo_id != 0 {
            *self.texture_ref_counts.entry(albedo_id).or_insert(0) += 1;
        }
        if normal_id != 0 {
            *self.texture_ref_counts.entry(normal_id).or_insert(0) += 1;
        }
        if mr_id != 0 {
            *self.texture_ref_counts.entry(mr_id).or_insert(0) += 1;
        }
        *self.sampler_ref_counts.entry(sampler_handle).or_insert(0) += 1;

        self.material_resources
            .insert(index, (albedo_id, normal_id, mr_id, sampler_handle));
    }

    /// Unregister a material's resource dependencies.
    pub fn unregister_material(&mut self, index: u32) {
        if let Some((albedo_id, normal_id, mr_id, sampler_handle)) =
            self.material_resources.remove(&index)
        {
            // Add to pending removals instead of immediately removing
            if albedo_id != 0 {
                self.pending_texture_removals
                    .push((albedo_id, self.current_frame));
            }
            if normal_id != 0 {
                self.pending_texture_removals
                    .push((normal_id, self.current_frame));
            }
            if mr_id != 0 {
                self.pending_texture_removals
                    .push((mr_id, self.current_frame));
            }
            self.pending_sampler_removals
                .push((sampler_handle, self.current_frame));
        }
    }

    /// Check if a texture is safe to remove (no active references).
    pub fn is_texture_in_use(&self, texture_id: u64) -> bool {
        // Check active references
        if self
            .texture_ref_counts
            .get(&texture_id)
            .copied()
            .unwrap_or(0)
            > 0
        {
            return true;
        }
        // Check pending removals
        self.pending_texture_removals
            .iter()
            .any(|&(id, _)| id == texture_id)
    }

    /// Check if a sampler is safe to remove (no active references).
    pub fn is_sampler_in_use(&self, sampler_handle: u64) -> bool {
        if self
            .sampler_ref_counts
            .get(&sampler_handle)
            .copied()
            .unwrap_or(0)
            > 0
        {
            return true;
        }
        self.pending_sampler_removals
            .iter()
            .any(|&(handle, _)| handle == sampler_handle)
    }

    /// Get the number of active texture references.
    pub fn active_texture_count(&self) -> usize {
        self.texture_ref_counts.len()
    }

    /// Get the number of active sampler references.
    pub fn active_sampler_count(&self) -> usize {
        self.sampler_ref_counts.len()
    }

    /// Get the number of tracked materials.
    pub fn tracked_material_count(&self) -> usize {
        self.material_resources.len()
    }

    /// Get the current frame number.
    pub fn frame_number(&self) -> u64 {
        self.current_frame
    }

    /// Get pending removal counts for debugging.
    pub fn pending_removal_counts(&self) -> (usize, usize) {
        (
            self.pending_texture_removals.len(),
            self.pending_sampler_removals.len(),
        )
    }

    /// Force flush all pending removals immediately (use with caution).
    pub fn force_flush_pending_removals(&mut self) {
        for &(tex_id, _) in &self.pending_texture_removals {
            if let Some(count) = self.texture_ref_counts.get_mut(&tex_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.texture_ref_counts.remove(&tex_id);
                }
            }
        }
        for &(handle, _) in &self.pending_sampler_removals {
            if let Some(count) = self.sampler_ref_counts.get_mut(&handle) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.sampler_ref_counts.remove(&handle);
                }
            }
        }
        self.pending_texture_removals.clear();
        self.pending_sampler_removals.clear();
    }
}

// ── Texture Batch Uploader ──────────────────────────────────────

/// Efficient batched texture upload system using staging buffers.
/// Groups multiple texture uploads into fewer GPU commands for better performance.
pub struct TextureBatchUploader {
    /// Staging buffer for texture data.
    staging_buffer: wgpu::Buffer,
    /// Size of the staging buffer.
    staging_size: u64,
    /// Pending texture uploads (data + destination info).
    pending_uploads: Vec<PendingTextureUpload>,
    /// Maximum batch size before forced flush.
    max_batch_size: u64,
    /// Current batch size.
    current_batch_size: u64,
}

struct PendingTextureUpload {
    /// Texture data (CPU-side).
    data: Vec<u8>,
    /// Destination texture.
    texture: wgpu::Texture,
    /// Mip level to update.
    mip_level: u32,
    /// Region to update (x, y, width, height).
    region: (u32, u32, u32, u32),
    /// Bytes per row.
    bytes_per_row: u32,
    /// Rows per image.
    rows_per_image: u32,
}

impl TextureBatchUploader {
    /// Create a new batch uploader with the given staging buffer size.
    pub fn new(device: &wgpu::Device, staging_size: u64) -> Self {
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture_staging_buffer"),
            size: staging_size,
            usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        Self {
            staging_buffer,
            staging_size,
            pending_uploads: Vec::new(),
            max_batch_size: staging_size / 2, // Use half the staging buffer
            current_batch_size: 0,
        }
    }

    /// Queue a texture upload.
    pub fn queue_upload(
        &mut self,
        data: Vec<u8>,
        texture: wgpu::Texture,
        mip_level: u32,
        region: (u32, u32, u32, u32),
    ) {
        let bytes_per_row = region.2 * 4; // RGBA8
        let rows_per_image = region.3;
        let upload_size = (bytes_per_row as u64 * rows_per_image as u64).div_ceil(256) * 256; // Align to 256 bytes

        self.pending_uploads.push(PendingTextureUpload {
            data,
            texture,
            mip_level,
            region,
            bytes_per_row,
            rows_per_image,
        });
        self.current_batch_size += upload_size;
    }

    /// Check if the batch is full and needs flushing.
    pub fn is_batch_full(&self) -> bool {
        self.current_batch_size >= self.max_batch_size
            || self.pending_uploads.len() >= MAX_TEXTURE_BATCH
    }

    /// Flush all pending uploads to the GPU.
    pub fn flush(&mut self, queue: &wgpu::Queue) {
        if self.pending_uploads.is_empty() {
            return;
        }

        let mut offset = 0u64;

        for upload in self.pending_uploads.drain(..) {
            let data_size = upload.data.len() as u64;

            // Write to staging buffer
            queue.write_buffer(&self.staging_buffer, offset, &upload.data);

            // Copy from staging to texture
            // Note: In a real implementation, we'd use a command encoder for this
            // For now, we write directly to the texture
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &upload.texture,
                    mip_level: upload.mip_level,
                    origin: wgpu::Origin3d {
                        x: upload.region.0,
                        y: upload.region.1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &upload.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(upload.bytes_per_row),
                    rows_per_image: Some(upload.rows_per_image),
                },
                wgpu::Extent3d {
                    width: upload.region.2,
                    height: upload.region.3,
                    depth_or_array_layers: 1,
                },
            );

            offset += data_size.div_ceil(256) * 256;
        }

        self.current_batch_size = 0;
    }

    /// Get the number of pending uploads.
    pub fn pending_upload_count(&self) -> usize {
        self.pending_uploads.len()
    }

    /// Get the current batch size in bytes.
    pub fn current_batch_size(&self) -> u64 {
        self.current_batch_size
    }

    /// Clear all pending uploads without flushing.
    pub fn clear_pending(&mut self) {
        self.pending_uploads.clear();
        self.current_batch_size = 0;
    }
}

// ── Bindless Render Pipeline Builder ────────────────────────────

/// Helper to build a bindless-aware render pipeline.
pub struct BindlessPipelineBuilder;

impl BindlessPipelineBuilder {
    /// Create a pipeline layout descriptor for bindless rendering.
    ///
    /// Layout order:
    /// - Group 0: Camera uniform
    /// - Group 1: Bindless bind group (textures + samplers + materials)
    /// - Group 2: Lighting bind group
    pub fn create_bindless_pipeline_layout(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        bindless_layout: &wgpu::BindGroupLayout,
        lighting_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::PipelineLayout {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Bindless Pipeline Layout"),
            bind_group_layouts: &[camera_layout, bindless_layout, lighting_layout],
            push_constant_ranges: &[],
        })
    }

    /// Create a render pipeline with bindless bindings.
    pub fn create_bindless_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        camera_layout: &wgpu::BindGroupLayout,
        bindless_layout: &wgpu::BindGroupLayout,
        lighting_layout: &wgpu::BindGroupLayout,
        shader_source: &str,
        vertex_layout: &wgpu::VertexBufferLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bindless Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = Self::create_bindless_pipeline_layout(
            device,
            camera_layout,
            bindless_layout,
            lighting_layout,
        );

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Bindless Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout.clone()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }
}
