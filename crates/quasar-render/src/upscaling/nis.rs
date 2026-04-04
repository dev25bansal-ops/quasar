//! NVIDIA Image Scaling (NIS) implementation.
//!
//! NIS is a spatial upscaler that provides good quality without
//! requiring motion vectors or temporal history.
//!
//! Key features:
//! - Single-pass spatial upscaling
//! - Optional sharpening pass
//! - No temporal artifacts

use bytemuck::{Pod, Zeroable};

/// NIS configuration.
#[derive(Debug, Clone)]
pub struct NisSettings {
    /// Upscaling mode.
    pub mode: NisMode,
    /// Sharpness (0.0 - 1.0).
    pub sharpness: f32,
    /// Use HDR mode.
    pub hdr: bool,
}

impl Default for NisSettings {
    fn default() -> Self {
        Self {
            mode: NisMode::ScalerSharp,
            sharpness: 0.5,
            hdr: false,
        }
    }
}

/// NIS operation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NisMode {
    /// Upscale only (no sharpening).
    Scaler,
    /// Upscale with sharpening.
    ScalerSharp,
    /// Sharpen only (no upscaling).
    Sharpness,
}

/// NIS upscaling pass.
pub struct NisPass {
    pub settings: NisSettings,
    /// Compute pipeline.
    pub pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Current bind group.
    pub bind_group: Option<wgpu::BindGroup>,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Coefficient texture (precomputed NIS coefficients).
    pub coef_texture: Option<wgpu::Texture>,
    pub coef_view: Option<wgpu::TextureView>,
    /// Output texture.
    pub output_texture: Option<wgpu::Texture>,
    pub output_view: Option<wgpu::TextureView>,
    /// Input resolution.
    pub input_width: u32,
    pub input_height: u32,
    /// Output resolution.
    pub output_width: u32,
    pub output_height: u32,
}

impl NisPass {
    /// Create NIS pass.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input_width: u32,
        input_height: u32,
        output_width: u32,
        output_height: u32,
        settings: NisSettings,
    ) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nis_uniform"),
            size: std::mem::size_of::<NisConstants>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let coef_texture = Self::create_coefficient_texture(device, queue);
        let coef_view = coef_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nis_output"),
            size: wgpu::Extent3d {
                width: output_width,
                height: output_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nis_bgl"),
            entries: &[
                // 0: constants
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
                // 2: coefficients
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // 3: output
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // 4: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        Self {
            settings,
            pipeline: None,
            bind_group_layout,
            bind_group: None,
            uniform_buffer,
            coef_texture: Some(coef_texture),
            coef_view: Some(coef_view),
            output_texture: Some(output_texture),
            output_view: Some(output_view),
            input_width,
            input_height,
            output_width,
            output_height,
        }
    }

    /// Create precomputed NIS coefficient texture.
    fn create_coefficient_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
        const COEF_WIDTH: u32 = 128;
        const COEF_HEIGHT: u32 = 128;

        let mut data = vec![0.0f32; (COEF_WIDTH * COEF_HEIGHT * 4) as usize];

        for y in 0..COEF_HEIGHT {
            for x in 0..COEF_WIDTH {
                let idx = ((y * COEF_WIDTH + x) * 4) as usize;
                let fx = x as f32 / COEF_WIDTH as f32;
                let fy = y as f32 / COEF_HEIGHT as f32;

                let kernel_x = Self::nis_kernel(fx);
                let kernel_y = Self::nis_kernel(fy);

                data[idx] = kernel_x;
                data[idx + 1] = kernel_y;
                data[idx + 2] = kernel_x * kernel_y;
                data[idx + 3] = 1.0;
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nis_coef"),
            size: wgpu::Extent3d {
                width: COEF_WIDTH,
                height: COEF_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&data),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(COEF_WIDTH * 16),
                rows_per_image: Some(COEF_HEIGHT),
            },
            wgpu::Extent3d {
                width: COEF_WIDTH,
                height: COEF_HEIGHT,
                depth_or_array_layers: 1,
            },
        );

        texture
    }

    /// NIS Lanczos-style kernel approximation.
    fn nis_kernel(x: f32) -> f32 {
        let a = 3.0;
        if x.abs() < 1e-6 {
            1.0
        } else if x.abs() < a {
            let pi_x = std::f32::consts::PI * x;
            (pi_x.sin() / pi_x) * (pi_x.sin() / a / pi_x)
        } else {
            0.0
        }
    }

    /// Resize the upscaler.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        input_width: u32,
        input_height: u32,
        output_width: u32,
        output_height: u32,
    ) {
        self.input_width = input_width;
        self.input_height = input_height;
        self.output_width = output_width;
        self.output_height = output_height;

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nis_output"),
            size: wgpu::Extent3d {
                width: output_width,
                height: output_height,
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
    pub fn prepare_constants(&self) -> NisConstants {
        let scale_x = self.input_width as f32 / self.output_width as f32;
        let scale_y = self.input_height as f32 / self.output_height as f32;

        NisConstants {
            input_resolution: [self.input_width as f32, self.input_height as f32],
            output_resolution: [self.output_width as f32, self.output_height as f32],
            scale: [scale_x, scale_y],
            sharpness: self.settings.sharpness,
            mode: self.settings.mode as u32,
            _pad: [0.0; 2],
        }
    }

    /// Dispatch NIS upscaling.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        constants: &NisConstants,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(constants));

        let Some(pipeline) = &self.pipeline else {
            return;
        };
        let Some(bind_group) = &self.bind_group else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("nis_upscale"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);

        let groups_x = self.output_width.div_ceil(16);
        let groups_y = self.output_height.div_ceil(16);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }

    /// Get the output view.
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.output_view.as_ref()
    }
}

/// NIS shader constants.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct NisConstants {
    /// Input resolution (xy).
    pub input_resolution: [f32; 2],
    /// Output resolution (xy).
    pub output_resolution: [f32; 2],
    /// Scale factor (xy).
    pub scale: [f32; 2],
    /// Sharpness (0-1).
    pub sharpness: f32,
    /// Mode (0=Scaler, 1=ScalerSharp, 2=Sharpness).
    pub mode: u32,
    /// Padding.
    pub _pad: [f32; 2],
}

/// DLSS placeholder - requires NVIDIA SDK integration.
///
/// To enable DLSS:
/// 1. Download the NVIDIA DLSS SDK
/// 2. Link the native DLSS library
/// 3. Enable the `dlss` feature flag
pub struct DlssPass {
    /// DLSS is unavailable without the SDK.
    pub available: bool,
    /// Placeholder for SDK handle.
    _sdk_handle: Option<()>,
}

impl DlssPass {
    pub fn new() -> Self {
        Self {
            available: false,
            _sdk_handle: None,
        }
    }

    pub fn is_available(&self) -> bool {
        self.available
    }
}

impl Default for DlssPass {
    fn default() -> Self {
        Self::new()
    }
}
