//! Vertex layout — defines how vertex data is fed to the GPU.

use bytemuck::{Pod, Zeroable};

/// A single vertex with position, normal, UV, and color.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct Vertex {
    /// 3D position.
    pub position: [f32; 3],
    /// Surface normal (unit vector).
    pub normal: [f32; 3],
    /// Texture coordinates.
    pub uv: [f32; 2],
    /// Vertex color (RGBA).
    pub color: [f32; 4],
}

impl Vertex {
    /// The wgpu vertex buffer layout for this vertex type.
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: float32x3
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // normal: float32x3
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // uv: float32x2
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color: float32x4
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }

    /// Create a vertex with position and color (normal/uv zeroed).
    pub fn pos_color(position: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position,
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
            color,
        }
    }

    /// Create a vertex with position, normal, and color.
    pub fn pos_normal_color(position: [f32; 3], normal: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position,
            normal,
            uv: [0.0, 0.0],
            color,
        }
    }
}
