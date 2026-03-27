//! Screen-Space Reflections (SSR).
//!
//! Traces rays in screen-space along the view-reflected direction using
//! hierarchical ray-marching against the depth buffer.  Falls back to the
//! environment map when the ray leaves the screen or no intersection is found.
//!
//! Pipeline:
//!   1. SSR trace (compute/fragment) — writes reflected color + confidence to
//!      an Rgba16Float texture.
//!   2. Temporal resolve — blends with previous frame to reduce noise.
//!   3. Composite — mixes SSR result into the lit scene using roughness.

use bytemuck::{Pod, Zeroable};

/// Configuration resource.
#[derive(Debug, Clone, Copy)]
pub struct SsrSettings {
    /// Maximum ray-march steps per pixel.
    pub max_steps: u32,
    /// Step size in UV space (smaller = higher quality, slower).
    pub step_size: f32,
    /// Thickness threshold — how thick a surface is considered for hit test.
    pub thickness: f32,
    /// Maximum roughness at which SSR is applied (above = env-map only).
    pub max_roughness: f32,
    /// Temporal blend factor (0 = no history, 1 = full history).
    pub temporal_weight: f32,
    /// Master on/off.
    pub enabled: bool,
}

impl Default for SsrSettings {
    fn default() -> Self {
        Self {
            max_steps: 64,
            step_size: 0.015,
            thickness: 0.1,
            max_roughness: 0.5,
            temporal_weight: 0.9,
            enabled: true,
        }
    }
}

/// GPU uniform matching the WGSL struct.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SsrUniform {
    /// inv(ViewProj) for reconstructing world position.
    pub inv_view_proj: [[f32; 4]; 4],
    /// ViewProj (current frame) for re-projecting world → screen.
    pub view_proj: [[f32; 4]; 4],
    /// Camera position (xyz), max_steps (w as f32).
    pub camera_pos_steps: [f32; 4],
    /// step_size, thickness, max_roughness, temporal_weight.
    pub params: [f32; 4],
    /// Screen resolution (xy), _pad, _pad.
    pub resolution: [f32; 4],
}

/// GPU resources for the SSR pass.
pub struct SsrPass {
    pub ssr_texture: wgpu::Texture,
    pub ssr_view: wgpu::TextureView,
    pub history_texture: wgpu::Texture,
    pub history_view: wgpu::TextureView,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub resolve_pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl SsrPass {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        gbuffer_read_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let ssr_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Result"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let ssr_view = ssr_texture.create_view(&Default::default());

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR History"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let history_view = history_texture.create_view(&Default::default());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSR Uniform"),
            size: std::mem::size_of::<SsrUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSR Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSR BGL"),
            entries: &[
                // 0: uniform
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
                // 1: scene color (lit)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 2: depth
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
                // 3: history
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 4: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // placeholder 1×1 textures for initial bind group
        let placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ph_view = placeholder.create_view(&Default::default());
        let depth_ph = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Depth PH"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_ph_view = depth_ph.create_view(&Default::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&ph_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth_ph_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&history_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSR Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/ssr.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSR Pipeline Layout"),
            bind_group_layouts: &[gbuffer_read_layout, &bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = create_ssr_render_pipeline(
            device,
            &pipeline_layout,
            &shader,
            "fs_ssr_trace",
            wgpu::TextureFormat::Rgba16Float,
        );
        let resolve_pipeline = create_ssr_render_pipeline(
            device,
            &pipeline_layout,
            &shader,
            "fs_ssr_resolve",
            wgpu::TextureFormat::Rgba16Float,
        );

        Self {
            ssr_texture,
            ssr_view,
            history_texture,
            history_view,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            resolve_pipeline,
            width,
            height,
        }
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        w: u32,
        h: u32,
        gbuffer_layout: &wgpu::BindGroupLayout,
    ) {
        *self = Self::new(device, w, h, gbuffer_layout);
    }
}

fn create_ssr_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}

// SSR WGSL is loaded from assets/shaders/ssr.wgsl via include_str! in SsrPass::new().
