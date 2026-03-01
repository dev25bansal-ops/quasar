//! Mesh — GPU-side vertex and index buffers.

use wgpu::util::DeviceExt;

use super::vertex::Vertex;

/// Raw mesh data on the CPU side (before upload to GPU).
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Generate a unit cube centered at the origin (side length = 1).
    ///
    /// Each face has a distinct color for easy visual debugging.
    pub fn cube() -> Self {
        // 6 faces × 4 vertices = 24 vertices, 6 faces × 6 indices = 36 indices.
        let vertices = vec![
            // Front face (blue) — +Z
            Vertex::pos_normal_color([-0.5, -0.5,  0.5], [ 0.0,  0.0,  1.0], [0.2, 0.4, 0.9, 1.0]),
            Vertex::pos_normal_color([ 0.5, -0.5,  0.5], [ 0.0,  0.0,  1.0], [0.2, 0.4, 0.9, 1.0]),
            Vertex::pos_normal_color([ 0.5,  0.5,  0.5], [ 0.0,  0.0,  1.0], [0.3, 0.5, 1.0, 1.0]),
            Vertex::pos_normal_color([-0.5,  0.5,  0.5], [ 0.0,  0.0,  1.0], [0.3, 0.5, 1.0, 1.0]),
            // Back face (red) — -Z
            Vertex::pos_normal_color([ 0.5, -0.5, -0.5], [ 0.0,  0.0, -1.0], [0.9, 0.2, 0.2, 1.0]),
            Vertex::pos_normal_color([-0.5, -0.5, -0.5], [ 0.0,  0.0, -1.0], [0.9, 0.2, 0.2, 1.0]),
            Vertex::pos_normal_color([-0.5,  0.5, -0.5], [ 0.0,  0.0, -1.0], [1.0, 0.3, 0.3, 1.0]),
            Vertex::pos_normal_color([ 0.5,  0.5, -0.5], [ 0.0,  0.0, -1.0], [1.0, 0.3, 0.3, 1.0]),
            // Top face (green) — +Y
            Vertex::pos_normal_color([-0.5,  0.5,  0.5], [ 0.0,  1.0,  0.0], [0.2, 0.9, 0.3, 1.0]),
            Vertex::pos_normal_color([ 0.5,  0.5,  0.5], [ 0.0,  1.0,  0.0], [0.2, 0.9, 0.3, 1.0]),
            Vertex::pos_normal_color([ 0.5,  0.5, -0.5], [ 0.0,  1.0,  0.0], [0.3, 1.0, 0.4, 1.0]),
            Vertex::pos_normal_color([-0.5,  0.5, -0.5], [ 0.0,  1.0,  0.0], [0.3, 1.0, 0.4, 1.0]),
            // Bottom face (yellow) — -Y
            Vertex::pos_normal_color([-0.5, -0.5, -0.5], [ 0.0, -1.0,  0.0], [0.9, 0.9, 0.2, 1.0]),
            Vertex::pos_normal_color([ 0.5, -0.5, -0.5], [ 0.0, -1.0,  0.0], [0.9, 0.9, 0.2, 1.0]),
            Vertex::pos_normal_color([ 0.5, -0.5,  0.5], [ 0.0, -1.0,  0.0], [1.0, 1.0, 0.3, 1.0]),
            Vertex::pos_normal_color([-0.5, -0.5,  0.5], [ 0.0, -1.0,  0.0], [1.0, 1.0, 0.3, 1.0]),
            // Right face (magenta) — +X
            Vertex::pos_normal_color([ 0.5, -0.5,  0.5], [ 1.0,  0.0,  0.0], [0.9, 0.2, 0.9, 1.0]),
            Vertex::pos_normal_color([ 0.5, -0.5, -0.5], [ 1.0,  0.0,  0.0], [0.9, 0.2, 0.9, 1.0]),
            Vertex::pos_normal_color([ 0.5,  0.5, -0.5], [ 1.0,  0.0,  0.0], [1.0, 0.3, 1.0, 1.0]),
            Vertex::pos_normal_color([ 0.5,  0.5,  0.5], [ 1.0,  0.0,  0.0], [1.0, 0.3, 1.0, 1.0]),
            // Left face (cyan) — -X
            Vertex::pos_normal_color([-0.5, -0.5, -0.5], [-1.0,  0.0,  0.0], [0.2, 0.9, 0.9, 1.0]),
            Vertex::pos_normal_color([-0.5, -0.5,  0.5], [-1.0,  0.0,  0.0], [0.2, 0.9, 0.9, 1.0]),
            Vertex::pos_normal_color([-0.5,  0.5,  0.5], [-1.0,  0.0,  0.0], [0.3, 1.0, 1.0, 1.0]),
            Vertex::pos_normal_color([-0.5,  0.5, -0.5], [-1.0,  0.0,  0.0], [0.3, 1.0, 1.0, 1.0]),
        ];

        let indices = vec![
            0,  1,  2,  0,  2,  3,   // front
            4,  5,  6,  4,  6,  7,   // back
            8,  9, 10,  8, 10, 11,   // top
            12, 13, 14, 12, 14, 15,  // bottom
            16, 17, 18, 16, 18, 19,  // right
            20, 21, 22, 20, 22, 23,  // left
        ];

        Self { vertices, indices }
    }

    /// Generate a flat grid/plane on the XZ plane.
    pub fn plane(size: f32) -> Self {
        let half = size / 2.0;
        let color = [0.5, 0.5, 0.5, 1.0];
        let normal = [0.0, 1.0, 0.0];

        let vertices = vec![
            Vertex::pos_normal_color([-half, 0.0, -half], normal, color),
            Vertex::pos_normal_color([ half, 0.0, -half], normal, color),
            Vertex::pos_normal_color([ half, 0.0,  half], normal, color),
            Vertex::pos_normal_color([-half, 0.0,  half], normal, color),
        ];

        let indices = vec![0, 1, 2, 0, 2, 3];

        Self { vertices, indices }
    }
}

/// A mesh uploaded to the GPU — ready for drawing.
pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl Mesh {
    /// Upload CPU-side mesh data to the GPU.
    pub fn from_data(device: &wgpu::Device, data: &MeshData) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: data.indices.len() as u32,
        }
    }
}
