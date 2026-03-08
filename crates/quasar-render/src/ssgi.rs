//! Screen-Space Global Illumination (SSGI).
//!
//! Computes one-bounce indirect diffuse lighting by tracing short rays in
//! screen space against the depth buffer and sampling the colour buffer at
//! hit points.  Normals are reconstructed from the depth buffer so no
//! G-Buffer normal attachment is required (works with forward rendering).
//!
//! The current frame's raw indirect output is stored in an Rgba16Float
//! texture.  An optional temporal blend pass accumulates the result over
//! multiple frames for reduced noise.
//!
//! ## Integration
//!
//! 1. After the main colour + depth pass, call [`SsgiPass::dispatch`].
//! 2. Use [`SsgiPass::output_view`] to read the indirect texture (e.g. as
//!    additive lighting in a composite pass or as input to the post-process
//!    chain).

/// Uniform data uploaded to the SSGI compute shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SsgiParams {
    /// Inverse projection matrix — stored as 4 × vec4.
    pub inv_proj_0: [f32; 4],
    pub inv_proj_1: [f32; 4],
    pub inv_proj_2: [f32; 4],
    pub inv_proj_3: [f32; 4],
    /// (ray_count, max_steps, ray_length, thickness)
    pub trace_params: [f32; 4],
    /// (width, height, temporal_blend, frame)
    pub resolution: [f32; 4],
}

/// Settings exposed to the user for tuning SSGI quality vs. performance.
#[derive(Debug, Clone, Copy)]
pub struct SsgiSettings {
    /// Number of rays per pixel (more = less noise, slower).
    pub ray_count: u32,
    /// Maximum step count per ray.
    pub max_steps: u32,
    /// Maximum ray length in view-space units.
    pub ray_length: f32,
    /// Depth thickness threshold for hit detection.
    pub thickness: f32,
    /// Temporal accumulation blend factor (0 = all history, 1 = all current).
    pub temporal_blend: f32,
}

impl Default for SsgiSettings {
    fn default() -> Self {
        Self {
            ray_count: 4,
            max_steps: 8,
            ray_length: 2.0,
            thickness: 0.3,
            temporal_blend: 0.1,
        }
    }
}

/// SSGI compute pass — manages the pipeline, output textures, and temporal
/// accumulation history.
pub struct SsgiPass {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    /// Current frame raw indirect output.
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    /// Ping-pong textures for temporal accumulation.
    history: [wgpu::Texture; 2],
    history_views: [wgpu::TextureView; 2],
    current_history: usize,
    frame_index: u32,
    pub width: u32,
    pub height: u32,
    pub settings: SsgiSettings,
}

impl SsgiPass {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let fmt = wgpu::TextureFormat::Rgba16Float;

        let create_tex = |label: &str| {
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
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        let output_texture = create_tex("SSGI Output");
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let history = [create_tex("SSGI History 0"), create_tex("SSGI History 1")];
        let history_views = [
            history[0].create_view(&wgpu::TextureViewDescriptor::default()),
            history[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("SSGI BGL"),
                entries: &[
                    // 0: scene colour (read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    // 1: depth (read)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    // 2: uniforms
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // 3: output (write)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: fmt,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSGI Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/ssgi.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSGI Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SSGI Compute"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_ssgi"),
            compilation_options: Default::default(),
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSGI Uniforms"),
            size: std::mem::size_of::<SsgiParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
            output_texture,
            output_view,
            history,
            history_views,
            current_history: 0,
            frame_index: 0,
            width,
            height,
            settings: SsgiSettings::default(),
        }
    }

    /// Dispatch the SSGI compute pass.
    ///
    /// - `color_view` — the HDR colour output of the current frame.
    /// - `depth_view` — the depth buffer (Depth32Float).
    /// - `inv_proj` — inverse of the camera projection matrix.
    pub fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        inv_proj: &glam::Mat4,
    ) {
        let cols = inv_proj.to_cols_array_2d();
        let uniforms = SsgiParams {
            inv_proj_0: cols[0],
            inv_proj_1: cols[1],
            inv_proj_2: cols[2],
            inv_proj_3: cols[3],
            trace_params: [
                self.settings.ray_count as f32,
                self.settings.max_steps as f32,
                self.settings.ray_length,
                self.settings.thickness,
            ],
            resolution: [
                self.width as f32,
                self.height as f32,
                self.settings.temporal_blend,
                self.frame_index as f32,
            ],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSGI BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.output_view),
                },
            ],
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SSGI Compute"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let wg_x = (self.width + 7) / 8;
            let wg_y = (self.height + 7) / 8;
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        self.frame_index = self.frame_index.wrapping_add(1);
    }

    /// The raw SSGI output texture view for the current frame.
    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    /// Recreate textures on resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        let fmt = wgpu::TextureFormat::Rgba16Float;

        let create_tex = |label: &str| {
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
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        self.output_texture = create_tex("SSGI Output");
        self.output_view = self.output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.history = [create_tex("SSGI History 0"), create_tex("SSGI History 1")];
        self.history_views = [
            self.history[0].create_view(&wgpu::TextureViewDescriptor::default()),
            self.history[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];
        self.current_history = 0;
        self.frame_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssgi_settings_default() {
        let s = SsgiSettings::default();
        assert_eq!(s.ray_count, 4);
        assert_eq!(s.max_steps, 8);
        assert!((s.ray_length - 2.0).abs() < f32::EPSILON);
        assert!((s.thickness - 0.3).abs() < f32::EPSILON);
        assert!((s.temporal_blend - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn ssgi_params_pod_size() {
        // 6 × vec4 = 6 × 16 = 96 bytes
        assert_eq!(std::mem::size_of::<SsgiParams>(), 96);
    }
}
