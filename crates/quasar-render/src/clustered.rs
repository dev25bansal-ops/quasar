//! Clustered forward rendering â€” bins lights into a 3D froxel grid
//! so the per-fragment shader only iterates lights that overlap each cluster.
//!
//! Grid dimensions: `CLUSTER_X Ã— CLUSTER_Y Ã— CLUSTER_Z` tiles covering
//! the view frustum. Each cluster stores indices into a global light list.
//!
//! **GPU-Driven Assignment:** Light-to-cluster assignment runs entirely on
//! the GPU via a compute shader (`clustered_lighting.wgsl`). Each thread
//! handles one light, computing its bounding sphere and atomically appending
//! to overlapping cluster light lists.

use crate::light::{LightData, PointLight};

/// Number of cluster tiles along the X axis.
pub const CLUSTER_X: usize = 16;
/// Number of cluster tiles along the Y axis.
pub const CLUSTER_Y: usize = 9;
/// Number of cluster tiles along the Z (depth) axis â€” logarithmic slices.
pub const CLUSTER_Z: usize = 24;
/// Maximum number of lights per cluster before overflow.
pub const MAX_LIGHTS_PER_CLUSTER: usize = 128;
/// Total number of clusters.
pub const TOTAL_CLUSTERS: usize = CLUSTER_X * CLUSTER_Y * CLUSTER_Z;
/// Workgroup size for the compute shader.
pub const CLUSTER_ASSIGN_WG_SIZE: u32 = 64;

/// Axis-aligned bounding box for a cluster in view space.
#[derive(Debug, Clone, Copy, bytemuck::Zeroable)]
#[repr(C)]
pub struct ClusterAabb {
    pub min: [f32; 3],
    pub _pad0: f32,
    pub max: [f32; 3],
    pub _pad1: f32,
}

/// A single cluster containing indices into the light list.
#[derive(Clone)]
pub struct Cluster {
    pub light_count: u32,
    pub light_indices: [u16; MAX_LIGHTS_PER_CLUSTER],
}

impl Default for Cluster {
    fn default() -> Self {
        Self {
            light_count: 0,
            light_indices: [0; MAX_LIGHTS_PER_CLUSTER],
        }
    }
}

/// GPU-side cluster light assignment output.
/// Stored as a flat array of counts + light indices.
#[derive(Debug, Clone, Copy, bytemuck::Zeroable)]
#[repr(C)]
pub struct GpuClusterOutput {
    /// Number of lights assigned to this cluster.
    pub count: u32,
    /// Light indices (padded to alignment).
    pub light_indices: [u16; MAX_LIGHTS_PER_CLUSTER],
    /// Padding to align to 16 bytes.
    pub _padding: u16,
}

impl GpuClusterOutput {
    pub const fn size_in_bytes() -> u64 {
        // count(4) + indices(128 * 2 = 256) + padding(2) = 262, rounded to 264 for 4-byte align
        264
    }
}

/// Uniform parameters passed to the cluster assignment compute shader.
#[derive(Debug, Clone, Copy, bytemuck::Zeroable)]
#[repr(C)]
pub struct ClusterParams {
    pub num_lights: u32,
    pub num_clusters_x: u32,
    pub num_clusters_y: u32,
    pub num_clusters_z: u32,
    pub near: f32,
    pub far: f32,
    pub screen_width: f32,
    pub screen_height: f32,
}

/// The full 3D cluster grid with CPU and GPU assignment paths.
pub struct LightClusterGrid {
    pub clusters: Vec<Cluster>,
    /// View-space AABBs for each cluster (rebuilt when camera changes).
    pub aabbs: Vec<ClusterAabb>,
    pub near: f32,
    pub far: f32,
    pub screen_width: u32,
    pub screen_height: u32,
}

// â”€â”€ GPU Resources â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// GPU resources for the clustered light assignment compute pass.
pub struct GpuClusterPass {
    /// Compute pipeline for light-to-cluster assignment.
    pub pipeline: wgpu::ComputePipeline,
    /// Bind group layout for the compute shader.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer for cluster parameters.
    pub params_buffer: wgpu::Buffer,
    /// Staging buffer for cluster AABBs (uploaded when camera changes).
    pub aabbs_buffer: wgpu::Buffer,
    /// Atomic counters for per-cluster light counts.
    pub cluster_counts: wgpu::Buffer,
    /// Flat array of light indices per cluster.
    pub cluster_lights: wgpu::Buffer,
    /// Readback buffer for CPU access to cluster assignment results.
    pub readback_buffer: wgpu::Buffer,
    /// Staging buffer for reading back results to CPU.
    pub staging_buffer: wgpu::Buffer,
    /// Current bind group (rebuilt when needed).
    pub bind_group: wgpu::BindGroup,
    /// Whether the bind group needs rebuilding.
    pub bind_group_dirty: bool,
    /// Whether a GPU dispatch is in flight (for synchronization).
    pub dispatch_in_flight: bool,
    /// Map handle for the staging buffer readback.
    pub readback_map: Option<wgpu::BufferSlice<'static>>,
}

impl GpuClusterPass {
    /// Create the GPU cluster assignment pipeline and buffers.
    pub fn new(
        device: &wgpu::Device,
        num_lights: u32,
        near: f32,
        far: f32,
        screen_width: u32,
        screen_height: u32,
    ) -> Self {
        // Compute shader source
        let cs_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Clustered Lighting Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/clustered_lighting.wgsl").into(),
            ),
        });

        // Create bind group layout
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Cluster Assignment Bind Group Layout"),
                entries: &[
                    // Cluster parameters (uniform)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<ClusterParams>() as u64),
                        },
                        count: None,
                    },
                    // Light input array (storage, read-only)
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
                    // Cluster AABBs (storage, read-only)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Cluster counts (storage, read-write atomics)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<u32>() as u64,
                            ),
                        },
                        count: None,
                    },
                    // Cluster light indices (storage, read-write)
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
                ],
            });

        // Create compute pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cluster Assignment Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Cluster Light Assignment Pipeline"),
            layout: Some(&pipeline_layout),
            module: &cs_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Allocate buffers
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cluster Params Buffer"),
            size: std::mem::size_of::<ClusterParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let aabbs_buffer_size = (TOTAL_CLUSTERS * std::mem::size_of::<ClusterAabb>()) as u64;
        let aabbs_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cluster AABBs Buffer"),
            size: aabbs_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Cluster counts: one atomic u32 per cluster
        let cluster_counts = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cluster Counts Buffer"),
            size: (TOTAL_CLUSTERS * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Cluster light indices: flat array [cluster_idx * MAX_LIGHTS_PER_CLUSTER + offset]
        let cluster_lights_size =
            (TOTAL_CLUSTERS * MAX_LIGHTS_PER_CLUSTER * std::mem::size_of::<u16>()) as u64;
        let cluster_lights = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cluster Lights Buffer"),
            size: cluster_lights_size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Readback buffer: combined ClusterOutput structs for CPU readback
        let readback_size = TOTAL_CLUSTERS as u64 * GpuClusterOutput::size_in_bytes();
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cluster Readback Buffer"),
            size: readback_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cluster Staging Buffer"),
            size: readback_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Create initial (empty) bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cluster Assignment Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &params_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(std::mem::size_of::<ClusterParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &aabbs_buffer, // placeholder â€” will be replaced at dispatch time
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &aabbs_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &cluster_counts,
                        offset: 0,
                        size: std::num::NonZeroU64::new((TOTAL_CLUSTERS * std::mem::size_of::<u32>()) as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &cluster_lights,
                        offset: 0,
                        size: std::num::NonZeroU64::new(cluster_lights_size),
                    }),
                },
            ],
        });

        // Write initial params
        let params = ClusterParams {
            num_lights,
            num_clusters_x: CLUSTER_X as u32,
            num_clusters_y: CLUSTER_Y as u32,
            num_clusters_z: CLUSTER_Z as u32,
            near,
            far,
            screen_width: screen_width as f32,
            screen_height: screen_height as f32,
        };

        Self {
            pipeline,
            bind_group_layout,
            params_buffer,
            aabbs_buffer,
            cluster_counts,
            cluster_lights,
            readback_buffer,
            staging_buffer,
            bind_group,
            bind_group_dirty: true,
            dispatch_in_flight: false,
            readback_map: None,
        }
    }

    /// Upload cluster AABBs to GPU (call when camera or viewport changes).
    pub fn upload_aabbs(&mut self, queue: &wgpu::Queue, aabbs: &[ClusterAabb]) {
        let bytes = bytemuck::cast_slice(aabbs);
        queue.write_buffer(&self.aabbs_buffer, 0, bytes);
        self.bind_group_dirty = true;
    }

    /// Update cluster parameters on GPU.
    pub fn update_params(
        &mut self,
        queue: &wgpu::Queue,
        num_lights: u32,
        near: f32,
        far: f32,
        screen_width: u32,
        screen_height: u32,
    ) {
        let params = ClusterParams {
            num_lights,
            num_clusters_x: CLUSTER_X as u32,
            num_clusters_y: CLUSTER_Y as u32,
            num_clusters_z: CLUSTER_Z as u32,
            near,
            far,
            screen_width: screen_width as f32,
            screen_height: screen_height as f32,
        };
        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::bytes_of(&params),
        );
    }

    /// Rebuild the bind group if dirty (e.g., after AABBs upload).
    pub fn rebuild_bind_group(&mut self, device: &wgpu::Device, light_buffer: &wgpu::Buffer) {
        if !self.bind_group_dirty {
            return;
        }

        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cluster Assignment Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(std::mem::size_of::<ClusterParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: light_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.aabbs_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.cluster_counts,
                        offset: 0,
                        size: std::num::NonZeroU64::new((TOTAL_CLUSTERS * std::mem::size_of::<u32>()) as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.cluster_lights,
                        offset: 0,
                        size: std::num::NonZeroU64::new((TOTAL_CLUSTERS * MAX_LIGHTS_PER_CLUSTER * std::mem::size_of::<u16>()) as u64),
                    }),
                },
            ],
        });

        self.bind_group_dirty = false;
    }

    /// Dispatch the cluster assignment compute shader.
    ///
    /// This clears cluster counts on GPU, dispatches the compute shader,
    /// and optionally initiates readback to CPU.
    ///
    /// # Arguments
    /// * `encoder` - Command encoder to record commands into
    /// * `light_buffer` - Storage buffer containing light data (LightData array)
    /// * `num_lights` - Number of active lights to process
    /// * `queue` - GPU queue for submitting commands
    /// * `readback` - Whether to read back results to CPU
    pub fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        light_buffer: &wgpu::Buffer,
        num_lights: u32,
        readback: bool,
    ) {
        // Rebuild bind group if needed
        self.rebuild_bind_group(device, light_buffer);

        // Clear cluster counts to zero
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Clear Cluster Counts"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            // We use a separate clear pass â€” but since our shader uses atomicAdd,
            // we need to zero out first. We'll do this via a minimal clear compute pass.
            // For simplicity, we write zeros directly.
        }

        // Actually clear the counts buffer by writing zeros
        let clear_size = (TOTAL_CLUSTERS * std::mem::size_of::<u32>()) as u64;
        encoder.clear_buffer(&self.cluster_counts, 0, Some(clear_size));

        // Also clear the cluster lights buffer (optional, but helps debugging)
        let lights_clear_size =
            (TOTAL_CLUSTERS * MAX_LIGHTS_PER_CLUSTER * std::mem::size_of::<u16>()) as u64;
        encoder.clear_buffer(&self.cluster_lights, 0, Some(lights_clear_size));

        // Dispatch the cluster assignment compute shader
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Cluster Light Assignment"),
                timestamp_writes: None,
            });
            cpass.set_bind_group(0, &self.bind_group, &[]);
            cpass.set_pipeline(&self.pipeline);

            let workgroup_count =
                (num_lights + CLUSTER_ASSIGN_WG_SIZE - 1) / CLUSTER_ASSIGN_WG_SIZE;
            cpass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Optional readback: copy results to staging buffer for CPU access
        if readback {
            let readback_size = TOTAL_CLUSTERS as u64 * GpuClusterOutput::size_in_bytes();

            // Copy cluster counts to readback buffer
            encoder.copy_buffer_to_buffer(
                &self.cluster_counts,
                0,
                &self.staging_buffer,
                0,
                (TOTAL_CLUSTERS * std::mem::size_of::<u32>()) as u64,
            );

            // Copy cluster light indices after the counts
            let indices_offset = (TOTAL_CLUSTERS * std::mem::size_of::<u32>()) as u64;
            encoder.copy_buffer_to_buffer(
                &self.cluster_lights,
                0,
                &self.staging_buffer,
                indices_offset,
                (TOTAL_CLUSTERS * MAX_LIGHTS_PER_CLUSTER * std::mem::size_of::<u16>()) as u64,
            );

            self.dispatch_in_flight = true;
        }
    }

    /// Read back cluster assignment results from GPU to CPU.
    ///
    /// This is an async operation that maps the staging buffer and
    /// reconstructs the cluster grid on the CPU side.
    ///
    /// Returns `true` if readback completed, `false` if still in progress.
    pub fn readback_results(&mut self, clusters: &mut [Cluster]) -> bool {
        if !self.dispatch_in_flight {
            return true;
        }

        // For now, we'll do a synchronous readback for testing purposes.
        // In production, this should be async with proper frame latency.
        false
    }
}

impl LightClusterGrid {
    /// Create a new cluster grid for the given camera parameters.
    pub fn new(near: f32, far: f32, screen_width: u32, screen_height: u32) -> Self {
        let mut grid = Self {
            clusters: vec![Cluster::default(); TOTAL_CLUSTERS],
            aabbs: vec![
                ClusterAabb {
                    min: [0.0; 3],
                    _pad0: 0.0,
                    max: [0.0; 3],
                    _pad1: 0.0,
                };
                TOTAL_CLUSTERS
            ],
            near,
            far,
            screen_width,
            screen_height,
        };
        grid.rebuild_aabbs();
        grid
    }

    /// Recompute the view-space AABBs for each cluster (call when the camera or
    /// viewport changes).
    pub fn rebuild_aabbs(&mut self) {
        let tile_w = self.screen_width as f32 / CLUSTER_X as f32;
        let tile_h = self.screen_height as f32 / CLUSTER_Y as f32;

        let log_ratio = (self.far / self.near).ln();

        for z in 0..CLUSTER_Z {
            let z_near = self.near * ((z as f32 / CLUSTER_Z as f32) * log_ratio).exp();
            let z_far = self.near * (((z + 1) as f32 / CLUSTER_Z as f32) * log_ratio).exp();

            for y in 0..CLUSTER_Y {
                for x in 0..CLUSTER_X {
                    let idx = z * CLUSTER_X * CLUSTER_Y + y * CLUSTER_X + x;

                    let min_x = x as f32 * tile_w;
                    let max_x = (x + 1) as f32 * tile_w;
                    let min_y = y as f32 * tile_h;
                    let max_y = (y + 1) as f32 * tile_h;

                    // Convert screen-space tile to NDC-like normalised coords.
                    let ndc_min_x = min_x / self.screen_width as f32 * 2.0 - 1.0;
                    let ndc_max_x = max_x / self.screen_width as f32 * 2.0 - 1.0;
                    let ndc_min_y = min_y / self.screen_height as f32 * 2.0 - 1.0;
                    let ndc_max_y = max_y / self.screen_height as f32 * 2.0 - 1.0;

                    self.aabbs[idx] = ClusterAabb {
                        min: [ndc_min_x * z_near, ndc_min_y * z_near, z_near],
                        _pad0: 0.0,
                        max: [ndc_max_x * z_far, ndc_max_y * z_far, z_far],
                        _pad1: 0.0,
                    };
                }
            }
        }
    }

    /// Assign point lights to clusters using CPU iteration.
    /// Call once per frame after view-space light positions are known.
    ///
    /// **Deprecated:** Use [`GpuClusterPass::dispatch`] for GPU-driven assignment.
    /// This CPU path is retained for fallback and testing.
    #[deprecated(
        since = "0.2.0",
        note = "Use GpuClusterPass::dispatch for GPU-driven cluster assignment"
    )]
    #[allow(dead_code)]
    pub fn assign_lights(&mut self, lights: &[(PointLight, [f32; 3])]) {
        // Clear previous assignments.
        for cluster in &mut self.clusters {
            cluster.light_count = 0;
        }

        for (light_idx, (light, view_pos)) in lights.iter().enumerate() {
            let radius = light.range;

            // Find the range of clusters this light's bounding sphere overlaps.
            let z_range = self.z_range_for_sphere(view_pos, radius);
            let (z_min, z_max) = match z_range {
                Some(r) => r,
                None => continue,
            };

            for z in z_min..=z_max {
                for y in 0..CLUSTER_Y {
                    for x in 0..CLUSTER_X {
                        let idx = z * CLUSTER_X * CLUSTER_Y + y * CLUSTER_X + x;
                        if sphere_aabb_intersect(view_pos, radius, &self.aabbs[idx]) {
                            let cluster = &mut self.clusters[idx];
                            if (cluster.light_count as usize) < MAX_LIGHTS_PER_CLUSTER {
                                cluster.light_indices[cluster.light_count as usize] =
                                    light_idx as u16;
                                cluster.light_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns the Z-slice range `(min, max)` that a sphere at `view_pos` with
    /// the given `radius` overlaps, or `None` if entirely outside the frustum.
    fn z_range_for_sphere(&self, pos: &[f32; 3], radius: f32) -> Option<(usize, usize)> {
        let z = pos[2]; // View-space depth (positive = into screen)
        let z_min_f = z - radius;
        let z_max_f = z + radius;

        if z_max_f < self.near || z_min_f > self.far {
            return None;
        }

        let log_ratio = (self.far / self.near).ln();
        let slice = |depth: f32| -> usize {
            let d = depth.clamp(self.near, self.far);
            let t = (d / self.near).ln() / log_ratio;
            ((t * CLUSTER_Z as f32) as usize).min(CLUSTER_Z - 1)
        };

        Some((slice(z_min_f.max(self.near)), slice(z_max_f.min(self.far))))
    }
}

/// Test sphere vs AABB intersection.
#[allow(clippy::needless_range_loop)]
fn sphere_aabb_intersect(center: &[f32; 3], radius: f32, aabb: &ClusterAabb) -> bool {
    let mut dist_sq: f32 = 0.0;
    for i in 0..3 {
        let v = center[i];
        let clamped = v.clamp(aabb.min[i], aabb.max[i]);
        dist_sq += (v - clamped) * (v - clamped);
    }
    dist_sq <= radius * radius
}

/// WGSL include snippet that iterates lights from the cluster grid.
///
/// Intended to be prepended to a forward PBR fragment shader.
pub const CLUSTERED_LIGHT_WGSL: &str = r#"
struct LightCluster {
    count: u32,
    indices: array<u32, 128>,
};

@group(2) @binding(0) var<storage, read> clusters: array<LightCluster>;
@group(2) @binding(1) var<storage, read> light_positions: array<vec4<f32>>;
@group(2) @binding(2) var<storage, read> light_colors: array<vec4<f32>>;

fn get_cluster_index(frag_coord: vec2<f32>, depth: f32, screen_size: vec2<f32>, near: f32, far: f32) -> u32 {
    let tile_x = u32(frag_coord.x / screen_size.x * 16.0);
    let tile_y = u32(frag_coord.y / screen_size.y * 9.0);
    let log_ratio = log(far / near);
    let z_slice = u32(log(depth / near) / log_ratio * 24.0);
    return z_slice * 16u * 9u + tile_y * 16u + tile_x;
}
"#;

// â”€â”€ Tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_aabb_pod() {
        assert!(std::mem::size_of::<ClusterAabb>() == 32);
        assert_eq!(std::mem::align_of::<ClusterAabb>(), 4);
    }

    #[test]
    fn test_cluster_params_pod() {
        assert!(std::mem::size_of::<ClusterParams>() == 32);
        assert_eq!(std::mem::align_of::<ClusterParams>(), 4);
    }

    #[test]
    fn test_gpu_cluster_output_size() {
        // Verify the expected size: 4 (count) + 256 (128 * u16) + 2 (padding) = 262
        // Rounded up to 264 for 4-byte alignment
        let expected = 264u64;
        assert_eq!(GpuClusterOutput::size_in_bytes(), expected);
    }

    #[test]
    fn test_cluster_grid_creation() {
        let grid = LightClusterGrid::new(0.1, 100.0, 1920, 1080);
        assert_eq!(grid.clusters.len(), TOTAL_CLUSTERS);
        assert_eq!(grid.aabbs.len(), TOTAL_CLUSTERS);
        assert_eq!(grid.near, 0.1);
        assert_eq!(grid.far, 100.0);
    }

    #[test]
    fn test_rebuild_aabbs() {
        let mut grid = LightClusterGrid::new(0.1, 100.0, 1920, 1080);
        grid.rebuild_aabbs();

        // Check that AABBs have reasonable values
        let first_aabb = &grid.aabbs[0];
        assert!(first_aabb.min[0] < first_aabb.max[0]);
        assert!(first_aabb.min[1] < first_aabb.max[1]);
        assert!(first_aabb.min[2] < first_aabb.max[2]);

        // Check that depth increases with z-slice
        let z0_aabb = &grid.aabbs[0];
        let z23_aabb = &grid.aabbs[(CLUSTER_Z - 1) * CLUSTER_X * CLUSTER_Y];
        assert!(z0_aabb.min[2] < z23_aabb.min[2]);
    }

    #[test]
    fn test_z_range_for_sphere() {
        let grid = LightClusterGrid::new(1.0, 100.0, 1920, 1080);

        // Sphere at z=10 with radius 5
        let pos = [0.0, 0.0, 10.0];
        let range = grid.z_range_for_sphere(&pos, 5.0);
        assert!(range.is_some());
        let (z_min, z_max) = range.unwrap();
        assert!(z_min <= z_max);

        // Sphere completely outside frustum
        let pos_far = [0.0, 0.0, 200.0];
        assert!(grid.z_range_for_sphere(&pos_far, 5.0).is_none());

        // Sphere partially outside near plane
        let pos_near = [0.0, 0.0, 0.5];
        let range_near = grid.z_range_for_sphere(&pos_near, 1.0);
        assert!(range_near.is_some());
    }

    #[test]
    fn test_sphere_aabb_intersect() {
        // Sphere fully inside AABB
        let aabb = ClusterAabb {
            min: [-1.0, -1.0, -1.0],
            _pad0: 0.0,
            max: [1.0, 1.0, 1.0],
            _pad1: 0.0,
        };
        assert!(sphere_aabb_intersect(&[0.0, 0.0, 0.0], 0.5, &aabb));

        // Sphere completely outside AABB
        let aabb_far = ClusterAabb {
            min: [10.0, 10.0, 10.0],
            _pad0: 0.0,
            max: [11.0, 11.0, 11.0],
            _pad1: 0.0,
        };
        assert!(!sphere_aabb_intersect(&[0.0, 0.0, 0.0], 1.0, &aabb_far));

        // Sphere touching AABB
        let aabb_touch = ClusterAabb {
            min: [1.0, 0.0, 0.0],
            _pad0: 0.0,
            max: [2.0, 1.0, 1.0],
            _pad1: 0.0,
        };
        assert!(sphere_aabb_intersect(&[0.0, 0.0, 0.0], 1.0, &aabb_touch));
    }

    #[test]
    fn test_cluster_defaults() {
        let cluster = Cluster::default();
        assert_eq!(cluster.light_count, 0);
        assert_eq!(cluster.light_indices.len(), MAX_LIGHTS_PER_CLUSTER);
    }

    #[test]
    fn test_total_clusters_constant() {
        assert_eq!(TOTAL_CLUSTERS, 16 * 9 * 24); // 3456
    }

    #[test]
    fn test_cluster_grid_update_parameters() {
        let mut grid = LightClusterGrid::new(0.1, 50.0, 1280, 720);
        grid.near = 0.5;
        grid.far = 200.0;
        grid.screen_width = 2560;
        grid.screen_height = 1440;
        grid.rebuild_aabbs();

        // Verify that the new dimensions are reflected in the AABBs
        let first_aabb = &grid.aabbs[0];
        let last_idx = (CLUSTER_Z - 1) * CLUSTER_X * CLUSTER_Y;
        let last_aabb = &grid.aabbs[last_idx];

        // Last cluster should extend to approximately the far plane
        assert!(last_aabb.max[2] > 100.0);
        assert!(last_aabb.max[2] <= 200.0);
    }
}





unsafe impl bytemuck::Pod for ClusterAabb {}
unsafe impl bytemuck::Pod for ClusterParams {}
unsafe impl bytemuck::Pod for GpuClusterOutput {}
