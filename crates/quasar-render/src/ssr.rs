//! Screen-Space Reflections (SSR).
//!
//! Traces rays in screen-space along the view-reflected direction using
//! hierarchical ray-marching against the depth buffer.  Falls back to the
//! environment map when the ray leaves the screen or no intersection is found.
//!
//! Pipeline:
//!   1. SSR trace (compute/fragment) — writes reflected color + confidence to
//!      an Rgba16Float texture.
//!   2. Temporal resolve — blends with previous frame to reduce noise.
//!   3. Composite — mixes SSR result into the lit scene using roughness.

use bytemuck::{Pod, Zeroable};

/// Configuration resource.
#[derive(Debug, Clone, Copy)]
pub struct SsrSettings {
    /// Maximum ray-march steps per pixel.
    pub max_steps: u32,
    /// Step size in UV space (smaller = higher quality, slower).
    pub step_size: f32,
    /// Thickness threshold — how thick a surface is considered for hit test.
    pub thickness: f32,
    /// Maximum roughness at which SSR is applied (above = env-map only).
    pub max_roughness: f32,
    /// Temporal blend factor (0 = no history, 1 = full history).
    pub temporal_weight: f32,
    /// Master on/off.
    pub enabled: bool,
}

impl Default for SsrSettings {
    fn default() -> Self {
        Self {
            max_steps: 64,
            step_size: 0.015,
            thickness: 0.1,
            max_roughness: 0.5,
            temporal_weight: 0.9,
            enabled: true,
        }
    }
}

/// GPU uniform matching the WGSL struct.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SsrUniform {
    /// inv(ViewProj) for reconstructing world position.
    pub inv_view_proj: [[f32; 4]; 4],
    /// ViewProj (current frame) for re-projecting world → screen.
    pub view_proj: [[f32; 4]; 4],
    /// Camera position (xyz), max_steps (w as f32).
    pub camera_pos_steps: [f32; 4],
    /// step_size, thickness, max_roughness, temporal_weight.
    pub params: [f32; 4],
    /// Screen resolution (xy), _pad, _pad.
    pub resolution: [f32; 4],
}

/// GPU resources for the SSR pass.
pub struct SsrPass {
    pub ssr_texture: wgpu::Texture,
    pub ssr_view: wgpu::TextureView,
    pub history_texture: wgpu::Texture,
    pub history_view: wgpu::TextureView,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub resolve_pipeline: wgpu::RenderPipeline,
    pub width: u32,
    pub height: u32,
}

impl SsrPass {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        gbuffer_read_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let ssr_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Result"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let ssr_view = ssr_texture.create_view(&Default::default());

        let history_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR History"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let history_view = history_texture.create_view(&Default::default());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SSR Uniform"),
            size: std::mem::size_of::<SsrUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSR Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("SSR BGL"),
                entries: &[
                    // 0: uniform
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
                    // 1: scene color (lit)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // 2: depth
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Depth,
                        },
                        count: None,
                    },
                    // 3: history
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
                    // 4: sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // placeholder 1×1 textures for initial bind group
        let placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let ph_view = placeholder.create_view(&Default::default());
        let depth_ph = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SSR Depth PH"),
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
        let depth_ph_view = depth_ph.create_view(&Default::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSR BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&ph_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&depth_ph_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&history_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SSR Shader"),
            source: wgpu::ShaderSource::Wgsl(SSR_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSR Pipeline Layout"),
            bind_group_layouts: &[gbuffer_read_layout, &bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = create_ssr_render_pipeline(
            device, &pipeline_layout, &shader, "fs_ssr_trace",
            wgpu::TextureFormat::Rgba16Float,
        );
        let resolve_pipeline = create_ssr_render_pipeline(
            device, &pipeline_layout, &shader, "fs_ssr_resolve",
            wgpu::TextureFormat::Rgba16Float,
        );

        Self {
            ssr_texture,
            ssr_view,
            history_texture,
            history_view,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            resolve_pipeline,
            width,
            height,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32, gbuffer_layout: &wgpu::BindGroupLayout) {
        *self = Self::new(device, w, h, gbuffer_layout);
    }
}

fn create_ssr_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    })
}

/// Inline WGSL for screen-space reflections.
const SSR_WGSL: &str = r#"
struct SsrUniform {
    inv_view_proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    camera_pos_steps: vec4<f32>,
    params: vec4<f32>,
    resolution: vec4<f32>,
};

// GBuffer
@group(0) @binding(0) var t_albedo: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_rm: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var s_gbuffer: sampler;

@group(1) @binding(0) var<uniform> ssr: SsrUniform;
@group(1) @binding(1) var t_scene: texture_2d<f32>;
@group(1) @binding(2) var t_scene_depth: texture_depth_2d;
@group(1) @binding(3) var t_history: texture_2d<f32>;
@group(1) @binding(4) var s_ssr: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> VsOut {
    var positions = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var out: VsOut;
    out.pos = vec4(positions[idx], 0.0, 1.0);
    out.uv = (positions[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

fn reconstruct_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec4<f32>(uv * 2.0 - 1.0, depth, 1.0);
    let world = ssr.inv_view_proj * ndc;
    return world.xyz / world.w;
}

@fragment
fn fs_ssr_trace(in: VsOut) -> @location(0) vec4<f32> {
    let depth = textureSample(t_depth, s_gbuffer, in.uv);
    if depth >= 1.0 { return vec4(0.0); }

    let roughness = textureSample(t_rm, s_gbuffer, in.uv).r;
    let max_roughness = ssr.params.z;
    if roughness > max_roughness { return vec4(0.0); }

    let world_pos = reconstruct_position(in.uv, depth);
    let normal = normalize(textureSample(t_normal, s_gbuffer, in.uv).xyz * 2.0 - 1.0);
    let camera_pos = ssr.camera_pos_steps.xyz;
    let view_dir = normalize(world_pos - camera_pos);
    let reflect_dir = reflect(view_dir, normal);

    let max_steps = u32(ssr.camera_pos_steps.w);
    let step_sz = ssr.params.x;
    let thickness = ssr.params.y;

    var ray_pos = world_pos + reflect_dir * 0.01;
    var hit_color = vec3<f32>(0.0);
    var confidence = 0.0;

    for (var i: u32 = 0u; i < max_steps; i = i + 1u) {
        ray_pos += reflect_dir * step_sz * (1.0 + f32(i) * 0.05);

        let clip = ssr.view_proj * vec4(ray_pos, 1.0);
        if clip.w <= 0.0 { break; }
        let ray_uv = (clip.xy / clip.w) * 0.5 + 0.5;
        let ray_uv_flipped = vec2(ray_uv.x, 1.0 - ray_uv.y);

        if ray_uv_flipped.x < 0.0 || ray_uv_flipped.x > 1.0 ||
           ray_uv_flipped.y < 0.0 || ray_uv_flipped.y > 1.0 { break; }

        let scene_depth = textureSample(t_scene_depth, s_ssr, ray_uv_flipped);
        let ray_depth = clip.z / clip.w;
        let diff = ray_depth - scene_depth;

        if diff > 0.0 && diff < thickness {
            hit_color = textureSample(t_scene, s_ssr, ray_uv_flipped).rgb;
            // Fade near screen edges
            let edge_fade = smoothstep(0.0, 0.05, min(
                min(ray_uv_flipped.x, 1.0 - ray_uv_flipped.x),
                min(ray_uv_flipped.y, 1.0 - ray_uv_flipped.y)
            ));
            confidence = edge_fade * (1.0 - roughness / max_roughness);
            break;
        }
    }

    return vec4(hit_color, confidence);
}

@fragment
fn fs_ssr_resolve(in: VsOut) -> @location(0) vec4<f32> {
    let current = textureSample(t_scene, s_ssr, in.uv);
    let history = textureSample(t_history, s_ssr, in.uv);
    let temporal_w = ssr.params.w;
    return mix(current, history, temporal_w);
}
"#;
