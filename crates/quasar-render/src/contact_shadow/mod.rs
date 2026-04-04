//! Screen-space contact shadows.
//!
//! Provides high-frequency detail shadows that standard
//! shadow maps miss:
//! - Contact shadows for small geometry
//! - Self-shadowing for details
//! - Screen-space ambient occlusion with directional bias

use bytemuck::{Pod, Zeroable};

/// Contact shadow settings.
#[derive(Debug, Clone, Copy)]
pub struct ContactShadowSettings {
    /// Number of ray-marching steps.
    pub steps: u32,
    /// Maximum trace distance in world units.
    pub max_distance: f32,
    /// Step size multiplier.
    pub step_size: f32,
    /// Thickness threshold for occlusion.
    pub thickness: f32,
    /// Intensity multiplier.
    pub intensity: f32,
    /// Falloff exponent.
    pub falloff: f32,
    /// Enable temporal filtering.
    pub temporal_filter: bool,
    /// Temporal blend factor.
    pub temporal_blend: f32,
    /// Enable spatial denoising.
    pub spatial_denoise: bool,
    /// Denoiser radius.
    pub denoise_radius: u32,
}

impl Default for ContactShadowSettings {
    fn default() -> Self {
        Self {
            steps: 16,
            max_distance: 0.5,
            step_size: 0.03,
            thickness: 0.2,
            intensity: 1.0,
            falloff: 1.0,
            temporal_filter: true,
            temporal_blend: 0.9,
            spatial_denoise: true,
            denoise_radius: 2,
        }
    }
}

/// Screen-space contact shadow pass.
pub struct ContactShadowPass {
    pub settings: ContactShadowSettings,
    /// Compute pipeline for ray-marching.
    pub pipeline: Option<wgpu::ComputePipeline>,
    /// Denoiser pipeline.
    pub denoise_pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Output shadow texture.
    pub shadow_texture: Option<wgpu::Texture>,
    pub shadow_view: Option<wgpu::TextureView>,
    /// Denoised output.
    pub denoised_texture: Option<wgpu::Texture>,
    pub denoised_view: Option<wgpu::TextureView>,
    /// History for temporal filtering.
    pub history_texture: Option<wgpu::Texture>,
    pub history_view: Option<wgpu::TextureView>,
    /// Resolution.
    pub width: u32,
    pub height: u32,
    /// Frame index.
    pub frame_index: u32,
}

impl ContactShadowPass {
    /// Create contact shadow pass.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        settings: ContactShadowSettings,
    ) -> Self {
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("contact_shadow_uniform"),
            size: std::mem::size_of::<ContactShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact_shadow"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let denoised_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact_shadow_denoised"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let denoised_view = denoised_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact_shadow_history"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let history_view = history_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("contact_shadow_bgl"),
            entries: &[
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        Self {
            settings,
            pipeline: None,
            denoise_pipeline: None,
            bind_group_layout,
            uniform_buffer,
            shadow_texture: Some(shadow_texture),
            shadow_view: Some(shadow_view),
            denoised_texture: Some(denoised_texture),
            denoised_view: Some(denoised_view),
            history_texture: Some(history_texture),
            history_view: Some(history_view),
            width: half_w,
            height: half_h,
            frame_index: 0,
        }
    }

    /// Resize the pass.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        self.width = half_w;
        self.height = half_h;

        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact_shadow"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.shadow_view =
            Some(shadow_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.shadow_texture = Some(shadow_texture);

        let denoised_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact_shadow_denoised"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.denoised_view =
            Some(denoised_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.denoised_texture = Some(denoised_texture);

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("contact_shadow_history"),
            size: wgpu::Extent3d {
                width: half_w,
                height: half_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.history_view =
            Some(history_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.history_texture = Some(history_texture);
    }

    /// Prepare uniform data.
    pub fn prepare_uniform(
        &self,
        inv_view_proj: [[f32; 4]; 4],
        view_proj: [[f32; 4]; 4],
        light_dir: [f32; 3],
        camera_pos: [f32; 3],
    ) -> ContactShadowUniform {
        ContactShadowUniform {
            inv_view_proj,
            view_proj,
            light_dir,
            camera_pos,
            resolution: [self.width as f32, self.height as f32],
            max_distance: self.settings.max_distance,
            step_size: self.settings.step_size,
            thickness: self.settings.thickness,
            intensity: self.settings.intensity,
            falloff: self.settings.falloff,
            steps: self.settings.steps,
            temporal_blend: self.settings.temporal_blend,
            denoise_radius: self.settings.denoise_radius,
            frame_index: self.frame_index,
            _pad: [0; 3],
        }
    }

    /// Dispatch contact shadow calculation.
    pub fn dispatch(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        uniform: &ContactShadowUniform,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));

        let Some(pipeline) = &self.pipeline else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("contact_shadow_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);

        let groups_x = self.width.div_ceil(8);
        let groups_y = self.height.div_ceil(8);
        pass.dispatch_workgroups(groups_x, groups_y, 1);

        self.frame_index += 1;
    }

    /// Apply spatial denoiser.
    pub fn denoise(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.settings.spatial_denoise {
            return;
        }

        let Some(pipeline) = &self.denoise_pipeline else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("contact_shadow_denoise"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);

        let groups_x = self.width.div_ceil(8);
        let groups_y = self.height.div_ceil(8);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }

    /// Get output view.
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        if self.settings.spatial_denoise {
            self.denoised_view.as_ref()
        } else {
            self.shadow_view.as_ref()
        }
    }
}

/// Contact shadow uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ContactShadowUniform {
    pub inv_view_proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub light_dir: [f32; 3],
    pub camera_pos: [f32; 3],
    pub resolution: [f32; 2],
    pub max_distance: f32,
    pub step_size: f32,
    pub thickness: f32,
    pub intensity: f32,
    pub falloff: f32,
    pub steps: u32,
    pub temporal_blend: f32,
    pub denoise_radius: u32,
    pub frame_index: u32,
    pub _pad: [u32; 3],
}

/// Screen-space contact shadow trace.
pub fn trace_contact_shadow(
    depth: f32,
    depth_buffer: &[f32],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    light_dir: [f32; 3],
    steps: u32,
    max_distance: f32,
    thickness: f32,
) -> f32 {
    let mut occlusion = 0.0f32;

    let step_size = max_distance / steps as f32;

    for i in 1..=steps {
        let t = i as f32 * step_size;

        let sample_x = x as f32 + light_dir[0] * t;
        let sample_y = y as f32 + light_dir[1] * t;

        if sample_x < 0.0 || sample_x >= width as f32 || sample_y < 0.0 || sample_y >= height as f32
        {
            break;
        }

        let sx = sample_x as u32;
        let sy = sample_y as u32;

        let sample_depth = depth_buffer[(sy * width + sx) as usize];

        let depth_diff = depth - sample_depth;

        if depth_diff > 0.0 && depth_diff < thickness {
            occlusion = (t / max_distance).max(occlusion);
        }
    }

    1.0 - occlusion
}

/// Bilateral denoiser for contact shadows.
pub fn bilateral_denoise(
    shadow: &[f32],
    depth: &[f32],
    normal: &[[f32; 3]],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    radius: u32,
) -> f32 {
    let center_idx = (y * width + x) as usize;
    let center_shadow = shadow[center_idx];
    let center_depth = depth[center_idx];
    let center_normal = normal[center_idx];

    let mut sum = 0.0;
    let mut weight_sum = 0.0;

    let sigma_depth = 0.01;
    let sigma_normal = 0.1;
    let sigma_space = radius as f32;

    for dy in -(radius as i32)..=radius as i32 {
        for dx in -(radius as i32)..=radius as i32 {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;

            if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                continue;
            }

            let nidx = (ny as u32 * width + nx as u32) as usize;
            let sample_shadow = shadow[nidx];
            let sample_depth = depth[nidx];
            let sample_normal = normal[nidx];

            let depth_diff = (center_depth - sample_depth).abs();
            let normal_diff = (center_normal[0] - sample_normal[0]).abs()
                + (center_normal[1] - sample_normal[1]).abs()
                + (center_normal[2] - sample_normal[2]).abs();
            let space_diff = ((dx * dx + dy * dy) as f32).sqrt();

            let w_depth = (-depth_diff * depth_diff / (2.0 * sigma_depth * sigma_depth)).exp();
            let w_normal = (-normal_diff * normal_diff / (2.0 * sigma_normal * sigma_normal)).exp();
            let w_space = (-space_diff * space_diff / (2.0 * sigma_space * sigma_space)).exp();

            let weight = w_depth * w_normal * w_space;

            sum += sample_shadow * weight;
            weight_sum += weight;
        }
    }

    if weight_sum > 0.0 {
        sum / weight_sum
    } else {
        center_shadow
    }
}

/// Combine contact shadows with regular shadows.
pub fn combine_shadows(base_shadow: f32, contact_shadow: f32, intensity: f32) -> f32 {
    base_shadow * (1.0 - intensity * (1.0 - contact_shadow))
}
