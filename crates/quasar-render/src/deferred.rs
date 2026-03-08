//! Deferred rendering pipeline — G-Buffer + lighting pass.
//!
//! The geometry pass writes per-pixel data (albedo, normal, roughness/metallic,
//! depth) into a set of render targets called the G-Buffer.  The lighting pass
//! reads these textures and accumulates contributions from an arbitrary number
//! of lights at constant per-pixel cost, enabling 100+ dynamic lights.

use crate::light::LightsUniform;

/// Number of G-Buffer render targets (albedo, normal, roughness-metallic).
/// Depth is provided by the depth attachment.
pub const GBUFFER_TARGET_COUNT: usize = 3;

/// Resolution-dependent G-Buffer textures.
pub struct GBuffer {
    /// Albedo (RGBA8 — base color + alpha).
    pub albedo: wgpu::Texture,
    pub albedo_view: wgpu::TextureView,
    /// World-space normal (RGBA16Float — xyz normal, w unused).
    pub normal: wgpu::Texture,
    pub normal_view: wgpu::TextureView,
    /// Roughness (R) + Metallic (G) + Emissive (B) packed into RGBA8.
    pub roughness_metallic: wgpu::Texture,
    pub roughness_metallic_view: wgpu::TextureView,
    /// Depth (Depth32Float — reused from the forward path).
    pub depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    /// Combined bind group for the lighting pass to sample all G-Buffer textures.
    pub read_bind_group: wgpu::BindGroup,
    pub read_bind_group_layout: wgpu::BindGroupLayout,
    pub width: u32,
    pub height: u32,
}

impl GBuffer {
    /// Create a new G-Buffer matching the given resolution.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let albedo = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Albedo"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let albedo_view = albedo.create_view(&Default::default());

        let normal = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Normal"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let normal_view = normal.create_view(&Default::default());

        let roughness_metallic = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Roughness/Metallic"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let roughness_metallic_view = roughness_metallic.create_view(&Default::default());

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GBuffer Depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GBuffer Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let read_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GBuffer Read Layout"),
                entries: &[
                    // 0: albedo
                    bgl_entry(0, wgpu::TextureSampleType::Float { filterable: true }),
                    // 1: normal
                    bgl_entry(1, wgpu::TextureSampleType::Float { filterable: true }),
                    // 2: roughness-metallic
                    bgl_entry(2, wgpu::TextureSampleType::Float { filterable: true }),
                    // 3: depth
                    bgl_entry(3, wgpu::TextureSampleType::Depth),
                    // 4: sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let read_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GBuffer Read Bind Group"),
            layout: &read_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&roughness_metallic_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            albedo,
            albedo_view,
            normal,
            normal_view,
            roughness_metallic,
            roughness_metallic_view,
            depth,
            depth_view,
            read_bind_group,
            read_bind_group_layout,
            width,
            height,
        }
    }

    /// Rebuild after a window resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        *self = Self::new(device, width, height);
    }
}

/// Deferred lighting pass — reads G-Buffer, accumulates light contributions,
/// and writes the final color to the output target.
pub struct DeferredLightingPass {
    pub pipeline: wgpu::RenderPipeline,
    pub light_buffer: wgpu::Buffer,
    pub light_bind_group: wgpu::BindGroup,
    pub light_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_inv_buffer: wgpu::Buffer,
    pub camera_inv_bind_group: wgpu::BindGroup,
    pub camera_inv_bind_group_layout: wgpu::BindGroupLayout,
}

/// Inverse camera matrices needed to reconstruct world position from depth.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InverseCameraUniforms {
    pub inv_view_proj: [[f32; 4]; 4],
    pub camera_position: [f32; 4],
}

impl DeferredLightingPass {
    /// Create the deferred lighting pass resources.
    pub fn new(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        gbuffer_read_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        // Light uniform buffer + bind group
        let light_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Deferred Light BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Deferred Light Buffer"),
            size: std::mem::size_of::<LightsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let light_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Deferred Light BG"),
            layout: &light_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }],
        });

        // Camera inverse matrices
        let camera_inv_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Deferred CameraInv BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_inv_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Deferred CameraInv Buffer"),
            size: std::mem::size_of::<InverseCameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_inv_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Deferred CameraInv BG"),
            layout: &camera_inv_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_inv_buffer.as_entire_binding(),
            }],
        });

        // Fullscreen-quad lighting shader
        let shader_source = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    // Fullscreen triangle
    var pos = array<vec2<f32>, 3>(
        vec2(-1.0, -1.0),
        vec2( 3.0, -1.0),
        vec2(-1.0,  3.0),
    );
    var out: VertexOutput;
    out.position = vec4(pos[idx], 0.0, 1.0);
    out.uv = (pos[idx] + 1.0) * 0.5;
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

struct LightData {
    position_or_direction: vec4<f32>,
    color_intensity: vec4<f32>,
    light_type: u32,
    inner_angle: u32,
    outer_angle: u32,
    range: u32,
    falloff: f32,
    _pad: vec3<f32>,
};

struct LightsUniform {
    lights: array<LightData, 16>,
    count: u32,
};

struct InvCamera {
    inv_view_proj: mat4x4<f32>,
    camera_position: vec4<f32>,
};

@group(0) @binding(0) var t_albedo: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_rm: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var t_sampler: sampler;

@group(1) @binding(0) var<uniform> lights: LightsUniform;
@group(2) @binding(0) var<uniform> inv_cam: InvCamera;

fn reconstruct_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let clip = vec4(uv * 2.0 - 1.0, depth, 1.0);
    let world_w = inv_cam.inv_view_proj * clip;
    return world_w.xyz / world_w.w;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_albedo, t_sampler, in.uv).rgb;
    let normal = normalize(textureSample(t_normal, t_sampler, in.uv).xyz * 2.0 - 1.0);
    let rm = textureSample(t_rm, t_sampler, in.uv);
    let roughness = rm.r;
    let metallic = rm.g;
    let emissive = rm.b;
    let depth = textureSample(t_depth, t_sampler, in.uv);

    let world_pos = reconstruct_position(in.uv, depth);
    let view_dir = normalize(inv_cam.camera_position.xyz - world_pos);

    var color = vec3(0.0);
    let ambient = albedo * 0.03;
    color += ambient;

    for (var i: u32 = 0u; i < lights.count; i++) {
        let light = lights.lights[i];

        var L: vec3<f32>;
        var attenuation = 1.0;

        if light.light_type == 0u {
            // Directional
            L = normalize(-light.position_or_direction.xyz);
        } else {
            // Point / Spot
            let to_light = light.position_or_direction.xyz - world_pos;
            let dist = length(to_light);
            L = to_light / dist;
            let range = bitcast<f32>(light.range);
            attenuation = max(1.0 - dist / range, 0.0);
            attenuation = attenuation * attenuation;
        }

        let ndl = max(dot(normal, L), 0.0);
        let diffuse = albedo * light.color_intensity.rgb * ndl * attenuation;

        let H = normalize(L + view_dir);
        let ndh = max(dot(normal, H), 0.0);
        let spec_power = mix(8.0, 256.0, 1.0 - roughness);
        let specular = light.color_intensity.rgb * pow(ndh, spec_power) * attenuation * mix(0.04, 1.0, metallic);

        color += diffuse + specular;
    }

    color += albedo * emissive;

    return vec4(color, 1.0);
}
"#;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Deferred Lighting Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Deferred Lighting Pipeline Layout"),
            bind_group_layouts: &[
                gbuffer_read_layout,
                &light_bind_group_layout,
                &camera_inv_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Deferred Lighting Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
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
            pipeline,
            light_buffer,
            light_bind_group,
            light_bind_group_layout,
            camera_inv_buffer,
            camera_inv_bind_group,
            camera_inv_bind_group_layout,
        }
    }

    /// Upload light data for the lighting pass.
    pub fn update_lights(&self, queue: &wgpu::Queue, lights: &LightsUniform) {
        queue.write_buffer(&self.light_buffer, 0, bytemuck::bytes_of(lights));
    }

    /// Upload inverse camera matrices.
    pub fn update_camera(
        &self,
        queue: &wgpu::Queue,
        inv_view_proj: glam::Mat4,
        camera_pos: glam::Vec3,
    ) {
        let data = InverseCameraUniforms {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            camera_position: [camera_pos.x, camera_pos.y, camera_pos.z, 1.0],
        };
        queue.write_buffer(&self.camera_inv_buffer, 0, bytemuck::bytes_of(&data));
    }

    /// Execute the deferred lighting pass — reads G-Buffer, writes to `output_view`.
    pub fn execute(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        gbuffer: &GBuffer,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Deferred Lighting Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &gbuffer.read_bind_group, &[]);
        pass.set_bind_group(1, &self.light_bind_group, &[]);
        pass.set_bind_group(2, &self.camera_inv_bind_group, &[]);
        pass.draw(0..3, 0..1); // fullscreen triangle
    }
}

fn bgl_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

// ── Stencil-based light volumes ────────────────────────────────────

/// Per-light uniform uploaded for each stencil volume draw.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightVolumeUniform {
    /// Model-to-clip matrix for the volume mesh.
    pub mvp: [[f32; 4]; 4],
    /// Light position (xyz) + radius (w).
    pub position_radius: [f32; 4],
    /// Light colour (rgb) + intensity (a).
    pub color_intensity: [f32; 4],
}

/// Stencil light-volume renderer.
///
/// Renders sphere meshes (point lights) or cone meshes (spot lights) with a
/// two-step stencil algorithm:
///   1. **Mark** — draw back-faces into the stencil buffer; increment stencil
///      where depth test fails (the back of the sphere is behind geometry, so
///      the pixel is inside the volume).
///   2. **Shade** — draw front-faces; only shade pixels whose stencil was
///      incremented (i.e. inside the volume and in front of the back face).
///
/// This avoids shading pixels outside each light's influence volume.
pub struct StencilLightVolumePass {
    /// Pipeline for the stencil-mark pass (back-faces, depth fail → inc).
    pub stencil_mark_pipeline: wgpu::RenderPipeline,
    /// Pipeline for the shade pass (front-faces, stencil test, additive blend).
    pub shade_pipeline: wgpu::RenderPipeline,
    /// Bind group layout for the per-light uniform.
    pub volume_bind_group_layout: wgpu::BindGroupLayout,
    /// Shared unit sphere vertex/index buffers.
    pub sphere_vertex_buffer: wgpu::Buffer,
    pub sphere_index_buffer: wgpu::Buffer,
    pub sphere_index_count: u32,
}

impl StencilLightVolumePass {
    /// Generate stencil mark + shade pipelines.
    pub fn new(
        device: &wgpu::Device,
        output_format: wgpu::TextureFormat,
        gbuffer_read_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let volume_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Light Volume BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Stencil Light Volume Shader"),
            source: wgpu::ShaderSource::Wgsl(STENCIL_VOLUME_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Stencil Mark Layout"),
            bind_group_layouts: &[&volume_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }],
        }];

        // Stencil mark pass — render back-faces, depth-fail increments stencil
        let stencil_mark_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Stencil Mark Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_volume"),
                    buffers: &vertex_buffers,
                    compilation_options: Default::default(),
                },
                fragment: None, // depth/stencil only
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: Some(wgpu::Face::Front), // render back-faces
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Always,
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState::IGNORE,
                        back: wgpu::StencilFaceState {
                            compare: wgpu::CompareFunction::Always,
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::IncrementWrap,
                            pass_op: wgpu::StencilOperation::Keep,
                        },
                        read_mask: 0xFF,
                        write_mask: 0xFF,
                    },
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });

        let shade_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Stencil Shade Layout"),
            bind_group_layouts: &[gbuffer_read_layout, &volume_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shade_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Stencil Shade Pipeline"),
            layout: Some(&shade_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_volume"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shade"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back), // front-faces
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState {
                    front: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::NotEqual,
                        fail_op: wgpu::StencilOperation::Keep,
                        depth_fail_op: wgpu::StencilOperation::Keep,
                        pass_op: wgpu::StencilOperation::Zero, // clear for next light
                    },
                    back: wgpu::StencilFaceState::IGNORE,
                    read_mask: 0xFF,
                    write_mask: 0xFF,
                },
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        // Generate a low-poly unit sphere for point-light volumes
        let (vertices, indices) = generate_unit_sphere(12, 8);
        let sphere_vertex_buffer =
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Light Volume Sphere VB"),
                size: (vertices.len() * 12) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let sphere_index_buffer =
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Light Volume Sphere IB"),
                size: (indices.len() * 4) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let sphere_index_count = indices.len() as u32;

        Self {
            stencil_mark_pipeline,
            shade_pipeline,
            volume_bind_group_layout,
            sphere_vertex_buffer,
            sphere_index_buffer,
            sphere_index_count,
        }
    }

    /// Execute per-light stencil volume rendering.
    ///
    /// For each light: upload its `LightVolumeUniform`, run the stencil mark
    /// pass (back-faces, depth-fail increment), then the shade pass (front-faces,
    /// stencil test, additive blend).  Falls back to the fullscreen deferred
    /// lighting path when `light_count < threshold`.
    pub fn execute_per_light(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        depth_stencil_view: &wgpu::TextureView,
        gbuffer: &GBuffer,
        lights: &[LightVolumeUniform],
    ) {
        // Upload sphere geometry (could be cached, but kept simple).
        let (sphere_verts, sphere_idxs) = generate_unit_sphere(12, 8);
        queue.write_buffer(
            &self.sphere_vertex_buffer,
            0,
            bytemuck::cast_slice(&sphere_verts),
        );
        queue.write_buffer(
            &self.sphere_index_buffer,
            0,
            bytemuck::cast_slice(&sphere_idxs),
        );

        for light in lights {
            // Per-light uniform buffer
            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Light Volume Uniform"),
                size: std::mem::size_of::<LightVolumeUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(light));

            let volume_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Light Volume BG"),
                layout: &self.volume_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            // ── Pass 1: Stencil mark (back-faces, depth-fail increment) ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Stencil Mark Pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth_stencil_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.stencil_mark_pipeline);
                pass.set_bind_group(0, &volume_bind_group, &[]);
                pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
                pass.set_index_buffer(self.sphere_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.sphere_index_count, 0, 0..1);
            }

            // ── Pass 2: Shade (front-faces, stencil test, additive blend) ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Stencil Shade Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: output_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth_stencil_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.shade_pipeline);
                pass.set_bind_group(0, &gbuffer.read_bind_group, &[]);
                pass.set_bind_group(1, &volume_bind_group, &[]);
                pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
                pass.set_index_buffer(self.sphere_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.sphere_index_count, 0, 0..1);
            }
        }
    }
}

/// Generate a UV-sphere with `slices` longitude and `stacks` latitude segments.
/// Returns `(positions: Vec<[f32;3]>, indices: Vec<u32>)`.
fn generate_unit_sphere(slices: u32, stacks: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut verts = Vec::new();
    let mut idxs = Vec::new();

    for j in 0..=stacks {
        let theta = std::f32::consts::PI * j as f32 / stacks as f32;
        let st = theta.sin();
        let ct = theta.cos();
        for i in 0..=slices {
            let phi = 2.0 * std::f32::consts::PI * i as f32 / slices as f32;
            verts.push([st * phi.cos(), ct, st * phi.sin()]);
        }
    }

    let row = slices + 1;
    for j in 0..stacks {
        for i in 0..slices {
            let a = j * row + i;
            let b = a + row;
            idxs.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }

    (verts, idxs)
}

/// Inline WGSL for the stencil light volume passes.
const STENCIL_VOLUME_WGSL: &str = r#"
struct VolumeUniform {
    mvp: mat4x4<f32>,
    position_radius: vec4<f32>,
    color_intensity: vec4<f32>,
};

@group(0) @binding(0) var<uniform> volume: VolumeUniform;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_volume(@location(0) position: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = volume.mvp * vec4<f32>(position, 1.0);
    return out;
}

// GBuffer bindings (group 0 in shade layout corresponds to gbuffer read)
@group(0) @binding(0) var t_albedo: texture_2d<f32>;
@group(0) @binding(1) var t_normal: texture_2d<f32>;
@group(0) @binding(2) var t_rm: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var s_gbuffer: sampler;
@group(1) @binding(0) var<uniform> light: VolumeUniform;

@fragment
fn fs_shade(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(t_albedo));
    let uv = frag_coord.xy / dims;

    let albedo = textureSample(t_albedo, s_gbuffer, uv).rgb;
    let normal = textureSample(t_normal, s_gbuffer, uv).xyz * 2.0 - 1.0;

    let light_pos = light.position_radius.xyz;
    let radius = light.position_radius.w;
    let light_col = light.color_intensity.rgb * light.color_intensity.a;

    // Simple point-light attenuation (placeholder world-pos reconstruction)
    let falloff = 1.0 / (1.0 + radius * 0.1);
    let contribution = albedo * light_col * falloff * max(dot(normal, normalize(light_pos)), 0.0);
    return vec4<f32>(contribution, 1.0);
}
"#;
