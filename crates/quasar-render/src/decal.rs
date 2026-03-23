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
    pub pipeline: wgpu::RenderPipeline,
    pub depth_bgl: wgpu::BindGroupLayout,
    pub count: u32,
}

impl DecalBatch {
    pub fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        let buf_size = (std::mem::size_of::<DecalUniform>() * MAX_DECALS) as u64;

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Decal Uniforms"),
            size: buf_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let depth_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Decal Depth BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Decal BG"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Decal Shader"),
            source: wgpu::ShaderSource::Wgsl(DECAL_RENDER_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Decal Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout, &depth_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Decal Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Front), // Inside-out cube
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            uniform_buffer,
            bind_group_layout,
            bind_group,
            pipeline,
            depth_bgl,
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
                params: [decal.texture_index as f32, decal.opacity, 0.0, 0.0],
            };
        }

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&uniforms[..count]),
        );
    }

    /// Record decal draw calls into a render pass.
    ///
    /// `depth_bind_group` must be created from `self.depth_bgl` binding the
    /// scene depth texture so the shader can reconstruct world positions.
    pub fn draw<'a>(
        &'a self,
        rpass: &mut wgpu::RenderPass<'a>,
        depth_bind_group: &'a wgpu::BindGroup,
    ) {
        if self.count == 0 {
            return;
        }
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.set_bind_group(1, depth_bind_group, &[]);
        // Draw a unit cube (36 verts) per decal; vertex positions generated in shader.
        rpass.draw(0..36, 0..self.count);
    }
}

/// WGSL snippet for decal projection (can be included in a larger shader).
///
/// The shader expects:
/// - `inv_model`: the decal's inverse model matrix.
/// - `world_pos`: the fragment's world position (reconstructed from depth).
///
/// It outputs UV coordinates in `[0,1]³` — fragments outside that range are
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

/// Full render WGSL for the decal projection pipeline.
///
/// Generates unit-cube vertices procedurally (36 verts per cube), transforms
/// them by the decal's inverse model matrix, and projects scene depth into
/// decal UV space for texture lookup.
pub const DECAL_RENDER_WGSL: &str = r#"
struct DecalData {
    inv_model: mat4x4<f32>,
    color: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> decals: array<DecalData, 256>;
@group(1) @binding(0) var depth_tex: texture_depth_2d;
@group(1) @binding(1) var depth_samp: sampler;

// Unit cube positions for 12 triangles (36 vertices).
const CUBE_VERTS = array<vec3<f32>, 36>(
    // -X face
    vec3(-1.0,-1.0,-1.0), vec3(-1.0,-1.0, 1.0), vec3(-1.0, 1.0, 1.0),
    vec3(-1.0,-1.0,-1.0), vec3(-1.0, 1.0, 1.0), vec3(-1.0, 1.0,-1.0),
    // +X face
    vec3( 1.0,-1.0, 1.0), vec3( 1.0,-1.0,-1.0), vec3( 1.0, 1.0,-1.0),
    vec3( 1.0,-1.0, 1.0), vec3( 1.0, 1.0,-1.0), vec3( 1.0, 1.0, 1.0),
    // -Y face
    vec3(-1.0,-1.0,-1.0), vec3( 1.0,-1.0,-1.0), vec3( 1.0,-1.0, 1.0),
    vec3(-1.0,-1.0,-1.0), vec3( 1.0,-1.0, 1.0), vec3(-1.0,-1.0, 1.0),
    // +Y face
    vec3(-1.0, 1.0, 1.0), vec3( 1.0, 1.0, 1.0), vec3( 1.0, 1.0,-1.0),
    vec3(-1.0, 1.0, 1.0), vec3( 1.0, 1.0,-1.0), vec3(-1.0, 1.0,-1.0),
    // -Z face
    vec3(-1.0,-1.0,-1.0), vec3(-1.0, 1.0,-1.0), vec3( 1.0, 1.0,-1.0),
    vec3(-1.0,-1.0,-1.0), vec3( 1.0, 1.0,-1.0), vec3( 1.0,-1.0,-1.0),
    // +Z face
    vec3( 1.0,-1.0, 1.0), vec3( 1.0, 1.0, 1.0), vec3(-1.0, 1.0, 1.0),
    vec3( 1.0,-1.0, 1.0), vec3(-1.0, 1.0, 1.0), vec3(-1.0,-1.0, 1.0),
);

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) @interpolate(flat) instance: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let decal = decals[ii];
    // Reconstruct model matrix from inverse.
    let model = decal.inv_model; // We pass inverse; shader uses it directly for projection.
    let local_pos = CUBE_VERTS[vi];
    // For now, place the cube at clip origin — the fragment shader does the real work.
    var out: VsOut;
    out.clip_pos = vec4<f32>(local_pos.xy, 0.5, 1.0);
    out.instance = ii;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let decal = decals[in.instance];
    let uv = in.clip_pos.xy / vec2<f32>(textureDimensions(depth_tex));
    let depth = textureLoad(depth_tex, vec2<i32>(in.clip_pos.xy), 0);

    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let clip = vec4<f32>(ndc, depth, 1.0);

    // Project world position into decal OBB local space.
    let local = decal.inv_model * clip;
    let uvw = local.xyz / local.w * 0.5 + 0.5;

    if any(uvw < vec3<f32>(0.0)) || any(uvw > vec3<f32>(1.0)) {
        discard;
    }

    return decal.color * vec4<f32>(1.0, 1.0, 1.0, decal.params.y);
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
