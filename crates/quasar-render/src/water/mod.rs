//! Water rendering system with caustics.
//!
//! Provides realistic water surfaces with:
//! - Fresnel reflections and refractions
//! - Caustics on underwater surfaces
//! - Wave displacement (Gerstner waves)
//! - Foam and spray effects

use bytemuck::{Pod, Zeroable};

/// Water configuration.
#[derive(Debug, Clone)]
pub struct WaterSettings {
    /// Water color (deep).
    pub deep_color: [f32; 4],
    /// Water color (shallow).
    pub shallow_color: [f32; 4],
    /// Water transparency.
    pub transparency: f32,
    /// Fresnel bias.
    pub fresnel_bias: f32,
    /// Fresnel power.
    pub fresnel_power: f32,
    /// Refraction distortion scale.
    pub refraction_distortion: f32,
    /// Reflection intensity.
    pub reflection_intensity: f32,
    /// Wave amplitude.
    pub wave_amplitude: f32,
    /// Wave frequency.
    pub wave_frequency: f32,
    /// Wave speed.
    pub wave_speed: f32,
    /// Number of Gerstner waves.
    pub wave_count: u32,
    /// Caustics intensity.
    pub caustics_intensity: f32,
    /// Caustics scale.
    pub caustics_scale: f32,
    /// Caustics speed.
    pub caustics_speed: f32,
    /// Foam threshold (wave height).
    pub foam_threshold: f32,
    /// Foam intensity.
    pub foam_intensity: f32,
    /// Normal map intensity.
    pub normal_intensity: f32,
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            deep_color: [0.0, 0.05, 0.2, 1.0],
            shallow_color: [0.0, 0.3, 0.5, 1.0],
            transparency: 0.8,
            fresnel_bias: 0.1,
            fresnel_power: 2.0,
            refraction_distortion: 0.02,
            reflection_intensity: 0.5,
            wave_amplitude: 1.0,
            wave_frequency: 0.1,
            wave_speed: 1.0,
            wave_count: 4,
            caustics_intensity: 0.5,
            caustics_scale: 1.0,
            caustics_speed: 0.5,
            foam_threshold: 0.8,
            foam_intensity: 0.3,
            normal_intensity: 1.0,
        }
    }
}

/// Gerstner wave parameters.
#[derive(Debug, Clone, Copy)]
pub struct GerstnerWave {
    /// Wave direction (normalized).
    pub direction: [f32; 2],
    /// Wave steepness (0-1).
    pub steepness: f32,
    /// Wavelength.
    pub wavelength: f32,
    /// Wave speed multiplier.
    pub speed: f32,
}

impl GerstnerWave {
    /// Create a new Gerstner wave.
    pub fn new(direction: [f32; 2], steepness: f32, wavelength: f32, speed: f32) -> Self {
        let len = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
        Self {
            direction: [direction[0] / len, direction[1] / len],
            steepness,
            wavelength,
            speed,
        }
    }

    /// Evaluate wave displacement at a point.
    pub fn evaluate(&self, x: f32, z: f32, time: f32) -> [f32; 3] {
        let k = 2.0 * std::f32::consts::PI / self.wavelength;
        let c = (9.8 / k).sqrt() * self.speed;
        let f = k * (self.direction[0] * x + self.direction[1] * z - c * time);

        let a = self.steepness / k;

        [
            self.direction[0] * a * f.cos(),
            a * f.sin(),
            self.direction[1] * a * f.cos(),
        ]
    }

    /// Evaluate wave normal at a point.
    pub fn normal(&self, x: f32, z: f32, time: f32) -> [f32; 3] {
        let k = 2.0 * std::f32::consts::PI / self.wavelength;
        let c = (9.8 / k).sqrt() * self.speed;
        let f = k * (self.direction[0] * x + self.direction[1] * z - c * time);

        let _a = self.steepness / k;

        [
            -self.direction[0] * self.steepness * f.cos(),
            -self.steepness * f.sin(),
            -self.direction[1] * self.steepness * f.cos(),
        ]
    }
}

/// Water surface renderer.
pub struct WaterPass {
    pub settings: WaterSettings,
    /// Render pipeline.
    pub pipeline: Option<wgpu::RenderPipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Gerstner wave buffer.
    pub wave_buffer: wgpu::Buffer,
    /// Normal map texture.
    pub normal_texture: Option<wgpu::Texture>,
    pub normal_view: Option<wgpu::TextureView>,
    /// Caustics texture.
    pub caustics_texture: Option<wgpu::Texture>,
    pub caustics_view: Option<wgpu::TextureView>,
    /// Foam texture.
    pub foam_texture: Option<wgpu::Texture>,
    pub foam_view: Option<wgpu::TextureView>,
    /// Reflection texture (from environment).
    pub reflection_texture: Option<wgpu::Texture>,
    pub reflection_view: Option<wgpu::TextureView>,
    /// Refraction texture (captured scene).
    pub refraction_texture: Option<wgpu::Texture>,
    pub refraction_view: Option<wgpu::TextureView>,
    /// Depth texture (scene).
    pub depth_view: Option<wgpu::TextureView>,
    /// Waves configuration.
    pub waves: Vec<GerstnerWave>,
    /// Water plane vertex buffer.
    pub vertex_buffer: Option<wgpu::Buffer>,
    /// Index buffer.
    pub index_buffer: Option<wgpu::Buffer>,
    /// Index count.
    pub index_count: u32,
}

impl WaterPass {
    /// Create water pass.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, settings: WaterSettings) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water_uniform"),
            size: std::mem::size_of::<WaterUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let waves = vec![
            GerstnerWave::new([1.0, 0.0], 0.25, 60.0, 1.0),
            GerstnerWave::new([1.0, 0.6], 0.15, 31.0, 1.1),
            GerstnerWave::new([1.0, 1.3], 0.1, 18.0, 1.2),
            GerstnerWave::new([1.0, 2.0], 0.08, 8.0, 1.3),
        ];

        let wave_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water_waves"),
            size: std::mem::size_of::<GerstnerWaveData>() as u64 * 8,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water_bgl"),
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
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
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let (vertex_buffer, index_buffer, index_count) =
            Self::create_water_mesh(device, queue, 256, 256);

        Self {
            settings,
            pipeline: None,
            bind_group_layout,
            uniform_buffer,
            wave_buffer,
            normal_texture: None,
            normal_view: None,
            caustics_texture: None,
            caustics_view: None,
            foam_texture: None,
            foam_view: None,
            reflection_texture: None,
            reflection_view: None,
            refraction_texture: None,
            refraction_view: None,
            depth_view: None,
            waves,
            vertex_buffer: Some(vertex_buffer),
            index_buffer: Some(index_buffer),
            index_count,
        }
    }

    /// Create water mesh (grid).
    fn create_water_mesh(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> (wgpu::Buffer, wgpu::Buffer, u32) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let half_w = width as f32 * 0.5;
        let half_h = height as f32 * 0.5;

        for y in 0..=height {
            for x in 0..=width {
                let px = x as f32 - half_w;
                let pz = y as f32 - half_h;

                vertices.push(WaterVertex {
                    position: [px, 0.0, pz],
                    uv: [x as f32 / width as f32, y as f32 / height as f32],
                });
            }
        }

        for y in 0..height {
            for x in 0..width {
                let i0 = y * (width + 1) + x;
                let i1 = i0 + 1;
                let i2 = i0 + width + 1;
                let i3 = i2 + 1;

                indices.push(i0);
                indices.push(i2);
                indices.push(i1);

                indices.push(i1);
                indices.push(i2);
                indices.push(i3);
            }
        }

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water_vertices"),
            size: (vertices.len() * std::mem::size_of::<WaterVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water_indices"),
            size: (indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&indices));

        (vertex_buffer, index_buffer, indices.len() as u32)
    }

    /// Prepare uniform data.
    pub fn prepare_uniform(
        &self,
        camera_pos: [f32; 3],
        view_proj: [[f32; 4]; 4],
        inv_view_proj: [[f32; 4]; 4],
        time: f32,
        sun_dir: [f32; 3],
    ) -> WaterUniform {
        WaterUniform {
            view_proj,
            inv_view_proj,
            camera_pos,
            sun_dir,
            deep_color: self.settings.deep_color,
            shallow_color: self.settings.shallow_color,
            transparency: self.settings.transparency,
            fresnel_bias: self.settings.fresnel_bias,
            fresnel_power: self.settings.fresnel_power,
            refraction_distortion: self.settings.refraction_distortion,
            reflection_intensity: self.settings.reflection_intensity,
            wave_amplitude: self.settings.wave_amplitude,
            wave_frequency: self.settings.wave_frequency,
            wave_speed: self.settings.wave_speed,
            wave_count: self.settings.wave_count,
            caustics_intensity: self.settings.caustics_intensity,
            caustics_scale: self.settings.caustics_scale,
            caustics_speed: self.settings.caustics_speed,
            foam_threshold: self.settings.foam_threshold,
            foam_intensity: self.settings.foam_intensity,
            normal_intensity: self.settings.normal_intensity,
            time,
            _pad: [0; 3],
        }
    }

    /// Render water surface.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        uniform: &WaterUniform,
        target_view: &wgpu::TextureView,
    ) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        let Some(vertex_buffer) = &self.vertex_buffer else {
            return;
        };
        let Some(index_buffer) = &self.index_buffer else {
            return;
        };

        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));

        let wave_data: Vec<GerstnerWaveData> = self.waves.iter().map(|w| (*w).into()).collect();
        queue.write_buffer(&self.wave_buffer, 0, bytemuck::cast_slice(&wave_data));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water_pass"),
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

        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

/// Water vertex.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct WaterVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// Water uniform buffer.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct WaterUniform {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub sun_dir: [f32; 3],
    pub deep_color: [f32; 4],
    pub shallow_color: [f32; 4],
    pub transparency: f32,
    pub fresnel_bias: f32,
    pub fresnel_power: f32,
    pub refraction_distortion: f32,
    pub reflection_intensity: f32,
    pub wave_amplitude: f32,
    pub wave_frequency: f32,
    pub wave_speed: f32,
    pub wave_count: u32,
    pub caustics_intensity: f32,
    pub caustics_scale: f32,
    pub caustics_speed: f32,
    pub foam_threshold: f32,
    pub foam_intensity: f32,
    pub normal_intensity: f32,
    pub time: f32,
    pub _pad: [u32; 3],
}

/// Gerstner wave data for GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GerstnerWaveData {
    pub direction: [f32; 2],
    pub steepness: f32,
    pub wavelength: f32,
    pub speed: f32,
    pub _pad: [f32; 2],
}

impl From<GerstnerWave> for GerstnerWaveData {
    fn from(w: GerstnerWave) -> Self {
        Self {
            direction: w.direction,
            steepness: w.steepness,
            wavelength: w.wavelength,
            speed: w.speed,
            _pad: [0.0; 2],
        }
    }
}

/// Caustics generation pass.
pub struct CausticsPass {
    /// Compute pipeline for caustics.
    pub pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Output texture.
    pub output_texture: Option<wgpu::Texture>,
    pub output_view: Option<wgpu::TextureView>,
    /// Resolution.
    pub width: u32,
    pub height: u32,
}

impl CausticsPass {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("caustics_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("caustics_output"),
            size: wgpu::Extent3d {
                width: width / 2,
                height: height / 2,
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

        Self {
            pipeline: None,
            bind_group_layout,
            output_texture: Some(output_texture),
            output_view: Some(output_view),
            width: width / 2,
            height: height / 2,
        }
    }

    /// Generate caustics pattern.
    pub fn generate_caustics_pattern(uv: [f32; 2], time: f32, scale: f32) -> f32 {
        let x = uv[0] * scale + time * 0.1;
        let y = uv[1] * scale + time * 0.15;

        let v1 = (x.sin() * y.cos()).abs();
        let v2 = ((x * 1.5).cos() * (y * 1.5).sin()).abs();
        let v3 = ((x * 2.3).sin() * (y * 2.3).cos()).abs();

        (v1 + v2 + v3) / 3.0
    }
}

/// Fresnel approximation.
pub fn fresnel_schlick(cos_theta: f32, f0: f32) -> f32 {
    f0 + (1.0 - f0) * (1.0 - cos_theta).powi(5)
}

/// Depth-based water color blending.
pub fn water_depth_color(
    depth: f32,
    deep_color: [f32; 4],
    shallow_color: [f32; 4],
    max_depth: f32,
) -> [f32; 4] {
    let t = (depth / max_depth).min(1.0);
    [
        shallow_color[0] + (deep_color[0] - shallow_color[0]) * t,
        shallow_color[1] + (deep_color[1] - shallow_color[1]) * t,
        shallow_color[2] + (deep_color[2] - shallow_color[2]) * t,
        shallow_color[3] + (deep_color[3] - shallow_color[3]) * t,
    ]
}
