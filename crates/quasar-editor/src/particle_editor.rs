//! Particle Effect Editor — visual editing of particle systems.
//!
//! Provides:
//! - Real-time particle effect editing
//! - Emitter configuration UI
//! - Curve editors for animated properties
//! - Effect presets and templates
//! - GPU compute shader simulation (experimental)
//!
//! # Integration
//!
//! The particle editor integrates with the Quasar editor to provide
//! a node-based visual editing experience for particle effects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulationMode {
    Cpu,
    GpuCompute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuParticleConfig {
    pub max_particles: u32,
    pub workgroup_size: u32,
    pub use_double_buffer: bool,
    pub sort_on_gpu: bool,
    pub collision_enabled: bool,
}

impl Default for GpuParticleConfig {
    fn default() -> Self {
        Self {
            max_particles: 100000,
            workgroup_size: 256,
            use_double_buffer: true,
            sort_on_gpu: true,
            collision_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GpuParticle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub color: [f32; 4],
    pub scale: f32,
    pub lifetime: f32,
    pub age: f32,
    pub _padding: f32,
}

impl Default for GpuParticle {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            velocity: [0.0; 3],
            color: [1.0, 1.0, 1.0, 1.0],
            scale: 1.0,
            lifetime: 1.0,
            age: 0.0,
            _padding: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GpuEmitterData {
    pub position: [f32; 3],
    pub direction: [f32; 3],
    pub emission_rate: f32,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub speed_min: f32,
    pub speed_max: f32,
    pub scale_min: f32,
    pub scale_max: f32,
    pub gravity: f32,
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    pub time: f32,
    pub delta_time: f32,
    pub particle_count: u32,
    pub max_particles: u32,
}

pub const PARTICLE_SHADER_SRC: &str = r"
struct Particle {
    position: vec3<f32>,
    velocity: vec3<f32>,
    color: vec4<f32>,
    scale: f32,
    lifetime: f32,
    age: f32,
    _padding: f32,
}

struct EmitterData {
    position: vec3<f32>,
    direction: vec3<f32>,
    emission_rate: f32,
    lifetime_min: f32,
    lifetime_max: f32,
    speed_min: f32,
    speed_max: f32,
    scale_min: f32,
    scale_max: f32,
    gravity: f32,
    color_start: vec4<f32>,
    color_end: vec4<f32>,
    time: f32,
    delta_time: f32,
    particle_count: u32,
    max_particles: u32,
}

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<uniform> emitter: EmitterData;

var<private> rng_state: u32;

fn rand() -> f32 {
    rng_state = rng_state * 1103515245u + 12345u;
    return f32(rng_state) / f32(0xFFFFFFFFu);
}

fn rand_range(min: f32, max: f32) -> f32 {
    return min + rand() * (max - min);
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= emitter.max_particles) {
        return;
    }

    var p = particles[idx];

    if (p.age >= p.lifetime) {
        if (f32(idx) < f32(emitter.particle_count)) {
            p.position = emitter.position;
            
            let spread = 0.5;
            p.velocity = vec3<f32>(
                (rand() - 0.5) * spread,
                1.0,
                (rand() - 0.5) * spread
            ) * rand_range(emitter.speed_min, emitter.speed_max);
            
            p.lifetime = rand_range(emitter.lifetime_min, emitter.lifetime_max);
            p.age = 0.0;
            p.scale = rand_range(emitter.scale_min, emitter.scale_max);
            p.color = emitter.color_start;
        }
    } else {
        p.age += emitter.delta_time;
        p.velocity.y -= emitter.gravity * emitter.delta_time;
        p.position += p.velocity * emitter.delta_time;
        
        let t = p.age / p.lifetime;
        p.color = mix(emitter.color_start, emitter.color_end, t);
        p.scale *= 1.0 - t * 0.3;
    }

    particles[idx] = p;
}
";

pub struct GpuParticleSimulator {
    config: GpuParticleConfig,
    particles: Vec<GpuParticle>,
    alive_count: u32,
    emitter_data: GpuEmitterData,
    initialized: bool,
}

impl GpuParticleSimulator {
    pub fn new(config: GpuParticleConfig) -> Self {
        let particle_count = config.max_particles as usize;
        Self {
            config,
            particles: vec![GpuParticle::default(); particle_count],
            alive_count: 0,
            emitter_data: GpuEmitterData {
                position: [0.0; 3],
                direction: [0.0, 1.0, 0.0],
                emission_rate: 10.0,
                lifetime_min: 1.0,
                lifetime_max: 2.0,
                speed_min: 1.0,
                speed_max: 2.0,
                scale_min: 0.1,
                scale_max: 0.2,
                gravity: 9.81,
                color_start: [1.0, 1.0, 1.0, 1.0],
                color_end: [1.0, 1.0, 1.0, 0.0],
                time: 0.0,
                delta_time: 0.016,
                particle_count: 0,
                max_particles: particle_count as u32,
            },
            initialized: false,
        }
    }

    pub fn config(&self) -> &GpuParticleConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut GpuParticleConfig {
        &mut self.config
    }

    pub fn set_emitter_position(&mut self, pos: [f32; 3]) {
        self.emitter_data.position = pos;
    }

    pub fn set_emission_rate(&mut self, rate: f32) {
        self.emitter_data.emission_rate = rate;
    }

    pub fn set_lifetime_range(&mut self, min: f32, max: f32) {
        self.emitter_data.lifetime_min = min;
        self.emitter_data.lifetime_max = max;
    }

    pub fn set_speed_range(&mut self, min: f32, max: f32) {
        self.emitter_data.speed_min = min;
        self.emitter_data.speed_max = max;
    }

    pub fn set_scale_range(&mut self, min: f32, max: f32) {
        self.emitter_data.scale_min = min;
        self.emitter_data.scale_max = max;
    }

    pub fn set_gravity(&mut self, gravity: f32) {
        self.emitter_data.gravity = gravity;
    }

    pub fn set_colors(&mut self, start: [f32; 4], end: [f32; 4]) {
        self.emitter_data.color_start = start;
        self.emitter_data.color_end = end;
    }

    pub fn particle_count(&self) -> u32 {
        self.alive_count
    }

    pub fn max_particles(&self) -> u32 {
        self.config.max_particles
    }

    pub fn particles(&self) -> &[GpuParticle] {
        &self.particles
    }

    pub fn simulate_cpu(&mut self, dt: f32) {
        self.emitter_data.delta_time = dt;
        self.emitter_data.time += dt;

        let rate = self.emitter_data.emission_rate;
        let emit_count = ((rate * dt).floor() as u32)
            .min(self.config.max_particles.saturating_sub(self.alive_count));

        for _i in 0..emit_count {
            if self.alive_count >= self.config.max_particles {
                break;
            }
            let idx = self.find_dead_particle();
            if let Some(idx) = idx {
                self.emit_particle(idx);
                self.alive_count += 1;
            }
        }

        for particle in &mut self.particles {
            if particle.age < particle.lifetime {
                particle.age += dt;
                particle.velocity[1] -= self.emitter_data.gravity * dt;
                particle.position[0] += particle.velocity[0] * dt;
                particle.position[1] += particle.velocity[1] * dt;
                particle.position[2] += particle.velocity[2] * dt;

                let t = (particle.age / particle.lifetime).min(1.0);
                for i in 0..4 {
                    particle.color[i] = self.emitter_data.color_start[i]
                        + (self.emitter_data.color_end[i] - self.emitter_data.color_start[i]) * t;
                }
                particle.scale *= 1.0 - t * 0.3;

                if particle.age >= particle.lifetime {
                    particle.age = particle.lifetime;
                    self.alive_count = self.alive_count.saturating_sub(1);
                }
            }
        }
    }

    fn find_dead_particle(&self) -> Option<usize> {
        self.particles.iter().position(|p| p.age >= p.lifetime)
    }

    fn emit_particle(&mut self, idx: usize) {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let p = &mut self.particles[idx];
        p.position = self.emitter_data.position;

        let spread = 0.5;
        p.velocity = [
            (rng.gen::<f32>() - 0.5) * spread,
            1.0,
            (rng.gen::<f32>() - 0.5) * spread,
        ];

        let speed = rng.gen_range(self.emitter_data.speed_min..=self.emitter_data.speed_max);
        p.velocity[0] *= speed;
        p.velocity[1] *= speed;
        p.velocity[2] *= speed;

        p.lifetime = rng.gen_range(self.emitter_data.lifetime_min..=self.emitter_data.lifetime_max);
        p.age = 0.0;
        p.scale = rng.gen_range(self.emitter_data.scale_min..=self.emitter_data.scale_max);
        p.color = self.emitter_data.color_start;
    }

    pub fn reset(&mut self) {
        for particle in &mut self.particles {
            *particle = GpuParticle::default();
        }
        self.alive_count = 0;
        self.emitter_data.time = 0.0;
    }

    pub fn get_shader_source() -> &'static str {
        PARTICLE_SHADER_SRC
    }

    pub fn emitter_uniforms(&self) -> &GpuEmitterData {
        &self.emitter_data
    }
}

impl Default for GpuParticleSimulator {
    fn default() -> Self {
        Self::new(GpuParticleConfig::default())
    }
}

/// Particle effect asset for editing and serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleEffectAsset {
    /// Unique identifier.
    pub id: u64,
    /// Human-readable name.
    pub name: String,
    /// Emitter configurations.
    pub emitters: Vec<EmitterConfig>,
    /// Duration of the effect (0 = looping).
    pub duration: f32,
    /// Whether the effect loops.
    pub looping: bool,
    /// Pre-warm time in seconds.
    pub prewarm_time: f32,
}

impl Default for ParticleEffectAsset {
    fn default() -> Self {
        Self {
            id: 0,
            name: "New Particle Effect".to_string(),
            emitters: vec![EmitterConfig::default()],
            duration: 0.0,
            looping: true,
            prewarm_time: 0.0,
        }
    }
}

/// Configuration for a single particle emitter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterConfig {
    /// Name of this emitter.
    pub name: String,
    /// Maximum particles this emitter can have alive.
    pub max_particles: u32,
    /// Emission rate (particles per second).
    pub emission_rate: CurveOrValue,
    /// Burst emissions.
    pub bursts: Vec<BurstConfig>,
    /// Particle lifetime.
    pub lifetime: RangeOrCurve,
    /// Initial speed.
    pub speed: RangeOrCurve,
    /// Initial size/scale.
    pub size: RangeOrCurve,
    /// Color over lifetime.
    pub color: ColorGradient,
    /// Gravity multiplier.
    pub gravity_multiplier: f32,
    /// Velocity over lifetime.
    pub velocity_over_lifetime: VelocityModule,
    /// Size over lifetime.
    pub size_over_lifetime: CurveOrValue,
    /// Rotation over lifetime.
    pub rotation_over_lifetime: CurveOrValue,
    /// Noise/turbulence module.
    pub noise_module: Option<NoiseModule>,
    /// Collision module.
    pub collision_module: Option<CollisionModule>,
    /// Trail module.
    pub trail_module: Option<TrailModule>,
    /// Render settings.
    pub render_settings: ParticleRenderSettings,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            name: "Emitter".to_string(),
            max_particles: 1000,
            emission_rate: CurveOrValue::Constant(10.0),
            bursts: Vec::new(),
            lifetime: RangeOrCurve::Range { min: 1.0, max: 2.0 },
            speed: RangeOrCurve::Range { min: 1.0, max: 2.0 },
            size: RangeOrCurve::Range { min: 0.1, max: 0.2 },
            color: ColorGradient::default(),
            gravity_multiplier: 1.0,
            velocity_over_lifetime: VelocityModule::default(),
            size_over_lifetime: CurveOrValue::Constant(1.0),
            rotation_over_lifetime: CurveOrValue::Constant(0.0),
            noise_module: None,
            collision_module: None,
            trail_module: None,
            render_settings: ParticleRenderSettings::default(),
        }
    }
}

/// Burst emission configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstConfig {
    /// Time to trigger the burst.
    pub time: f32,
    /// Number of particles to emit.
    pub count: u32,
    /// Probability of burst (0-1).
    pub probability: f32,
}

/// Value that can be constant, a range, or animated curve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CurveOrValue {
    Constant(f32),
    Range { min: f32, max: f32 },
    Curve(Vec<CurveKey>),
}

impl Default for CurveOrValue {
    fn default() -> Self {
        Self::Constant(1.0)
    }
}

/// Keyframe in an animation curve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurveKey {
    /// Time (0-1 normalized).
    pub time: f32,
    /// Value at this key.
    pub value: f32,
    /// In tangent.
    pub in_tangent: f32,
    /// Out tangent.
    pub out_tangent: f32,
}

/// Range or curve for particle properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RangeOrCurve {
    Range { min: f32, max: f32 },
    Curve(Vec<CurveKey>),
}

impl Default for RangeOrCurve {
    fn default() -> Self {
        Self::Range { min: 1.0, max: 1.0 }
    }
}

/// Gradient for color over lifetime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorGradient {
    /// Color keys (time, color).
    pub keys: Vec<(f32, [f32; 4])>,
}

impl Default for ColorGradient {
    fn default() -> Self {
        Self {
            keys: vec![(0.0, [1.0, 1.0, 1.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 0.0])],
        }
    }
}

impl ColorGradient {
    /// Evaluate the gradient at a time (0-1).
    pub fn evaluate(&self, t: f32) -> [f32; 4] {
        if self.keys.is_empty() {
            return [1.0, 1.0, 1.0, 1.0];
        }

        if self.keys.len() == 1 {
            return self.keys[0].1;
        }

        let t = t.clamp(0.0, 1.0);

        for i in 0..self.keys.len() - 1 {
            let (t0, c0) = self.keys[i];
            let (t1, c1) = self.keys[i + 1];

            if t >= t0 && t <= t1 {
                let lerp = (t - t0) / (t1 - t0);
                return [
                    c0[0] + (c1[0] - c0[0]) * lerp,
                    c0[1] + (c1[1] - c0[1]) * lerp,
                    c0[2] + (c1[2] - c0[2]) * lerp,
                    c0[3] + (c1[3] - c0[3]) * lerp,
                ];
            }
        }

        self.keys.last().map(|(_, c)| *c).unwrap_or([1.0; 4])
    }
}

/// Velocity over lifetime module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VelocityModule {
    /// Linear velocity.
    pub linear: [CurveOrValue; 3],
    /// Orbital velocity around center.
    pub orbital: [CurveOrValue; 3],
    /// Speed modifier.
    pub speed_multiplier: CurveOrValue,
}

/// Noise/turbulence module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseModule {
    /// Noise strength.
    pub strength: f32,
    /// Noise frequency.
    pub frequency: f32,
    /// Number of octaves.
    pub octaves: u32,
    /// Whether to remap to a range.
    pub remap: Option<(f32, f32)>,
}

impl Default for NoiseModule {
    fn default() -> Self {
        Self {
            strength: 1.0,
            frequency: 1.0,
            octaves: 1,
            remap: None,
        }
    }
}

/// Collision module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollisionModule {
    /// Collision mode.
    pub mode: CollisionMode,
    /// Bounce factor (0-1).
    pub bounce: f32,
    /// Lifetime loss on collision (0-1).
    pub lifetime_loss: f32,
    /// Kill particles on collision.
    pub kill_on_collision: bool,
}

impl Default for CollisionModule {
    fn default() -> Self {
        Self {
            mode: CollisionMode::Plane,
            bounce: 0.5,
            lifetime_loss: 0.0,
            kill_on_collision: false,
        }
    }
}

/// Collision detection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollisionMode {
    /// Collide with a plane at Y=0.
    Plane,
    /// Collide with world geometry.
    World,
    /// Custom collision.
    Custom,
}

/// Trail module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailModule {
    /// Trail lifetime.
    pub lifetime: f32,
    /// Minimum vertex distance.
    pub min_vertex_distance: f32,
    /// Trail width.
    pub width: CurveOrValue,
    /// Trail color.
    pub color: ColorGradient,
}

impl Default for TrailModule {
    fn default() -> Self {
        Self {
            lifetime: 0.5,
            min_vertex_distance: 0.1,
            width: CurveOrValue::Constant(0.1),
            color: ColorGradient::default(),
        }
    }
}

/// Render settings for particles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleRenderSettings {
    /// Blend mode.
    pub blend_mode: BlendMode,
    /// Render alignment.
    pub alignment: ParticleAlignment,
    /// Flipbook texture (sprite sheet).
    pub flipbook: Option<FlipbookConfig>,
    /// Material override.
    pub material: Option<String>,
    /// Sorting mode.
    pub sorting: ParticleSorting,
    /// Render layer.
    pub render_layer: u32,
}

impl Default for ParticleRenderSettings {
    fn default() -> Self {
        Self {
            blend_mode: BlendMode::Additive,
            alignment: ParticleAlignment::View,
            flipbook: None,
            material: None,
            sorting: ParticleSorting::Distance,
            render_layer: 0,
        }
    }
}

/// Blend mode for particle rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Opaque,
    Alpha,
    Additive,
    Multiply,
}

/// Particle alignment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticleAlignment {
    /// Always face the camera.
    View,
    /// Face the camera but only around the Y axis.
    WorldY,
    /// Stretch along velocity direction.
    VelocityStretch,
    /// Fixed world orientation.
    Fixed,
}

/// Flipbook animation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlipbookConfig {
    /// Number of columns in the sprite sheet.
    pub columns: u32,
    /// Number of rows in the sprite sheet.
    pub rows: u32,
    /// Total frames (if less than columns * rows).
    pub frame_count: u32,
    /// Animation speed (frames per second).
    pub fps: f32,
    /// Whether to loop the animation.
    pub looping: bool,
}

/// Particle sorting mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParticleSorting {
    None,
    Distance,
    OldestFirst,
    YoungestFirst,
}

// ---------------------------------------------------------------------------
// Particle Effect Editor State
// ---------------------------------------------------------------------------

/// State for the particle effect editor.
pub struct ParticleEditorState {
    /// Currently edited effect.
    pub current_effect: ParticleEffectAsset,
    /// Selected emitter index.
    pub selected_emitter: Option<usize>,
    /// Simulation time for preview.
    pub simulation_time: f32,
    /// Whether the preview is playing.
    pub is_playing: bool,
    /// Playback speed.
    pub playback_speed: f32,
    /// Show bounding boxes.
    pub show_bounds: bool,
    /// Grid visibility.
    pub show_grid: bool,
    /// Background color for preview.
    pub background_color: [f32; 4],
}

impl Default for ParticleEditorState {
    fn default() -> Self {
        Self {
            current_effect: ParticleEffectAsset::default(),
            selected_emitter: Some(0),
            simulation_time: 0.0,
            is_playing: true,
            playback_speed: 1.0,
            show_bounds: true,
            show_grid: true,
            background_color: [0.1, 0.1, 0.15, 1.0],
        }
    }
}

impl ParticleEditorState {
    /// Create a new editor state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset simulation time.
    pub fn reset_simulation(&mut self) {
        self.simulation_time = 0.0;
    }

    /// Update simulation.
    pub fn update(&mut self, dt: f32) {
        if self.is_playing {
            self.simulation_time += dt * self.playback_speed;

            if self.current_effect.looping && self.current_effect.duration > 0.0
                && self.simulation_time >= self.current_effect.duration {
                    self.simulation_time %= self.current_effect.duration;
                }
        }
    }

    /// Add a new emitter.
    pub fn add_emitter(&mut self) {
        self.current_effect.emitters.push(EmitterConfig::default());
        self.selected_emitter = Some(self.current_effect.emitters.len() - 1);
    }

    /// Remove selected emitter.
    pub fn remove_selected_emitter(&mut self) {
        if let Some(idx) = self.selected_emitter {
            if self.current_effect.emitters.len() > 1 {
                self.current_effect.emitters.remove(idx);
                self.selected_emitter = Some(idx.min(self.current_effect.emitters.len() - 1));
            }
        }
    }

    /// Duplicate selected emitter.
    pub fn duplicate_selected_emitter(&mut self) {
        if let Some(idx) = self.selected_emitter {
            let mut new_emitter = self.current_effect.emitters[idx].clone();
            new_emitter.name = format!("{}_copy", new_emitter.name);
            self.current_effect.emitters.push(new_emitter);
            self.selected_emitter = Some(self.current_effect.emitters.len() - 1);
        }
    }
}

/// Preset particle effects.
pub mod presets {
    use super::*;

    /// Fire effect preset.
    pub fn fire() -> ParticleEffectAsset {
        let mut effect = ParticleEffectAsset::default();
        effect.name = "Fire".to_string();

        let emitter = &mut effect.emitters[0];
        emitter.name = "FireEmitter".to_string();
        emitter.emission_rate = CurveOrValue::Constant(30.0);
        emitter.lifetime = RangeOrCurve::Range { min: 0.5, max: 1.5 };
        emitter.speed = RangeOrCurve::Range { min: 1.0, max: 3.0 };
        emitter.size = RangeOrCurve::Range { min: 0.3, max: 0.5 };
        emitter.gravity_multiplier = -0.5;
        emitter.color = ColorGradient {
            keys: vec![
                (0.0, [1.0, 0.8, 0.0, 1.0]),
                (0.5, [1.0, 0.3, 0.0, 0.8]),
                (1.0, [0.3, 0.1, 0.0, 0.0]),
            ],
        };
        emitter.render_settings.blend_mode = BlendMode::Additive;

        effect
    }

    /// Smoke effect preset.
    pub fn smoke() -> ParticleEffectAsset {
        let mut effect = ParticleEffectAsset::default();
        effect.name = "Smoke".to_string();

        let emitter = &mut effect.emitters[0];
        emitter.name = "SmokeEmitter".to_string();
        emitter.emission_rate = CurveOrValue::Constant(5.0);
        emitter.lifetime = RangeOrCurve::Range { min: 2.0, max: 4.0 };
        emitter.speed = RangeOrCurve::Range { min: 0.3, max: 0.8 };
        emitter.size = RangeOrCurve::Range { min: 0.5, max: 1.0 };
        emitter.gravity_multiplier = 0.2;
        emitter.color = ColorGradient {
            keys: vec![(0.0, [0.3, 0.3, 0.3, 0.6]), (1.0, [0.5, 0.5, 0.5, 0.0])],
        };
        emitter.size_over_lifetime = CurveOrValue::Curve(vec![
            CurveKey {
                time: 0.0,
                value: 0.5,
                in_tangent: 0.0,
                out_tangent: 1.0,
            },
            CurveKey {
                time: 1.0,
                value: 2.0,
                in_tangent: 1.0,
                out_tangent: 0.0,
            },
        ]);
        emitter.render_settings.blend_mode = BlendMode::Alpha;

        effect
    }

    /// Explosion effect preset.
    pub fn explosion() -> ParticleEffectAsset {
        let mut effect = ParticleEffectAsset::default();
        effect.name = "Explosion".to_string();
        effect.duration = 2.0;
        effect.looping = false;

        let emitter = &mut effect.emitters[0];
        emitter.name = "ExplosionCore".to_string();
        emitter.emission_rate = CurveOrValue::Constant(0.0);
        emitter.bursts = vec![BurstConfig {
            time: 0.0,
            count: 100,
            probability: 1.0,
        }];
        emitter.lifetime = RangeOrCurve::Range { min: 0.5, max: 1.0 };
        emitter.speed = RangeOrCurve::Range {
            min: 5.0,
            max: 15.0,
        };
        emitter.size = RangeOrCurve::Range { min: 0.2, max: 0.4 };
        emitter.gravity_multiplier = 0.5;
        emitter.color = ColorGradient {
            keys: vec![
                (0.0, [1.0, 1.0, 0.5, 1.0]),
                (0.3, [1.0, 0.5, 0.0, 0.8]),
                (1.0, [0.2, 0.1, 0.0, 0.0]),
            ],
        };
        emitter.render_settings.blend_mode = BlendMode::Additive;

        effect
    }

    /// Sparkle effect preset.
    pub fn sparkle() -> ParticleEffectAsset {
        let mut effect = ParticleEffectAsset::default();
        effect.name = "Sparkle".to_string();

        let emitter = &mut effect.emitters[0];
        emitter.name = "SparkleEmitter".to_string();
        emitter.emission_rate = CurveOrValue::Constant(20.0);
        emitter.lifetime = RangeOrCurve::Range { min: 0.3, max: 0.8 };
        emitter.speed = RangeOrCurve::Range { min: 0.5, max: 1.5 };
        emitter.size = RangeOrCurve::Range {
            min: 0.05,
            max: 0.1,
        };
        emitter.gravity_multiplier = 0.0;
        emitter.color = ColorGradient {
            keys: vec![
                (0.0, [1.0, 1.0, 1.0, 1.0]),
                (0.5, [1.0, 1.0, 0.8, 0.8]),
                (1.0, [1.0, 1.0, 1.0, 0.0]),
            ],
        };
        emitter.render_settings.blend_mode = BlendMode::Additive;

        effect
    }
}
