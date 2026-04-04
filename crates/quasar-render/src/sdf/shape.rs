//! SDF shape primitives and rendering.
//!
//! Provides resolution-independent shapes:
//! - Basic shapes (rect, circle, rounded rect, ellipse)
//! - Complex shapes (paths, polygons)
//! - Boolean operations (union, intersection, subtraction)

use bytemuck::{Pod, Zeroable};

/// SDF shape types.
#[derive(Debug, Clone, Copy)]
pub enum SdfShape {
    /// Rectangle (half-size xy).
    Rect { half_size: [f32; 2] },
    /// Rounded rectangle (half-size xy, corner_radius).
    RoundedRect {
        half_size: [f32; 2],
        corner_radius: f32,
    },
    /// Circle (radius).
    Circle { radius: f32 },
    /// Ellipse (radii xy).
    Ellipse { radii: [f32; 2] },
    /// Capsule (half-height, radius).
    Capsule { half_height: f32, radius: f32 },
    /// Triangle (half-size xy).
    Triangle { half_size: [f32; 2] },
    /// Star (outer_radius, inner_radius, points).
    Star {
        outer_radius: f32,
        inner_radius: f32,
        points: u32,
    },
    /// Regular polygon (radius, sides).
    RegularPolygon { radius: f32, sides: u32 },
    /// Annulus (outer_radius, inner_radius).
    Annulus {
        outer_radius: f32,
        inner_radius: f32,
    },
    /// Cross (half-size xy, thickness).
    Cross { half_size: [f32; 2], thickness: f32 },
}

impl SdfShape {
    /// Evaluate the signed distance at a point.
    pub fn distance(&self, p: [f32; 2]) -> f32 {
        match self {
            Self::Rect { half_size } => sd_rect(p, *half_size),
            Self::RoundedRect {
                half_size,
                corner_radius,
            } => sd_rounded_rect(p, *half_size, *corner_radius),
            Self::Circle { radius } => sd_circle(p, *radius),
            Self::Ellipse { radii } => sd_ellipse(p, *radii),
            Self::Capsule {
                half_height,
                radius,
            } => sd_capsule(p, *half_height, *radius),
            Self::Triangle { half_size } => sd_triangle(p, *half_size),
            Self::Star {
                outer_radius,
                inner_radius,
                points,
            } => sd_star(p, *outer_radius, *inner_radius, *points),
            Self::RegularPolygon { radius, sides } => sd_polygon(p, *radius, *sides),
            Self::Annulus {
                outer_radius,
                inner_radius,
            } => sd_annulus(p, *outer_radius, *inner_radius),
            Self::Cross {
                half_size,
                thickness,
            } => sd_cross(p, *half_size, *thickness),
        }
    }

    /// Get the bounding box (min_xy, max_xy).
    pub fn bounds(&self) -> [[f32; 2]; 2] {
        match self {
            Self::Rect { half_size } => {
                [[-half_size[0], -half_size[1]], [half_size[0], half_size[1]]]
            }
            Self::RoundedRect { half_size, .. } => {
                [[-half_size[0], -half_size[1]], [half_size[0], half_size[1]]]
            }
            Self::Circle { radius } => [[-*radius, -*radius], [*radius, *radius]],
            Self::Ellipse { radii } => [[-radii[0], -radii[1]], [radii[0], radii[1]]],
            Self::Capsule {
                half_height,
                radius,
            } => [
                [-*radius, -*half_height - *radius],
                [*radius, *half_height + *radius],
            ],
            Self::Triangle { half_size } => {
                [[-half_size[0], -half_size[1]], [half_size[0], half_size[1]]]
            }
            Self::Star { outer_radius, .. } => [
                [-*outer_radius, -*outer_radius],
                [*outer_radius, *outer_radius],
            ],
            Self::RegularPolygon { radius, .. } => [[-*radius, -*radius], [*radius, *radius]],
            Self::Annulus { outer_radius, .. } => [
                [-*outer_radius, -*outer_radius],
                [*outer_radius, *outer_radius],
            ],
            Self::Cross { half_size, .. } => {
                [[-half_size[0], -half_size[1]], [half_size[0], half_size[1]]]
            }
        }
    }
}

/// Signed distance to rectangle.
#[inline]
pub fn sd_rect(p: [f32; 2], half_size: [f32; 2]) -> f32 {
    let d = [p[0].abs() - half_size[0], p[1].abs() - half_size[1]];
    let outside = f32::max(d[0], d[1]);
    let inside = f32::min(f32::max(d[0], d[1]), 0.0);
    outside + inside
}

/// Signed distance to rounded rectangle.
#[inline]
pub fn sd_rounded_rect(p: [f32; 2], half_size: [f32; 2], corner_radius: f32) -> f32 {
    let d = [
        p[0].abs() - half_size[0] + corner_radius,
        p[1].abs() - half_size[1] + corner_radius,
    ];
    let outside = f32::max(d[0], d[1]);
    let inside = f32::min(f32::max(d[0], d[1]), 0.0);
    (outside + inside) - corner_radius
}

/// Signed distance to circle.
#[inline]
pub fn sd_circle(p: [f32; 2], radius: f32) -> f32 {
    (p[0] * p[0] + p[1] * p[1]).sqrt() - radius
}

/// Signed distance to ellipse.
#[inline]
pub fn sd_ellipse(p: [f32; 2], radii: [f32; 2]) -> f32 {
    let p0 = [p[0] / radii[0], p[1] / radii[1]];
    let len = (p0[0] * p0[0] + p0[1] * p0[1]).sqrt();
    let m = len.sqrt();
    let d = (len - 1.0) / m;
    d * radii[0].min(radii[1])
}

/// Signed distance to capsule (vertical).
#[inline]
pub fn sd_capsule(p: [f32; 2], half_height: f32, radius: f32) -> f32 {
    let py = p[1].abs() - half_height;
    let p2 = [p[0], py.max(0.0)];
    (p2[0] * p2[0] + p2[1] * p2[1]).sqrt() - radius + py.min(0.0).max(0.0)
}

/// Signed distance to equilateral triangle.
#[inline]
pub fn sd_triangle(p: [f32; 2], half_size: [f32; 2]) -> f32 {
    let q = [p[0].abs(), p[1]];
    let sx = half_size[0];
    let sy = half_size[1];

    let k = sx * 2.0 / sy;
    let px = q[0] - q[1] * k;
    let py = q[1];

    let c1 = px.max(0.0);
    let c2 = py.max(0.0);
    let c = (c1 * c1 + c2 * c2).sqrt();

    let d = (px + sy).max(0.0);
    let h = (py + sx).max(0.0);

    f32::max(c - sx, f32::min(d, h)) * f32::copysign(1.0, f32::max(px + sy, py + sx))
}

/// Signed distance to star.
#[inline]
pub fn sd_star(p: [f32; 2], outer_radius: f32, inner_radius: f32, points: u32) -> f32 {
    let n = points as f32;
    let k = std::f32::consts::PI / n;
    let an = p[0].atan2(p[1]);
    let bn = ((an / k + 0.5).floor() * 2.0 + 1.0) * k;

    let cos_an = an.cos();
    let sin_an = an.sin();
    let cos_bn = bn.cos();
    let sin_bn = bn.sin();

    let qx = outer_radius * cos_bn - inner_radius * cos_an;
    let qy = outer_radius * sin_bn - inner_radius * sin_an;

    let d = (p[0] * sin_bn - p[1] * cos_bn).max(0.0);
    let len = (qx * qx + qy * qy).sqrt();

    ((p[0] * cos_bn + p[1] * sin_bn) * len - inner_radius * len) / (outer_radius - inner_radius)
        + d * (d * d / len).min(len)
}

/// Signed distance to regular polygon.
#[inline]
pub fn sd_polygon(p: [f32; 2], radius: f32, sides: u32) -> f32 {
    let n = sides as f32;
    let k = std::f32::consts::PI / n;
    let an = p[0].atan2(p[1]);
    let bn = ((an / k + 0.5).floor() * 2.0 + 1.0) * k;

    let d = (p[0] * bn.cos() + p[1] * bn.sin()).min(0.0);
    let r = radius;

    d + (p[0] * bn.sin() - p[1] * bn.cos()).abs() - r * k.cos()
}

/// Signed distance to annulus (ring).
#[inline]
pub fn sd_annulus(p: [f32; 2], outer_radius: f32, inner_radius: f32) -> f32 {
    let d = (p[0] * p[0] + p[1] * p[1]).sqrt();
    (d - inner_radius).abs() - (outer_radius - inner_radius)
}

/// Signed distance to cross.
#[inline]
pub fn sd_cross(p: [f32; 2], half_size: [f32; 2], thickness: f32) -> f32 {
    let q = [p[0].abs(), p[1].abs()];
    let w = [half_size[0], thickness / 2.0];
    let _u = [thickness / 2.0, half_size[1]];

    f32::min(
        f32::max(f32::max(q[0] - w[0], q[1] - w[1]), 0.0),
        f32::max(f32::min(q[0] - w[0], q[1] - w[1]), 0.0),
    )
}

/// Boolean operations on SDFs.
pub mod boolean {
    /// Union of two SDFs.
    #[inline]
    pub fn union(d1: f32, d2: f32) -> f32 {
        d1.min(d2)
    }

    /// Intersection of two SDFs.
    #[inline]
    pub fn intersection(d1: f32, d2: f32) -> f32 {
        d1.max(d2)
    }

    /// Subtraction (d1 - d2).
    #[inline]
    pub fn subtraction(d1: f32, d2: f32) -> f32 {
        d1.max(-d2)
    }

    /// Smooth union with blend factor k.
    #[inline]
    pub fn smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
        let h = (0.5 + 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
        (1.0 - h) * d2 + h * d1 - k * h * (1.0 - h)
    }

    /// Smooth intersection with blend factor k.
    #[inline]
    pub fn smooth_intersection(d1: f32, d2: f32, k: f32) -> f32 {
        let h = (0.5 - 0.5 * (d2 - d1) / k).clamp(0.0, 1.0);
        (1.0 - h) * d2 + h * d1 + k * h * (1.0 - h)
    }

    /// Smooth subtraction with blend factor k.
    #[inline]
    pub fn smooth_subtraction(d1: f32, d2: f32, k: f32) -> f32 {
        let h = (0.5 - 0.5 * (d1 + d2) / k).clamp(0.0, 1.0);
        (1.0 - h) * d1 + h * (-d2) + k * h * (1.0 - h)
    }
}

/// SDF shape instance for rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SdfShapeInstance {
    /// Position (xy).
    pub position: [f32; 2],
    /// Size (xy).
    pub size: [f32; 2],
    /// Rotation in radians.
    pub rotation: f32,
    /// Shape type (0=rect, 1=circle, 2=rounded_rect, etc).
    pub shape_type: u32,
    /// Shape parameters (depends on type).
    pub params: [f32; 4],
    /// Fill color (RGBA).
    pub fill_color: [f32; 4],
    /// Outline width.
    pub outline_width: f32,
    /// Outline color (RGBA).
    pub outline_color: [f32; 4],
    /// Shadow blur.
    pub shadow_blur: f32,
    /// Shadow color (RGBA).
    pub shadow_color: [f32; 4],
}

/// SDF shape renderer.
pub struct SdfShapeRenderer {
    /// Render pipeline.
    pub pipeline: Option<wgpu::RenderPipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Vertex buffer (fullscreen quad).
    pub vertex_buffer: wgpu::Buffer,
    /// Instance buffer.
    pub instance_buffer: Option<wgpu::Buffer>,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
}

impl SdfShapeRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let _vertices: [[f32; 2]; 6] = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [-1.0, 1.0],
            [-1.0, 1.0],
            [1.0, -1.0],
            [1.0, 1.0],
        ];

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sdf_shape_vertices"),
            size: std::mem::size_of::<[[f32; 2]; 6]>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sdf_shape_uniform"),
            size: std::mem::size_of::<super::SdfUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sdf_shape_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        Self {
            pipeline: None,
            bind_group_layout,
            vertex_buffer,
            instance_buffer: None,
            uniform_buffer,
        }
    }

    /// Render SDF shapes.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        instances: &[SdfShapeInstance],
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        if instances.is_empty() {
            return;
        }

        let Some(pipeline) = &self.pipeline else {
            return;
        };

        let uniform = super::SdfUniform {
            transform: [
                [2.0 / width as f32, 0.0, 0.0, 0.0],
                [0.0, -2.0 / height as f32, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0, 1.0],
            ],
            color: [1.0, 1.0, 1.0, 1.0],
            edge_params: [1.0, 0.0, 0.0, 0.0],
            glow_color: [0.0; 4],
            shadow_params: [0.0; 4],
            resolution: [width as f32, height as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sdf_shape_pass"),
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
        pass.draw(0..6, 0..instances.len() as u32);
    }
}
