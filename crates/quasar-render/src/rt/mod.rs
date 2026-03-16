//! Hardware ray-tracing acceleration structures and hybrid GI.
//!
//! Provides:
//! - **BLAS** (Bottom-Level Acceleration Structure) — per-mesh geometry
//! - **TLAS** (Top-Level Acceleration Structure) — per-scene instance list
//! - **RtGiPass** — hybrid RT global illumination (1 shadow + 1 GI ray/pixel,
//!   fallback to screen-space GI when RT hardware is unavailable)
//!
//! Gated on `#[cfg(feature = "raytracing")]`.

use glam::Mat4;
use wgpu;

// ── Acceleration-structure descriptors ──────────────────────────

/// Per-mesh bottom-level acceleration structure.
///
/// Wraps a wgpu BLAS handle plus the source vertex/index data ranges needed
/// to build or refit the structure.
pub struct Blas {
    /// Opaque handle to the GPU-side BLAS. Actual type depends on backend.
    /// Stored as raw buffer containing the serialised AS data.
    pub buffer: wgpu::Buffer,
    /// Byte offset into the vertex buffer where this mesh's vertices begin.
    pub vertex_offset: u64,
    /// Number of vertices in the mesh segment.
    pub vertex_count: u32,
    /// Byte offset into the index buffer where this mesh's indices begin.
    pub index_offset: u64,
    /// Number of indices.
    pub index_count: u32,
    /// Whether the BLAS has been built at least once.
    pub built: bool,
    /// Unique ID for deduplication.
    pub mesh_id: u64,
}

impl Blas {
    /// Create a new BLAS descriptor (does NOT build the AS on the GPU).
    pub fn new(
        device: &wgpu::Device,
        mesh_id: u64,
        vertex_count: u32,
        index_count: u32,
        vertex_offset: u64,
        index_offset: u64,
    ) -> Self {
        let size = Self::estimate_size(vertex_count, index_count);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("blas_{mesh_id}")),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            vertex_offset,
            vertex_count,
            index_offset,
            index_count,
            built: false,
            mesh_id,
        }
    }

    /// Conservative byte-size estimate for the AS buffer.
    fn estimate_size(vertex_count: u32, index_count: u32) -> u64 {
        // Rule of thumb: ~64 bytes per triangle + header.
        let tri_count = if index_count > 0 {
            index_count / 3
        } else {
            vertex_count / 3
        };
        (tri_count as u64 * 64).max(1024) + 256
    }
}

/// Top-level acceleration structure — references a set of BLAS instances with transform.
pub struct Tlas {
    pub buffer: wgpu::Buffer,
    pub instances: Vec<TlasInstance>,
    pub built: bool,
}

/// One instance entry inside the TLAS.
#[derive(Debug, Clone)]
pub struct TlasInstance {
    /// Transform from object space → world space.
    pub transform: Mat4,
    /// Index into the BLAS list.
    pub blas_index: u32,
    /// Custom instance ID (visible in shaders).
    pub instance_id: u32,
    /// Visibility mask for ray filtering.
    pub mask: u8,
}

impl Tlas {
    pub fn new(device: &wgpu::Device, max_instances: u32) -> Self {
        let size = (max_instances as u64) * 128 + 512;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tlas"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            instances: Vec::new(),
            built: false,
        }
    }

    /// Clear all instances (call before rebuilding each frame).
    pub fn clear(&mut self) {
        self.instances.clear();
        self.built = false;
    }

    pub fn push_instance(&mut self, inst: TlasInstance) {
        self.instances.push(inst);
    }

    /// Upload the instance list to the GPU buffer.
    pub fn upload(&self, queue: &wgpu::Queue) {
        let data: Vec<u8> = self
            .instances
            .iter()
            .flat_map(|inst| {
                let cols = inst.transform.to_cols_array();
                let mut bytes: Vec<u8> = bytemuck::cast_slice(&cols).to_vec();
                bytes.extend_from_slice(bytemuck::bytes_of(&inst.blas_index));
                bytes.extend_from_slice(bytemuck::bytes_of(&inst.instance_id));
                bytes.extend_from_slice(&[inst.mask, 0, 0, 0]);
                bytes
            })
            .collect();
        if !data.is_empty() && data.len() as u64 <= self.buffer.size() {
            queue.write_buffer(&self.buffer, 0, &data);
        }
    }
}

// ── Hybrid RT Global Illumination ───────────────────────────────

/// Configuration for the RT GI pass.
#[derive(Debug, Clone)]
pub struct RtGiSettings {
    /// Number of shadow rays per lit pixel.
    pub shadow_rays: u32,
    /// Number of GI diffuse rays per pixel.
    pub gi_rays: u32,
    /// Maximum ray distance for GI bounces (world units).
    pub max_gi_distance: f32,
    /// Temporal accumulation blend factor (0 = only new, 1 = only history).
    pub temporal_blend: f32,
    /// If true, fall back to screen-space GI when RT is unavailable.
    pub ssgi_fallback: bool,
}

impl Default for RtGiSettings {
    fn default() -> Self {
        Self {
            shadow_rays: 1,
            gi_rays: 1,
            max_gi_distance: 100.0,
            temporal_blend: 0.9,
            ssgi_fallback: true,
        }
    }
}

/// Hybrid RT GI pass.
///
/// When RT-capable hardware is detected: dispatches 1 shadow ray + 1 GI ray
/// per lit pixel via compute shaders that read the TLAS.
///
/// Fallback: re-uses the engine's SSGI pass with identical settings so that
/// the lighting result is seamless.
pub struct RtGiPass {
    pub settings: RtGiSettings,
    /// Compute pipeline for the RT dispatch (None if RT unavailable).
    pub pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group holding TLAS + G-buffer textures.
    pub bind_group: Option<wgpu::BindGroup>,
    /// Output irradiance texture (half-res, RGBA16Float).
    pub irradiance_texture: Option<wgpu::Texture>,
    pub irradiance_view: Option<wgpu::TextureView>,
    /// History irradiance for temporal accumulation.
    pub history_texture: Option<wgpu::Texture>,
    pub history_view: Option<wgpu::TextureView>,
    /// Width / Height of the irradiance textures.
    pub width: u32,
    pub height: u32,
    /// Whether hardware RT is available (detected on init).
    pub rt_available: bool,
}

impl RtGiPass {
    /// Create the RT GI pass. Detects hardware RT availability via
    /// adapter features. If unavailable and `settings.ssgi_fallback` is true,
    /// the pass becomes a no-op and the engine should run the SSGI pass instead.
    pub fn new(
        device: &wgpu::Device,
        _adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
        settings: RtGiSettings,
    ) -> Self {
        // wgpu doesn't expose RT extensions yet in stable — check features.
        // When wgpu gains ray-query support the feature flag will be checked here.
        let rt_available = false; // placeholder until wgpu exposes RT

        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        let irradiance_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_gi_irradiance"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let irradiance_view = irradiance_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_gi_history"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let history_view = history_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            settings,
            pipeline: None,
            bind_group: None,
            irradiance_texture: Some(irradiance_texture),
            irradiance_view: Some(irradiance_view),
            history_texture: Some(history_texture),
            history_view: Some(history_view),
            width: half_w,
            height: half_h,
            rt_available,
        }
    }

    /// Returns true when the pass will produce meaningful output (i.e. RT is
    /// available OR ssgi_fallback is configured).
    pub fn is_active(&self) -> bool {
        self.rt_available || self.settings.ssgi_fallback
    }

    /// Resize internal textures on window resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        let irr = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_gi_irradiance"),
            size: wgpu::Extent3d { width: half_w, height: half_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let hist = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rt_gi_history"),
            size: wgpu::Extent3d { width: half_w, height: half_h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        self.irradiance_view = Some(irr.create_view(&wgpu::TextureViewDescriptor::default()));
        self.irradiance_texture = Some(irr);
        self.history_view = Some(hist.create_view(&wgpu::TextureViewDescriptor::default()));
        self.history_texture = Some(hist);
        self.width = half_w;
        self.height = half_h;
        // Bind group will need to be recreated by the caller.
        self.bind_group = None;
    }

    /// Dispatch the RT GI compute pass. No-op if RT unavailable.
    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.rt_available {
            return;
        }
        let Some(pipeline) = &self.pipeline else { return };
        let Some(bind_group) = &self.bind_group else { return };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rt_gi_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        let groups_x = self.width.div_ceil(8);
        let groups_y = self.height.div_ceil(8);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }
}
