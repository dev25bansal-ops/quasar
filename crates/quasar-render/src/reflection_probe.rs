//! Reflection Probes — cubemap capture + blending for local IBL.
//!
//! A `ReflectionProbe` is an ECS component that defines a capture position,
//! an influence volume (AABB), and an optional parallax-correction box.
//! At bake-time each probe renders six faces into a cubemap, which is then
//! prefiltered for roughness levels.  At runtime the fragment shader picks the
//! nearest probe whose influence volume contains the surface point and
//! uses its prefiltered cubemap for specular IBL.

use bytemuck::Zeroable;
use glam::Vec3;

/// Maximum number of active reflection probes.
pub const MAX_REFLECTION_PROBES: usize = 16;

/// Cubemap face resolution (per face).
pub const PROBE_FACE_SIZE: u32 = 128;

/// Number of roughness mip levels for the prefiltered probe cubemap.
pub const PROBE_MIP_LEVELS: u32 = 5;

/// ECS component for a reflection probe.
#[derive(Debug, Clone)]
pub struct ReflectionProbe {
    /// World-space capture position.
    pub position: Vec3,
    /// Half-extents of the influence AABB centered at `position`.
    pub influence_half_extents: Vec3,
    /// Half-extents for parallax-corrected cubemap sampling.
    /// When `None`, the probe is treated as an infinite-distance sky probe.
    pub box_projection_half_extents: Option<Vec3>,
    /// Whether this probe is marked dirty and needs re-bake.
    pub dirty: bool,
    /// Internal index into the cubemap array (set by the probe manager).
    pub(crate) slot: Option<usize>,
}

impl ReflectionProbe {
    pub fn new(position: Vec3, influence_half_extents: Vec3) -> Self {
        Self {
            position,
            influence_half_extents,
            box_projection_half_extents: Some(influence_half_extents),
            dirty: true,
            slot: None,
        }
    }

    /// Returns `true` if the given world-space point is inside the influence volume.
    pub fn contains(&self, point: Vec3) -> bool {
        let d = (point - self.position).abs();
        d.x <= self.influence_half_extents.x
            && d.y <= self.influence_half_extents.y
            && d.z <= self.influence_half_extents.z
    }
}

/// Uniform data uploaded per probe (matches WGSL struct).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ReflectionProbeUniform {
    /// xyz = position, w = mip_count
    pub position_mip: [f32; 4],
    /// xyz = influence half-extents, w = 0 (or 1 if box-projection enabled)
    pub influence: [f32; 4],
    /// xyz = box-projection half-extents, w = probe slot index
    pub box_proj: [f32; 4],
}

/// Manages a set of cubemap slots for reflection probes.
pub struct ReflectionProbeManager {
    /// GPU cubemap array texture (6-layer per probe, slots stacked).
    pub cubemap_array: wgpu::Texture,
    pub cubemap_array_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    /// Uniform buffer containing probe metadata.
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    /// Number of probes currently allocated.
    pub probe_count: u32,
}

impl ReflectionProbeManager {
    pub fn new(device: &wgpu::Device) -> Self {
        let cubemap_array = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ReflectionProbe Cubemap Array"),
            size: wgpu::Extent3d {
                width: PROBE_FACE_SIZE,
                height: PROBE_FACE_SIZE,
                depth_or_array_layers: 6 * MAX_REFLECTION_PROBES as u32,
            },
            mip_level_count: PROBE_MIP_LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let cubemap_array_view = cubemap_array.create_view(&wgpu::TextureViewDescriptor {
            label: Some("ReflectionProbe Cubemap Array View"),
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ReflectionProbe Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ReflectionProbe Uniforms"),
            size: (std::mem::size_of::<ReflectionProbeUniform>() * MAX_REFLECTION_PROBES + 16)
                as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ReflectionProbe BGL"),
                entries: &[
                    // Cubemap array
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::CubeArray,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // Sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // Probe metadata
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ReflectionProbe BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cubemap_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            cubemap_array,
            cubemap_array_view,
            sampler,
            uniform_buffer,
            bind_group_layout,
            bind_group,
            probe_count: 0,
        }
    }

    /// Update the uniform buffer with current probe data and allocate slots.
    ///
    /// Call once per frame (or when probes change).  Pass all
    /// `ReflectionProbe` components from the ECS query.
    pub fn update_probes(&mut self, queue: &wgpu::Queue, probes: &mut [(Vec3, &mut ReflectionProbe)]) {
        let count = probes.len().min(MAX_REFLECTION_PROBES);
        self.probe_count = count as u32;

        let mut uniforms = vec![ReflectionProbeUniform::zeroed(); MAX_REFLECTION_PROBES];

        for (i, (_pos, probe)) in probes.iter_mut().enumerate().take(count) {
            probe.slot = Some(i);

            let box_proj_enabled = if probe.box_projection_half_extents.is_some() {
                1.0
            } else {
                0.0
            };
            let bp = probe
                .box_projection_half_extents
                .unwrap_or(probe.influence_half_extents);

            uniforms[i] = ReflectionProbeUniform {
                position_mip: [
                    probe.position.x,
                    probe.position.y,
                    probe.position.z,
                    PROBE_MIP_LEVELS as f32,
                ],
                influence: [
                    probe.influence_half_extents.x,
                    probe.influence_half_extents.y,
                    probe.influence_half_extents.z,
                    box_proj_enabled,
                ],
                box_proj: [bp.x, bp.y, bp.z, i as f32],
            };
        }

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&uniforms),
        );
    }

    /// Get the six view matrices for rendering into a cubemap from `position`.
    pub fn cubemap_view_matrices(position: Vec3) -> [glam::Mat4; 6] {
        use glam::Mat4;
        [
            // +X
            Mat4::look_at_rh(position, position + Vec3::X, -Vec3::Y),
            // -X
            Mat4::look_at_rh(position, position - Vec3::X, -Vec3::Y),
            // +Y
            Mat4::look_at_rh(position, position + Vec3::Y, Vec3::Z),
            // -Y
            Mat4::look_at_rh(position, position - Vec3::Y, -Vec3::Z),
            // +Z
            Mat4::look_at_rh(position, position + Vec3::Z, -Vec3::Y),
            // -Z
            Mat4::look_at_rh(position, position - Vec3::Z, -Vec3::Y),
        ]
    }

    /// Projection matrix for a single cubemap face (90° FoV, square).
    pub fn cubemap_projection() -> glam::Mat4 {
        glam::Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_2,
            1.0,
            0.1,
            1000.0,
        )
    }

    /// Create per-face `TextureView`s for writing into a probe's cubemap slot.
    ///
    /// Returns 6 views (one per face: +X, −X, +Y, −Y, +Z, −Z) at mip 0.
    pub fn face_views(&self, probe_slot: u32) -> [wgpu::TextureView; 6] {
        std::array::from_fn(|face| {
            self.cubemap_array.create_view(&wgpu::TextureViewDescriptor {
                label: Some("Probe Face View"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: probe_slot * 6 + face as u32,
                array_layer_count: Some(1),
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
    }

    /// Bake a single reflection probe by rendering the scene into its six cubemap faces.
    ///
    /// `render_face` is called six times with `(encoder, face_view, view_matrix, projection_matrix)`.
    /// The caller should render the scene from the probe's position into the supplied `face_view`.
    pub fn bake_probe<F>(
        &self,
        device: &wgpu::Device,
        probe: &ReflectionProbe,
        mut render_face: F,
    ) -> wgpu::CommandBuffer
    where
        F: FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView, glam::Mat4, glam::Mat4),
    {
        let Some(slot) = probe.slot else {
            return device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Reflection Probe Bake (no slot)"),
            }).finish();
        };
        let face_views = self.face_views(slot as u32);
        let view_matrices = Self::cubemap_view_matrices(probe.position);
        let projection = Self::cubemap_projection();

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Reflection Probe Bake"),
        });

        for (face, (view, mat)) in face_views.iter().zip(view_matrices.iter()).enumerate() {
            let _ = face; // unused but useful for debugging
            render_face(&mut encoder, view, *mat, projection);
        }

        encoder.finish()
    }
}

// ── ECS system integration ─────────────────────────────────────────

/// System that bakes dirty reflection probes each frame.
///
/// Runs during the `PreRender` stage.  For each probe whose `dirty` flag
/// is `true`, it invokes `bake_probe()` with the user-supplied render
/// closure, submits the resulting command buffer, and clears the flag.
///
/// # Usage
///
/// ```ignore
/// let system = ReflectionProbeSystem::new(|encoder, face_view, view, proj| {
///     // draw scene geometry into `face_view`
/// });
/// app.schedule.add_system(SystemStage::PreRender, Box::new(system));
/// ```
pub struct ReflectionProbeSystem<F> {
    render_face: F,
}

impl<F> ReflectionProbeSystem<F>
where
    F: FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView, glam::Mat4, glam::Mat4)
        + Send
        + Sync
        + 'static,
{
    pub fn new(render_face: F) -> Self {
        Self { render_face }
    }
}

impl<F> quasar_core::ecs::System for ReflectionProbeSystem<F>
where
    F: FnMut(&mut wgpu::CommandEncoder, &wgpu::TextureView, glam::Mat4, glam::Mat4)
        + Send
        + Sync
        + 'static,
{
    fn name(&self) -> &str {
        "reflection_probe_bake"
    }

    fn run(&mut self, world: &mut quasar_core::ecs::World) {
        // Collect dirty probes.
        let dirty_probes: Vec<(quasar_core::ecs::Entity, ReflectionProbe)> = world
            .query::<ReflectionProbe>()
            .into_iter()
            .filter(|(_, p)| p.dirty && p.slot.is_some())
            .map(|(e, p)| (e, p.clone()))
            .collect();

        if dirty_probes.is_empty() {
            return;
        }

        // We need the device, queue, and probe manager from the world.
        // These are stored as resources by the renderer.
        let manager_ptr: *const ReflectionProbeManager = {
            let Some(mgr) = world.resource::<ReflectionProbeManager>() else {
                return;
            };
            mgr as *const _
        };

        // SAFETY: Single-threaded system access; we don't mutate the manager
        // during bake — only the encoder and the render closure.
        let manager = unsafe { &*manager_ptr };

        for (entity, probe) in &dirty_probes {
            let cmd = manager.bake_probe(
                // The device is needed only for CommandEncoder creation.
                // We get it from a GpuDevice resource.
                {
                    let Some(gpu) = world.resource::<GpuDevice>() else {
                        return;
                    };
                    &gpu.device
                },
                probe,
                &mut self.render_face,
            );

            // Submit the command buffer.
            if let Some(gpu) = world.resource::<GpuDevice>() {
                gpu.queue.submit(std::iter::once(cmd));
            }

            // Mark the probe as no longer dirty.
            if let Some(p) = world.get_mut::<ReflectionProbe>(*entity) {
                p.dirty = false;
            }
        }
    }
}

/// Resource holding the wgpu device and queue for rendering subsystems.
///
/// Inserted by the renderer at startup so that PreRender systems can
/// submit GPU work without direct access to the renderer.
pub struct GpuDevice {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_contains_point() {
        let probe = ReflectionProbe::new(Vec3::ZERO, Vec3::splat(5.0));
        assert!(probe.contains(Vec3::new(1.0, 2.0, 3.0)));
        assert!(!probe.contains(Vec3::new(10.0, 0.0, 0.0)));
    }

    #[test]
    fn cubemap_views_are_six() {
        let views = ReflectionProbeManager::cubemap_view_matrices(Vec3::ZERO);
        assert_eq!(views.len(), 6);
    }
}
