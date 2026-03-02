//! Material system — describes the visual properties of a surface.
//!
//! Each material references a texture bind group and stores PBR-lite properties.
//! The material uniform is uploaded to the GPU and accessed in the fragment shader.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Per-entity material override (ECS component).
///
/// Attach this to an entity alongside [`MeshShape`](super::mesh::MeshShape)
/// to give it a custom surface appearance.  Entities without this component
/// fall back to the renderer's default white material.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MaterialOverride {
    /// Base color (RGB 0–1).
    pub base_color: [f32; 3],
    /// Roughness (0 = mirror, 1 = fully diffuse).
    pub roughness: f32,
    /// Metallic (0 = dielectric, 1 = metallic).
    pub metallic: f32,
    /// Emissive strength.
    pub emissive: f32,
}

impl Default for MaterialOverride {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            emissive: 0.0,
        }
    }
}

impl MaterialOverride {
    /// Convert to GPU-ready [`MaterialUniform`].
    pub fn to_uniform(&self) -> MaterialUniform {
        MaterialUniform {
            base_color: [
                self.base_color[0],
                self.base_color[1],
                self.base_color[2],
                1.0,
            ],
            roughness_metallic: [self.roughness, self.metallic],
            emissive: self.emissive,
            _pad: 0.0,
        }
    }
}

/// GPU-side material uniform data.
///
/// Uploaded via a uniform buffer and accessed in the fragment shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MaterialUniform {
    /// Base color multiplied with texture. RGBA.
    pub base_color: [f32; 4],
    /// Roughness (0 = mirror, 1 = diffuse). Stored in x, metallic in y.
    pub roughness_metallic: [f32; 2],
    /// Emissive strength.
    pub emissive: f32,
    /// Padding to align to 16 bytes.
    pub _pad: f32,
}

impl Default for MaterialUniform {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            roughness_metallic: [0.5, 0.0],
            emissive: 0.0,
            _pad: 0.0,
        }
    }
}

/// A renderable material combining a texture reference and PBR-lite properties.
pub struct Material {
    pub name: String,
    pub uniform: MaterialUniform,
    pub buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl Material {
    /// Create a new material with default properties.
    pub fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, name: &str) -> Self {
        Self::from_uniform(device, layout, name, MaterialUniform::default())
    }

    /// Create a material from a [`MaterialOverride`] component.
    pub fn from_override(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        name: &str,
        material_override: &MaterialOverride,
    ) -> Self {
        let mat = Self::from_uniform(device, layout, name, material_override.to_uniform());
        mat.update(queue);
        mat
    }

    /// Create a material from a raw [`MaterialUniform`].
    pub fn from_uniform(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        name: &str,
        uniform: MaterialUniform,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("Material Buffer: {}", name)),
            size: std::mem::size_of::<MaterialUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Material Bind Group: {}", name)),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            name: name.to_string(),
            uniform,
            buffer,
            bind_group,
        }
    }

    /// Set the base color (RGBA).
    pub fn set_base_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.uniform.base_color = [r, g, b, a];
    }

    /// Set roughness (0–1).
    pub fn set_roughness(&mut self, roughness: f32) {
        self.uniform.roughness_metallic[0] = roughness;
    }

    /// Set metallic (0–1).
    pub fn set_metallic(&mut self, metallic: f32) {
        self.uniform.roughness_metallic[1] = metallic;
    }

    /// Set emissive strength.
    pub fn set_emissive(&mut self, emissive: f32) {
        self.uniform.emissive = emissive;
    }

    /// Upload the current uniform data to the GPU.
    pub fn update(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[self.uniform]));
    }

    /// Create the bind group layout for material uniforms.
    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Material Bind Group Layout"),
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
        })
    }
}

/// A light in the scene.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LightUniform {
    /// Direction for directional light (normalized xyz, w=0) or position (w=1).
    pub direction: [f32; 4],
    /// Light color and intensity (RGB, A = intensity).
    pub color: [f32; 4],
    /// Ambient color (RGB, A = ambient strength).
    pub ambient: [f32; 4],
}

impl Default for LightUniform {
    fn default() -> Self {
        Self {
            // Sun-like directional light from upper-left.
            direction: [-0.5, -1.0, -0.3, 0.0],
            color: [1.0, 0.95, 0.9, 1.0],
            ambient: [0.15, 0.15, 0.2, 1.0],
        }
    }
}
