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

    /// Generate a UV sphere.
    ///
    /// `sectors` = longitude slices, `stacks` = latitude slices.
    pub fn sphere(radius: f32, sectors: u32, stacks: u32) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let pi = std::f32::consts::PI;
        let two_pi = 2.0 * pi;

        for i in 0..=stacks {
            let stack_angle = pi / 2.0 - (i as f32) * pi / (stacks as f32); // from π/2 to -π/2
            let xy = radius * stack_angle.cos();
            let y = radius * stack_angle.sin();

            for j in 0..=sectors {
                let sector_angle = (j as f32) * two_pi / (sectors as f32);
                let x = xy * sector_angle.cos();
                let z = xy * sector_angle.sin();

                let nx = x / radius;
                let ny = y / radius;
                let nz = z / radius;

                let u = j as f32 / sectors as f32;
                let v = i as f32 / stacks as f32;

                // Color based on normal for nice visual.
                let color = [
                    (nx * 0.5 + 0.5).max(0.2),
                    (ny * 0.5 + 0.5).max(0.2),
                    (nz * 0.5 + 0.5).max(0.2),
                    1.0,
                ];

                vertices.push(Vertex {
                    position: [x, y, z],
                    normal: [nx, ny, nz],
                    uv: [u, v],
                    color,
                });
            }
        }

        // Generate indices.
        for i in 0..stacks {
            for j in 0..sectors {
                let first = i * (sectors + 1) + j;
                let second = first + sectors + 1;

                if i != 0 {
                    indices.push(first);
                    indices.push(second);
                    indices.push(first + 1);
                }
                if i != stacks - 1 {
                    indices.push(first + 1);
                    indices.push(second);
                    indices.push(second + 1);
                }
            }
        }

        Self { vertices, indices }
    }

    /// Generate a cylinder along the Y axis.
    pub fn cylinder(radius: f32, height: f32, segments: u32) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let half = height / 2.0;
        let two_pi = 2.0 * std::f32::consts::PI;

        // Side vertices: two rings.
        for i in 0..=segments {
            let angle = (i as f32) * two_pi / (segments as f32);
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            let nx = angle.cos();
            let nz = angle.sin();
            let u = i as f32 / segments as f32;

            // Top ring
            vertices.push(Vertex {
                position: [x, half, z],
                normal: [nx, 0.0, nz],
                uv: [u, 0.0],
                color: [0.8, 0.6, 0.3, 1.0],
            });
            // Bottom ring
            vertices.push(Vertex {
                position: [x, -half, z],
                normal: [nx, 0.0, nz],
                uv: [u, 1.0],
                color: [0.6, 0.4, 0.2, 1.0],
            });
        }

        // Side indices.
        for i in 0..segments {
            let top_left = i * 2;
            let bottom_left = top_left + 1;
            let top_right = top_left + 2;
            let bottom_right = top_left + 3;

            indices.push(top_left);
            indices.push(bottom_left);
            indices.push(top_right);

            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(bottom_right);
        }

        // Top cap.
        let top_center_idx = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, half, 0.0],
            normal: [0.0, 1.0, 0.0],
            uv: [0.5, 0.5],
            color: [0.9, 0.7, 0.4, 1.0],
        });
        for i in 0..=segments {
            let angle = (i as f32) * two_pi / (segments as f32);
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            vertices.push(Vertex {
                position: [x, half, z],
                normal: [0.0, 1.0, 0.0],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: [0.9, 0.7, 0.4, 1.0],
            });
        }
        for i in 0..segments {
            indices.push(top_center_idx);
            indices.push(top_center_idx + 1 + i);
            indices.push(top_center_idx + 2 + i);
        }

        // Bottom cap.
        let bottom_center_idx = vertices.len() as u32;
        vertices.push(Vertex {
            position: [0.0, -half, 0.0],
            normal: [0.0, -1.0, 0.0],
            uv: [0.5, 0.5],
            color: [0.6, 0.4, 0.2, 1.0],
        });
        for i in 0..=segments {
            let angle = (i as f32) * two_pi / (segments as f32);
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            vertices.push(Vertex {
                position: [x, -half, z],
                normal: [0.0, -1.0, 0.0],
                uv: [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()],
                color: [0.6, 0.4, 0.2, 1.0],
            });
        }
        for i in 0..segments {
            indices.push(bottom_center_idx);
            indices.push(bottom_center_idx + 2 + i);
            indices.push(bottom_center_idx + 1 + i);
        }

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
