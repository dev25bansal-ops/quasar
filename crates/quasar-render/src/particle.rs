//! Particle system — GPU-accelerated particle effects.
//!
//! Provides particle emitters with configurable spawn rate, lifetime,
//! velocity, and gravity. Uses compute shaders for GPU simulation
//! or CPU instanced rendering as a fallback.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

pub const MAX_PARTICLES: u32 = 100_000;
pub const PARTICLE_GROUP_SIZE: u32 = 256;

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct ParticleData {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub color: [f32; 4],
    pub scale: f32,
    pub lifetime: f32,
    pub age: f32,
    pub _pad: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleEmitterConfig {
    pub max_particles: u32,
    pub spawn_rate: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub velocity_min: glam::Vec3,
    pub velocity_max: glam::Vec3,
    pub gravity_scale: f32,
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    pub scale_start: f32,
    pub scale_end: f32,
}

impl Default for ParticleEmitterConfig {
    fn default() -> Self {
        Self {
            max_particles: 1000,
            spawn_rate: 10.0,
            lifetime_min: 1.0,
            lifetime_max: 3.0,
            velocity_min: glam::Vec3::new(-1.0, 2.0, -1.0),
            velocity_max: glam::Vec3::new(1.0, 4.0, 1.0),
            gravity_scale: 1.0,
            color_start: [1.0, 1.0, 1.0, 1.0],
            color_end: [1.0, 1.0, 1.0, 0.0],
            scale_start: 0.1,
            scale_end: 0.01,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParticleEmitter {
    pub config: ParticleEmitterConfig,
    pub particles: Vec<ParticleData>,
    pub spawn_accumulator: f32,
    pub is_playing: bool,
}

impl ParticleEmitter {
    pub fn new(config: ParticleEmitterConfig) -> Self {
        Self {
            config,
            particles: Vec::with_capacity(MAX_PARTICLES as usize),
            spawn_accumulator: 0.0,
            is_playing: true,
        }
    }

    pub fn update(&mut self, dt: f32, gravity: glam::Vec3) {
        if !self.is_playing {
            return;
        }

        self.spawn_accumulator += dt * self.config.spawn_rate;
        while self.spawn_accumulator >= 1.0
            && self.particles.len() < self.config.max_particles as usize
        {
            self.spawn_particle();
            self.spawn_accumulator -= 1.0;
        }

        let gravity_scaled = gravity * self.config.gravity_scale;
        for particle in &mut self.particles {
            particle.velocity[0] += gravity_scaled.x * dt;
            particle.velocity[1] += gravity_scaled.y * dt;
            particle.velocity[2] += gravity_scaled.z * dt;

            particle.position[0] += particle.velocity[0] * dt;
            particle.position[1] += particle.velocity[1] * dt;
            particle.position[2] += particle.velocity[2] * dt;

            particle.age += dt;

            let t = (particle.age / particle.lifetime).min(1.0);
            let start = self.config.color_start;
            let end = self.config.color_end;
            particle.color[0] = start[0] * (1.0 - t) + end[0] * t;
            particle.color[1] = start[1] * (1.0 - t) + end[1] * t;
            particle.color[2] = start[2] * (1.0 - t) + end[2] * t;
            particle.color[3] = start[3] * (1.0 - t) + end[3] * t;

            particle.scale = self.config.scale_start * (1.0 - t) + self.config.scale_end * t;
        }

        self.particles.retain(|p| p.age < p.lifetime);
    }

    fn spawn_particle(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut rng_state = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        let mut rand_f32 = || -> f32 {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            ((rng_state >> 16) & 0x7fffffff) as f32 / 0x7fffffff as f32
        };

        let lifetime = self.config.lifetime_min
            + rand_f32() * (self.config.lifetime_max - self.config.lifetime_min);
        let vx = self.config.velocity_min.x
            + rand_f32() * (self.config.velocity_max.x - self.config.velocity_min.x);
        let vy = self.config.velocity_min.y
            + rand_f32() * (self.config.velocity_max.y - self.config.velocity_min.y);
        let vz = self.config.velocity_min.z
            + rand_f32() * (self.config.velocity_max.z - self.config.velocity_min.z);

        self.particles.push(ParticleData {
            position: [0.0, 0.0, 0.0],
            velocity: [vx, vy, vz],
            color: self.config.color_start,
            scale: self.config.scale_start,
            lifetime,
            age: 0.0,
            _pad: 0.0,
        });
    }

    pub fn play(&mut self) {
        self.is_playing = true;
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn stop(&mut self) {
        self.is_playing = false;
        self.particles.clear();
        self.spawn_accumulator = 0.0;
    }

    pub fn set_position(&mut self, position: glam::Vec3) {
        for particle in &mut self.particles {
            particle.position[0] += position.x;
            particle.position[1] += position.y;
            particle.position[2] += position.z;
        }
    }
}

pub struct GpuParticleSystem {
    pub particle_buffer_a: wgpu::Buffer,
    pub particle_buffer_b: wgpu::Buffer,
    pub indirect_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub compute_pipeline: wgpu::ComputePipeline,
    pub render_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub alive_count: u32,
}

impl GpuParticleSystem {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let particle_buffer_a = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Buffer A"),
            size: (MAX_PARTICLES as usize * std::mem::size_of::<ParticleData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let particle_buffer_b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Buffer B"),
            size: (MAX_PARTICLES as usize * std::mem::size_of::<ParticleData>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Indirect Buffer"),
            size: std::mem::size_of::<wgpu::util::DrawIndirectArgs>() as u64,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Uniform Buffer"),
            size: std::mem::size_of::<ParticleUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Particle Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
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
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particle Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: particle_buffer_a.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: particle_buffer_b.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/particle_compute.wgsl").into(),
            ),
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Compute Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Particle Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Render Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/particle.wgsl").into(),
            ),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Render Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let vertices = create_particle_quad();
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Vertex Buffer"),
            size: (vertices.len() * std::mem::size_of::<ParticleVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Index Buffer"),
            size: (indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[ParticleVertex::buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            particle_buffer_a,
            particle_buffer_b,
            indirect_buffer,
            uniform_buffer,
            bind_group,
            bind_group_layout,
            compute_pipeline,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            alive_count: 0,
        }
    }

    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder, count: u32) {
        let groups = count.div_ceil(PARTICLE_GROUP_SIZE);
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Particle Compute Pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.bind_group, &[]);
        compute_pass.dispatch_workgroups(groups, 1, 1);
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.render_pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed_indirect(&self.indirect_buffer, 0);
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ParticleUniforms {
    delta_time: f32,
    gravity: [f32; 3],
    alive_count: u32,
    _pad: [f32; 3],
}

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct ParticleVertex {
    pub position: [f32; 2],
}

impl ParticleVertex {
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ParticleVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}

fn create_particle_quad() -> Vec<ParticleVertex> {
    vec![
        ParticleVertex {
            position: [-0.5, -0.5],
        },
        ParticleVertex {
            position: [0.5, -0.5],
        },
        ParticleVertex {
            position: [0.5, 0.5],
        },
        ParticleVertex {
            position: [-0.5, 0.5],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_emitter_update() {
        let mut emitter = ParticleEmitter::new(ParticleEmitterConfig::default());
        emitter.update(0.1, glam::Vec3::new(0.0, -9.81, 0.0));

        assert!(emitter.particles.len() <= emitter.config.max_particles as usize);
    }

    #[test]
    fn particle_emitter_play_pause_stop() {
        let mut emitter = ParticleEmitter::new(ParticleEmitterConfig::default());

        emitter.play();
        assert!(emitter.is_playing);

        emitter.pause();
        assert!(!emitter.is_playing);

        emitter.stop();
        assert!(emitter.particles.is_empty());
    }
}
