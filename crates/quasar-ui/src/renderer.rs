//! GPU renderer for the retained-mode UI — draws textured/colored quads
//! using wgpu.

use crate::layout::LayoutSolver;
use crate::style::Color;
use crate::widget::{UiTree, WidgetId};

// ── Vertex layout ──────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl UiVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// ── Render pass ────────────────────────────────────────────────

/// A simple wgpu-based UI render pass that draws the laid-out widget tree
/// as colored quads.
pub struct UiRenderPass {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    max_quads: usize,
}

const MAX_QUADS: usize = 4096;
const VERTICES_PER_QUAD: usize = 4;
const INDICES_PER_QUAD: usize = 6;

impl UiRenderPass {
    /// Create a new UI render pass.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui_shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui_pipeline_layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[UiVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui_vertex_buffer"),
            size: (MAX_QUADS * VERTICES_PER_QUAD * std::mem::size_of::<UiVertex>())
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui_index_buffer"),
            size: (MAX_QUADS * INDICES_PER_QUAD * std::mem::size_of::<u32>())
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            max_quads: MAX_QUADS,
        }
    }

    /// Build vertex/index data from the widget tree and its computed layout,
    /// upload to GPU, and record draw commands into the render pass.
    pub fn draw<'a>(
        &'a self,
        queue: &wgpu::Queue,
        rpass: &mut wgpu::RenderPass<'a>,
        tree: &UiTree,
        layout: &LayoutSolver,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        let mut vertices: Vec<UiVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        for &root in tree.roots() {
            self.build_quad(
                tree,
                layout,
                root,
                viewport_width,
                viewport_height,
                &mut vertices,
                &mut indices,
            );
        }

        if indices.is_empty() {
            return;
        }

        let quad_count = indices.len() / INDICES_PER_QUAD;
        let quad_count = quad_count.min(self.max_quads);
        let index_count = quad_count * INDICES_PER_QUAD;
        let vertex_count = quad_count * VERTICES_PER_QUAD;

        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices[..vertex_count]),
        );
        queue.write_buffer(
            &self.index_buffer,
            0,
            bytemuck::cast_slice(&indices[..index_count]),
        );

        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..index_count as u32, 0, 0..1);
    }

    fn build_quad(
        &self,
        tree: &UiTree,
        layout: &LayoutSolver,
        id: WidgetId,
        vw: f32,
        vh: f32,
        vertices: &mut Vec<UiVertex>,
        indices: &mut Vec<u32>,
    ) {
        let node = match tree.get(id) {
            Some(n) => n,
            None => return,
        };

        if !node.style.visible {
            return;
        }

        if let Some(rect) = layout.rect(id) {
            if vertices.len() / VERTICES_PER_QUAD >= self.max_quads {
                return;
            }

            let color = color_to_array(&node.style.background_color);

            // Convert screen-space rect to NDC [-1, 1].
            let x0 = rect.x / vw * 2.0 - 1.0;
            let y0 = 1.0 - rect.y / vh * 2.0;
            let x1 = (rect.x + rect.width) / vw * 2.0 - 1.0;
            let y1 = 1.0 - (rect.y + rect.height) / vh * 2.0;

            let base = vertices.len() as u32;
            vertices.push(UiVertex { position: [x0, y0], uv: [0.0, 0.0], color });
            vertices.push(UiVertex { position: [x1, y0], uv: [1.0, 0.0], color });
            vertices.push(UiVertex { position: [x1, y1], uv: [1.0, 1.0], color });
            vertices.push(UiVertex { position: [x0, y1], uv: [0.0, 1.0], color });
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        // Recurse into children.
        let children: Vec<WidgetId> = node.children.clone();
        for child in children {
            self.build_quad(tree, layout, child, vw, vh, vertices, indices);
        }
    }
}

fn color_to_array(c: &Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

// ── Inline WGSL shader ────────────────────────────────────────

const UI_SHADER_WGSL: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;
