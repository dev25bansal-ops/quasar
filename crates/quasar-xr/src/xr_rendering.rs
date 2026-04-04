//! XR Rendering — stereoscopic rendering for VR/AR.
//!
//! Provides:
//! - Stereo rendering pipeline
//! - Distortion correction
//! - Late latching for reduced latency
//! - Foveated rendering support

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3, Vec4};
use wgpu;

pub use crate::XrFov;

/// Maximum number of views (eyes) supported.
pub const MAX_VIEWS: usize = 2;

/// Per-view rendering data.
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct XrViewUniform {
    /// View matrix.
    pub view: [[f32; 4]; 4],
    /// Projection matrix.
    pub projection: [[f32; 4]; 4],
    /// View position for lighting calculations.
    pub view_position: [f32; 4],
    /// Field of view for foveation.
    pub fov: [f32; 4],
    /// Padding.
    pub _pad: [f32; 4],
}

/// Compute projection matrix from FOV.
pub fn fov_to_projection(fov: &XrFov, near: f32, far: f32) -> Mat4 {
    let tan_left = fov.angle_left.tan();
    let tan_right = fov.angle_right.tan();
    let tan_up = fov.angle_up.tan();
    let tan_down = fov.angle_down.tan();

    let tan_width = tan_right - tan_left;
    let tan_height = tan_up - tan_down;

    let a = 2.0 / tan_width;
    let b = 2.0 / tan_height;
    let c = (tan_right + tan_left) / tan_width;
    let d = (tan_up + tan_down) / tan_height;

    let range = far / (near - far);

    Mat4::from_cols(
        Vec4::new(a, 0.0, 0.0, 0.0),
        Vec4::new(0.0, b, 0.0, 0.0),
        Vec4::new(c, d, range, -1.0),
        Vec4::new(0.0, 0.0, range * near, 0.0),
    )
}

/// XR render target configuration.
pub struct XrRenderTarget {
    /// Texture for each eye.
    pub textures: Vec<wgpu::Texture>,
    /// Texture views for each eye.
    pub views: Vec<wgpu::TextureView>,
    /// Depth texture.
    pub depth: wgpu::Texture,
    /// Depth view.
    pub depth_view: wgpu::TextureView,
    /// Resolution per eye.
    pub resolution: [u32; 2],
    /// Format of the color targets.
    pub format: wgpu::TextureFormat,
}

impl XrRenderTarget {
    /// Create a new XR render target.
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let mut textures = Vec::with_capacity(MAX_VIEWS);
        let mut views = Vec::with_capacity(MAX_VIEWS);

        for _ in 0..MAX_VIEWS {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("xr_eye_texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            textures.push(texture);
            views.push(view);
        }

        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("xr_depth_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            textures,
            views,
            depth,
            depth_view,
            resolution: [width, height],
            format,
        }
    }
}

/// Foveation parameters for performance optimization.
#[derive(Debug, Clone, Copy)]
pub struct FoveationParams {
    /// Foveation pattern.
    pub level: FoveationLevel,
    /// Foveation center (usually eye gaze).
    pub center: Vec2,
    /// Horizontal foveation factor.
    pub x_scale: f32,
    /// Vertical foveation factor.
    pub y_scale: f32,
}

/// Foveation quality level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoveationLevel {
    /// No foveation.
    None,
    /// Low foveation (subtle).
    Low,
    /// Medium foveation.
    Medium,
    /// High foveation (aggressive).
    High,
}

impl Default for FoveationLevel {
    fn default() -> Self {
        Self::None
    }
}

impl FoveationParams {
    /// Default foveation parameters (disabled).
    pub fn disabled() -> Self {
        Self {
            level: FoveationLevel::None,
            center: Vec2::new(0.5, 0.5),
            x_scale: 1.0,
            y_scale: 1.0,
        }
    }

    /// Get the inverse foveation scale factors.
    pub fn inverse_scale(&self) -> Vec2 {
        match self.level {
            FoveationLevel::None => Vec2::new(1.0, 1.0),
            FoveationLevel::Low => Vec2::new(0.9, 0.9),
            FoveationLevel::Medium => Vec2::new(0.7, 0.7),
            FoveationLevel::High => Vec2::new(0.5, 0.5),
        }
    }
}

/// Stereo rendering pipeline for XR.
pub struct XrStereoRenderer {
    /// Bind group layout for view uniforms.
    pub view_layout: wgpu::BindGroupLayout,
    /// Pipeline for rendering.
    pub pipeline: wgpu::RenderPipeline,
}

impl XrStereoRenderer {
    /// Create a new stereo renderer.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("xr_view_layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("xr_stereo_pipeline_layout"),
            bind_group_layouts: &[&view_layout],
            push_constant_ranges: &[],
        });

        // Create a simple passthrough shader for stereo rendering
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("xr_stereo_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("xr_stereo.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("xr_stereo_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: Some(std::num::NonZeroU32::new(MAX_VIEWS as u32).unwrap()),
            cache: None,
        });

        Self {
            view_layout,
            pipeline,
        }
    }

    /// Render a single eye.
    pub fn render_eye(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view_uniform: &wgpu::Buffer,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("xr_eye_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.draw(0..3, 0..1);
    }
}
