//! GPU-driven indirect draw support.
//!
//! Enables fully GPU-driven rendering where draw calls are emitted
//! by compute passes writing to DrawIndirectBuffer. Eliminates
//! CPU-GPU drawcall bottleneck for large scenes.

use std::sync::Arc;

/// Indirect draw command for meshlet rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndirect {
    /// Number of vertices to draw.
    pub vertex_count: u32,
    /// Number of instances (always 1 for meshlets).
    pub instance_count: u32,
    /// First vertex index.
    pub first_vertex: u32,
    /// First instance index.
    pub first_instance: u32,
}

/// Indexed draw command for GPU-driven rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndexedIndirect {
    /// Number of indices to draw.
    pub index_count: u32,
    /// Number of instances.
    pub instance_count: u32,
    /// First index offset.
    pub first_index: u32,
    /// Vertex offset.
    pub base_vertex: u32,
    /// First instance index.
    pub first_instance: u32,
}

/// Dispatch command for compute passes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DispatchIndirect {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

/// GPU-managed indirect draw buffer.
pub struct IndirectDrawBuffer {
    /// Buffer containing DrawIndexedIndirect commands.
    pub buffer: wgpu::Buffer,
    /// Maximum number of commands the buffer can hold.
    pub capacity: u32,
    /// Current number of valid commands.
    pub count: wgpu::Buffer, // Atomic counter on GPU
}

impl IndirectDrawBuffer {
    pub fn new(device: &wgpu::Device, capacity: u32) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("IndirectDrawBuffer"),
            size: (capacity as u64) * std::mem::size_of::<DrawIndexedIndirect>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        let count = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("IndirectDrawCount"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        Self { buffer, capacity, count }
    }
    
    /// Reset the counter for a new frame.
    pub fn reset(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.clear_buffer(&self.count, 0, Some(4));
    }
}

/// Cull meshlets on GPU and write visible ones to indirect draw buffer.
pub struct GpuMeshletCullPass {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuMeshletCullPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("meshlet_cull"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../../assets/shaders/meshlet_cull.wgsl"))),
        });
        
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("MeshletCullBindGroupLayout"),
            entries: &[
                // Meshlet data (bounds, lod info)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                // Hi-Z pyramid
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
                    count: None,
                },
                // Indirect draw buffer (output)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                // Draw counter (atomic)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("MeshletCullPipelineLayout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("MeshletCullPipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        
        Self { pipeline, bind_group_layout }
    }
    
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        meshlet_buffer: &wgpu::Buffer,
        hiz_view: &wgpu::TextureView,
        indirect_buffer: &IndirectDrawBuffer,
        meshlet_count: u32,
    ) {
        let bind_group = encoder.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("MeshletCullBindGroup"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: meshlet_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(hiz_view) },
                wgpu::BindGroupEntry { binding: 2, resource: indirect_buffer.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: indirect_buffer.count.as_entire_binding() },
            ],
        });
        
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("MeshletCull"),
            timestamp_writes: None,
        });
        
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((meshlet_count + 63) / 64, 1, 1);
    }
}
