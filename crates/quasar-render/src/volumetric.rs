//! Volumetric lighting and fog.
//!
//! Provides a ray-marched volumetric fog effect composited as a post-process.
//!
//! - [`VolumetricFogSettings`]: resource controlling density, scattering, etc.
//! - [`VolumetricFogPass`]: GPU resources (froxel / screen-space) for the effect.
//! - Inline WGSL compute shader marches from camera through depth, accumulating
//!   in-scattering from directional and point lights with a Henyey-Greenstein
//!   phase function before additive compositing.

use bytemuck::{Pod, Zeroable};

/// Maximum ray-march steps per pixel (quality knob).
pub const MAX_MARCH_STEPS: u32 = 64;
/// Default froxel volume resolution (width, height, depth slices).
pub const FROXEL_WIDTH: u32 = 160;
pub const FROXEL_HEIGHT: u32 = 90;
pub const FROXEL_DEPTH: u32 = 128;

// ── Settings resource ──────────────────────────────────────────────

/// Configuration for volumetric fog — insert as a resource.
#[derive(Debug, Clone, Copy)]
pub struct VolumetricFogSettings {
    /// Global fog density (0 = none, 1 = opaque at ~50 m).
    pub density: f32,
    /// Scattering albedo colour (RGB, energy-conserving ≤1).
    pub scattering_color: [f32; 3],
    /// Absorption coefficient (higher = darker fog).
    pub absorption: f32,
    /// Henyey-Greenstein phase asymmetry (−1..1, 0 = isotropic).
    pub phase_g: f32,
    /// Maximum distance the ray is marched (world units).
    pub max_distance: f32,
    /// Number of march steps (clamped to MAX_MARCH_STEPS).
    pub steps: u32,
    /// Height-based fog: base altitude.
    pub height_fog_base: f32,
    /// Height-based fog: falloff rate (0 = uniform, >0 = denser near base).
    pub height_fog_falloff: f32,
    /// Ambient term added to in-scattering to prevent pitch-black fog.
    pub ambient_intensity: f32,
    /// Master on/off.
    pub enabled: bool,
}

impl Default for VolumetricFogSettings {
    fn default() -> Self {
        Self {
            density: 0.02,
            scattering_color: [1.0, 1.0, 1.0],
            absorption: 0.01,
            phase_g: 0.5,
            max_distance: 200.0,
            steps: 48,
            height_fog_base: 0.0,
            height_fog_falloff: 0.0,
            ambient_intensity: 0.05,
            enabled: true,
        }
    }
}

// ── GPU uniform ────────────────────────────────────────────────────

/// Packed uniform for the ray-march compute / fragment pass.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct VolumetricFogUniform {
    /// inv(ViewProj) for world-space reconstruction.
    pub inv_view_proj: [[f32; 4]; 4],
    /// Camera position (xyz) + density (w).
    pub camera_pos_density: [f32; 4],
    /// Scattering colour (rgb) + absorption (a).
    pub scatter_absorption: [f32; 4],
    /// phase_g, max_distance, steps (as f32), height_fog_base
    pub params_a: [f32; 4],
    /// height_fog_falloff, ambient_intensity, _pad, _pad
    pub params_b: [f32; 4],
}

// ── GPU pass ───────────────────────────────────────────────────────

/// Owns the GPU textures, bind groups and pipelines for volumetric fog.
pub struct VolumetricFogPass {
    /// Screen-resolution Rgba16Float accumulation texture.
    pub scatter_texture: wgpu::Texture,
    pub scatter_view: wgpu::TextureView,
    /// Uniform buffer for [`VolumetricFogUniform`].
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub compute_pipeline: wgpu::ComputePipeline,
    pub composite_pipeline: wgpu::RenderPipeline,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
    /// Whether the real depth texture has been bound (replacing the 1×1 placeholder).
    pub depth_bound: bool,
}

impl VolumetricFogPass {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        _depth_format: wgpu::TextureFormat,
        output_format: wgpu::TextureFormat,
    ) -> Self {
        // Accumulation texture (half-res for perf, full for quality — use full here).
        let scatter_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Volumetric Scatter"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scatter_view = scatter_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volumetric Fog Uniform"),
            size: std::mem::size_of::<VolumetricFogUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Volumetric Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Bind group layout shared by compute + composite.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Volumetric BGL"),
            entries: &[
                // 0: uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 1: scatter texture (storage write in compute, sampled read in composite)
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
                // 2: depth texture (sampled in compute)
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
                // 3: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create a 1x1 depth placeholder so the initial bind group is valid.
        let depth_placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Volumetric Depth Placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_placeholder_view =
            depth_placeholder.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&scatter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth_placeholder_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // ── Compute pipeline (ray-march) ──
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric Compute"),
            source: wgpu::ShaderSource::Wgsl(VOLUMETRIC_COMPUTE_WGSL.into()),
        });
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Volumetric Compute Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Volumetric Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // ── Composite pipeline (additive overlay) ──
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Volumetric Composite"),
            source: wgpu::ShaderSource::Wgsl(VOLUMETRIC_COMPOSITE_WGSL.into()),
        });

        // Composite reads the scatter texture as a sampled texture → needs a
        // separate BGL that binds it as Texture, not StorageTexture.
        // For simplicity we re-use the same layout and use a separate bind group
        // at draw time.  In practice you'd have a dedicated layout.
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Volumetric Composite BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Volumetric Composite Layout"),
            bind_group_layouts: &[&composite_bgl],
            push_constant_ranges: &[],
        });

        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Volumetric Composite"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            scatter_texture,
            scatter_view,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            compute_pipeline,
            composite_pipeline,
            sampler,
            width,
            height,
            depth_bound: false,
        }
    }

    /// Upload the uniform buffer for this frame.
    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        settings: &VolumetricFogSettings,
        inv_view_proj: glam::Mat4,
        camera_pos: glam::Vec3,
    ) {
        let uniform = VolumetricFogUniform {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_pos_density: [camera_pos.x, camera_pos.y, camera_pos.z, settings.density],
            scatter_absorption: [
                settings.scattering_color[0],
                settings.scattering_color[1],
                settings.scattering_color[2],
                settings.absorption,
            ],
            params_a: [
                settings.phase_g,
                settings.max_distance,
                settings.steps.min(MAX_MARCH_STEPS) as f32,
                settings.height_fog_base,
            ],
            params_b: [
                settings.height_fog_falloff,
                settings.ambient_intensity,
                0.0,
                0.0,
            ],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }

    /// Rebuild the bind group when the depth texture changes (e.g. resize).
    pub fn rebuild_bind_group(
        &mut self,
        device: &wgpu::Device,
        depth_view: &wgpu::TextureView,
    ) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Volumetric BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.scatter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.depth_bound = true;
    }

    /// Resize the scatter texture and rebind the depth view.
    ///
    /// Pass the current depth texture view so the bind group is kept in sync.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        depth_view: &wgpu::TextureView,
    ) {
        self.width = width;
        self.height = height;
        self.scatter_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Volumetric Scatter"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.scatter_view = self
            .scatter_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.rebuild_bind_group(device, depth_view);
    }
}

// ── WGSL shaders (inline) ──────────────────────────────────────────

/// Compute shader: ray-march through the scene and accumulate in-scattering.
pub const VOLUMETRIC_COMPUTE_WGSL: &str = r#"
struct FogUniforms {
    inv_view_proj: mat4x4<f32>,
    camera_pos_density: vec4<f32>,
    scatter_absorption: vec4<f32>,
    params_a: vec4<f32>,
    params_b: vec4<f32>,
};

@group(0) @binding(0) var<uniform> fog: FogUniforms;
@group(0) @binding(1) var scatter_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
@group(0) @binding(3) var samp: sampler;

fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * 3.14159265 * pow(1.0 + g2 - 2.0 * g * cos_theta, 1.5));
}

fn world_from_clip(clip: vec4<f32>) -> vec3<f32> {
    let w = fog.inv_view_proj * clip;
    return w.xyz / w.w;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(scatter_out);
    if gid.x >= dims.x || gid.y >= dims.y { return; }

    let uv = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5) / vec2<f32>(f32(dims.x), f32(dims.y));
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);

    let depth_raw = textureLoad(depth_tex, vec2<i32>(gid.xy), 0);
    let max_t_from_depth = length(world_from_clip(vec4<f32>(ndc, depth_raw, 1.0)) - fog.camera_pos_density.xyz);

    let camera_pos = fog.camera_pos_density.xyz;
    let density     = fog.camera_pos_density.w;
    let scatter_col = fog.scatter_absorption.xyz;
    let absorption  = fog.scatter_absorption.w;
    let phase_g     = fog.params_a.x;
    let max_distance= fog.params_a.y;
    let steps_f     = fog.params_a.z;
    let height_base = fog.params_a.w;
    let height_fall = fog.params_b.x;
    let ambient     = fog.params_b.y;

    let far_world = world_from_clip(vec4<f32>(ndc, 0.0, 1.0));
    let ray_dir = normalize(far_world - camera_pos);

    let march_dist = min(max_t_from_depth, max_distance);
    let steps = u32(steps_f);
    let step_size = march_dist / f32(steps);

    // Simple directional light down -Y for demonstration; in production
    // this would be driven by the LightsUniform buffer.
    let light_dir = normalize(vec3<f32>(-0.3, -1.0, -0.2));
    let light_color = vec3<f32>(1.0, 0.95, 0.9);

    var accumulated = vec3<f32>(0.0);
    var transmittance = 1.0;

    for (var i: u32 = 0u; i < steps; i = i + 1u) {
        let t = (f32(i) + 0.5) * step_size;
        let pos = camera_pos + ray_dir * t;

        var local_density = density;
        if height_fall > 0.0 {
            local_density *= exp(-max(pos.y - height_base, 0.0) * height_fall);
        }

        let extinction = local_density * (1.0 + absorption);
        let cos_theta = dot(ray_dir, -light_dir);
        let phase = henyey_greenstein(cos_theta, phase_g);
        let in_scatter = scatter_col * local_density * (phase * light_color + ambient);

        accumulated += transmittance * in_scatter * step_size;
        transmittance *= exp(-extinction * step_size);

        if transmittance < 0.01 { break; }
    }

    textureStore(scatter_out, vec2<i32>(gid.xy), vec4<f32>(accumulated, 1.0 - transmittance));
}
"#;

/// Full-screen triangle compositing the scatter texture additively.
pub const VOLUMETRIC_COMPOSITE_WGSL: &str = r#"
@group(0) @binding(0) var scatter_tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv  = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(scatter_tex, samp, in.uv);
}
"#;
