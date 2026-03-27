//! HDR rendering and tonemapping.
//!
//! Renders to a floating-point HDR texture (Rgba16Float), then applies
//! tonemapping (Reinhard or ACES filmic) to map HDR values to LDR.

/// HDR render target with tonemapping support.
pub struct HdrRenderTarget {
    /// The HDR texture (Rgba16Float format).
    pub texture: wgpu::Texture,
    /// Texture view for rendering.
    pub view: wgpu::TextureView,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl HdrRenderTarget {
    /// Create a new HDR render target.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("HDR Render Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
        }
    }

    /// Resize the HDR target.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width != width || self.height != height {
            *self = Self::new(device, width, height);
        }
    }
}

/// Tonemapping algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tonemapping {
    /// Reinhard: L = L / (1 + L). Simple but can flatten contrast.
    Reinhard,
    /// ACES Filmic: cinematic look with good highlight rolloff.
    #[default]
    AcesFilmic,
    /// No tonemapping (passthrough).
    None,
}

/// Post-process pass for applying tonemapping.
pub struct TonemappingPass {
    /// The fullscreen quad pipeline.
    pipeline: wgpu::RenderPipeline,
    /// Bind group for HDR texture.
    bind_group: Option<wgpu::BindGroup>,
    /// Bind group layout.
    bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler for HDR texture.
    sampler: wgpu::Sampler,
    /// Current tonemapping mode.
    pub tonemapping: Tonemapping,
}

impl TonemappingPass {
    /// Create the tonemapping pass.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader_source = include_str!("../../../assets/shaders/tonemap.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tonemapping Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tonemapping Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Tonemapping Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tonemapping Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Tonemapping Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group: None,
            bind_group_layout,
            sampler,
            tonemapping: Tonemapping::AcesFilmic,
        }
    }

    /// Update the HDR texture bind group.
    pub fn update_texture(&mut self, device: &wgpu::Device, hdr_view: &wgpu::TextureView) {
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tonemapping Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
    }

    /// Execute the tonemapping pass.
    pub fn execute(&self, encoder: &mut wgpu::CommandEncoder, target_view: &wgpu::TextureView) {
        let Some(bind_group) = &self.bind_group else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Tonemapping Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Brightness and contrast adjustments.
#[derive(Debug, Clone, Copy)]
pub struct ColorGrading {
    /// Exposure adjustment (stops).
    pub exposure: f32,
    /// Contrast multiplier.
    pub contrast: f32,
    /// Saturation multiplier.
    pub saturation: f32,
    /// Gamma correction.
    pub gamma: f32,
}

impl Default for ColorGrading {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            gamma: 2.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tonemapping_default() {
        let tm = Tonemapping::default();
        assert_eq!(tm, Tonemapping::AcesFilmic);
    }

    #[test]
    fn color_grading_default() {
        let cg = ColorGrading::default();
        assert!((cg.gamma - 2.2).abs() < 0.01);
    }
}
