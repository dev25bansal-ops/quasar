//! Foliage rendering system with wind animation.
//!
//! Provides instanced rendering for:
//! - Grass and ground cover
//! - Trees and bushes
//! - Wind animation with gusts
//! - LOD for distant foliage

use bytemuck::{Pod, Zeroable};

/// Foliage configuration.
#[derive(Debug, Clone)]
pub struct FoliageSettings {
    /// Wind direction (normalized).
    pub wind_direction: [f32; 2],
    /// Wind speed.
    pub wind_speed: f32,
    /// Wind strength (amplitude).
    pub wind_strength: f32,
    /// Wind gust frequency.
    pub gust_frequency: f32,
    /// Wind gust amplitude.
    pub gust_amplitude: f32,
    /// LOD distances (0 = near, 1 = mid, 2 = far).
    pub lod_distances: [f32; 3],
    /// Maximum instances per draw.
    pub max_instances: u32,
    /// Billboarding for distant foliage.
    pub billboard_start_distance: f32,
    /// Fade distance for billboards.
    pub billboard_fade_distance: f32,
}

impl Default for FoliageSettings {
    fn default() -> Self {
        Self {
            wind_direction: [1.0, 0.0],
            wind_speed: 1.0,
            wind_strength: 0.3,
            gust_frequency: 0.5,
            gust_amplitude: 0.5,
            lod_distances: [50.0, 150.0, 400.0],
            max_instances: 100000,
            billboard_start_distance: 200.0,
            billboard_fade_distance: 50.0,
        }
    }
}

/// Foliage instance data.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct FoliageInstance {
    /// Position (xyz), scale (w).
    pub position_scale: [f32; 4],
    /// Rotation (xyz), wind_weight (w).
    pub rotation_wind: [f32; 4],
    /// Color tint (rgba).
    pub color: [f32; 4],
    /// LOD parameters (lod_level, _pad).
    pub lod_params: [f32; 4],
}

impl FoliageInstance {
    pub fn new(position: [f32; 3], scale: f32, rotation: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position_scale: [position[0], position[1], position[2], scale],
            rotation_wind: [rotation[0], rotation[1], rotation[2], 1.0],
            color,
            lod_params: [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn with_wind_weight(mut self, weight: f32) -> Self {
        self.rotation_wind[3] = weight;
        self
    }
}

/// Foliage type configuration.
#[derive(Debug, Clone)]
pub struct FoliageType {
    /// Mesh handle.
    pub mesh_id: u64,
    /// Material handle.
    pub material_id: u64,
    /// Height range for placement.
    pub height_range: [f32; 2],
    /// Slope range for placement (0-90 degrees).
    pub slope_range: [f32; 2],
    /// Density per square unit.
    pub density: f32,
    /// Scale range.
    pub scale_range: [f32; 2],
    /// Random scale variation.
    pub scale_variation: f32,
    /// Color variation range.
    pub color_variation: [f32; 4],
    /// Wind bending factor (0 = rigid, 1 = flexible).
    pub wind_bending: f32,
    /// LOD mesh handles (0=high, 1=mid, 2=low).
    pub lod_meshes: [u64; 3],
}

impl Default for FoliageType {
    fn default() -> Self {
        Self {
            mesh_id: 0,
            material_id: 0,
            height_range: [0.0, 100.0],
            slope_range: [0.0, 45.0],
            density: 1.0,
            scale_range: [0.8, 1.2],
            scale_variation: 0.2,
            color_variation: [0.1, 0.1, 0.1, 0.0],
            wind_bending: 0.5,
            lod_meshes: [0; 3],
        }
    }
}

/// Foliage renderer.
pub struct FoliageRenderer {
    pub settings: FoliageSettings,
    /// Render pipeline.
    pub pipeline: Option<wgpu::RenderPipeline>,
    /// Pipeline for billboards.
    pub billboard_pipeline: Option<wgpu::RenderPipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Instance buffer.
    pub instance_buffer: Option<wgpu::Buffer>,
    /// Indirect draw buffer.
    pub indirect_buffer: Option<wgpu::Buffer>,
    /// Wind noise texture.
    pub wind_texture: Option<wgpu::Texture>,
    pub wind_view: Option<wgpu::TextureView>,
    /// Current instance count.
    pub instance_count: u32,
    /// Time accumulator.
    pub time: f32,
}

impl FoliageRenderer {
    /// Create foliage renderer.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, settings: FoliageSettings) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("foliage_uniform"),
            size: std::mem::size_of::<FoliageUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("foliage_instances"),
            size: std::mem::size_of::<FoliageInstance>() as u64 * settings.max_instances as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("foliage_indirect"),
            size: std::mem::size_of::<DrawIndexedIndirect>() as u64,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let wind_texture = Self::create_wind_texture(device, queue, 256);
        let wind_view = wind_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("foliage_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
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
            ],
        });

        Self {
            settings,
            pipeline: None,
            billboard_pipeline: None,
            bind_group_layout,
            uniform_buffer,
            instance_buffer: Some(instance_buffer),
            indirect_buffer: Some(indirect_buffer),
            wind_texture: Some(wind_texture),
            wind_view: Some(wind_view),
            instance_count: 0,
            time: 0.0,
        }
    }

    /// Create wind noise texture.
    fn create_wind_texture(device: &wgpu::Device, queue: &wgpu::Queue, size: u32) -> wgpu::Texture {
        let mut data = vec![0u8; (size * size * 4) as usize];

        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                let fx = x as f32 / size as f32;
                let fy = y as f32 / size as f32;

                let noise = wind_noise(fx * 8.0, fy * 8.0);
                let value = ((noise + 1.0) * 0.5 * 255.0) as u8;

                data[idx] = value;
                data[idx + 1] = value;
                data[idx + 2] = value;
                data[idx + 3] = 255;
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wind_noise"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
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
                depth_or_array_layers: 1,
            },
        );

        texture
    }

    /// Prepare uniform data.
    pub fn prepare_uniform(
        &self,
        view_proj: [[f32; 4]; 4],
        camera_pos: [f32; 3],
        time: f32,
    ) -> FoliageUniform {
        FoliageUniform {
            view_proj,
            camera_pos,
            wind_direction: self.settings.wind_direction,
            wind_speed: self.settings.wind_speed,
            wind_strength: self.settings.wind_strength,
            gust_frequency: self.settings.gust_frequency,
            gust_amplitude: self.settings.gust_amplitude,
            lod_distances: self.settings.lod_distances,
            billboard_start: self.settings.billboard_start_distance,
            billboard_fade: self.settings.billboard_fade_distance,
            time,
            _pad: [0; 3],
        }
    }

    /// Update instances.
    pub fn update_instances(&self, queue: &wgpu::Queue, instances: &[FoliageInstance]) {
        if instances.is_empty() {
            return;
        }

        let count = instances.len().min(self.settings.max_instances as usize);
        queue.write_buffer(
            self.instance_buffer.as_ref().unwrap(),
            0,
            bytemuck::cast_slice(&instances[..count]),
        );
    }

    /// Render foliage.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        uniform: &FoliageUniform,
        target_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        index_count: u32,
    ) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        let Some(instance_buffer) = &self.instance_buffer else {
            return;
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("foliage_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, instance_buffer.slice(..));
        pass.draw(0..index_count, 0..self.instance_count);
    }
}

/// Foliage uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct FoliageUniform {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub wind_direction: [f32; 2],
    pub wind_speed: f32,
    pub wind_strength: f32,
    pub gust_frequency: f32,
    pub gust_amplitude: f32,
    pub lod_distances: [f32; 3],
    pub billboard_start: f32,
    pub billboard_fade: f32,
    pub time: f32,
    pub _pad: [u32; 3],
}

/// Draw indexed indirect command.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct DrawIndexedIndirect {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}

/// Wind noise function.
pub fn wind_noise(x: f32, y: f32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0;

    for _ in 0..4 {
        let xi = (x * frequency).floor() as i32;
        let yi = (y * frequency).floor() as i32;

        let n =
            hash_2d(xi, yi) + hash_2d(xi + 1, yi) + hash_2d(xi, yi + 1) + hash_2d(xi + 1, yi + 1);
        value += amplitude * n / 4.0;

        amplitude *= 0.5;
        frequency *= 2.0;
    }

    value
}

fn hash_2d(x: i32, y: i32) -> f32 {
    let mut h = x as u32;
    h = h.wrapping_mul(374761393);
    h ^= y as u32;
    h = h.wrapping_mul(374761393);
    (h as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Calculate wind displacement for a vertex.
pub fn wind_displacement(
    _position: [f32; 3],
    wind_direction: [f32; 2],
    wind_strength: f32,
    gust_phase: f32,
    height_factor: f32,
) -> [f32; 3] {
    let gust = (gust_phase * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5;
    let strength = wind_strength * (1.0 + gust);

    [
        wind_direction[0] * strength * height_factor,
        0.0,
        wind_direction[1] * strength * height_factor,
    ]
}

/// Foliage LOD selector.
pub fn select_lod(distance: f32, lod_distances: [f32; 3]) -> u32 {
    if distance < lod_distances[0] {
        0
    } else if distance < lod_distances[1] {
        1
    } else if distance < lod_distances[2] {
        2
    } else {
        3
    }
}

/// Grass blade generator.
pub fn generate_grass_blade(
    blade_count: u32,
    area_size: f32,
    density: f32,
    height_range: [f32; 2],
) -> Vec<FoliageInstance> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let count = (blade_count as f32 * density) as usize;
    let mut instances = Vec::with_capacity(count);

    for _ in 0..count {
        let x = rng.gen_range(-area_size..area_size);
        let z = rng.gen_range(-area_size..area_size);
        let height = rng.gen_range(height_range[0]..height_range[1]);
        let scale = rng.gen_range(0.8..1.2);
        let rotation = rng.gen_range(0.0..std::f32::consts::TAU);

        let color_var = rng.gen_range(0.0..0.1);
        let color = [0.2 + color_var, 0.5 + color_var, 0.1 + color_var, 1.0];

        instances.push(FoliageInstance::new(
            [x, height, z],
            scale,
            [0.0, rotation, 0.0],
            color,
        ));
    }

    instances
}
