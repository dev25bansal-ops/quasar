//! Hierarchical Z-Buffer (Hi-Z) occlusion culling.
//!
//! After the depth pre-pass (or after the geometry pass in deferred mode) the
//! Hi-Z system builds a mip-chain of the depth buffer. Each object's
//! screen-space bounding rect is tested against the appropriate mip level —
//! if the closest depth in the mip tile is nearer than the object, the object
//! is fully occluded and can be skipped.
//!
//! This implementation does the test on the CPU side for simplicity.
//! A GPU compute variant could be added later for higher throughput.

#![allow(clippy::too_many_arguments)]

use glam::{Mat4, Vec3, Vec4};

/// Number of mip levels in the depth pyramid.
pub const HIZ_MIP_LEVELS: usize = 8;

/// Maximum number of objects the GPU cull pass supports per dispatch.
pub const GPU_CULL_MAX_OBJECTS: u32 = 65536;

// ── GPU-accelerated Hi-Z culling ──────────────────────────────────

/// Per-object AABB data uploaded to the GPU for culling.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAabb {
    /// (min_x, min_y, min_z, _pad)
    pub min: [f32; 4],
    /// (max_x, max_y, max_z, _pad)
    pub max: [f32; 4],
}

/// Uniform data for the GPU cull compute shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuCullUniforms {
    pub view_proj: [[f32; 4]; 4],
    /// (screen_width, screen_height, num_objects, hiz_mip_levels)
    pub params: [f32; 4],
}

/// Statistics for the culling pipeline — tracks how many objects are culled
/// by frustum vs Hi-Z occlusion testing.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
pub struct CullStats {
    /// Objects culled by frustum test (outside view frustum).
    pub frustum_culled: u32,
    /// Objects culled by Hi-Z occlusion test (behind geometry).
    pub hiz_occluded: u32,
    /// Objects that passed all culling tests and are visible.
    pub visible: u32,
    /// Total objects tested.
    pub total: u32,
}

/// Indirect draw args written by the cull shader (matches `DrawIndexedIndirect`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndexedIndirectArgs {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: i32,
    pub first_instance: u32,
}

/// GPU-driven occlusion culling pass.
///
/// Reads a Hi-Z depth pyramid, tests each object AABB, and writes
/// visibility results into a storage buffer.  A separate indirect
/// draw buffer is built so only visible objects are drawn.
pub struct GpuCullPass {
    pub pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Storage buffer holding `GpuAabb` array.
    pub aabb_buffer: wgpu::Buffer,
    /// Uniform buffer for `GpuCullUniforms`.
    pub uniform_buffer: wgpu::Buffer,
    /// Output: u32 per object (1 = visible, 0 = culled).
    pub visibility_buffer: wgpu::Buffer,
    /// Output: indirect draw args buffer (read by `draw_indexed_indirect`).
    pub indirect_buffer: wgpu::Buffer,
    /// Output: atomic counter for visible objects.
    pub draw_count_buffer: wgpu::Buffer,
    /// Output: cull statistics buffer.
    pub stats_buffer: wgpu::Buffer,
}

impl GpuCullPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GPU Cull BGL"),
            entries: &[
                // 0: uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 1: AABB buffer (read)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 2: visibility output buffer (write)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 3: indirect draw buffer (write)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 4: draw count (atomic)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 5: Hi-Z depth pyramid texture (sampled, read)
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // 6: Hi-Z sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                // 7: cull statistics (write)
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Cull Shader"),
            source: wgpu::ShaderSource::Wgsl(GPU_CULL_WGSL.into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Cull Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GPU Cull Pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("cull_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let aabb_buf_size = (GPU_CULL_MAX_OBJECTS as u64) * std::mem::size_of::<GpuAabb>() as u64;
        let aabb_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Cull AABBs"),
            size: aabb_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Cull Uniforms"),
            size: std::mem::size_of::<GpuCullUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vis_buf_size = (GPU_CULL_MAX_OBJECTS as u64) * 4;
        let visibility_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Cull Visibility"),
            size: vis_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let indirect_buf_size =
            (GPU_CULL_MAX_OBJECTS as u64) * std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
        let indirect_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Cull Indirect"),
            size: indirect_buf_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let draw_count_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Cull Draw Count"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Statistics buffer: 4x u32 (frustum_culled, hiz_occluded, visible, total).
        let stats_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Cull Stats"),
            size: std::mem::size_of::<CullStats>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            aabb_buffer,
            uniform_buffer,
            visibility_buffer,
            indirect_buffer,
            draw_count_buffer,
            stats_buffer,
        }
    }

    /// Create the bind group for a dispatch.  Call after uploading AABBs + uniforms.
    ///
    /// `hiz_texture` and `hiz_sampler` come from the previous frame's depth buffer.
    /// A placeholder (1×1 white texture) can be used for the first frame.
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        hiz_view: &wgpu::TextureView,
        hiz_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Cull BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.aabb_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.visibility_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.indirect_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.draw_count_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(hiz_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(hiz_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: self.stats_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Dispatch the cull compute pass.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        num_objects: u32,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GPU Cull"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, bind_group, &[]);
        cpass.dispatch_workgroups(num_objects.div_ceil(64), 1, 1);
    }
}

/// GPU cull compute WGSL.
///
/// Performs two-stage culling:
/// 1. Frustum cull — discard objects outside the view frustum.
/// 2. Hi-Z occlusion test — discard objects hidden behind geometry from the
///    previous frame's depth buffer.
///
/// Statistics are written to the `CullStats` buffer at binding 7.
pub const GPU_CULL_WGSL: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    params: vec4<f32>, // (screen_w, screen_h, num_objects, hiz_mip_levels)
};

struct Aabb {
    aabb_min: vec4<f32>,
    aabb_max: vec4<f32>,
};

struct CullStats {
    frustum_culled: atomic<u32>,
    hiz_occluded: atomic<u32>,
    visible: atomic<u32>,
    total: atomic<u32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> aabbs: array<Aabb>;
@group(0) @binding(2) var<storage, read_write> visibility: array<u32>;
@group(0) @binding(3) var<storage, read_write> indirect: array<u32>;
@group(0) @binding(4) var<storage, read_write> draw_count: atomic<u32>;
@group(0) @binding(5) var t_hiz: texture_2d<f32>;
@group(0) @binding(6) var s_hiz: sampler;

// Cull statistics (binding 7 — NOT used in the Rust code yet, written by shader).
@group(0) @binding(7) var<storage, read_write> stats: CullStats;

/// Project a 3D AABB to screen-space, returning (min_x, min_y, max_x, max_y)
/// in NDC coordinates [-1, 1].
fn project_aabb(aabb_min: vec3<f32>, aabb_max: vec3<f32>) -> vec4<f32> {
    var lo = vec2<f32>(1.0);
    var hi = vec2<f32>(-1.0);
    for (var i = 0u; i < 8u; i++) {
        let corner = vec3<f32>(
            select(aabb_min.x, aabb_max.x, (i & 1u) != 0u),
            select(aabb_min.y, aabb_max.y, (i & 2u) != 0u),
            select(aabb_min.z, aabb_max.z, (i & 4u) != 0u),
        );
        let clip = uniforms.view_proj * vec4<f32>(corner, 1.0);
        if clip.w <= 0.0 { return vec4<f32>(-1.0, -1.0, 1.0, 1.0); }
        let ndc = clip.xy / clip.w;
        lo = min(lo, ndc);
        hi = max(hi, ndc);
    }
    return vec4<f32>(lo, hi);
}

/// Test whether an AABB is fully occluded by geometry using the Hi-Z depth pyramid.
///
/// Returns `true` if the AABB is occluded (can be culled), `false` if visible.
///
/// The algorithm:
/// 1. Project the AABB to screen space, getting (min_x, min_y, max_x, max_y) in NDC.
/// 2. Convert to texel coordinates [0, screen_size].
/// 3. Select an appropriate mip level based on the object's screen-space size.
///    Smaller objects use higher mip levels (coarser depth data).
/// 4. Sample the Hi-Z texture at the center of the AABB at the chosen mip level.
/// 5. If the AABB's nearest depth is greater (farther) than the sampled depth,
///    the entire AABB is behind geometry and can be culled.
fn is_occluded_by_hiz(
    aabb_min: vec3<f32>,
    aabb_max: vec3<f32>,
    screen_size: vec2<f32>,
    hiz_mip_levels: f32,
) -> bool {
    // Project all 8 corners to find the screen-space bounding rect and min depth.
    var ndc_min = vec2<f32>(1.0);
    var ndc_max = vec2<f32>(-1.0);
    var min_depth = 1.0;

    for (var i = 0u; i < 8u; i++) {
        let corner = vec3<f32>(
            select(aabb_min.x, aabb_max.x, (i & 1u) != 0u),
            select(aabb_min.y, aabb_max.y, (i & 2u) != 0u),
            select(aabb_min.z, aabb_max.z, (i & 4u) != 0u),
        );
        let clip = uniforms.view_proj * vec4<f32>(corner, 1.0);
        if clip.w <= 0.0 {
            // Behind camera — conservatively NOT occluded.
            return false;
        }
        let ndc = clip.xy / clip.w;
        ndc_min = min(ndc_min, ndc);
        ndc_max = max(ndc_max, ndc);
        min_depth = min(min_depth, clip.z / clip.w);
    }

    // Check if the AABB is entirely off-screen.
    if ndc_min.x > 1.0 || ndc_max.x < -1.0 || ndc_min.y > 1.0 || ndc_max.y < -1.0 {
        return false; // Off-screen, not occluded (handled by frustum cull).
    }

    // Clamp to screen bounds.
    ndc_min = clamp(ndc_min, vec2<f32>(-1.0), vec2<f32>(1.0));
    ndc_max = clamp(ndc_max, vec2<f32>(-1.0), vec2<f32>(1.0));

    // Convert NDC to texel coordinates [0, screen_size].
    let screen_min = (ndc_min * 0.5 + 0.5) * screen_size;
    let screen_max = (ndc_max * 0.5 + 0.5) * screen_size;

    // Compute the screen-space size to pick a mip level.
    let size = screen_max - screen_min;
    let max_dim = max(size.x, size.y);

    // Select mip level: larger screen area = lower mip, smaller = higher mip.
    // We want the mip where one texel roughly covers the AABB size.
    let mip_f = log2(max(max_dim, 1.0));
    let mip_level = u32(min(mip_f, hiz_mip_levels - 1.0));

    // Sample Hi-Z at the center of the AABB at the chosen mip level.
    let center_ndc = (ndc_min + ndc_max) * 0.5;
    // Convert to texture coordinates [0, 1] for sampling.
    let uv = center_ndc * 0.5 + 0.5;

    // Use textureSampleLevel to read from the specific mip level.
    let hiz_depth = textureSampleLevel(t_hiz, s_hiz, uv, f32(mip_level)).r;

    // Standard Z: smaller = closer, larger = farther.
    // If the object's nearest point is farther than the depth in the Hi-Z,
    // the object is fully occluded.
    return min_depth > hiz_depth;
}

@compute @workgroup_size(64)
fn cull_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let num = u32(uniforms.params.z);
    if idx >= num { return; }

    atomicAdd(&stats.total, 1u);

    let aabb = aabbs[idx];
    let rect = project_aabb(aabb.aabb_min.xyz, aabb.aabb_max.xyz);

    // ── Stage 1: Frustum cull ──
    // If the AABB is entirely outside the view frustum, cull immediately.
    let in_frustum = !(rect.z < -1.0 || rect.x > 1.0 || rect.w < -1.0 || rect.y > 1.0);

    if !in_frustum {
        visibility[idx] = 0u;
        atomicAdd(&stats.frustum_culled, 1u);
        return;
    }

    // ── Stage 2: Hi-Z occlusion test ──
    let screen_size = uniforms.params.xy;
    let hiz_levels = uniforms.params.w;
    let occluded = is_occluded_by_hiz(aabb.aabb_min.xyz, aabb.aabb_max.xyz, screen_size, hiz_levels);

    if occluded {
        visibility[idx] = 0u;
        atomicAdd(&stats.hiz_occluded, 1u);
        return;
    }

    // Object passed both culling stages.
    visibility[idx] = 1u;
    indirect[idx * 5u + 1u] = 1u;
    atomicAdd(&draw_count, 1u);
    atomicAdd(&stats.visible, 1u);
}
"#;

/// Hi-Z depth pyramid — a conservative depth mip-chain.
pub struct HiZBuffer {
    /// Mip levels, index 0 = full-res, each subsequent level is half.
    pub mips: Vec<HiZMip>,
}

/// A single level of the Hi-Z mip-chain.
pub struct HiZMip {
    pub width: u32,
    pub height: u32,
    /// Depth values (row-major). Each value is the **maximum** (farthest)
    /// depth among the 2×2 parent texels — because in a reversed-Z setup the
    /// closer depth is *larger*, but we use a standard `[0,1]` depth where 0 is
    /// near and 1 is far, so storing the max is the *conservative* choice for
    /// occlusion: if the max (farthest) pixel is still closer than the object,
    /// the whole tile occludes it.
    pub data: Vec<f32>,
}

impl HiZBuffer {
    /// Build the Hi-Z pyramid from a full-resolution depth buffer.
    ///
    /// `src` is row-major f32 depth values for a `width × height` buffer.
    pub fn build(src: &[f32], width: u32, height: u32) -> Self {
        assert_eq!(src.len(), (width * height) as usize);

        let mut mips = Vec::with_capacity(HIZ_MIP_LEVELS);

        // Level 0 = copy of the original depth buffer.
        mips.push(HiZMip {
            width,
            height,
            data: src.to_vec(),
        });

        // Successive 2× downsample using max.
        for _ in 1..HIZ_MIP_LEVELS {
            let Some(prev) = mips.last() else { break };
            let mw = (prev.width / 2).max(1);
            let mh = (prev.height / 2).max(1);
            let mut data = vec![0.0_f32; (mw * mh) as usize];

            for y in 0..mh {
                for x in 0..mw {
                    let sx = (x * 2) as usize;
                    let sy = (y * 2) as usize;
                    let pw = prev.width as usize;
                    let ph = prev.height as usize;

                    let s00 = prev.data[sy * pw + sx];
                    let s10 = if sx + 1 < pw {
                        prev.data[sy * pw + sx + 1]
                    } else {
                        s00
                    };
                    let s01 = if sy + 1 < ph {
                        prev.data[(sy + 1) * pw + sx]
                    } else {
                        s00
                    };
                    let s11 = if sx + 1 < pw && sy + 1 < ph {
                        prev.data[(sy + 1) * pw + sx + 1]
                    } else {
                        s00
                    };

                    // Max = farthest depth in the 2×2 block (standard Z: 0 near, 1 far).
                    data[(y * mw + x) as usize] = s00.max(s10).max(s01).max(s11);
                }
            }

            mips.push(HiZMip {
                width: mw,
                height: mh,
                data,
            });

            if mw == 1 && mh == 1 {
                break;
            }
        }

        Self { mips }
    }

    /// Test whether an axis-aligned bounding box (in world-space) is occluded.
    ///
    /// Returns `true` if the AABB is **visible** (not occluded), `false` if
    /// it is fully behind closer geometry and can be culled.
    pub fn is_visible(
        &self,
        aabb_min: Vec3,
        aabb_max: Vec3,
        view_proj: &Mat4,
        screen_width: f32,
        screen_height: f32,
    ) -> bool {
        // Project all 8 AABB corners to screen space.
        let corners = [
            Vec3::new(aabb_min.x, aabb_min.y, aabb_min.z),
            Vec3::new(aabb_max.x, aabb_min.y, aabb_min.z),
            Vec3::new(aabb_min.x, aabb_max.y, aabb_min.z),
            Vec3::new(aabb_max.x, aabb_max.y, aabb_min.z),
            Vec3::new(aabb_min.x, aabb_min.y, aabb_max.z),
            Vec3::new(aabb_max.x, aabb_min.y, aabb_max.z),
            Vec3::new(aabb_min.x, aabb_max.y, aabb_max.z),
            Vec3::new(aabb_max.x, aabb_max.y, aabb_max.z),
        ];

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;

        for c in &corners {
            let clip = *view_proj * Vec4::new(c.x, c.y, c.z, 1.0);
            if clip.w <= 0.0 {
                // Behind the camera — conservatively visible.
                return true;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            let ndc_z = clip.z / clip.w;

            // NDC to [0, screen_width/height]
            let sx = (ndc_x * 0.5 + 0.5) * screen_width;
            let sy = (0.5 - ndc_y * 0.5) * screen_height;

            min_x = min_x.min(sx);
            max_x = max_x.max(sx);
            min_y = min_y.min(sy);
            max_y = max_y.max(sy);
            min_z = min_z.min(ndc_z);
        }

        // Clamp screen rect
        let x0 = (min_x.floor() as i32).max(0) as u32;
        let y0 = (min_y.floor() as i32).max(0) as u32;
        let x1 = (max_x.ceil() as i32).max(0) as u32;
        let y1 = (max_y.ceil() as i32).max(0) as u32;

        if x0 >= x1 || y0 >= y1 {
            return false; // zero-area on screen
        }

        // Choose mip level based on bounding rect size.
        let rect_w = (x1 - x0) as f32;
        let rect_h = (y1 - y0) as f32;
        let max_dim = rect_w.max(rect_h);
        let mip = if max_dim <= 1.0 {
            0
        } else {
            (max_dim.log2().ceil() as usize).min(self.mips.len() - 1)
        };

        let level = &self.mips[mip];

        // Scale coordinates to the mip level.
        let scale_x = level.width as f32 / screen_width;
        let scale_y = level.height as f32 / screen_height;

        let mx0 = ((x0 as f32 * scale_x).floor() as u32).min(level.width.saturating_sub(1));
        let my0 = ((y0 as f32 * scale_y).floor() as u32).min(level.height.saturating_sub(1));
        let mx1 = ((x1 as f32 * scale_x).ceil() as u32).min(level.width);
        let my1 = ((y1 as f32 * scale_y).ceil() as u32).min(level.height);

        // Find the farthest depth in the mip region.
        let mut max_depth = 0.0_f32;
        for ry in my0..my1 {
            for rx in mx0..mx1 {
                let d = level.data[(ry * level.width + rx) as usize];
                max_depth = max_depth.max(d);
            }
        }

        // If the closest point of the AABB is farther than the farthest depth
        // in the hi-z tile, the object is occluded.
        // (standard Z: smaller Z = closer)
        if min_z > max_depth {
            return false; // occluded
        }

        true // visible
    }
}

// ── GPU-driven indirect rendering ─────────────────────────────────

/// Per-draw-call information stored CPU-side to feed the GPU cull pass.
#[derive(Clone)]
pub struct MeshDrawCommand {
    /// Index count for the mesh.
    pub index_count: u32,
    /// First index in the index buffer.
    pub first_index: u32,
    /// Base vertex offset.
    pub base_vertex: i32,
    /// World-space AABB minimum.
    pub aabb_min: Vec3,
    /// World-space AABB maximum.
    pub aabb_max: Vec3,
    /// Material bind-group index (for sorting).
    pub material_index: u32,
}

/// Manages a list of mesh draw commands and produces GPU buffers
/// suitable for indirect rendering via [`GpuCullPass`].
pub struct IndirectDrawManager {
    draws: Vec<MeshDrawCommand>,
}

impl IndirectDrawManager {
    pub fn new() -> Self {
        Self { draws: Vec::new() }
    }

    /// Clear all draw commands for a new frame.
    pub fn clear(&mut self) {
        self.draws.clear();
    }

    /// Push a mesh draw command.
    pub fn push(&mut self, cmd: MeshDrawCommand) {
        self.draws.push(cmd);
    }

    /// Number of queued draw commands.
    pub fn count(&self) -> u32 {
        self.draws.len() as u32
    }

    /// Upload AABBs to the cull pass buffer.
    pub fn upload_aabbs(&self, queue: &wgpu::Queue, cull: &GpuCullPass) {
        let aabbs: Vec<GpuAabb> = self
            .draws
            .iter()
            .map(|d| GpuAabb {
                min: [d.aabb_min.x, d.aabb_min.y, d.aabb_min.z, 0.0],
                max: [d.aabb_max.x, d.aabb_max.y, d.aabb_max.z, 0.0],
            })
            .collect();
        queue.write_buffer(&cull.aabb_buffer, 0, bytemuck::cast_slice(&aabbs));
    }

    /// Upload uniforms to the cull pass buffer.
    ///
    /// `hiz_mip_levels` is the number of mip levels in the Hi-Z pyramid
    /// (0 disables Hi-Z testing).
    pub fn upload_uniforms(
        &self,
        queue: &wgpu::Queue,
        cull: &GpuCullPass,
        view_proj: &Mat4,
        screen_width: f32,
        screen_height: f32,
        hiz_mip_levels: f32,
    ) {
        let cols = view_proj.to_cols_array_2d();
        let uniforms = GpuCullUniforms {
            view_proj: cols,
            params: [
                screen_width,
                screen_height,
                self.draws.len() as f32,
                hiz_mip_levels,
            ],
        };
        queue.write_buffer(&cull.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Reset the cull statistics buffer before dispatch.
    pub fn reset_stats(&self, queue: &wgpu::Queue, cull: &GpuCullPass) {
        let zeros: [u8; 16] = [0; 16];
        queue.write_buffer(&cull.stats_buffer, 0, &zeros);
    }

    /// Read back cull statistics from the GPU buffer.
    pub fn read_stats(&self, _device: &wgpu::Device, _cull: &GpuCullPass) -> CullStats {
        // In production, you'd use a readback buffer + map_async.
        CullStats::default()
    }

    /// After the cull compute shader runs, build `DrawIndexedIndirect`
    /// commands on GPU. Currently this writes a pre-populated indirect buffer
    /// before the compute pass so the shader can compact it.
    pub fn prepare_indirect_buffer(&self, queue: &wgpu::Queue, cull: &GpuCullPass) {
        let args: Vec<DrawIndexedIndirectArgs> = self
            .draws
            .iter()
            .map(|d| DrawIndexedIndirectArgs {
                index_count: d.index_count,
                instance_count: 0, // compute shader sets to 1 if visible
                first_index: d.first_index,
                base_vertex: d.base_vertex,
                first_instance: 0,
            })
            .collect();
        queue.write_buffer(&cull.indirect_buffer, 0, bytemuck::cast_slice(&args));
        // Reset draw count
        queue.write_buffer(&cull.draw_count_buffer, 0, &[0u8; 4]);
    }

    /// Record GPU-driven draw calls.  Each object has a pre-populated
    /// `DrawIndexedIndirect` in the buffer; the cull shader sets
    /// `instance_count = 0` for culled objects, so the GPU skips them.
    pub fn execute_indirect<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        cull: &'a GpuCullPass,
    ) {
        let count = self.draws.len() as u32;
        // Use multi_draw_indexed_indirect when supported (avoids per-draw CPU overhead).
        render_pass.multi_draw_indexed_indirect(&cull.indirect_buffer, 0, count);
    }

    /// Fallback: issue individual `draw_indexed_indirect` calls when
    /// `multi_draw_indirect` is unavailable.
    pub fn execute_indirect_fallback<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        cull: &'a GpuCullPass,
    ) {
        let stride = std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
        for i in 0..self.draws.len() as u64 {
            render_pass.draw_indexed_indirect(&cull.indirect_buffer, i * stride);
        }
    }
}

impl Default for IndirectDrawManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-draw-indirect-count wrapper.
///
/// When `MULTI_DRAW_INDIRECT_COUNT` is supported, the GPU itself determines
/// how many draw calls to issue from a count buffer written by the cull shader.
/// This avoids CPU readback of the compacted draw count.
pub struct MultiDrawIndirectCount {
    /// Whether the device supports multi_draw_indirect_count.
    pub supported: bool,
}

impl MultiDrawIndirectCount {
    pub fn new(device_features: wgpu::Features) -> Self {
        Self {
            supported: device_features.contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT),
        }
    }

    /// Execute indirect draws using the count buffer if supported, otherwise
    /// fall back to `multi_draw_indexed_indirect` with a fixed count.
    pub fn execute<'a>(
        &self,
        render_pass: &mut wgpu::RenderPass<'a>,
        indirect_buffer: &'a wgpu::Buffer,
        count_buffer: &'a wgpu::Buffer,
        max_draw_count: u32,
    ) {
        let stride = std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
        if self.supported {
            render_pass.multi_draw_indexed_indirect_count(
                indirect_buffer,
                0,
                count_buffer,
                0,
                max_draw_count,
            );
        } else {
            render_pass.multi_draw_indexed_indirect(indirect_buffer, 0, max_draw_count);
        }
        let _ = stride; // used when falling back to per-draw calls
    }
}

// ── Bindless resource table ───────────────────────────────────────

/// Maximum number of materials in the bindless material buffer.
pub const BINDLESS_MAX_MATERIALS: u32 = 1024;

/// Maximum number of textures in the bindless texture array.
pub const BINDLESS_MAX_TEXTURES: u32 = 256;

/// GPU-side packed material data for bindless rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMaterial {
    pub base_color: [f32; 4],
    /// (roughness, metallic, emissive, texture_index)
    pub params: [f32; 4],
}

/// Per-instance data written alongside indirect draw args so the shader
/// can look up the correct material and texture by index.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawInstanceData {
    pub material_index: u32,
    pub texture_index: u32,
    pub _pad: [u32; 2],
}

/// Manages a global material storage buffer and (optionally) a texture
/// binding array for fully bindless rendering.
///
/// When `TEXTURE_BINDING_ARRAY` is available the shader can index into a
/// table of textures.  Otherwise the material table alone is useful for
/// reducing per-draw bind-group switches.
pub struct BindlessResources {
    /// Storage buffer holding `GpuMaterial` array.
    pub material_buffer: wgpu::Buffer,
    /// Storage buffer holding `DrawInstanceData` per object.
    pub instance_data_buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: Option<wgpu::BindGroup>,
    /// Number of materials currently uploaded.
    pub material_count: u32,
    /// Whether the device supports texture binding arrays.
    pub has_texture_array: bool,
}

impl BindlessResources {
    pub fn new(device: &wgpu::Device, has_texture_array: bool) -> Self {
        let mat_buf_size =
            (BINDLESS_MAX_MATERIALS as u64) * std::mem::size_of::<GpuMaterial>() as u64;
        let material_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bindless Material Buffer"),
            size: mat_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let inst_buf_size =
            (GPU_CULL_MAX_OBJECTS as u64) * std::mem::size_of::<DrawInstanceData>() as u64;
        let instance_data_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bindless Instance Data"),
            size: inst_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut entries = vec![
            // 0: material array (storage, read)
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // 1: per-instance data (storage, read)
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];

        if has_texture_array {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: std::num::NonZeroU32::new(BINDLESS_MAX_TEXTURES),
            });
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bindless Resources BGL"),
            entries: &entries,
        });

        Self {
            material_buffer,
            instance_data_buffer,
            bind_group_layout,
            bind_group: None,
            material_count: 0,
            has_texture_array,
        }
    }

    /// Upload materials to the GPU buffer.
    pub fn upload_materials(&mut self, queue: &wgpu::Queue, materials: &[GpuMaterial]) {
        let count = (materials.len() as u32).min(BINDLESS_MAX_MATERIALS);
        queue.write_buffer(
            &self.material_buffer,
            0,
            bytemuck::cast_slice(&materials[..count as usize]),
        );
        self.material_count = count;
    }

    /// Upload per-instance data (material/texture indices).
    pub fn upload_instance_data(&self, queue: &wgpu::Queue, data: &[DrawInstanceData]) {
        let max = (GPU_CULL_MAX_OBJECTS as usize).min(data.len());
        queue.write_buffer(
            &self.instance_data_buffer,
            0,
            bytemuck::cast_slice(&data[..max]),
        );
    }

    /// Build (or rebuild) the bind group.  When `texture_views` is provided
    /// and the device supports binding arrays, textures are included.
    pub fn rebuild_bind_group(
        &mut self,
        device: &wgpu::Device,
        texture_views: Option<&[&wgpu::TextureView]>,
        sampler: Option<&wgpu::Sampler>,
    ) {
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: self.material_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: self.instance_data_buffer.as_entire_binding(),
            },
        ];

        if self.has_texture_array {
            if let (Some(views), Some(s)) = (texture_views, sampler) {
                entries.push(wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureViewArray(views),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(s),
                });
            }
        }

        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bindless Resources BG"),
            layout: &self.bind_group_layout,
            entries: &entries,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiz_build_downsample() {
        let depth = vec![0.5_f32; 16]; // 4×4
        let hiz = HiZBuffer::build(&depth, 4, 4);
        assert!(hiz.mips.len() >= 3);
        assert_eq!(hiz.mips[0].width, 4);
        assert_eq!(hiz.mips[1].width, 2);
        assert_eq!(hiz.mips[2].width, 1);
    }

    #[test]
    fn hiz_occluded_behind_wall() {
        // A simple 4×4 depth buffer where everything is at depth 0.1 (very close).
        let depth = vec![0.1_f32; 16];
        let hiz = HiZBuffer::build(&depth, 4, 4);

        let vp = Mat4::IDENTITY;
        // An object at depth 0.5 — it's behind the wall at 0.1.
        assert!(!hiz.is_visible(
            Vec3::new(-0.1, -0.1, 0.5),
            Vec3::new(0.1, 0.1, 0.5),
            &vp,
            4.0,
            4.0,
        ));
    }
}

// ── GPU Hi-Z depth pyramid builder ────────────────────────────────

/// GPU-side uniform for the Hi-Z downsample compute shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HizParams {
    /// (parent_w, parent_h, output_w, output_h)
    pub dims: [u32; 4],
}

/// GPU Hi-Z depth pyramid builder.
///
/// Creates a compute pipeline that downsamples a depth texture one mip at a
/// time using a 2×2 max filter.  Each dispatch writes a single mip level of
/// the destination `R32Float` texture.
pub struct GpuHiZBuilder {
    pub pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
}

impl GpuHiZBuilder {
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HiZ Build BGL"),
            entries: &[
                // 0: uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // 1: source mip (sampled)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // 2: destination mip (storage write)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HiZ Build Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/hiz_build.wgsl").into(),
            ),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HiZ Build Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("HiZ Build Pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("hiz_downsample"),
            compilation_options: Default::default(),
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HiZ Build Uniform"),
            size: std::mem::size_of::<HizParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
        }
    }

    /// Build the Hi-Z mip chain on the GPU.
    ///
    /// `hiz_texture` must be an `R32Float` texture with at least `mip_levels`
    /// mip levels.  Mip 0 must be pre-filled with the full-resolution depth.
    pub fn build_mips(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        hiz_texture: &HiZTexture,
    ) {
        let mut w = hiz_texture.width;
        let mut h = hiz_texture.height;

        for mip in 1..hiz_texture.mip_levels {
            let src_w = w;
            let src_h = h;
            w = (w / 2).max(1);
            h = (h / 2).max(1);

            let params = HizParams {
                dims: [src_w, src_h, w, h],
            };
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&params));

            let src_view = hiz_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor {
                    label: Some("HiZ Build Src View"),
                    base_mip_level: mip - 1,
                    mip_level_count: Some(1),
                    ..Default::default()
                });

            let dst_view = hiz_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor {
                    label: Some("HiZ Build Dst View"),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                });

            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HiZ Build BG"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dst_view),
                    },
                ],
            });

            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("HiZ Build"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
    }
}

// ── Hi-Z GPU texture resource ─────────────────────────────────────

/// GPU-resident Hi-Z depth pyramid texture.
///
/// An `R32Float` texture with mip levels, where each mip level stores
/// the maximum (farthest) depth in its 2×2 block.  Used by the GPU
/// cull pass for occlusion testing.
pub struct HiZTexture {
    /// The full mip-chained depth texture.
    pub texture: wgpu::Texture,
    /// Pre-built texture view for the entire mip chain.
    pub view: wgpu::TextureView,
    /// Sampler for sampling the Hi-Z pyramid.
    pub sampler: wgpu::Sampler,
    /// Width of mip level 0 (full resolution).
    pub width: u32,
    /// Height of mip level 0 (full resolution).
    pub height: u32,
    /// Number of mip levels in the texture.
    pub mip_levels: u32,
}

impl HiZTexture {
    /// Create a new Hi-Z depth pyramid texture.
    ///
    /// The texture is `R32Float` with a full mip chain.  Mip level 0
    /// must be filled by copying from the previous frame's depth buffer.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let mip_levels = Self::compute_mip_levels(width, height);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Hi-Z Depth Pyramid"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Point sampler with clamp-to-edge — we want exact texel reads.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Hi-Z Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            width,
            height,
            mip_levels,
        }
    }

    /// Compute the number of mip levels for the given dimensions.
    fn compute_mip_levels(width: u32, height: u32) -> u32 {
        let max_dim = width.max(height).max(1);
        max_dim.ilog2() + 1
    }

    /// Resize the Hi-Z texture to new dimensions.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        *self = Self::new(device, width, height);
    }
}

// ── Hi-Z depth copy pass ──────────────────────────────────────────

/// WGSL shader to copy depth buffer into Hi-Z mip level 0.
const HI_Z_COPY_WGSL: &str = r#"
struct CopyParams {
    width: u32,
    height: u32,
    _pad: vec2<u32>,
};

@group(0) @binding(0) var t_depth: texture_depth_2d;
@group(0) @binding(1) var t_hiz: texture_storage_2d<r32float, write>;
@group(0) @binding(2) var<uniform> params: CopyParams;

@compute @workgroup_size(8, 8)
fn copy_depth_to_hiz(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = gid.x;
    let y = gid.y;
    if x >= params.width || y >= params.height {
        return;
    }
    let d = textureLoad(t_depth, vec2<u32>(x, y), 0);
    textureStore(t_hiz, vec2<u32>(x, y), vec4<f32>(d, 0.0, 0.0, 1.0));
}
"#;

/// Compute pass that copies the depth buffer into Hi-Z mip level 0.
pub struct HiZCopyPass {
    pub pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
}

impl HiZCopyPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HiZ Copy BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("HiZ Copy Shader"),
            source: wgpu::ShaderSource::Wgsl(HI_Z_COPY_WGSL.into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HiZ Copy Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("HiZ Copy Pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("copy_depth_to_hiz"),
            compilation_options: Default::default(),
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HiZ Copy Uniform"),
            size: 16, // vec4<u32>
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            uniform_buffer,
        }
    }

    /// Dispatch a copy of the depth texture into Hi-Z mip level 0.
    pub fn dispatch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_view: &wgpu::TextureView,
        hiz_texture: &HiZTexture,
        width: u32,
        height: u32,
    ) {
        let dst_view = hiz_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                label: Some("HiZ Mip 0 View"),
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            });

        let params: [u32; 4] = [width, height, 0, 0];
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&params));

        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HiZ Copy BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dst_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("HiZ Copy Depth"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &bg, &[]);
        cpass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests2 {
    use super::*;

    #[test]
    fn hiz_build_downsample() {
        let depth = vec![0.5_f32; 16]; // 4×4
        let hiz = HiZBuffer::build(&depth, 4, 4);
        assert!(hiz.mips.len() >= 3);
        assert_eq!(hiz.mips[0].width, 4);
        assert_eq!(hiz.mips[1].width, 2);
        assert_eq!(hiz.mips[2].width, 1);
    }

    #[test]
    fn hiz_occluded_behind_wall() {
        // A simple 4×4 depth buffer where everything is at depth 0.1 (very close).
        let depth = vec![0.1_f32; 16];
        let hiz = HiZBuffer::build(&depth, 4, 4);

        let vp = Mat4::IDENTITY;
        // An object at depth 0.5 — it's behind the wall at 0.1.
        assert!(!hiz.is_visible(
            Vec3::new(-0.1, -0.1, 0.5),
            Vec3::new(0.1, 0.1, 0.5),
            &vp,
            4.0,
            4.0,
        ));
    }

    #[test]
    fn cull_stats_default() {
        let stats = CullStats::default();
        assert_eq!(stats.frustum_culled, 0);
        assert_eq!(stats.hiz_occluded, 0);
        assert_eq!(stats.visible, 0);
        assert_eq!(stats.total, 0);
    }
}
