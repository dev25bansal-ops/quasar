//! Volumetric cloud rendering system.
//!
//! Provides real-time volumetric clouds using:
//! - Ray-marching through 3D noise textures
//! - Height-based density falloff
//! - Light scattering and absorption
//! - Temporal reprojection for performance

use bytemuck::{Pod, Zeroable};

/// Cloud configuration.
#[derive(Debug, Clone)]
pub struct CloudSettings {
    /// Cloud density multiplier.
    pub density: f32,
    /// Cloud coverage (0 = clear, 1 = overcast).
    pub coverage: f32,
    /// Cloud thickness in world units.
    pub thickness: f32,
    /// Cloud altitude range (min, max).
    pub altitude: [f32; 2],
    /// Wind direction and speed.
    pub wind: [f32; 2],
    /// Wind speed multiplier.
    pub wind_speed: f32,
    /// Light absorption factor.
    pub absorption: f32,
    /// Forward scattering intensity.
    pub forward_scatter: f32,
    /// Backward scattering intensity.
    pub back_scatter: f32,
    /// Number of ray-marching steps.
    pub steps: u32,
    /// Shadow steps for light sampling.
    pub shadow_steps: u32,
    /// Enable temporal reprojection.
    pub temporal_reprojection: bool,
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            density: 0.5,
            coverage: 0.5,
            thickness: 3000.0,
            altitude: [1500.0, 4500.0],
            wind: [1.0, 0.0],
            wind_speed: 100.0,
            absorption: 0.1,
            forward_scatter: 0.8,
            back_scatter: 0.2,
            steps: 64,
            shadow_steps: 8,
            temporal_reprojection: true,
        }
    }
}

/// Volumetric cloud pass.
pub struct VolumetricCloudPass {
    pub settings: CloudSettings,
    /// Compute pipeline for cloud ray-marching.
    pub pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Noise texture (Perlin-Worley).
    pub noise_texture: Option<wgpu::Texture>,
    pub noise_view: Option<wgpu::TextureView>,
    /// Detail noise texture.
    pub detail_texture: Option<wgpu::Texture>,
    pub detail_view: Option<wgpu::TextureView>,
    /// Cloud output texture.
    pub cloud_texture: Option<wgpu::Texture>,
    pub cloud_view: Option<wgpu::TextureView>,
    /// History texture for temporal reprojection.
    pub history_texture: Option<wgpu::Texture>,
    pub history_view: Option<wgpu::TextureView>,
    /// Depth texture for reprojection.
    pub reprojection_buffer: Option<wgpu::Buffer>,
    /// Resolution.
    pub width: u32,
    pub height: u32,
    /// Frame index for temporal.
    pub frame_index: u32,
}

impl VolumetricCloudPass {
    /// Create the volumetric cloud pass.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        settings: CloudSettings,
    ) -> Self {
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cloud_uniform"),
            size: std::mem::size_of::<CloudUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let noise_texture = Self::create_noise_texture(device, queue, 128);
        let noise_view = noise_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let detail_texture = Self::create_noise_texture(device, queue, 32);
        let detail_view = detail_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let cloud_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud_output"),
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
        let cloud_view = cloud_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud_history"),
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

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud_bgl"),
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
                        view_dimension: wgpu::TextureViewDimension::D3,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D3,
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
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
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
            uniform_buffer,
            noise_texture: Some(noise_texture),
            noise_view: Some(noise_view),
            detail_texture: Some(detail_texture),
            detail_view: Some(detail_view),
            cloud_texture: Some(cloud_texture),
            cloud_view: Some(cloud_view),
            history_texture: Some(history_texture),
            history_view: Some(history_view),
            reprojection_buffer: None,
            width: half_w,
            height: half_h,
            frame_index: 0,
        }
    }

    /// Create 3D noise texture for clouds.
    fn create_noise_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: u32,
    ) -> wgpu::Texture {
        let mut data = vec![0u8; (size * size * size * 4) as usize];

        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let idx = ((z * size * size + y * size + x) * 4) as usize;

                    let fx = x as f32 / size as f32;
                    let fy = y as f32 / size as f32;
                    let fz = z as f32 / size as f32;

                    let perlin = perlin_noise_3d(fx * 4.0, fy * 4.0, fz * 4.0);
                    let worley = worley_noise_3d(fx * 3.0, fy * 3.0, fz * 3.0);

                    let value = (perlin * 0.5 + worley * 0.5 + 0.5).clamp(0.0, 1.0);
                    let byte = (value * 255.0) as u8;

                    data[idx] = byte;
                    data[idx + 1] = byte;
                    data[idx + 2] = byte;
                    data[idx + 3] = 255;
                }
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("cloud_noise_{}", size)),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: size,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
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
            &data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: size,
            },
        );

        texture
    }

    /// Resize the cloud pass.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let half_w = (width / 2).max(1);
        let half_h = (height / 2).max(1);

        self.width = half_w;
        self.height = half_h;

        let cloud_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud_output"),
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
        self.cloud_view = Some(cloud_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.cloud_texture = Some(cloud_texture);

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud_history"),
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
        self.history_view =
            Some(history_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.history_texture = Some(history_texture);
    }

    /// Prepare uniform data.
    pub fn prepare_uniform(
        &self,
        camera_pos: [f32; 3],
        time: f32,
        sun_dir: [f32; 3],
    ) -> CloudUniform {
        let wind_offset = [
            time * self.settings.wind[0] * self.settings.wind_speed,
            time * self.settings.wind[1] * self.settings.wind_speed,
        ];

        CloudUniform {
            camera_pos,
            sun_dir,
            altitude: self.settings.altitude,
            thickness: self.settings.thickness,
            density: self.settings.density,
            coverage: self.settings.coverage,
            absorption: self.settings.absorption,
            forward_scatter: self.settings.forward_scatter,
            back_scatter: self.settings.back_scatter,
            wind_offset,
            resolution: [self.width as f32, self.height as f32],
            steps: self.settings.steps,
            shadow_steps: self.settings.shadow_steps,
            frame_index: self.frame_index,
            _pad: [0; 3],
        }
    }

    /// Dispatch cloud rendering.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        uniform: &CloudUniform,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));

        let Some(pipeline) = &self.pipeline else {
            return;
        };

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("volumetric_cloud_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);

        let groups_x = self.width.div_ceil(8);
        let groups_y = self.height.div_ceil(8);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }

    /// Get the output view for compositing.
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.cloud_view.as_ref()
    }
}

/// Cloud uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CloudUniform {
    /// Camera position (world space).
    pub camera_pos: [f32; 3],
    /// Sun direction.
    pub sun_dir: [f32; 3],
    /// Cloud altitude range (min, max).
    pub altitude: [f32; 2],
    /// Cloud thickness.
    pub thickness: f32,
    /// Cloud density.
    pub density: f32,
    /// Cloud coverage.
    pub coverage: f32,
    /// Light absorption.
    pub absorption: f32,
    /// Forward scattering.
    pub forward_scatter: f32,
    /// Backward scattering.
    pub back_scatter: f32,
    /// Wind offset (xy).
    pub wind_offset: [f32; 2],
    /// Resolution (xy).
    pub resolution: [f32; 2],
    /// Ray-marching steps.
    pub steps: u32,
    /// Shadow sampling steps.
    pub shadow_steps: u32,
    /// Frame index.
    pub frame_index: u32,
    /// Padding.
    pub _pad: [u32; 3],
}

/// 3D Perlin noise approximation.
pub fn perlin_noise_3d(x: f32, y: f32, z: f32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let zi = z.floor() as i32;

    let xf = x - x.floor();
    let yf = y - y.floor();
    let zf = z - z.floor();

    let u = fade(xf);
    let v = fade(yf);
    let w = fade(zf);

    let aaa = hash_3d(xi, yi, zi);
    let baa = hash_3d(xi + 1, yi, zi);
    let aba = hash_3d(xi, yi + 1, zi);
    let bba = hash_3d(xi + 1, yi + 1, zi);
    let aab = hash_3d(xi, yi, zi + 1);
    let bab = hash_3d(xi + 1, yi, zi + 1);
    let abb = hash_3d(xi, yi + 1, zi + 1);
    let bbb = hash_3d(xi + 1, yi + 1, zi + 1);

    let x1 = lerp(aaa, baa, u);
    let x2 = lerp(aba, bba, u);
    let y1 = lerp(x1, x2, v);

    let x3 = lerp(aab, bab, u);
    let x4 = lerp(abb, bbb, u);
    let y2 = lerp(x3, x4, v);

    lerp(y1, y2, w)
}

/// 3D Worley noise (cellular).
pub fn worley_noise_3d(x: f32, y: f32, z: f32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let zi = z.floor() as i32;

    let mut min_dist = f32::MAX;

    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cell_x = xi + dx;
                let cell_y = yi + dy;
                let cell_z = zi + dz;

                let point_x = cell_x as f32 + hash_3d(cell_x, cell_y, cell_z) % 1.0;
                let point_y = cell_y as f32 + hash_3d(cell_x + 31, cell_y, cell_z) % 1.0;
                let point_z = cell_z as f32 + hash_3d(cell_x, cell_y + 17, cell_z) % 1.0;

                let dist =
                    ((x - point_x).powi(2) + (y - point_y).powi(2) + (z - point_z).powi(2)).sqrt();
                min_dist = min_dist.min(dist);
            }
        }
    }

    min_dist
}

#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

fn hash_3d(x: i32, y: i32, z: i32) -> f32 {
    let mut h = x as u32;
    h = h.wrapping_mul(374761393);
    h ^= y as u32;
    h = h.wrapping_mul(374761393);
    h ^= z as u32;
    h = h.wrapping_mul(374761393);
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Height-based cloud density falloff.
pub fn height_fog(height: f32, altitude_min: f32, altitude_max: f32) -> f32 {
    let mid = (altitude_min + altitude_max) * 0.5;
    let range = (altitude_max - altitude_min) * 0.5;

    let normalized = (height - mid) / range;
    let falloff = 1.0 - normalized.abs();

    falloff.max(0.0)
}
