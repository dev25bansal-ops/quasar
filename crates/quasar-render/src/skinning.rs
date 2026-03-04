//! GPU skinning — skeleton animation with bone matrices.
//!
//! Provides skeletal animation support with bone matrices uploaded to GPU
//! and vertex skinning weights/indices for real-time deformation.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

pub const MAX_BONES: usize = 256;
pub const MAX_BONE_INFLUENCES: usize = 4;

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub joint_indices: [u32; MAX_BONE_INFLUENCES],
    pub joint_weights: [f32; MAX_BONE_INFLUENCES],
    pub tangent: [f32; 3],
    pub _pad: f32,
}

impl SkinnedVertex {
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SkinnedVertex>() as wgpu::BufferAddress,
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
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub bone_matrices: Vec<glam::Mat4>,
    pub inverse_bind_matrices: Vec<glam::Mat4>,
}

impl Skeleton {
    pub fn new(bone_count: usize) -> Self {
        Self {
            bones: Vec::with_capacity(bone_count),
            bone_matrices: vec![glam::Mat4::IDENTITY; bone_count],
            inverse_bind_matrices: vec![glam::Mat4::IDENTITY; bone_count],
        }
    }

    pub fn add_bone(
        &mut self,
        name: String,
        parent_index: Option<usize>,
        inverse_bind: glam::Mat4,
    ) -> usize {
        let index = self.bones.len();
        self.bones.push(Bone {
            name,
            parent_index,
            local_transform: glam::Mat4::IDENTITY,
        });
        if index < self.inverse_bind_matrices.len() {
            self.inverse_bind_matrices[index] = inverse_bind;
        } else {
            self.inverse_bind_matrices.push(inverse_bind);
        }
        index
    }

    pub fn compute_bone_matrices(&mut self) {
        for (i, bone) in self.bones.iter().enumerate() {
            let parent_matrix = bone
                .parent_index
                .map(|p| self.bone_matrices[p])
                .unwrap_or(glam::Mat4::IDENTITY);

            let global_matrix = parent_matrix * bone.local_transform;
            self.bone_matrices[i] = global_matrix;
        }
    }

    pub fn get_skinning_matrices(&self) -> Vec<glam::Mat4> {
        self.bone_matrices
            .iter()
            .zip(self.inverse_bind_matrices.iter())
            .map(|(global, inverse_bind)| *global * *inverse_bind)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub parent_index: Option<usize>,
    pub local_transform: glam::Mat4,
}

#[derive(Debug, Clone, Copy)]
pub struct SkeletalAnimationState {
    pub clip_index: usize,
    pub time: f32,
    pub speed: f32,
    pub looping: bool,
}

impl Default for SkeletalAnimationState {
    fn default() -> Self {
        Self {
            clip_index: 0,
            time: 0.0,
            speed: 1.0,
            looping: true,
        }
    }
}

pub struct BoneMatricesBuffer {
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl BoneMatricesBuffer {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bone Matrices Buffer"),
            size: (MAX_BONES * std::mem::size_of::<glam::Mat4>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bone Matrices Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bone Matrices Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            buffer,
            bind_group,
            bind_group_layout,
        }
    }

    pub fn update(&self, queue: &wgpu::Queue, matrices: &[glam::Mat4]) {
        let mut data = Vec::with_capacity(MAX_BONES * 16);
        for matrix in matrices.iter().take(MAX_BONES) {
            data.extend_from_slice(&matrix.to_cols_array());
        }
        while data.len() < MAX_BONES * 16 {
            data.extend_from_slice(&glam::Mat4::IDENTITY.to_cols_array());
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&data));
    }
}

pub struct SkinnedMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl SkinnedMesh {
    pub fn new(device: &wgpu::Device, vertices: &[SkinnedVertex], indices: &[u32]) -> Self {
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Skinned Mesh Vertex Buffer"),
            size: (vertices.len() * std::mem::size_of::<SkinnedVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Skinned Mesh Index Buffer"),
            size: (indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
        }
    }

    pub fn update_vertices(&self, queue: &wgpu::Queue, vertices: &[SkinnedVertex]) {
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(vertices));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_bone_matrices() {
        let mut skeleton = Skeleton::new(2);
        skeleton.add_bone("root".to_string(), None, glam::Mat4::IDENTITY);
        skeleton.add_bone("child".to_string(), Some(0), glam::Mat4::IDENTITY);

        skeleton.bones[0].local_transform =
            glam::Mat4::from_translation(glam::Vec3::new(1.0, 0.0, 0.0));
        skeleton.bones[1].local_transform =
            glam::Mat4::from_translation(glam::Vec3::new(0.0, 1.0, 0.0));

        skeleton.compute_bone_matrices();

        assert!(skeleton.bone_matrices[0].is_finite());
        assert!(skeleton.bone_matrices[1].is_finite());
    }

    #[test]
    fn skinning_matrices() {
        let mut skeleton = Skeleton::new(1);
        skeleton.add_bone("root".to_string(), None, glam::Mat4::IDENTITY);
        skeleton.compute_bone_matrices();

        let matrices = skeleton.get_skinning_matrices();
        assert_eq!(matrices.len(), 1);
    }
}
