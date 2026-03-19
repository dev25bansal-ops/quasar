//! Pipeline cache — maps shader source paths to compiled render pipelines.
//!
//! The cache allows the hot-reload system to invalidate and rebuild only
//! the pipelines whose shader source changed, rather than recreating every
//! pipeline in the engine.

#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Metadata stored alongside a cached pipeline so that it can be rebuilt
/// from the same parameters when the shader source changes on disk.
pub struct PipelineCacheEntry {
    /// The compiled render pipeline.
    pub pipeline: wgpu::RenderPipeline,
    /// Path to the `.wgsl` source that was used to create this pipeline.
    pub shader_path: PathBuf,
    /// Surface texture format used when building the pipeline.
    pub format: wgpu::TextureFormat,
    /// Bind group layouts (indices mirror the pipeline layout order).
    pub bind_group_layout_ids: Vec<u64>,
}

/// Monotonically increasing id used to tag bind-group layouts so we can
/// match them later without storing `&wgpu::BindGroupLayout` references.
static NEXT_LAYOUT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn next_layout_id() -> u64 {
    NEXT_LAYOUT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A cache of compiled render pipelines keyed by their shader path.
///
/// # Usage
///
/// ```ignore
/// let mut cache = PipelineCache::new();
/// let pipeline = cache.get_or_create(device, shader_path, format, layouts, source);
/// // … later, when the shader file changes:
/// cache.invalidate(&shader_path);
/// let pipeline = cache.get_or_create(device, shader_path, format, layouts, new_source);
/// ```
pub struct PipelineCache {
    entries: HashMap<PathBuf, PipelineCacheEntry>,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Return the cached pipeline if it exists, otherwise compile from
    /// `shader_source` and cache the result.
    pub fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        shader_path: &Path,
        format: wgpu::TextureFormat,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        layout_ids: &[u64],
        shader_source: &str,
        vertex_layouts: &[wgpu::VertexBufferLayout<'_>],
    ) -> &wgpu::RenderPipeline {
        if !self.entries.contains_key(shader_path) {
            let pipeline = compile_pipeline(
                device,
                shader_path,
                format,
                bind_group_layouts,
                shader_source,
                vertex_layouts,
            );
            self.entries.insert(
                shader_path.to_path_buf(),
                PipelineCacheEntry {
                    pipeline,
                    shader_path: shader_path.to_path_buf(),
                    format,
                    bind_group_layout_ids: layout_ids.to_vec(),
                },
            );
        }
        &self.entries[shader_path].pipeline
    }

    /// Remove the cached entry so the next `get_or_create` call recompiles.
    pub fn invalidate(&mut self, shader_path: &Path) -> bool {
        self.entries.remove(shader_path).is_some()
    }

    /// Rebuild a pipeline in-place from new shader source.
    ///
    /// Returns `true` if the pipeline was successfully rebuilt, `false` if
    /// the path was not in the cache (caller should create it fresh).
    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        shader_path: &Path,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        shader_source: &str,
        vertex_layouts: &[wgpu::VertexBufferLayout<'_>],
    ) -> bool {
        if let Some(entry) = self.entries.get_mut(shader_path) {
            entry.pipeline = compile_pipeline(
                device,
                shader_path,
                entry.format,
                bind_group_layouts,
                shader_source,
                vertex_layouts,
            );
            true
        } else {
            false
        }
    }

    /// Iterator over all cached shader paths.
    pub fn shader_paths(&self) -> impl Iterator<Item = &Path> {
        self.entries.keys().map(|p| p.as_path())
    }

    /// Number of cached pipelines.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn compile_pipeline(
    device: &wgpu::Device,
    shader_path: &Path,
    format: wgpu::TextureFormat,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    shader_source: &str,
    vertex_layouts: &[wgpu::VertexBufferLayout<'_>],
) -> wgpu::RenderPipeline {
    let label = shader_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{label} Pipeline Layout")),
        bind_group_layouts,
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{label} Pipeline")),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: vertex_layouts,
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
