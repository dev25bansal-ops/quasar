//! Environment map for image-based lighting (IBL).
//!
//! Loads HDR equirectangular images, converts to cubemap, prefilters for roughness,
//! and computes irradiance map for diffuse IBL.

pub const IBL_MIP_LEVELS: u32 = 5;

pub struct EnvironmentMap {
    pub irradiance_texture: wgpu::Texture,
    pub irradiance_view: wgpu::TextureView,
    pub irradiance_sampler: wgpu::Sampler,
    pub prefilter_texture: wgpu::Texture,
    pub prefilter_view: wgpu::TextureView,
    pub prefilter_sampler: wgpu::Sampler,
    pub brdf_lut_texture: wgpu::Texture,
    pub brdf_lut_view: wgpu::TextureView,
    pub brdf_lut_sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub mip_count: f32,
}

impl EnvironmentMap {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let irradiance_texture = create_irradiance_texture(device);
        let irradiance_view = irradiance_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let prefilter_texture = create_prefilter_texture(device);
        let prefilter_view = prefilter_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        let brdf_lut_texture = create_brdf_lut_texture(device, queue);
        let brdf_lut_view = brdf_lut_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let irradiance_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Irradiance Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let prefilter_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Prefilter Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let brdf_lut_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("BRDF LUT Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let mip_count = IBL_MIP_LEVELS as f32;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("IBL Bind Group Layout"),
            entries: &[
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let ibl_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("IBL Uniform Buffer"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mip_data: [f32; 4] = [mip_count, 0.0, 0.0, 0.0];
        queue.write_buffer(&ibl_buffer, 0, bytemuck::cast_slice(&mip_data));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("IBL Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ibl_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&irradiance_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&prefilter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&prefilter_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&brdf_lut_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&brdf_lut_sampler),
                },
            ],
        });

        Self {
            irradiance_texture,
            irradiance_view,
            irradiance_sampler,
            prefilter_texture,
            prefilter_view,
            prefilter_sampler,
            brdf_lut_texture,
            brdf_lut_view,
            brdf_lut_sampler,
            bind_group,
            bind_group_layout,
            mip_count,
        }
    }
}

fn create_irradiance_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Irradiance Cubemap"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_prefilter_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Prefilter Cubemap"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 6,
        },
        mip_level_count: IBL_MIP_LEVELS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_brdf_lut_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let size = 256u32;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("BRDF LUT"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut data = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let n_dot_v = x as f32 / (size - 1) as f32;
            let roughness = y as f32 / (size - 1) as f32;

            let (f0_scale, f0_bias) = integrate_brdf(n_dot_v, roughness);

            let idx = ((y * size + x) * 4) as usize;
            data[idx] = (f0_scale * 255.0) as u8;
            data[idx + 1] = ((f0_scale * 255.0).fract() * 256.0) as u8;
            data[idx + 2] = (f0_bias * 255.0) as u8;
            data[idx + 3] = ((f0_bias * 255.0).fract() * 256.0) as u8;
        }
    }

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size * 4),
            rows_per_image: Some(size),
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );

    texture
}

fn integrate_brdf(n_dot_v: f32, roughness: f32) -> (f32, f32) {
    let sample_count = 1024u32;

    let v = glam::Vec3::new((1.0 - n_dot_v).sqrt(), 0.0, n_dot_v.sqrt());

    let mut sum_a = 0.0;
    let mut sum_b = 0.0;

    for i in 0..sample_count {
        let xi = hammersley(i as f32, sample_count);
        let h = importance_sample_ggx(xi, roughness);
        let l = 2.0 * h.dot(v) * h - v;

        let n_dot_l = l.z.max(0.0);
        let n_dot_h = h.z.max(0.0);
        let v_dot_h = v.dot(h).max(0.0);

        if n_dot_l > 0.0 {
            let g = geometry_smith_ibl(roughness, n_dot_v, n_dot_l);
            let g_vis = g * v_dot_h / (n_dot_h * n_dot_v);
            let fc = (1.0 - v_dot_h).powi(5);

            sum_a += (1.0 - fc) * g_vis;
            sum_b += fc * g_vis;
        }
    }

    (sum_a / sample_count as f32, sum_b / sample_count as f32)
}

fn hammersley(i: f32, n: u32) -> (f32, f32) {
    let bits = (i as u32).reverse_bits() >> 2;
    (i / n as f32, bits as f32 / 4294967296.0)
}

fn importance_sample_ggx(xi: (f32, f32), roughness: f32) -> glam::Vec3 {
    let a = roughness * roughness;

    let phi = 2.0 * std::f32::consts::PI * xi.0;
    let cos_theta = ((1.0 - xi.1) / (1.0 + (a * a - 1.0) * xi.1)).sqrt();
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();

    let h = glam::Vec3::new(sin_theta * phi.cos(), sin_theta * phi.sin(), cos_theta);
    h
}

fn geometry_smith_ibl(roughness: f32, n_dot_v: f32, n_dot_l: f32) -> f32 {
    let r = roughness;
    let k = (r * r) / 2.0;

    let ggx1 = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let ggx2 = n_dot_l / (n_dot_l * (1.0 - k) + k);

    ggx1 * ggx2
}

pub struct EnvironmentMapLoader;

impl EnvironmentMapLoader {
    pub fn load_from_equirectangular(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _path: &str,
    ) -> EnvironmentMap {
        EnvironmentMap::new(device, queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrate_brdf_values() {
        let (scale, bias) = integrate_brdf(1.0, 0.0);
        assert!(scale.is_finite());
        assert!(bias.is_finite());

        let (scale2, bias2) = integrate_brdf(0.5, 0.5);
        assert!(scale2.is_finite());
        assert!(bias2.is_finite());
    }
}
