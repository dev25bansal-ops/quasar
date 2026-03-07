//! Projected Decal System.
//!
//! Decals are rendered as screen-space projections using a unit cube.  Each
//! decal has a world-space oriented bounding box (OBB) — fragments inside the
//! box sample the decal texture and composite onto the G-Buffer (or onto the
//! forward color target when deferred is not active).
//!
//! The system provides:
//! - `Decal` — ECS component describing a projected decal.
//! - `DecalBatch` — collects visible decals and provides a draw call.

use bytemuck::Zeroable;
use glam::{Mat4, Vec3, Vec4};

/// Maximum decals drawn per frame.
pub const MAX_DECALS: usize = 256;

/// ECS component for a projected decal.
#[derive(Debug, Clone)]
pub struct Decal {
    /// World position of the decal center.
    pub position: Vec3,
    /// World-space half-extents of the projection box.
    pub half_extents: Vec3,
    /// Forward direction the decal is projected along (typically -Y or -Z).
    pub direction: Vec3,
    /// Up vector for orientation.
    pub up: Vec3,
    /// Albedo tint color (RGBA).
    pub color: [f32; 4],
    /// Index into a texture atlas or array for the decal image.
    pub texture_index: u32,
    /// Opacity multiplier [0..1].
    pub opacity: f32,
    /// Sort order — lower values are drawn first (behind higher values).
    pub sort_order: i32,
}

impl Decal {
    pub fn new(position: Vec3, half_extents: Vec3) -> Self {
        Self {
            position,
            half_extents,
            direction: -Vec3::Y,
            up: Vec3::Z,
            color: [1.0, 1.0, 1.0, 1.0],
            texture_index: 0,
            opacity: 1.0,
            sort_order: 0,
        }
    }

    /// Compute the model matrix that transforms the unit cube [-1,1]³
    /// into the decal's world-space OBB.
    pub fn model_matrix(&self) -> Mat4 {
        let forward = self.direction.normalize();
        let right = forward.cross(self.up).normalize();
        let corrected_up = right.cross(forward).normalize();

        Mat4::from_cols(
            Vec4::new(right.x, right.y, right.z, 0.0) * self.half_extents.x,
            Vec4::new(corrected_up.x, corrected_up.y, corrected_up.z, 0.0) * self.half_extents.y,
            Vec4::new(forward.x, forward.y, forward.z, 0.0) * self.half_extents.z,
            Vec4::new(self.position.x, self.position.y, self.position.z, 1.0),
        )
    }

    /// Inverse of `model_matrix` — used in the shader to transform
    /// world-space fragment positions into decal UV space.
    pub fn inverse_model_matrix(&self) -> Mat4 {
        self.model_matrix().inverse()
    }
}

/// Per-decal GPU uniform (matches WGSL struct).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DecalUniform {
    /// Inverse model matrix (4×4).
    pub inv_model: [[f32; 4]; 4],
    /// rgba color/tint.
    pub color: [f32; 4],
    /// x = texture_index, y = opacity, zw = padding.
    pub params: [f32; 4],
}

/// Collects decals for the current frame and uploads their uniforms.
pub struct DecalBatch {
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub count: u32,
}

impl DecalBatch {
    pub fn new(device: &wgpu::Device) -> Self {
        let buf_size = (std::mem::size_of::<DecalUniform>() * MAX_DECALS) as u64;

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Decal Uniforms"),
            size: buf_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Decal BGL"),
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Decal BG"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Self {
            uniform_buffer,
            bind_group_layout,
            bind_group,
            count: 0,
        }
    }

    /// Upload decal data for the current frame.
    ///
    /// Decals are sorted by `sort_order` before upload.
    pub fn update(&mut self, queue: &wgpu::Queue, decals: &mut [&Decal]) {
        decals.sort_by_key(|d| d.sort_order);

        let count = decals.len().min(MAX_DECALS);
        self.count = count as u32;

        let mut uniforms = vec![DecalUniform::zeroed(); MAX_DECALS];
        for (i, decal) in decals.iter().take(count).enumerate() {
            let inv = decal.inverse_model_matrix();
            uniforms[i] = DecalUniform {
                inv_model: inv.to_cols_array_2d(),
                color: decal.color,
                params: [
                    decal.texture_index as f32,
                    decal.opacity,
                    0.0,
                    0.0,
                ],
            };
        }

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&uniforms[..count]),
        );
    }
}

/// WGSL snippet for decal projection (can be included in a larger shader).
///
/// The shader expects:
/// - `inv_model`: the decal's inverse model matrix.
/// - `world_pos`: the fragment's world position (reconstructed from depth).
///
/// It outputs UV coordinates in [0,1]³ — fragments outside that range are
/// discarded (the fragment is not inside the decal OBB).
pub const DECAL_PROJECTION_WGSL: &str = r#"
fn decal_project(inv_model: mat4x4<f32>, world_pos: vec3<f32>) -> vec3<f32> {
    let local = inv_model * vec4<f32>(world_pos, 1.0);
    return local.xyz * 0.5 + 0.5; // map [-1,1] → [0,1]
}

fn decal_inside(uvw: vec3<f32>) -> bool {
    return all(uvw >= vec3<f32>(0.0)) && all(uvw <= vec3<f32>(1.0));
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decal_model_matrix_invertible() {
        let d = Decal::new(Vec3::new(1.0, 2.0, 3.0), Vec3::splat(2.0));
        let m = d.model_matrix();
        let inv = d.inverse_model_matrix();
        let identity = m * inv;
        // Should be approximately identity.
        for i in 0..4 {
            for j in 0..4 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (identity.col(j)[i] - expected).abs() < 1e-4,
                    "identity[{}][{}] = {} (expected {})",
                    i,
                    j,
                    identity.col(j)[i],
                    expected
                );
            }
        }
    }
}
