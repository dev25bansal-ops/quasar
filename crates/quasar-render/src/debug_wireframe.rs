//! Debug wireframe renderer for physics visualization.
//!
//! Provides GPU-based line rendering for debug visualization:
//! - Physics collider wireframes (boxes, spheres, capsules, cylinders)
//! - AABB visualization
//! - Joint connections
//! - Contact points
//! - Custom debug lines
//!
//! # Example
//!
//! ```ignore
//! use quasar_render::debug_wireframe::{DebugWireframeRenderer, DebugLine};
//! use quasar_core::debug_draw::{DebugDraw, DebugDrawConfig};
//!
//! let mut debug_renderer = DebugWireframeRenderer::new(device);
//!
//! // Generate lines from any system implementing DebugDraw:
//! let lines = physics_world.generate_debug_lines(&DebugDrawConfig::default());
//! debug_renderer.update(device, queue, &lines);
//! debug_renderer.render(encoder, view, depth_view, camera_matrix);
//! ```

use bytemuck::{Pod, Zeroable};
use std::mem;

// Re-export DebugLine from core so consumers can use it from this module.
pub use quasar_core::debug_draw::DebugLine;

pub const MAX_DEBUG_LINES: usize = 65536;

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct DebugLineVertex {
    pub position: [f32; 4],
    pub color: [f32; 4],
}

impl DebugLineVertex {
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct DebugWireframeRenderer {
    vertex_buffer: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    line_count: usize,
    enabled: bool,
    depth_test: bool,
}

impl DebugWireframeRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Debug Wireframe Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("debug_wireframe.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Debug Wireframe Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..64,
            }],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug Wireframe Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[DebugLineVertex::buffer_layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::GreaterEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Debug Wireframe Vertex Buffer"),
            size: (MAX_DEBUG_LINES * 2 * mem::size_of::<DebugLineVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            vertex_buffer,
            pipeline,
            line_count: 0,
            enabled: true,
            depth_test: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_depth_test(&mut self, enabled: bool) {
        self.depth_test = enabled;
    }

    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, lines: &[DebugLine]) {
        if !self.enabled || lines.is_empty() {
            self.line_count = 0;
            return;
        }

        let line_count = lines.len().min(MAX_DEBUG_LINES);
        let mut vertices = Vec::with_capacity(line_count * 2);

        for line in &lines[..line_count] {
            vertices.push(DebugLineVertex {
                position: [line.start[0], line.start[1], line.start[2], 1.0],
                color: line.color,
            });
            vertices.push(DebugLineVertex {
                position: [line.end[0], line.end[1], line.end[2], 1.0],
                color: line.color,
            });
        }

        if vertices.len() > self.line_count * 2 {
            self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Debug Wireframe Vertex Buffer"),
                size: (vertices.len() * mem::size_of::<DebugLineVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.line_count = line_count;
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: Option<&wgpu::TextureView>,
        view_proj: [[f32; 4]; 4],
    ) {
        if !self.enabled || self.line_count == 0 {
            return;
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Debug Wireframe Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_view.map(|dv| wgpu::RenderPassDepthStencilAttachment {
                view: dv,
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

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX,
            0,
            bytemuck::cast_slice(&view_proj),
        );
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..(self.line_count * 2) as u32, 0..1);
    }

    pub fn line_count(&self) -> usize {
        self.line_count
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

pub struct DebugWireframeConfig {
    pub draw_colliders: bool,
    pub draw_aabbs: bool,
    pub draw_joints: bool,
    pub draw_contacts: bool,
    pub draw_triggers: bool,
    pub collider_color: [f32; 4],
    pub aabb_color: [f32; 4],
    pub joint_color: [f32; 4],
    pub contact_color: [f32; 4],
    pub trigger_color: [f32; 4],
    pub line_width: f32,
}

impl Default for DebugWireframeConfig {
    fn default() -> Self {
        Self {
            draw_colliders: true,
            draw_aabbs: false,
            draw_joints: true,
            draw_contacts: true,
            draw_triggers: true,
            collider_color: [0.0, 1.0, 0.0, 1.0],
            aabb_color: [1.0, 1.0, 0.0, 0.5],
            joint_color: [0.0, 0.5, 1.0, 1.0],
            contact_color: [1.0, 0.0, 0.0, 1.0],
            trigger_color: [1.0, 0.0, 1.0, 0.6],
            line_width: 1.0,
        }
    }
}

pub struct DebugLineBuilder {
    lines: Vec<DebugLine>,
}

impl DebugLineBuilder {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn line(mut self, start: [f32; 3], end: [f32; 3], color: [f32; 4]) -> Self {
        self.lines.push(DebugLine::new(start, end, color));
        self
    }

    pub fn box_wireframe(
        mut self,
        center: [f32; 3],
        half_extents: [f32; 3],
        color: [f32; 4],
    ) -> Self {
        let [cx, cy, cz] = center;
        let [hx, hy, hz] = half_extents;

        let corners = [
            [cx - hx, cy - hy, cz - hz],
            [cx + hx, cy - hy, cz - hz],
            [cx + hx, cy + hy, cz - hz],
            [cx - hx, cy + hy, cz - hz],
            [cx - hx, cy - hy, cz + hz],
            [cx + hx, cy - hy, cz + hz],
            [cx + hx, cy + hy, cz + hz],
            [cx - hx, cy + hy, cz + hz],
        ];

        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];

        for (a, b) in edges {
            self.lines
                .push(DebugLine::new(corners[a], corners[b], color));
        }

        self
    }

    pub fn sphere_wireframe(
        mut self,
        center: [f32; 3],
        radius: f32,
        color: [f32; 4],
        segments: u32,
    ) -> Self {
        let step = std::f32::consts::TAU / segments as f32;

        for i in 0..segments {
            let a0 = i as f32 * step;
            let a1 = (i + 1) as f32 * step;

            self.lines.push(DebugLine::new(
                [
                    center[0] + radius * a0.cos(),
                    center[1],
                    center[2] + radius * a0.sin(),
                ],
                [
                    center[0] + radius * a1.cos(),
                    center[1],
                    center[2] + radius * a1.sin(),
                ],
                color,
            ));

            self.lines.push(DebugLine::new(
                [
                    center[0] + radius * a0.cos(),
                    center[1] + radius * a0.sin(),
                    center[2],
                ],
                [
                    center[0] + radius * a1.cos(),
                    center[1] + radius * a1.sin(),
                    center[2],
                ],
                color,
            ));

            self.lines.push(DebugLine::new(
                [
                    center[0],
                    center[1] + radius * a0.cos(),
                    center[2] + radius * a0.sin(),
                ],
                [
                    center[0],
                    center[1] + radius * a1.cos(),
                    center[2] + radius * a1.sin(),
                ],
                color,
            ));
        }

        self
    }

    pub fn cross(mut self, center: [f32; 3], size: f32, color: [f32; 4]) -> Self {
        let s = size / 2.0;
        self.lines.push(DebugLine::new(
            [center[0] - s, center[1], center[2]],
            [center[0] + s, center[1], center[2]],
            color,
        ));
        self.lines.push(DebugLine::new(
            [center[0], center[1] - s, center[2]],
            [center[0], center[1] + s, center[2]],
            color,
        ));
        self.lines.push(DebugLine::new(
            [center[0], center[1], center[2] - s],
            [center[0], center[1], center[2] + s],
            color,
        ));
        self
    }

    pub fn arrow(mut self, start: [f32; 3], end: [f32; 3], color: [f32; 4]) -> Self {
        self.lines.push(DebugLine::new(start, end, color));

        let dir = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len > 0.0 {
            let ndir = [dir[0] / len, dir[1] / len, dir[2] / len];
            let head_len = len * 0.2;

            let perp = if ndir[0].abs() > 0.5 {
                [0.0, 1.0, 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };

            let head1 = [
                end[0] - head_len * (ndir[0] + perp[0] * 0.5),
                end[1] - head_len * (ndir[1] + perp[1] * 0.5),
                end[2] - head_len * (ndir[2] + perp[2] * 0.5),
            ];
            let head2 = [
                end[0] - head_len * (ndir[0] - perp[0] * 0.5),
                end[1] - head_len * (ndir[1] - perp[1] * 0.5),
                end[2] - head_len * (ndir[2] - perp[2] * 0.5),
            ];

            self.lines.push(DebugLine::new(end, head1, color));
            self.lines.push(DebugLine::new(end, head2, color));
        }

        self
    }

    pub fn build(self) -> Vec<DebugLine> {
        self.lines
    }
}

impl Default for DebugLineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
