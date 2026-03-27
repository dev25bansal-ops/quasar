//! Temporal Anti-Aliasing (TAA).
//!
//! TAA works by jittering the camera projection each frame and blending the
//! current (jittered) result with a reprojected history buffer.  A
//! neighbourhood-clamp in YCoCg space prevents ghosting on disocclusions.
//!
//! ## Integration
//!
//! 1. Each frame, call [`TaaPass::jitter_offset`] and apply the returned
//!    sub-pixel offset to the camera projection matrix.
//! 2. Render the scene as usual (the geometry pass writes depth + a motion
//!    vector target).
//! 3. Call [`TaaPass::resolve`] — it blends the current colour with the
//!    clamped history and writes the result.  The output also becomes the
//!    next frame's history.

/// Number of Halton(2,3) jitter samples before the sequence wraps.
pub const TAA_JITTER_SAMPLES: usize = 16;

/// Pre-computed Halton(2,3) jitter offsets in [-0.5, 0.5].
const HALTON_SEQUENCE: [(f32, f32); TAA_JITTER_SAMPLES] = [
    (0.000000, -0.333333),
    (-0.500000, 0.333333),
    (0.250000, -0.111111),
    (-0.250000, -0.444444),
    (0.375000, 0.222222),
    (-0.125000, -0.222222),
    (0.187500, 0.444444),
    (-0.312500, -0.037037),
    (0.062500, -0.370370),
    (-0.437500, 0.296296),
    (0.312500, -0.148148),
    (-0.187500, -0.481481),
    (0.437500, 0.185185),
    (-0.062500, -0.259259),
    (0.156250, 0.407407),
    (-0.343750, -0.074074),
];

/// Uniform data uploaded to the TAA resolve shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TaaUniforms {
    /// (texel_size_x, texel_size_y, blend_factor, _pad)
    pub params: [f32; 4],
}

/// TAA pass — manages history buffers, jitter sequence, and the resolve
/// render pipeline.
pub struct TaaPass {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    /// Ping-pong history textures.  `history[current_history]` is the one we
    /// read from (previous frame); the other is the write target.
    pub history: [wgpu::Texture; 2],
    pub history_views: [wgpu::TextureView; 2],
    /// Index into `history` that holds last frame's result.
    current_history: usize,
    /// Frame counter for the Halton jitter sequence.
    frame_index: usize,
    pub width: u32,
    pub height: u32,
    /// Exponential blend factor α — smaller = more temporal smoothing.
    pub blend_factor: f32,
}

impl TaaPass {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        _output_format: wgpu::TextureFormat,
    ) -> Self {
        let fmt = wgpu::TextureFormat::Rgba16Float;

        let create_history = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        };

        let history = [
            create_history("TAA History 0"),
            create_history("TAA History 1"),
        ];
        let history_views = [
            history[0].create_view(&wgpu::TextureViewDescriptor::default()),
            history[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("TAA BGL"),
            entries: &[
                // 0: current colour
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 1: history
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
                // 2: motion vectors
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 3: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // 4: uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("TAA Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/taa.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("TAA Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("TAA Resolve"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_taa"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: fmt,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("TAA Uniforms"),
            size: std::mem::size_of::<TaaUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("TAA Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            sampler,
            history,
            history_views,
            current_history: 0,
            frame_index: 0,
            width,
            height,
            blend_factor: 0.1,
        }
    }

    /// Returns the sub-pixel jitter offset in **NDC units** for the current
    /// frame.  Apply this to the camera projection's `[2][0]` and `[2][1]`
    /// elements (the X and Y translation in clip space).
    pub fn jitter_offset(&self) -> (f32, f32) {
        let (hx, hy) = HALTON_SEQUENCE[self.frame_index % TAA_JITTER_SAMPLES];
        // Convert from [-0.5, 0.5] pixel offset to NDC offset.
        let jx = hx / self.width as f32;
        let jy = hy / self.height as f32;
        (jx * 2.0, jy * 2.0) // NDC spans [-1,1] so multiply by 2
    }

    /// Resolve: blend current jittered colour with clamped history.
    ///
    /// - `current_view` — the colour output of this frame's geometry pass.
    /// - `depth_view` — the geometry-pass depth buffer.
    /// - `motion_view` — RG16Float motion vectors (screen-space velocity).
    ///
    /// The resolve writes into the **write** history texture.  After calling
    /// this, use [`output_view`](Self::output_view) to get the resolved result.
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        current_view: &wgpu::TextureView,
        motion_view: &wgpu::TextureView,
    ) {
        let read_idx = self.current_history;
        let write_idx = 1 - read_idx;

        // Upload uniforms.
        let uniforms = TaaUniforms {
            params: [
                1.0 / self.width as f32,
                1.0 / self.height as f32,
                self.blend_factor,
                0.0,
            ],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TAA BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(current_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.history_views[read_idx]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(motion_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("TAA Resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.history_views[write_idx],
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        // Flip history ping-pong.
        self.current_history = write_idx;
        self.frame_index += 1;
    }

    /// The resolved TAA output for this frame (the texture view that was just
    /// written to by [`resolve`](Self::resolve)).
    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.history_views[self.current_history]
    }

    /// Recreate history buffers on resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let fmt = wgpu::TextureFormat::Rgba16Float;
        for (i, label) in ["TAA History 0", "TAA History 1"].iter().enumerate() {
            self.history[i] = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.history_views[i] =
                self.history[i].create_view(&wgpu::TextureViewDescriptor::default());
        }
        self.frame_index = 0;
        self.current_history = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_sequence_stays_bounded() {
        for i in 0..TAA_JITTER_SAMPLES * 3 {
            let (hx, hy) = HALTON_SEQUENCE[i % TAA_JITTER_SAMPLES];
            assert!(hx.abs() <= 0.5, "hx out of range: {hx}");
            assert!(hy.abs() <= 0.5, "hy out of range: {hy}");
        }
    }

    #[test]
    fn halton_no_duplicates() {
        for i in 0..TAA_JITTER_SAMPLES {
            for j in (i + 1)..TAA_JITTER_SAMPLES {
                let (ax, ay) = HALTON_SEQUENCE[i];
                let (bx, by) = HALTON_SEQUENCE[j];
                assert!(
                    (ax - bx).abs() > 1e-6 || (ay - by).abs() > 1e-6,
                    "duplicate jitter at {i} and {j}"
                );
            }
        }
    }
}
