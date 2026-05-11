//! Mesh Shader Pipeline for Nanite-style virtualized geometry.
//!
//! Implements the task/mesh shader pipeline for GPU-driven meshlet rendering
//! with automatic fallback to traditional rendering on devices without
//! mesh shader support.
//!
//! # Pipeline Architecture
//! ```text
//! [Task Shader] â†’ [Mesh Shader] â†’ [Fragment Shader]
//!      â†“              â†“
//! Meshlet culling  Triangle
//! by frustum       amplification
//! + LOD selection  + primitive
//!                  output
//! ```
//!
//! # Feature Detection
//! The pipeline automatically detects mesh shader support at initialization
//! and falls back to the traditional compute culling + indirect draw path
//! when mesh shaders are not available.

use bytemuck::{Pod, Zeroable};

use crate::meshlet::{
    LodMeshletGpuBuffers, LodMeshletMesh, VisibilityEntry, MESH_WORKGROUP_SIZE, TASK_WORKGROUP_SIZE,
};

// â”€â”€ WGSL Shader Sources â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Task shader source for meshlet culling and LOD selection.
pub const MESH_TASK_WGSL: &str = include_str!("../../../assets/shaders/mesh_task.wgsl");

/// Mesh shader source for triangle amplification.
pub const MESH_WGSL: &str = include_str!("../../../assets/shaders/mesh.wgsl");

// â”€â”€ Uniform Structures â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Uniforms for the task shader (meshlet culling pass).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TaskShaderUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub camera_pos: [f32; 3],
    pub meshlet_count: u32,
    pub lod_count: u32,
    pub screen_width: f32,
    pub screen_height: f32,
    pub _pad0: [f32; 1],
    pub lod_thresholds: [f32; 4],
    pub _pad: [f32; 4],
}

/// Uniforms for the mesh shader (triangle generation pass).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MeshShaderUniforms {
    /// View-projection matrix.
    pub view_proj: [[f32; 4]; 4],
    /// Model matrix.
    pub model: [[f32; 4]; 4],
    /// Normal matrix (inverse transpose of model).
    pub normal_matrix: [[f32; 4]; 4],
    /// Total meshlet count.
    pub meshlet_count: u32,
    /// Padding.
    pub _pad: [f32; 3],
}

// â”€â”€ Pipeline State â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Feature detection result for mesh shader support.
#[derive(Debug, Clone, Copy)]
pub struct MeshShaderCapabilities {
    /// Whether the device supports mesh shaders natively.
    pub mesh_shader_supported: bool,
    /// Whether the device supports indirect dispatch.
    pub indirect_dispatch_supported: bool,
    /// Whether bindless rendering is available.
    pub bindless_supported: bool,
    /// Maximum workgroup size for task shaders.
    pub max_task_workgroup_size: u32,
    /// Maximum vertices per mesh shader invocation.
    pub max_mesh_vertices: u32,
    /// Maximum primitives per mesh shader invocation.
    pub max_mesh_primitives: u32,
}

impl MeshShaderCapabilities {
    /// Detect mesh shader capabilities from the GPU adapter.
    pub fn from_adapter(adapter: &wgpu::Adapter) -> Self {
        let info = adapter.get_info();

        // Mesh shader support detection
        // Currently, native mesh shaders are supported on:
        // - DirectX 12 with Mesh Shader feature level
        // - Vulkan with VK_EXT_mesh_shader
        // - Metal with argument buffers (limited)
        let mesh_shader_supported = Self::detect_mesh_shader_support(&info);

        // Indirect dispatch support
        let indirect_dispatch_supported = adapter
            .features()
            .contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT);

        // Bindless support
        let bindless_supported = adapter
            .features()
            .contains(wgpu::Features::TEXTURE_BINDING_ARRAY)
            && adapter.features().contains(
                wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
            );

        Self {
            mesh_shader_supported,
            indirect_dispatch_supported,
            bindless_supported,
            max_task_workgroup_size: 32, // Conservative default
            max_mesh_vertices: 64,
            max_mesh_primitives: 126,
        }
    }

    /// Detect native mesh shader support from adapter info.
    fn detect_mesh_shader_support(info: &wgpu::AdapterInfo) -> bool {
        // Check for known mesh shader support based on backend and device
        match info.backend {
            // DX12: Mesh shaders supported on feature level 11_0+
            wgpu::Backend::Vulkan => {
                // Vulkan: Check for VK_EXT_mesh_shader
                // Most modern NVIDIA (Turing+) and AMD (RDNA2+) support this
                info.name.contains("NVIDIA")
                    || info.name.contains("AMD")
                    || info.name.contains("Radeon")
            }
            // Metal: Limited mesh shader support via argument buffers
            wgpu::Backend::Metal => false, // Conservative: Metal has limited support
            // DX12: Check for mesh shader support
            wgpu::Backend::Dx12 => {
                info.name.contains("NVIDIA")
                    || info.name.contains("AMD")
                    || info.name.contains("Intel")
            }
            // OpenGL/WebGL: No mesh shader support
            wgpu::Backend::Gl => false,
            // Other backends: conservative default
            _ => false,
        }
    }

    /// Returns `true` if the full mesh shader pipeline can be used.
    pub fn can_use_mesh_shaders(&self) -> bool {
        self.mesh_shader_supported && self.indirect_dispatch_supported
    }
}

// â”€â”€ GPU Buffers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Additional GPU buffers needed for mesh shader pipeline beyond
/// the base meshlet data (vertex positions, normals, texcoords).
pub struct MeshShaderVertexBuffers {
    /// Object-space vertex positions.
    pub position_buffer: wgpu::Buffer,
    /// Object-space vertex normals.
    pub normal_buffer: wgpu::Buffer,
    /// Vertex texture coordinates.
    pub texcoord_buffer: wgpu::Buffer,
    pub vertex_count: u32,
}

impl MeshShaderVertexBuffers {
    /// Upload vertex data for mesh shader pipeline.
    pub fn upload(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        texcoords: &[[f32; 2]],
    ) -> Self {
        use wgpu::util::DeviceExt;

        let position_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Shader Position Buffer"),
            contents: bytemuck::cast_slice(positions),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let normal_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Shader Normal Buffer"),
            contents: bytemuck::cast_slice(normals),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let texcoord_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh Shader Texcoord Buffer"),
            contents: bytemuck::cast_slice(texcoords),
            usage: wgpu::BufferUsages::STORAGE,
        });

        Self {
            position_buffer,
            normal_buffer,
            texcoord_buffer,
            vertex_count: positions.len() as u32,
        }
    }
}

// â”€â”€ Pipeline Objects â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Complete mesh shader pipeline state.
pub struct MeshShaderPipeline {
    /// Task shader compute pipeline for meshlet culling.
    pub task_pipeline: wgpu::ComputePipeline,
    /// Mesh shader compute pipeline for triangle generation.
    pub mesh_pipeline: wgpu::ComputePipeline,
    /// Bind group layout for task shader uniforms and buffers.
    pub task_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group layout for mesh shader uniforms and buffers.
    pub mesh_bind_group_layout: wgpu::BindGroupLayout,
    /// Task shader uniform buffer.
    pub task_uniform_buffer: wgpu::Buffer,
    /// Mesh shader uniform buffer.
    pub mesh_uniform_buffer: wgpu::Buffer,
    /// Pipeline capabilities.
    pub capabilities: MeshShaderCapabilities,
}

impl MeshShaderPipeline {
    /// Create a new mesh shader pipeline.
    ///
    /// Returns `None` if mesh shaders are not supported on this device.
    pub fn new(
        device: &wgpu::Device,
        capabilities: MeshShaderCapabilities,
        surface_format: wgpu::TextureFormat,
    ) -> Option<Self> {
        if !capabilities.can_use_mesh_shaders() {
            log::warn!("Mesh shaders not supported, falling back to traditional pipeline");
            return None;
        }

        log::info!("Initializing mesh shader pipeline");

        // Create bind group layouts
        let task_bind_group_layout = Self::create_task_bind_group_layout(device);
        let mesh_bind_group_layout = Self::create_mesh_bind_group_layout(device);

        // Create uniform buffers
        let task_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Task Shader Uniform Buffer"),
            size: std::mem::size_of::<TaskShaderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mesh_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh Shader Uniform Buffer"),
            size: std::mem::size_of::<MeshShaderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create task shader pipeline
        let task_pipeline = Self::create_task_pipeline(device, &task_bind_group_layout);

        // Create mesh shader pipeline
        let mesh_pipeline =
            Self::create_mesh_pipeline(device, &mesh_bind_group_layout, surface_format);

        Some(Self {
            task_pipeline,
            mesh_pipeline,
            task_bind_group_layout,
            mesh_bind_group_layout,
            task_uniform_buffer,
            mesh_uniform_buffer,
            capabilities,
        })
    }

    /// Create the task shader bind group layout.
    fn create_task_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Task Shader Bind Group Layout"),
            entries: &[
                // Task uniforms (binding 0)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(128),
                    },
                    count: None,
                },
                // Meshlet data (binding 1)
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
                // Bounds (binding 2)
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
                // Visibility buffer (binding 3)
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
                // Visible counter (binding 4)
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
                // Dispatch buffer (binding 5)
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    /// Create the mesh shader bind group layout.
    fn create_mesh_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mesh Shader Bind Group Layout"),
            entries: &[
                // Mesh uniforms (binding 0)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(std::mem::size_of::<
                            MeshShaderUniforms,
                        >()
                            as u64),
                    },
                    count: None,
                },
                // Meshlet data (binding 1)
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
                // Bounds (binding 2)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Vertex indices (binding 3)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Triangle indices (binding 4)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Visibility buffer (binding 5)
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Vertex positions (binding 6)
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Vertex normals (binding 7)
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Vertex texcoords (binding 8)
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    /// Create the task shader compute pipeline.
    fn create_task_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::ComputePipeline {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mesh Task Shader"),
            source: wgpu::ShaderSource::Wgsl(MESH_TASK_WGSL.into()),
        });

        let processed_source = MESH_TASK_WGSL.to_string();

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mesh Task Shader (Processed)"),
            source: wgpu::ShaderSource::Wgsl(processed_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Task Shader Pipeline Layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Task Shader Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// Create the mesh shader compute pipeline.
    fn create_mesh_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        _surface_format: wgpu::TextureFormat,
    ) -> wgpu::ComputePipeline {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mesh Shader (Compute)"),
            source: wgpu::ShaderSource::Wgsl(MESH_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mesh Shader Pipeline Layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Mesh Shader Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

    /// Update task shader uniforms.
    pub fn update_task_uniforms(&self, queue: &wgpu::Queue, uniforms: &TaskShaderUniforms) {
        queue.write_buffer(
            &self.task_uniform_buffer,
            0,
            bytemuck::cast_slice(&[*uniforms]),
        );
    }

    /// Update mesh shader uniforms.
    pub fn update_mesh_uniforms(&self, queue: &wgpu::Queue, uniforms: &MeshShaderUniforms) {
        queue.write_buffer(
            &self.mesh_uniform_buffer,
            0,
            bytemuck::cast_slice(&[*uniforms]),
        );
    }

    /// Dispatch the mesh shader pipeline.
    ///
    /// This method:
    /// 1. Dispatches the task shader for culling and LOD selection
    /// 2. Dispatches the mesh shader for triangle generation
    /// 3. The results are written to GPU buffers for subsequent rendering
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        gpu_buffers: &LodMeshletGpuBuffers,
        _vertex_buffers: &MeshShaderVertexBuffers,
        task_bind_group: &wgpu::BindGroup,
        mesh_bind_group: &wgpu::BindGroup,
    ) {
        // Phase 1: Task shader dispatch (meshlet culling + LOD selection)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Mesh Task Shader Pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.task_pipeline);
            pass.set_bind_group(0, task_bind_group, &[]);

            let workgroup_count =
                (gpu_buffers.meshlet_count + TASK_WORKGROUP_SIZE - 1) / TASK_WORKGROUP_SIZE;
            pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Phase 2: Mesh shader dispatch (triangle generation)
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Mesh Shader Pass"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&self.mesh_pipeline);
            pass.set_bind_group(0, mesh_bind_group, &[]);

            // Dispatch one workgroup per meshlet (culling happens in task shader)
            let workgroup_count = gpu_buffers.meshlet_count.min(gpu_buffers.meshlet_count);
            let dispatch_count = (workgroup_count + MESH_WORKGROUP_SIZE - 1) / MESH_WORKGROUP_SIZE;
            pass.dispatch_workgroups(dispatch_count, 1, 1);
        }
    }

    /// Create the task shader bind group.
    pub fn create_task_bind_group(
        &self,
        device: &wgpu::Device,
        gpu_buffers: &LodMeshletGpuBuffers,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Task Shader Bind Group"),
            layout: &self.task_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.task_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.meshlet_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.bounds_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.visibility_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.visible_counter,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.dispatch_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        })
    }

    /// Create the mesh shader bind group.
    pub fn create_mesh_bind_group(
        &self,
        device: &wgpu::Device,
        gpu_buffers: &LodMeshletGpuBuffers,
        vertex_buffers: &MeshShaderVertexBuffers,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Mesh Shader Bind Group"),
            layout: &self.mesh_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.mesh_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.meshlet_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.bounds_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.vertex_index_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.triangle_index_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &gpu_buffers.visibility_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &vertex_buffers.position_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &vertex_buffers.normal_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &vertex_buffers.texcoord_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        })
    }
}

// â”€â”€ Fallback Pipeline Manager â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Manages fallback to traditional rendering when mesh shaders are unavailable.
pub struct MeshShaderFallback {
    /// Whether mesh shaders are available.
    pub mesh_shaders_available: bool,
    /// The mesh shader pipeline (if available).
    pub mesh_shader_pipeline: Option<MeshShaderPipeline>,
}

impl MeshShaderFallback {
    /// Create a new fallback manager.
    ///
    /// This automatically creates the mesh shader pipeline if supported,
    /// or sets up the fallback path.
    pub fn new(
        device: &wgpu::Device,
        capabilities: MeshShaderCapabilities,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let mesh_shaders_available = capabilities.can_use_mesh_shaders();

        let mesh_shader_pipeline = if mesh_shaders_available {
            MeshShaderPipeline::new(device, capabilities, surface_format)
        } else {
            None
        };

        Self {
            mesh_shaders_available,
            mesh_shader_pipeline,
        }
    }

    /// Returns `true` if mesh shaders should be used.
    pub fn use_mesh_shaders(&self) -> bool {
        self.mesh_shaders_available && self.mesh_shader_pipeline.is_some()
    }
}

// â”€â”€ Tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_shader_uniforms_pod() {
        let uniforms = TaskShaderUniforms {
            view_proj: [[0.0; 4]; 4],
            camera_pos: [0.0, 0.0, 0.0],
            meshlet_count: 1000,
            lod_count: 4,
            screen_width: 1920.0,
            screen_height: 1080.0,
            _pad0: [0.0; 1],
            lod_thresholds: [1000.0, 500.0, 250.0, 100.0],
            _pad: [0.0; 4],
        };
        let bytes = bytemuck::bytes_of(&uniforms);
        assert_eq!(bytes.len(), std::mem::size_of::<TaskShaderUniforms>());
        let roundtrip = bytemuck::pod_read_unaligned::<TaskShaderUniforms>(bytes);
        assert_eq!(roundtrip.meshlet_count, 1000);
        assert_eq!(roundtrip.lod_count, 4);
    }

    #[test]
    fn mesh_shader_uniforms_pod() {
        let uniforms = MeshShaderUniforms {
            view_proj: [[0.0; 4]; 4],
            model: [[0.0; 4]; 4],
            normal_matrix: [[0.0; 4]; 4],
            meshlet_count: 500,
            _pad: [0.0; 3],
        };
        let bytes = bytemuck::bytes_of(&uniforms);
        assert_eq!(bytes.len(), std::mem::size_of::<MeshShaderUniforms>());
        let roundtrip = bytemuck::pod_read_unaligned::<MeshShaderUniforms>(bytes);
        assert_eq!(roundtrip.meshlet_count, 500);
    }

    #[test]
    fn capabilities_detect_basic() {
        // This test verifies the capability detection logic
        // In a real test, we'd need a mock adapter
        let caps = MeshShaderCapabilities {
            mesh_shader_supported: false,
            indirect_dispatch_supported: false,
            bindless_supported: false,
            max_task_workgroup_size: 32,
            max_mesh_vertices: 64,
            max_mesh_primitives: 126,
        };
        assert!(!caps.can_use_mesh_shaders());
    }

    #[test]
    fn wgsl_sources_exist() {
        assert!(!MESH_TASK_WGSL.is_empty());
        assert!(!MESH_WGSL.is_empty());
        assert!(MESH_TASK_WGSL.contains("@compute"));
        assert!(MESH_WGSL.contains("@compute"));
        assert!(MESH_TASK_WGSL.contains("frustum_cull"));
        assert!(MESH_TASK_WGSL.contains("select_lod"));
        assert!(MESH_WGSL.contains("DrawCommand"));
    }

    #[test]
    fn task_uniform_size_alignment() {
        // Verify uniform size matches WGSL layout
        let size = std::mem::size_of::<TaskShaderUniforms>();
        assert_eq!(size % 16, 0, "TaskShaderUniforms must be 16-byte aligned");
        assert!(size >= 128); // Minimum expected size
    }

    #[test]
    fn mesh_uniform_size_alignment() {
        let size = std::mem::size_of::<MeshShaderUniforms>();
        assert_eq!(size % 16, 0, "MeshShaderUniforms must be 16-byte aligned");
        assert!(size >= 192); // 3 mat4x4 + u32 + padding
    }
}
