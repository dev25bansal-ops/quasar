//! Cascade Shadow Maps (CSM) — splits camera frustum into multiple cascades.
//!
//! Each cascade renders its own shadow map, allowing higher resolution shadows
//! near the camera and eliminating shadow swimming artifacts in large scenes.

use crate::camera::CameraUniform;
use glam::Vec4Swizzles;

pub const CASCADE_COUNT: usize = 4;
pub const SHADOW_MAP_SIZE: u32 = 1024;

#[derive(Debug, Clone, Copy)]
pub struct Cascade {
    pub view_projection: glam::Mat4,
    pub split_depth: f32,
    pub resolution: f32,
}

#[derive(Debug, Clone)]
pub struct CascadeShadowMap {
    pub texture: wgpu::Texture,
    pub views: Vec<wgpu::TextureView>,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::RenderPipeline,
    pub camera_buffers: Vec<wgpu::Buffer>,
    pub camera_bind_groups: Vec<wgpu::BindGroup>,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub cascades: Vec<Cascade>,
    pub cascade_buffer: wgpu::Buffer,
}

impl CascadeShadowMap {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cascade Shadow Map Array"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: CASCADE_COUNT as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let views: Vec<wgpu::TextureView> = (0..CASCADE_COUNT)
            .map(|i| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(&format!("Cascade Shadow View {}", i)),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: i as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Cascade Shadow Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToBorder,
            address_mode_v: wgpu::AddressMode::ClampToBorder,
            address_mode_w: wgpu::AddressMode::ClampToBorder,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            border_color: Some(wgpu::SamplerBorderColor::OpaqueWhite),
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Cascade Shadow Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Cascade Shadow Array View"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cascade Shadow Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cascade Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_buffers: Vec<wgpu::Buffer> = (0..CASCADE_COUNT)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Cascade Camera Buffer"),
                    size: std::mem::size_of::<CameraUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();

        let camera_bind_groups: Vec<wgpu::BindGroup> = camera_buffers
            .iter()
            .map(|buffer| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Cascade Camera Bind Group"),
                    layout: &camera_bind_group_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                })
            })
            .collect();

        let cascade_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cascade Data Buffer"),
            size: (CASCADE_COUNT * std::mem::size_of::<CascadeUniform>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader_source = include_str!("../../../assets/shaders/shadow.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cascade Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cascade Shadow Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::vertex::Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cascade Shadow Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 4.0,
                    ..Default::default()
                },
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let cascades = Self::calculate_cascades(0.1, 100.0);

        Self {
            texture,
            views,
            sampler,
            bind_group,
            bind_group_layout,
            pipeline,
            camera_buffers,
            camera_bind_groups,
            camera_bind_group_layout,
            cascades,
            cascade_buffer,
        }
    }

    /// Practical Split Scheme (Nvidia GPU Gems 3, Ch. 10).
    ///
    /// Blends a logarithmic distribution (even in log-space) with a linear one
    /// using a `lambda` factor.  `lambda = 1.0` is fully logarithmic; `0.0` is
    /// fully linear.  Default `lambda = 0.75` is a good general-purpose value.
    pub fn calculate_cascades(near: f32, far: f32) -> Vec<Cascade> {
        Self::calculate_cascades_lambda(near, far, 0.75)
    }

    /// Same as [`calculate_cascades`] but with an explicit blend factor.
    pub fn calculate_cascades_lambda(near: f32, far: f32, lambda: f32) -> Vec<Cascade> {
        let mut cascades = Vec::with_capacity(CASCADE_COUNT);

        for i in 1..=CASCADE_COUNT {
            let t = i as f32 / CASCADE_COUNT as f32;
            let log_split = near * (far / near).powf(t);
            let lin_split = near + (far - near) * t;
            let split_depth = lambda * log_split + (1.0 - lambda) * lin_split;
            cascades.push(Cascade {
                view_projection: glam::Mat4::IDENTITY,
                split_depth,
                resolution: SHADOW_MAP_SIZE as f32,
            });
        }

        cascades
    }

    pub fn update_cascades(
        &mut self,
        camera_view_proj: glam::Mat4,
        _camera_position: glam::Vec3,
        light_direction: glam::Vec3,
        near: f32,
        far: f32,
    ) {
        let inv_view_proj = camera_view_proj.inverse();
        let _corners = Self::get_frustum_corners(inv_view_proj, near, far);

        let split_depths: Vec<f32> = self.cascades.iter().map(|c| c.split_depth).collect();

        for (i, cascade) in self.cascades.iter_mut().enumerate() {
            let prev_split = if i == 0 { near } else { split_depths[i - 1] };
            let split = cascade.split_depth;

            let cascade_corners = Self::get_frustum_corners(inv_view_proj, prev_split, split);
            let center = Self::get_corners_center(&cascade_corners);

            let max_dist = cascade_corners
                .iter()
                .map(|c| (*c - center).length())
                .fold(0.0_f32, f32::max);

            let radius = max_dist.ceil();

            let light_pos = center - light_direction * radius;
            let light_view = glam::Mat4::look_at_rh(light_pos, center, glam::Vec3::Y);

            let light_proj = glam::Mat4::orthographic_rh(
                -radius,
                radius,
                -radius,
                radius,
                -radius * 2.0,
                radius * 2.0,
            );

            cascade.view_projection = light_proj * light_view;
        }
    }

    fn get_frustum_corners(inv_view_proj: glam::Mat4, near: f32, far: f32) -> Vec<glam::Vec3> {
        let ndc_corners = [
            glam::Vec4::new(-1.0, -1.0, near, 1.0),
            glam::Vec4::new(1.0, -1.0, near, 1.0),
            glam::Vec4::new(-1.0, 1.0, near, 1.0),
            glam::Vec4::new(1.0, 1.0, near, 1.0),
            glam::Vec4::new(-1.0, -1.0, far, 1.0),
            glam::Vec4::new(1.0, -1.0, far, 1.0),
            glam::Vec4::new(-1.0, 1.0, far, 1.0),
            glam::Vec4::new(1.0, 1.0, far, 1.0),
        ];

        ndc_corners
            .iter()
            .map(|c| {
                let world = inv_view_proj * *c;
                world.xyz() / world.w
            })
            .collect()
    }

    fn get_corners_center(corners: &[glam::Vec3]) -> glam::Vec3 {
        corners.iter().fold(glam::Vec3::ZERO, |acc, c| acc + *c) / corners.len() as f32
    }

    pub fn render_cascade(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cascade_index: usize,
        objects: &[(&crate::mesh::Mesh, glam::Mat4)],
    ) -> wgpu::CommandBuffer {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(&format!("Cascade {} Pass Encoder", cascade_index)),
        });

        let cascade = &self.cascades[cascade_index];

        let mut uniform = CameraUniform::new();
        uniform.view_proj = cascade.view_projection.to_cols_array_2d();
        queue.write_buffer(
            &self.camera_buffers[cascade_index],
            0,
            bytemuck::bytes_of(&uniform),
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&format!("Cascade {} Render Pass", cascade_index)),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.views[cascade_index],
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_groups[cascade_index], &[]);

            for (mesh, _) in objects {
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        encoder.finish()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CascadeUniform {
    pub view_proj: [[f32; 4]; 4],
    pub split_depth: f32,
    pub _pad: [f32; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_calculation() {
        let cascades = CascadeShadowMap::calculate_cascades(0.1, 100.0);
        assert_eq!(cascades.len(), CASCADE_COUNT);

        for i in 0..CASCADE_COUNT - 1 {
            assert!(cascades[i].split_depth < cascades[i + 1].split_depth);
        }
    }

    #[test]
    fn frustum_corners() {
        let proj = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        let view_proj = proj * view;

        let corners = CascadeShadowMap::get_frustum_corners(view_proj.inverse(), 0.1, 100.0);
        assert_eq!(corners.len(), 8);

        for corner in corners {
            assert!(corner.is_finite());
        }
    }
}
