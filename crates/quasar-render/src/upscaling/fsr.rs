//! FidelityFX Super Resolution 2 implementation.
//!
//! FSR 2 is a temporal upscaler that uses motion vectors and depth
//! to reconstruct a high-resolution image from a lower-resolution render.
//!
//! Key features:
//! - Temporal accumulation with history validation
//! - Motion vector reactive masks
//! - Automatic sharpening (RCAS)

use bytemuck::{Pod, Zeroable};

use super::{JitterPattern, UpscaleQuality};

/// FSR 2 configuration.
#[derive(Debug, Clone)]
pub struct Fsr2Settings {
    /// Quality level.
    pub quality: UpscaleQuality,
    /// Enable RCAS sharpening.
    pub enable_rcas: bool,
    /// RCAS sharpness (0.0 - 1.0).
    pub sharpness: f32,
    /// Use deferred depth instead of linear.
    pub use_deferred_depth: bool,
    /// Auto-exposure for HDR content.
    pub auto_exposure: bool,
}

impl Default for Fsr2Settings {
    fn default() -> Self {
        Self {
            quality: UpscaleQuality::Quality,
            enable_rcas: true,
            sharpness: 0.25,
            use_deferred_depth: false,
            auto_exposure: false,
        }
    }
}

/// FSR 2 upscaling pass.
pub struct Fsr2Pass {
    pub settings: Fsr2Settings,
    /// Compute pipeline for temporal accumulation.
    pub accumulation_pipeline: Option<wgpu::ComputePipeline>,
    /// Compute pipeline for spatial reconstruction.
    pub reconstruct_pipeline: Option<wgpu::ComputePipeline>,
    /// Compute pipeline for RCAS sharpening.
    pub rcas_pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Current bind group.
    pub bind_group: Option<wgpu::BindGroup>,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Internal temporal history texture.
    pub history_texture: Option<wgpu::Texture>,
    pub history_view: Option<wgpu::TextureView>,
    /// Output texture.
    pub output_texture: Option<wgpu::Texture>,
    pub output_view: Option<wgpu::TextureView>,
    /// Render resolution.
    pub render_width: u32,
    pub render_height: u32,
    /// Display resolution.
    pub display_width: u32,
    pub display_height: u32,
    /// Current jitter pattern.
    pub jitter: JitterPattern,
    /// Previous jitter.
    pub prev_jitter: JitterPattern,
    /// Frame counter.
    pub frame_index: u32,
}

impl Fsr2Pass {
    /// Create FSR 2 pass.
    pub fn new(
        device: &wgpu::Device,
        display_width: u32,
        display_height: u32,
        settings: Fsr2Settings,
    ) -> Self {
        let (render_width, render_height) = settings
            .quality
            .render_resolution(display_width, display_height);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fsr2_uniform"),
            size: std::mem::size_of::<Fsr2Constants>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fsr2_bgl"),
            entries: &[
                // 0: constants uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 1: input color
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
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
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                // 3: motion vectors
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 4: history
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 5: output
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // 6: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let jitter = JitterPattern::fsr2(0, render_width, render_height);
        let prev_jitter = jitter;

        Self {
            settings,
            accumulation_pipeline: None,
            reconstruct_pipeline: None,
            rcas_pipeline: None,
            bind_group_layout,
            bind_group: None,
            uniform_buffer,
            history_texture: None,
            history_view: None,
            output_texture: None,
            output_view: None,
            render_width,
            render_height,
            display_width,
            display_height,
            jitter,
            prev_jitter,
            frame_index: 0,
        }
    }

    /// Resize the upscaler.
    pub fn resize(&mut self, device: &wgpu::Device, display_width: u32, display_height: u32) {
        let (render_width, render_height) = self
            .settings
            .quality
            .render_resolution(display_width, display_height);

        self.display_width = display_width;
        self.display_height = display_height;
        self.render_width = render_width;
        self.render_height = render_height;

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fsr2_history"),
            size: wgpu::Extent3d {
                width: display_width,
                height: display_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.history_view =
            Some(history_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.history_texture = Some(history_texture);

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fsr2_output"),
            size: wgpu::Extent3d {
                width: display_width,
                height: display_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.output_view =
            Some(output_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.output_texture = Some(output_texture);

        self.bind_group = None;
    }

    /// Prepare constants for the current frame.
    pub fn prepare_constants(&mut self) -> Fsr2Constants {
        self.prev_jitter = self.jitter;
        self.jitter = JitterPattern::fsr2(self.frame_index, self.render_width, self.render_height);

        let constants = Fsr2Constants {
            render_resolution: [self.render_width as f32, self.render_height as f32],
            display_resolution: [self.display_width as f32, self.display_height as f32],
            jitter_offset: self.jitter.offset,
            prev_jitter_offset: self.prev_jitter.offset,
            sharpness: self.settings.sharpness,
            frame_index: self.frame_index,
            _pad: [0.0; 3],
        };

        self.frame_index += 1;
        constants
    }

    /// Dispatch FSR 2 upscaling.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        constants: &Fsr2Constants,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(constants));

        let Some(reconstruct_pipeline) = &self.reconstruct_pipeline else {
            return;
        };
        let Some(bind_group) = &self.bind_group else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("fsr2_reconstruct"),
            timestamp_writes: None,
        });
        pass.set_pipeline(reconstruct_pipeline);
        pass.set_bind_group(0, bind_group, &[]);

        let groups_x = self.display_width.div_ceil(16);
        let groups_y = self.display_height.div_ceil(16);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }

    /// Get the output view for compositing.
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.output_view.as_ref()
    }
}

/// FSR 2 shader constants.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Fsr2Constants {
    /// Render resolution (xy).
    pub render_resolution: [f32; 2],
    /// Display resolution (xy).
    pub display_resolution: [f32; 2],
    /// Current jitter offset (xy).
    pub jitter_offset: [f32; 2],
    /// Previous jitter offset (xy).
    pub prev_jitter_offset: [f32; 2],
    /// Sharpness (0-1).
    pub sharpness: f32,
    /// Frame index.
    pub frame_index: u32,
    /// Padding.
    pub _pad: [f32; 3],
}

/// FSR 3 frame generation pass (interpolates frames between rendered frames).
pub struct Fsr3FrameGenPass {
    /// Optical flow pipeline.
    pub optical_flow_pipeline: Option<wgpu::ComputePipeline>,
    /// Frame interpolation pipeline.
    pub interpolate_pipeline: Option<wgpu::ComputePipeline>,
    /// Previous frame color.
    pub prev_color: Option<wgpu::Texture>,
    pub prev_color_view: Option<wgpu::TextureView>,
    /// Optical flow texture.
    pub flow_texture: Option<wgpu::Texture>,
    pub flow_view: Option<wgpu::TextureView>,
    /// Interpolated frame output.
    pub interpolated_texture: Option<wgpu::Texture>,
    pub interpolated_view: Option<wgpu::TextureView>,
    /// Resolution.
    pub width: u32,
    pub height: u32,
    /// Frame generation enabled.
    pub enabled: bool,
}

impl Fsr3FrameGenPass {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let prev_color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fsr3_prev_color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let prev_color_view = prev_color.create_view(&wgpu::TextureViewDescriptor::default());

        let flow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fsr3_optical_flow"),
            size: wgpu::Extent3d {
                width: width / 2,
                height: height / 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let flow_view = flow_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let interpolated_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fsr3_interpolated"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let interpolated_view =
            interpolated_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            optical_flow_pipeline: None,
            interpolate_pipeline: None,
            prev_color: Some(prev_color),
            prev_color_view: Some(prev_color_view),
            flow_texture: Some(flow_texture),
            flow_view: Some(flow_view),
            interpolated_texture: Some(interpolated_texture),
            interpolated_view: Some(interpolated_view),
            width,
            height,
            enabled: false,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        *self = Self::new(device, width, height);
    }
}
