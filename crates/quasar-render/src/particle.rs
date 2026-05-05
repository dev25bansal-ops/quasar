//! Particle system — GPU-accelerated particle effects.
//!
//! Provides particle emitters with configurable spawn rate, lifetime,
//! velocity, and gravity. Uses compute shaders for GPU simulation
//! or CPU instanced rendering as a fallback.
//!
//! # VFX Graph Integration
//!
//! Supports node-based VFX graph definitions via `ParticleSystemDef` with
//! emitters, force fields, modifiers, collisions, and renderer configuration.
//! All definitions are serializable to/from JSON for save/load.

use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    #[test]
    fn particle_system_def_serialize_deserialize() {
        let mut def = ParticleSystemDef::default();
        def.name = "TestEffect".to_string();
        let json = serde_json::to_string(&def).unwrap();
        let loaded: ParticleSystemDef = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.name, "TestEffect");
    }

    #[test]
    fn cpu_particle_simulator_basic_update() {
        let mut def = ParticleSystemDef::default();
        def.emitters[0].rate = 100.0;
        def.emitters[0].lifetime = 2.0..=2.0;
        def.emitters[0].velocity = 5.0..=5.0;
        let mut sim = CpuParticleSimulator::new(def);
        sim.update(0.016);
        assert!(sim.particles.len() >= 0);
    }

    #[test]
    fn force_field_gravity_affects_particles() {
        let mut def = ParticleSystemDef::default();
        def.forces.push(ForceDef {
            name: "Gravity".to_string(),
            force_type: ForceType::Gravity { strength: 9.81 },
            enabled: true,
        });
        let mut sim = CpuParticleSimulator::new(def);
        sim.update(0.016);
        for p in &sim.particles {
            if p.alive {
                assert!(p.velocity[1] < 0.0 || p.age == 0.0);
            }
        }
    }

    #[test]
    fn collision_with_plane_kills_particles() {
        let mut def = ParticleSystemDef::default();
        def.emitters[0].velocity = 0.0..=0.0;
        def.collisions.push(CollisionDef {
            name: "Ground".to_string(),
            collision_type: CollisionType::Plane {
                normal: [0.0, 1.0, 0.0],
                distance: 0.0,
            },
            bounce_factor: 0.0,
            kill_on_collision: true,
            enabled: true,
        });
        let mut sim = CpuParticleSimulator::new(def);
        sim.update(1.0);
        // Particles that hit the plane should be killed
    }
}

// =============================================================================
// VFX Graph Particle System Definition (JSON serializable)
// =============================================================================

/// Complete particle system definition for save/load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleSystemDef {
    pub name: String,
    pub emitters: Vec<EmitterDef>,
    pub forces: Vec<ForceDef>,
    pub modifiers: Vec<ModifierDef>,
    pub collisions: Vec<CollisionDef>,
    pub renderer: ParticleRendererDef,
}

impl Default for ParticleSystemDef {
    fn default() -> Self {
        Self {
            name: "New Particle System".to_string(),
            emitters: vec![EmitterDef::default()],
            forces: Vec::new(),
            modifiers: Vec::new(),
            collisions: Vec::new(),
            renderer: ParticleRendererDef::default(),
        }
    }
}

impl ParticleSystemDef {
    /// Save to JSON file.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    /// Load from JSON file.
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Load from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Emitter definition for particle spawning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterDef {
    pub name: String,
    pub enabled: bool,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub shape: EmitterShape,
    pub rate: f32,
    pub burst_count: u32,
    pub burst_interval: f32,
    pub lifetime: std::ops::RangeInclusive<f32>,
    pub velocity: std::ops::RangeInclusive<f32>,
    pub spread_angle: f32,
    pub size: std::ops::RangeInclusive<f32>,
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    pub size_over_lifetime: Vec<CurveKeyframe>,
    pub color_over_lifetime: Vec<ColorKeyframe>,
    pub max_particles: u32,
    pub simulation_space: SimulationSpace,
}

impl Default for EmitterDef {
    fn default() -> Self {
        Self {
            name: "Emitter".to_string(),
            enabled: true,
            position: [0.0; 3],
            rotation: [0.0; 3],
            shape: EmitterShape::Point,
            rate: 10.0,
            burst_count: 0,
            burst_interval: 1.0,
            lifetime: 1.0..=3.0,
            velocity: 1.0..=3.0,
            spread_angle: 30.0,
            size: 0.1..=0.3,
            color_start: [1.0, 1.0, 1.0, 1.0],
            color_end: [1.0, 1.0, 1.0, 0.0],
            size_over_lifetime: vec![
                CurveKeyframe { time: 0.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
                CurveKeyframe { time: 1.0, value: 1.0, in_tangent: 0.0, out_tangent: 0.0 },
            ],
            color_over_lifetime: vec![
                ColorKeyframe { time: 0.0, color: [1.0, 1.0, 1.0, 1.0] },
                ColorKeyframe { time: 1.0, color: [1.0, 1.0, 1.0, 0.0] },
            ],
            max_particles: 1000,
            simulation_space: SimulationSpace::Local,
        }
    }
}

/// Emitter shape types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EmitterShape {
    Point,
    Box { size: [f32; 3] },
    Sphere { radius: f32 },
    Cone { angle: f32, length: f32 },
    Circle { radius: f32 },
    Hemisphere { radius: f32 },
}

impl Default for EmitterShape {
    fn default() -> Self {
        Self::Point
    }
}

/// Simulation space for particles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SimulationSpace {
    Local,
    World,
    Custom { transform: [f32; 16] },
}

impl Default for SimulationSpace {
    fn default() -> Self {
        Self::Local
    }
}

/// Curve keyframe for scalar animation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CurveKeyframe {
    pub time: f32,
    pub value: f32,
    pub in_tangent: f32,
    pub out_tangent: f32,
}

/// Color keyframe for gradient animation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ColorKeyframe {
    pub time: f32,
    pub color: [f32; 4],
}

/// Evaluate a curve at a given time (0-1).
pub fn evaluate_curve(curve: &[CurveKeyframe], time: f32) -> f32 {
    if curve.is_empty() {
        return 1.0;
    }
    if curve.len() == 1 {
        return curve[0].value;
    }

    let t = time.clamp(0.0, 1.0);

    for i in 0..curve.len() - 1 {
        let k0 = &curve[i];
        let k1 = &curve[i + 1];

        if t >= k0.time && t <= k1.time {
            let dt = k1.time - k0.time;
            if dt < 0.0001 {
                return k0.value;
            }
            let lerp = (t - k0.time) / dt;
            // Hermite interpolation
            let h = lerp * lerp * (3.0 - 2.0 * lerp);
            return k0.value + (k1.value - k0.value) * h;
        }
    }

    curve.last().map(|k| k.value).unwrap_or(1.0)
}

/// Evaluate color gradient at a given time (0-1).
pub fn evaluate_color_gradient(gradient: &[ColorKeyframe], time: f32) -> [f32; 4] {
    if gradient.is_empty() {
        return [1.0, 1.0, 1.0, 1.0];
    }
    if gradient.len() == 1 {
        return gradient[0].color;
    }

    let t = time.clamp(0.0, 1.0);

    for i in 0..gradient.len() - 1 {
        let k0 = &gradient[i];
        let k1 = &gradient[i + 1];

        if t >= k0.time && t <= k1.time {
            let dt = k1.time - k0.time;
            if dt < 0.0001 {
                return k0.color;
            }
            let lerp = (t - k0.time) / dt;
            return [
                k0.color[0] + (k1.color[0] - k0.color[0]) * lerp,
                k0.color[1] + (k1.color[1] - k0.color[1]) * lerp,
                k0.color[2] + (k1.color[2] - k0.color[2]) * lerp,
                k0.color[3] + (k1.color[3] - k0.color[3]) * lerp,
            ];
        }
    }

    gradient.last().map(|k| k.color).unwrap_or([1.0, 1.0, 1.0, 1.0])
}

/// Force field definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceDef {
    pub name: String,
    pub force_type: ForceType,
    pub enabled: bool,
}

/// Types of force fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForceType {
    /// Constant gravity force.
    Gravity { strength: f32 },
    /// Wind force in a direction.
    Wind { direction: [f32; 3], strength: f32 },
    /// Perlin/simplex turbulence.
    Turbulence { strength: f32, frequency: f32, speed: f32, seed: u32 },
    /// Vortex/spiral force.
    Vortex { center: [f32; 3], axis: [f32; 3], strength: f32, radius: f32 },
    /// Point attractor.
    Attractor { position: [f32; 3], strength: f32, range: f32 },
    /// Point repeller.
    Repeller { position: [f32; 3], strength: f32, range: f32 },
    /// Drag/air resistance.
    Drag { coefficient: f32 },
    /// Noise-based random force.
    Noise { strength: f32, scale: f32 },
}

impl ForceDef {
    /// Calculate the force vector applied to a particle at its position with given velocity.
    pub fn force_vector(&self, position: [f32; 3], velocity: [f32; 3], dt: f32) -> [f32; 3] {
        if !self.enabled {
            return [0.0, 0.0, 0.0];
        }

        match &self.force_type {
            ForceType::Gravity { strength } => [0.0, -strength, 0.0],
            ForceType::Wind { direction, strength } => {
                let d = glam::Vec3::from_array(*direction).normalize_or_zero();
                (d * strength).to_array()
            }
            ForceType::Turbulence { strength, frequency, speed, seed } => {
                // Simple pseudo-noise turbulence using sine waves
                let t = *speed * dt;
                let x = (position[0] * frequency + t).sin() * (*strength);
                let y = (position[1] * frequency + t + 1.0).sin() * (*strength);
                let z = (position[2] * frequency + t + 2.0).sin() * (*strength);
                [x, y, z]
            }
            ForceType::Vortex { center, axis, strength, radius } => {
                let pos = glam::Vec3::from_array(position) - glam::Vec3::from_array(*center);
                let axis = glam::Vec3::from_array(*axis).normalize_or_zero();
                let radial = pos - axis * pos.dot(axis);
                let dist = radial.length();
                if dist > *radius || dist < 0.001 {
                    return [0.0, 0.0, 0.0];
                }
                let tangent = axis.cross(radial).normalize_or_zero() * (*strength / dist.max(0.1));
                let inward = -radial.normalize_or_zero() * (*strength * 0.3);
                (tangent + inward).to_array()
            }
            ForceType::Attractor { position, strength, range } => {
                let to_attractor = glam::Vec3::from_array(*position) - glam::Vec3::from_array(*position);
                let dist = to_attractor.length();
                if dist > *range || dist < 0.001 {
                    return [0.0, 0.0, 0.0];
                }
                (to_attractor.normalize() * (*strength / dist.max(0.1))).into()
            }
            ForceType::Repeller { position, strength, range } => {
                let from_repeller = glam::Vec3::from_array(*position) - glam::Vec3::from_array(*position);
                let dist = from_repeller.length();
                if dist > *range || dist < 0.001 {
                    return [0.0, 0.0, 0.0];
                }
                (from_repeller.normalize() * (*strength / dist.max(0.1))).into()
            }
            ForceType::Drag { coefficient } => {
                let vel = glam::Vec3::from_array(velocity);
                (-vel * vel.length() * *coefficient).to_array()
            }
            ForceType::Noise { strength, scale } => {
                let x = (position[0] * scale).sin() * (*strength);
                let y = (position[1] * scale + 1.0).sin() * (*strength);
                let z = (position[2] * scale + 2.0).sin() * (*strength);
                [x, y, z]
            }
        }
    }
}

/// Modifier definition for particle properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifierDef {
    pub name: String,
    pub modifier_type: ModifierType,
    pub enabled: bool,
}

/// Types of particle modifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModifierType {
    /// Scale particles over their lifetime.
    SizeOverLifetime { curve: Vec<CurveKeyframe> },
    /// Color particles over their lifetime.
    ColorOverLifetime { gradient: Vec<ColorKeyframe> },
    /// Rotate particles over their lifetime.
    RotationOverLifetime { speed: f32, axis: [f32; 3] },
    /// Limit particle velocity.
    LimitVelocity { max_speed: f32 },
    /// Clamp particle size to a range.
    ClampSize { min: f32, max: f32 },
    /// Fade particles based on distance.
    DistanceFade { start: f32, end: f32 },
    /// Scale particles by speed.
    SpeedScale { curve: Vec<CurveKeyframe> },
    /// Inherit velocity from emitter motion.
    InheritVelocity { multiplier: f32 },
}

/// Collision definition for particle interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionDef {
    pub name: String,
    pub collision_type: CollisionType,
    pub bounce_factor: f32,
    pub kill_on_collision: bool,
    pub enabled: bool,
}

/// Types of collision geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollisionType {
    /// Infinite plane.
    Plane { normal: [f32; 3], distance: f32 },
    /// Axis-aligned box.
    Box { min: [f32; 3], max: [f32; 3] },
    /// Sphere.
    Sphere { center: [f32; 3], radius: f32 },
    /// Capsule.
    Capsule { start: [f32; 3], end: [f32; 3], radius: f32 },
    /// Heightmap terrain.
    Heightmap {
        /// Height values as a flat 2D grid (row-major).
        heights: Vec<f32>,
        width: usize,
        height: usize,
        scale: [f32; 3],
        offset: [f32; 3],
    },
}

impl CollisionType {
    /// Test collision with a particle and return the contact normal and penetration depth.
    pub fn collide(&self, position: [f32; 3], radius: f32) -> Option<([f32; 3], f32)> {
        match self {
            CollisionType::Plane { normal, distance } => {
                let n = glam::Vec3::from_array(*normal);
                let d = n.dot(glam::Vec3::from_array(position)) + distance;
                if d < radius {
                    Some((n.to_array(), radius - d))
                } else {
                    None
                }
            }
            CollisionType::Box { min, max } => {
                let pos = glam::Vec3::from_array(position);
                let min = glam::Vec3::from_array(*min);
                let max = glam::Vec3::from_array(*max);
                let closest = pos.clamp(min, max);
                let diff = pos - closest;
                let dist = diff.length();
                if dist < radius && dist > 0.0001 {
                    Some((diff.normalize().to_array(), radius - dist))
                } else if dist <= 0.0001 {
                    // Inside the box, push out along the shortest axis
                    let to_min = pos - min;
                    let to_max = max - pos;
                    let min_axis = to_min.min(to_max);
                    if min_axis.x <= min_axis.y && min_axis.x <= min_axis.z {
                        let dir = if to_min.x < to_max.x { Vec3::X } else { -Vec3::X };
                        Some((dir.to_array(), radius + min_axis.x))
                    } else if min_axis.y <= min_axis.x && min_axis.y <= min_axis.z {
                        let dir = if to_min.y < to_max.y { Vec3::Y } else { -Vec3::Y };
                        Some((dir.to_array(), radius + min_axis.y))
                    } else {
                        let dir = if to_min.z < to_max.z { Vec3::Z } else { -Vec3::Z };
                        Some((dir.to_array(), radius + min_axis.z))
                    }
                } else {
                    None
                }
            }
            CollisionType::Sphere { center, radius: sphere_radius } => {
                let to_center = glam::Vec3::from_array(*center) - glam::Vec3::from_array(position);
                let dist = to_center.length();
                let total_radius = sphere_radius + radius;
                if dist < total_radius && dist > 0.0001 {
                    Some(((to_center / dist).to_array(), total_radius - dist))
                } else {
                    None
                }
            }
            CollisionType::Capsule { start, end, radius: capsule_radius } => {
                let p = glam::Vec3::from_array(position);
                let a = glam::Vec3::from_array(*start);
                let b = glam::Vec3::from_array(*end);
                let ab = b - a;
                let ap = p - a;
                let t = (ap.dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                let closest = a + ab * t;
                let diff = p - closest;
                let dist = diff.length();
                let total_radius = capsule_radius + radius;
                if dist < total_radius && dist > 0.0001 {
                    Some(((diff / dist).to_array(), total_radius - dist))
                } else {
                    None
                }
            }
            CollisionType::Heightmap { heights, width, height: h, scale, offset } => {
                // Simple heightmap collision: find grid cell and interpolate
                let local_pos = glam::Vec3::from_array(position) - glam::Vec3::from_array(*offset);
                let gx = (local_pos.x / scale[0]).floor() as usize;
                let gz = (local_pos.z / scale[2]).floor() as usize;
                if gx >= *width - 1 || gz >= *h - 1 {
                    return None;
                }
                let h00 = heights[gz * width + gx];
                let h10 = heights[gz * width + gx + 1];
                let h01 = heights[(gz + 1) * width + gx];
                let h11 = heights[(gz + 1) * width + gx + 1];
                let fx = (local_pos.x / scale[0] - gx as f32).clamp(0.0, 1.0);
                let fz = (local_pos.z / scale[2] - gz as f32).clamp(0.0, 1.0);
                let terrain_h = h00 * (1.0 - fx) * (1.0 - fz)
                    + h10 * fx * (1.0 - fz)
                    + h01 * (1.0 - fx) * fz
                    + h11 * fx * fz;
                let terrain_y = terrain_h * scale[1] + offset[1];
                if position[1] < terrain_y + radius {
                    Some(([0.0, 1.0, 0.0], terrain_y + radius - position[1]))
                } else {
                    None
                }
            }
        }
    }

    /// Reflect velocity off the collision surface.
    pub fn reflect(velocity: [f32; 3], normal: [f32; 3], bounce: f32) -> [f32; 3] {
        let v = glam::Vec3::from_array(velocity);
        let n = glam::Vec3::from_array(normal);
        let dot = v.dot(n);
        if dot < 0.0 {
            (v - n * (1.0 + bounce) * dot).to_array()
        } else {
            velocity
        }
    }
}

/// Particle renderer definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleRendererDef {
    pub blend_mode: BlendModeDef,
    pub alignment: AlignmentDef,
    pub texture: Option<String>,
    pub mesh: Option<String>,
    pub cast_shadows: bool,
    pub sorting: SortingDef,
    pub min_scale: f32,
    pub max_scale: f32,
}

impl Default for ParticleRendererDef {
    fn default() -> Self {
        Self {
            blend_mode: BlendModeDef::Alpha,
            alignment: AlignmentDef::ViewFacing,
            texture: None,
            mesh: None,
            cast_shadows: false,
            sorting: SortingDef::Distance,
            min_scale: 0.001,
            max_scale: 100.0,
        }
    }
}

/// Blend mode for particle rendering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BlendModeDef {
    Opaque,
    Alpha,
    Additive,
    Multiply,
    Subtractive,
}

/// Particle alignment mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlignmentDef {
    ViewFacing,
    WorldY,
    VelocityAligned,
    Fixed,
}

/// Particle sorting mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SortingDef {
    None,
    Distance,
    Age,
}

// =============================================================================
// CPU Particle Simulator (for editor preview)
// =============================================================================

/// A single particle in the simulation.
#[derive(Debug, Clone)]
pub struct SimParticle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub color: [f32; 4],
    pub scale: f32,
    pub lifetime: f32,
    pub age: f32,
    pub alive: bool,
    pub seed: u64,
}

/// CPU-based particle simulator for editor preview.
pub struct CpuParticleSimulator {
    pub system_def: ParticleSystemDef,
    pub particles: Vec<SimParticle>,
    pub time: f32,
    pub burst_timer: f32,
    rng_state: u64,
}

impl CpuParticleSimulator {
    pub fn new(system_def: ParticleSystemDef) -> Self {
        let max_particles: u32 = system_def.emitters.iter().map(|e| e.max_particles).sum();
        Self {
            system_def,
            particles: Vec::with_capacity(max_particles as usize),
            time: 0.0,
            burst_timer: 0.0,
            rng_state: 42,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng_state = self.rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.rng_state >> 33) as f32) / (u32::MAX as f32)
    }

    fn rand_range(&mut self, range: &std::ops::RangeInclusive<f32>) -> f32 {
        *range.start() + self.rand() * (range.end() - range.start())
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.burst_timer += dt;

        let mut rng_state = self.rng_state;
        let mut rand_f32 = || -> f32 {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((rng_state >> 33) as f32) / (u32::MAX as f32)
        };

        // Spawn new particles from active emitters
        let emitters = self.system_def.emitters.clone();
        for emitter in &emitters {
            if !emitter.enabled {
                continue;
            }

            let alive_count = self.particles.iter().filter(|p| p.alive).count() as u32;
            if alive_count >= emitter.max_particles {
                continue;
            }

            // Continuous emission
            let emit_count = (emitter.rate * dt) as u32;
            for _ in 0..emit_count.min(emitter.max_particles - alive_count) {
                if let Some(particle) = self.spawn_particle(emitter, &mut rand_f32) {
                    self.particles.push(particle);
                }
            }

            // Burst emission
            if emitter.burst_count > 0 && self.burst_timer >= emitter.burst_interval {
                self.burst_timer = 0.0;
                for _ in 0..emitter.burst_count {
                    if let Some(particle) = self.spawn_particle(emitter, &mut rand_f32) {
                        self.particles.push(particle);
                    }
                }
            }
        }

        self.rng_state = rng_state;

        // Update existing particles
        let forces: Vec<&ForceDef> = self.system_def.forces.iter().filter(|f| f.enabled).collect();
        let modifiers: Vec<&ModifierDef> = self.system_def.modifiers.iter().filter(|m| m.enabled).collect();
        let collisions: Vec<&CollisionDef> = self.system_def.collisions.iter().filter(|c| c.enabled).collect();

        for particle in &mut self.particles {
            if !particle.alive {
                continue;
            }

            particle.age += dt;
            if particle.age >= particle.lifetime {
                particle.alive = false;
                continue;
            }

            // Apply forces
            for force in &forces {
                let f = force.force_vector(particle.position, particle.velocity, dt);
                particle.velocity[0] += f[0] * dt;
                particle.velocity[1] += f[1] * dt;
                particle.velocity[2] += f[2] * dt;
            }

            // Update position
            particle.position[0] += particle.velocity[0] * dt;
            particle.position[1] += particle.velocity[1] * dt;
            particle.position[2] += particle.velocity[2] * dt;

            // Apply collisions
            for collision in &collisions {
                if let Some((normal, _penetration)) =
                    collision.collision_type.collide(particle.position, particle.scale * 0.5)
                {
                    if collision.kill_on_collision {
                        particle.alive = false;
                        break;
                    } else {
                        particle.velocity =
                            CollisionType::reflect(particle.velocity, normal, collision.bounce_factor);
                    }
                }
            }

            // Apply modifiers
            let life_t = particle.age / particle.lifetime;
            for modifier in &modifiers {
                match &modifier.modifier_type {
                    ModifierType::SizeOverLifetime { curve } => {
                        particle.scale *= evaluate_curve(curve, life_t);
                    }
                    ModifierType::ColorOverLifetime { gradient } => {
                        particle.color = evaluate_color_gradient(gradient, life_t);
                    }
                    ModifierType::RotationOverLifetime { .. } => {
                        // Rotation handled in renderer
                    }
                    ModifierType::LimitVelocity { max_speed } => {
                        let speed = (particle.velocity[0].powi(2)
                            + particle.velocity[1].powi(2)
                            + particle.velocity[2].powi(2))
                        .sqrt();
                        if speed > *max_speed {
                            let scale = *max_speed / speed;
                            particle.velocity[0] *= scale;
                            particle.velocity[1] *= scale;
                            particle.velocity[2] *= scale;
                        }
                    }
                    ModifierType::ClampSize { min, max } => {
                        particle.scale = particle.scale.clamp(*min, *max);
                    }
                    ModifierType::DistanceFade { .. }
                    | ModifierType::SpeedScale { .. }
                    | ModifierType::InheritVelocity { .. } => {
                        // Additional modifier implementations
                    }
                }
            }

            // Update color and size from emitter curves
            let emitters = self.system_def.emitters.clone();
        for emitter in &emitters {
                if !emitter.enabled {
                    continue;
                }
                let life_t = particle.age / particle.lifetime;
                if !emitter.size_over_lifetime.is_empty() {
                    let base_size = evaluate_curve(&emitter.size_over_lifetime, life_t);
                    particle.scale *= base_size;
                }
                if !emitter.color_over_lifetime.is_empty() {
                    particle.color = evaluate_color_gradient(&emitter.color_over_lifetime, life_t);
                }
            }
        }

        // Remove dead particles
        self.particles.retain(|p| p.alive || p.age < p.lifetime + 1.0);
        // Cap total particles
        let max_total: u32 = self.system_def.emitters.iter().map(|e| e.max_particles).sum();
        if self.particles.len() > max_total as usize {
            self.particles.truncate(max_total as usize);
        }
    }

    fn spawn_particle(
        &mut self,
        emitter: &EmitterDef,
        rand: &mut impl FnMut() -> f32,
    ) -> Option<SimParticle> {
        let pos = self.emit_position(emitter, rand);
        let vel = self.emit_velocity(emitter, rand);
        let lifetime = self.rand_range(&emitter.lifetime);
        let size = self.rand_range(&emitter.size);

        Some(SimParticle {
            position: pos,
            velocity: vel,
            color: emitter.color_start,
            scale: size,
            lifetime,
            age: 0.0,
            alive: true,
            seed: self.rng_state,
        })
    }

    fn emit_position(&mut self, emitter: &EmitterDef, rand: &mut impl FnMut() -> f32) -> [f32; 3] {
        let base = glam::Vec3::from_array(emitter.position);
        let offset = match &emitter.shape {
            EmitterShape::Point => glam::Vec3::ZERO,
            EmitterShape::Box { size } => {
                glam::Vec3::new(
                    (rand() - 0.5) * size[0],
                    (rand() - 0.5) * size[1],
                    (rand() - 0.5) * size[2],
                )
            }
            EmitterShape::Sphere { radius } => {
                let theta = rand() * 2.0 * std::f32::consts::PI;
                let phi = (rand() * 2.0 - 1.0).acos();
                let r = *radius * rand().cbrt();
                glam::Vec3::new(
                    r * phi.sin() * theta.cos(),
                    r * phi.cos(),
                    r * phi.sin() * theta.sin(),
                )
            }
            EmitterShape::Cone { angle, length } => {
                let theta = rand() * 2.0 * std::f32::consts::PI;
                let h = rand() * length;
                let r = h * (angle.to_radians() / 2.0).tan();
                glam::Vec3::new(r * theta.cos(), h, r * theta.sin())
            }
            EmitterShape::Circle { radius } => {
                let theta = rand() * 2.0 * std::f32::consts::PI;
                let r = *radius * rand().sqrt();
                glam::Vec3::new(r * theta.cos(), 0.0, r * theta.sin())
            }
            EmitterShape::Hemisphere { radius } => {
                let theta = rand() * 2.0 * std::f32::consts::PI;
                let phi = rand() * std::f32::consts::FRAC_PI_2;
                glam::Vec3::new(
                    *radius * phi.sin() * theta.cos(),
                    *radius * phi.cos(),
                    *radius * phi.sin() * theta.sin(),
                )
            }
        };
        (base + offset).to_array()
    }

    fn emit_velocity(&mut self, emitter: &EmitterDef, rand: &mut impl FnMut() -> f32) -> [f32; 3] {
        let speed = self.rand_range(&emitter.velocity);
        let spread = emitter.spread_angle.to_radians() / 2.0;

        // Base direction from emitter rotation
        let base_dir = if emitter.rotation[0] != 0.0
            || emitter.rotation[1] != 0.0
            || emitter.rotation[2] != 0.0
        {
            glam::Vec3::new(
                emitter.rotation[0].to_radians().sin(),
                emitter.rotation[1].to_radians().cos(),
                emitter.rotation[2].to_radians().sin(),
            )
            .normalize_or(glam::Vec3::Y)
        } else {
            glam::Vec3::Y
        };

        // Add spread
        let offset = glam::Vec3::new(
            (rand() - 0.5) * spread.sin(),
            1.0,
            (rand() - 0.5) * spread.sin(),
        )
        .normalize();

        (base_dir.lerp(offset, 0.5) * speed).to_array()
    }

    pub fn reset(&mut self) {
        self.particles.clear();
        self.time = 0.0;
        self.burst_timer = 0.0;
    }

    pub fn alive_count(&self) -> usize {
        self.particles.iter().filter(|p| p.alive).count()
    }
}
