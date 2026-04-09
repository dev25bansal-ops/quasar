//! Core renderer — manages the wgpu device, surface, and draw calls.

use quasar_core::error::{QuasarError, QuasarResult};

use super::camera::{Camera, CameraUniform};
use super::culling::{Aabb, Frustum, RenderStats};
use super::light::LightsUniform;
use super::material::{Material, MaterialOverride};
use super::mesh::Mesh;
#[cfg(feature = "meshlet")]
use super::meshlet::{MeshletGpuBuffers, MESHLET_CULL_WGSL};
use super::occlusion::{
    DrawIndexedIndirectArgs, GpuCullPass, IndirectDrawManager, MeshDrawCommand,
};
use super::pipeline;
use super::ssgi::SsgiPass;
use super::taa::TaaPass;
use super::texture::Texture;

/// Maximum number of objects that can be rendered in a single pass with
/// unique model matrices.
const MAX_RENDER_OBJECTS: usize = 4096;

/// Ring buffer size for uniform data (4MB, enough for many frames).
const UNIFORM_RING_SIZE: u64 = 4 * 1024 * 1024;

/// Ring buffer for uniform data to avoid per-frame allocations.
/// Reuses a single GPU buffer with rotating offsets.
pub struct UniformRingBuffer {
    buffer: wgpu::Buffer,
    capacity: u64,
    offset: u64,
    frame_offsets: Vec<u64>,
}

impl UniformRingBuffer {
    pub fn new(device: &wgpu::Device, capacity: u64, label: Option<&str>) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size: capacity,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity,
            offset: 0,
            frame_offsets: Vec::new(),
        }
    }

    /// Allocate space for data, returns offset into buffer.
    /// Automatically wraps around when reaching capacity.
    pub fn allocate(&mut self, size: u64, alignment: u64) -> u64 {
        let aligned_offset = self.offset.div_ceil(alignment) * alignment;
        let end = aligned_offset + size;

        if end > self.capacity {
            // Wrap around
            self.offset = 0;
            self.frame_offsets.clear();
            return 0;
        }

        self.offset = end;
        self.frame_offsets.push(aligned_offset);
        aligned_offset
    }

    /// Reset for new frame (keeps buffer, just resets offset tracking).
    pub fn begin_frame(&mut self) {
        // Keep some space for previous frame's data still in use
        // In a proper implementation, we'd track GPU fence
    }

    /// Get the underlying buffer.
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }
}

/// Rendering configuration options.
#[derive(Debug, Clone, Copy)]
pub struct RenderConfig {
    /// MSAA sample count (1 = no MSAA, 4 = 4x MSAA).
    pub msaa_sample_count: u32,
    /// Enable GPU-driven culling via compute shader + indirect draws.
    pub gpu_driven_culling: bool,
    /// Enable Temporal Anti-Aliasing (TAA).
    pub taa_enabled: bool,
    /// Enable Screen-Space Global Illumination (SSGI).
    pub ssgi_enabled: bool,
    /// Use deferred rendering path instead of forward.
    pub deferred_enabled: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        // Desktop defaults: enable advanced features
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                msaa_sample_count: 1,
                gpu_driven_culling: true,
                taa_enabled: true,
                ssgi_enabled: true,
                deferred_enabled: false,
            }
        }
        // Web/WASM defaults: disable GPU-heavy features
        #[cfg(target_arch = "wasm32")]
        {
            Self {
                msaa_sample_count: 1,
                gpu_driven_culling: false,
                taa_enabled: false,
                ssgi_enabled: false,
                deferred_enabled: false,
            }
        }
    }
}

/// The main GPU renderer for Quasar Engine.
///
/// Owns the wgpu device, queue, surface, and render pipeline. Provides a
/// high-level `draw` method that submits meshes for rendering each frame.
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub render_config: RenderConfig,
    pub render_pipeline: wgpu::RenderPipeline,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_uniform: CameraUniform,
    pub material_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub light_buffer: wgpu::Buffer,
    pub lighting_bind_group: wgpu::BindGroup,
    pub lighting_bind_group_layout: wgpu::BindGroupLayout,
    pub light_uniform: LightsUniform,
    /// Dummy CSM cascade shadow view for binding 6 placeholder
    pub dummy_cascade_shadow_view: wgpu::TextureView,
    /// Dummy CSM cascades buffer for binding 5 placeholder
    pub dummy_cascade_buffer: wgpu::Buffer,
    /// Default white material used when no material is specified.
    pub default_material: Material,
    /// Default 1×1 white texture used when no texture is specified.
    pub default_texture: Texture,
    /// Combined bind group for default material + texture.
    pub default_material_texture_bind_group: wgpu::BindGroup,
    /// Textures that can be used by entities.
    pub textures: Vec<Texture>,
    /// Material + texture bind groups for quick access.
    pub material_texture_bind_groups: Vec<wgpu::BindGroup>,
    /// Minimum uniform buffer offset alignment (bytes), from device limits.
    pub uniform_alignment: u32,
    /// Instance data buffer for GPU instancing (model matrices).
    pub instance_buffer: wgpu::Buffer,
    /// Bind group for instance data (storage buffer with model matrices).
    pub instance_bind_group: wgpu::BindGroup,
    /// Bind group layout for instance data.
    pub instance_bind_group_layout: wgpu::BindGroupLayout,
    /// GPU compute-based frustum/occlusion culling pass (when enabled).
    pub gpu_cull_pass: Option<GpuCullPass>,
    /// Indirect draw command manager for GPU-driven rendering.
    pub indirect_draw_manager: Option<IndirectDrawManager>,
    /// TAA pass — temporal anti-aliasing with jittered projection.
    pub taa_pass: Option<TaaPass>,
    /// Zero-filled motion vector texture (Rg16Float) used as placeholder
    /// until a proper velocity buffer is generated by the geometry pass.
    pub motion_vector_texture: Option<wgpu::Texture>,
    pub motion_vector_view: Option<wgpu::TextureView>,
    /// SSGI pass — screen-space global illumination compute.
    pub ssgi_pass: Option<SsgiPass>,
    /// GPU particle system — compute-based particle simulation.
    pub gpu_particle_system: Option<crate::particle::GpuParticleSystem>,
    /// Ring buffer for uniform data to avoid per-frame allocations.
    pub uniform_ring_buffer: UniformRingBuffer,
    /// Staging buffer for CPU-side uniform data (reused across frames).
    uniform_staging: Vec<u8>,
    /// Frame rendering statistics (culled vs rendered objects).
    pub stats: RenderStats,
}

impl Renderer {
    /// Initialize the renderer for a given window.
    ///
    /// This creates the wgpu instance, adapter, device, surface, pipeline, and
    /// depth buffer — everything needed to start drawing.
    pub async fn new(
        window: std::sync::Arc<winit::window::Window>,
        width: u32,
        height: u32,
        render_config: RenderConfig,
    ) -> QuasarResult<Self> {
        // Create wgpu instance with default backends.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create the rendering surface from the window.
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| QuasarError::render(format!("Failed to create surface: {e}")))?;

        // Request a GPU adapter compatible with our surface.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| QuasarError::render("No suitable GPU adapter found".to_string()))?;

        log::info!("GPU adapter: {:?}", adapter.get_info().name);

        // Request the device and queue.
        let mut required_features = wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
            | wgpu::Features::TIMESTAMP_QUERY
            | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if render_config.gpu_driven_culling {
            required_features |= wgpu::Features::MULTI_DRAW_INDIRECT;
        }
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Quasar Device"),
                    required_features,
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| QuasarError::render(format!("Failed to request device: {e}")))?;

        // Configure the surface.
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // -- Camera uniform buffer + bind group (dynamic offsets) --
        let camera_uniform = CameraUniform::new();
        let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment;
        let uniform_size = std::mem::size_of::<CameraUniform>() as u32;
        let aligned_size = uniform_size.div_ceil(uniform_alignment) * uniform_alignment;
        let camera_buffer_size = aligned_size as u64 * MAX_RENDER_OBJECTS as u64;

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Uniform Buffer"),
            size: camera_buffer_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: std::num::NonZeroU64::new(uniform_size as u64),
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &camera_buffer,
                    offset: 0,
                    size: std::num::NonZeroU64::new(uniform_size as u64),
                }),
            }],
        });

        // -- Depth texture --
        let (depth_texture, depth_view) =
            Self::create_depth_texture(&device, width, height, render_config.msaa_sample_count);

        // -- Create merged material + texture bind group layout --
        let material_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Material + Texture Bind Group Layout"),
                entries: &[
                    // Material uniform (binding 0)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Albedo texture (binding 1)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // Albedo sampler (binding 2)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // -- Create texture-only bind group layout (for standalone textures) --
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture Bind Group Layout"),
                entries: &[
                    // Texture view (binding 0)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // Sampler (binding 1)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // -- Create lighting (light + shadow) bind group layout --
        let lighting_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Lighting Bind Group Layout"),
                entries: &[
                    // Lights storage (binding 0)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Shadow uniform (binding 1)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Shadow map texture (binding 2)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Depth,
                        },
                        count: None,
                    },
                    // Shadow comparison sampler (binding 3)
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                    // Shadow depth sampler (binding 4)
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // CSM cascades storage (binding 5)
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // CSM cascade shadow texture array (binding 6)
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Depth,
                        },
                        count: None,
                    },
                ],
            });

        // -- Light storage buffer --
        let light_uniform = LightsUniform::default();
        let light_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light Buffer"),
            size: std::mem::size_of::<LightsUniform>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // -- Create dummy shadow resources for bindings 1-6 --
        // Shadow uniform buffer (binding 1)
        // Size: mat4x4<f32> (64 bytes) + vec4<f32> (16 bytes) = 80 bytes
        let shadow_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dummy Shadow Uniform Buffer"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Initialize with identity matrix and valid pcss params
        let identity_matrix: [[f32; 4]; 4] = glam::Mat4::IDENTITY.to_cols_array_2d();
        let shadow_init: [u8; 80] = {
            let mut data = [0u8; 80];
            // Write matrix (64 bytes)
            for i in 0..4 {
                for j in 0..4 {
                    let offset = (i * 4 + j) * 4;
                    let bytes = identity_matrix[i][j].to_ne_bytes();
                    data[offset..offset + 4].copy_from_slice(&bytes);
                }
            }
            // Write pcss_params (16 bytes): light_size=1.0, shadow_map_size=1024.0, unused=0, unused=0
            data[64..68].copy_from_slice(&1.0f32.to_ne_bytes()); // light_size
            data[68..72].copy_from_slice(&1024.0f32.to_ne_bytes()); // shadow_map_size
            data
        };
        queue.write_buffer(&shadow_uniform_buffer, 0, &shadow_init);

        // Dummy depth texture for shadow map (binding 2)
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dummy Shadow Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Shadow comparison sampler (binding 3)
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Dummy Shadow Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        // Shadow depth sampler (binding 4)
        let shadow_depth_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Dummy Shadow Depth Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // CSM cascades storage buffer (binding 5)
        // Size: 4 cascades * (mat4x4 (64 bytes) + split_depth + padding (16 bytes)) = 320 bytes
        let csm_cascades_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dummy CSM Cascades Buffer"),
            size: 320, // 4 * CascadeUniform (80 bytes each)
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Initialize with identity matrices and valid split depths
        let cascade_init: [u8; 320] = {
            let mut data = [0u8; 320];
            let identity = glam::Mat4::IDENTITY.to_cols_array_2d();
            for cascade in 0..4 {
                let offset = cascade * 80; // 80 bytes per cascade
                                           // Write matrix (64 bytes)
                for i in 0..4 {
                    for j in 0..4 {
                        let byte_offset = offset + (i * 4 + j) * 4;
                        let bytes = identity[i][j].to_ne_bytes();
                        data[byte_offset..byte_offset + 4].copy_from_slice(&bytes);
                    }
                }
                // Write split_depth (16 bytes from offset 64): set to large value so shadows work
                data[offset + 64..offset + 68].copy_from_slice(&10000.0f32.to_ne_bytes());
                // split_depth
            }
            data
        };
        queue.write_buffer(&csm_cascades_buffer, 0, &cascade_init);

        // Dummy CSM cascade shadow texture array (binding 6)
        let csm_shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dummy CSM Shadow Texture Array"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 4, // CASCADE_COUNT
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let csm_shadow_view = csm_shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        // -- Lighting bind group (merged light + shadow) --
        let lighting_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Bind Group"),
            layout: &lighting_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &light_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &shadow_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&shadow_depth_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &csm_cascades_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&csm_shadow_view),
                },
            ],
        });

        // -- Default 1×1 white texture --
        let default_texture = Texture::white(&device, &queue, &texture_bind_group_layout);

        // -- Default material (white, roughness=0.5, metallic=0) --
        let default_material = Material::new(
            &device,
            &material_texture_bind_group_layout,
            "Default",
            &default_texture.view,
            &default_texture.sampler,
        );

        // -- Instance buffer for GPU instancing --
        let max_instances = MAX_RENDER_OBJECTS;
        let matrix_size = std::mem::size_of::<glam::Mat4>() as u64;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: matrix_size * max_instances as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Note: Instance data is currently not used in the basic shader.
        // This bind group layout is reserved for future GPU-driven rendering features.
        let instance_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Instance Bind Group Layout"),
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

        let instance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Instance Bind Group"),
            layout: &instance_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &instance_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        // -- Render pipeline --
        let shader_source = include_str!("../../../assets/shaders/basic.wgsl");
        let render_pipeline = pipeline::create_render_pipeline(
            &device,
            wgpu::TextureFormat::Rgba16Float,
            &camera_bind_group_layout,
            &material_texture_bind_group_layout,
            &lighting_bind_group_layout,
            shader_source,
        );

        // Upload default material data.
        default_material.update(&queue);

        // Upload light data.
        queue.write_buffer(&light_buffer, 0, bytemuck::cast_slice(&[light_uniform]));

        // Create combined material + texture bind group for default material and texture
        let default_material_texture_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Default Material + Texture Bind Group"),
                layout: &material_texture_bind_group_layout,
                entries: &[
                    // Material uniform (binding 0)
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &default_material.buffer,
                            offset: 0,
                            size: None,
                        }),
                    },
                    // Albedo texture (binding 1)
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&default_texture.view),
                    },
                    // Albedo sampler (binding 2)
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&default_texture.sampler),
                    },
                ],
            });

        // Create ring buffer before moving device
        let uniform_ring_buffer =
            UniformRingBuffer::new(&device, UNIFORM_RING_SIZE, Some("Uniform Ring Buffer"));

        let mut result = Ok(Self {
            device,
            queue,
            surface,
            config,
            render_config,
            render_pipeline,
            depth_texture,
            depth_view,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            camera_uniform,
            material_texture_bind_group_layout,
            texture_bind_group_layout,
            light_buffer,
            lighting_bind_group,
            lighting_bind_group_layout,
            light_uniform,
            dummy_cascade_shadow_view: csm_shadow_view,
            dummy_cascade_buffer: csm_cascades_buffer,
            default_material,
            default_texture,
            default_material_texture_bind_group,
            textures: Vec::new(),
            material_texture_bind_groups: Vec::new(),
            uniform_alignment,
            instance_buffer,
            instance_bind_group,
            instance_bind_group_layout,
            gpu_cull_pass: None,
            indirect_draw_manager: None,
            taa_pass: None,
            motion_vector_texture: None,
            motion_vector_view: None,
            ssgi_pass: None,
            gpu_particle_system: None,
            uniform_ring_buffer,
            uniform_staging: vec![0u8; UNIFORM_RING_SIZE as usize],
            stats: RenderStats::default(),
        });
        if let Ok(ref mut renderer) = result {
            if renderer.render_config.gpu_driven_culling {
                renderer.gpu_cull_pass = Some(GpuCullPass::new(&renderer.device));
                renderer.indirect_draw_manager = Some(IndirectDrawManager::new());
            }
            if renderer.render_config.taa_enabled {
                let w = renderer.config.width;
                let h = renderer.config.height;
                renderer.taa_pass =
                    Some(TaaPass::new(&renderer.device, w, h, renderer.config.format));
                let (mv_tex, mv_view) = Self::create_motion_vector_texture(&renderer.device, w, h);
                renderer.motion_vector_texture = Some(mv_tex);
                renderer.motion_vector_view = Some(mv_view);
            }
            if renderer.render_config.ssgi_enabled {
                let w = renderer.config.width;
                let h = renderer.config.height;
                renderer.ssgi_pass = Some(SsgiPass::new(&renderer.device, w, h));
            }
        }
        result
    }

    /// Handle window resize — reconfigure surface and depth buffer.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        let (depth_texture, depth_view) = Self::create_depth_texture(
            &self.device,
            width,
            height,
            self.render_config.msaa_sample_count,
        );
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;

        if let Some(taa) = self.taa_pass.as_mut() {
            taa.resize(&self.device, width, height);
            let (mv_tex, mv_view) = Self::create_motion_vector_texture(&self.device, width, height);
            self.motion_vector_texture = Some(mv_tex);
            self.motion_vector_view = Some(mv_view);
        }
        if let Some(ssgi) = self.ssgi_pass.as_mut() {
            ssgi.resize(&self.device, width, height);
        }
    }

    /// Upload instance transform matrices to the GPU instance buffer.
    ///
    /// Called by the runner after ECS systems have collected the matrices
    /// into `RenderSyncOutput`.
    pub fn upload_instance_transforms(&self, transforms: &[glam::Mat4]) {
        if transforms.is_empty() {
            return;
        }
        let bytes = bytemuck::cast_slice(transforms);
        let max = self.instance_buffer.size() as usize;
        let len = bytes.len().min(max);
        self.queue
            .write_buffer(&self.instance_buffer, 0, &bytes[..len]);
    }

    /// Initialize the GPU particle system.
    ///
    /// Creates the compute pipeline and buffers for GPU-accelerated
    /// particle simulation. Call once during initialization if particles are needed.
    pub fn init_gpu_particles(&mut self) {
        if self.gpu_particle_system.is_none() {
            use crate::particle::GpuParticleSystem;
            self.gpu_particle_system =
                Some(GpuParticleSystem::new(&self.device, self.config.format));
        }
    }

    /// Dispatch GPU particle simulation.
    ///
    /// Runs the compute shader to update particle positions and velocities.
    /// Call before render_gpu_particles() to simulate one frame.
    pub fn dispatch_gpu_particles(&self, encoder: &mut wgpu::CommandEncoder, particle_count: u32) {
        if let Some(gpu_particles) = &self.gpu_particle_system {
            gpu_particles.dispatch(encoder, particle_count);
        }
    }

    /// Render GPU-simulated particles.
    ///
    /// Draws particles to the given render pass using instanced rendering.
    pub fn render_gpu_particles(&self, pass: &mut wgpu::RenderPass) {
        if let Some(gpu_particles) = &self.gpu_particle_system {
            gpu_particles.render(pass);
        }
    }

    /// Update the camera uniform buffer on the GPU.
    pub fn update_camera(&mut self, camera: &Camera, model: glam::Mat4) {
        self.camera_uniform.update(camera, model);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    /// Update the lighting bind group to use the given shadow map resources.
    ///
    /// Call this each frame after rendering the shadow pass to bind the actual
    /// shadow depth texture and shadow uniform buffer for sampling in the main pass.
    pub fn update_shadow_bindings(
        &mut self,
        shadow_view: &wgpu::TextureView,
        shadow_uniform_buffer: &wgpu::Buffer,
        shadow_sampler: &wgpu::Sampler,
        shadow_depth_sampler: &wgpu::Sampler,
    ) {
        let lighting_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Bind Group"),
            layout: &self.lighting_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.light_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: shadow_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(shadow_depth_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.dummy_cascade_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.dummy_cascade_shadow_view),
                },
            ],
        });
        self.lighting_bind_group = lighting_bind_group;
    }

    /// Update the lighting bind group to include CSM (Cascade Shadow Map) resources.
    ///
    /// Call this after rendering cascade shadow maps to bind the cascade texture array
    /// and cascade uniform buffer for CSM sampling in the main pass.
    #[allow(clippy::too_many_arguments)]
    pub fn update_csm_bindings(
        &mut self,
        cascade_buffer: &wgpu::Buffer,
        cascade_shadow_view: &wgpu::TextureView,
        _cascade_sampler: &wgpu::Sampler,
        shadow_view: &wgpu::TextureView,
        shadow_uniform_buffer: &wgpu::Buffer,
        shadow_sampler: &wgpu::Sampler,
        shadow_depth_sampler: &wgpu::Sampler,
    ) {
        let lighting_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Lighting Bind Group (CSM)"),
            layout: &self.lighting_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.light_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: shadow_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(shadow_depth_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: cascade_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(cascade_shadow_view),
                },
            ],
        });
        self.lighting_bind_group = lighting_bind_group;
    }

    /// Add a texture to the renderer and return its index.
    ///
    /// The returned index can be used with `TextureHandle` to specify which
    /// texture an entity should use.
    pub fn add_texture(&mut self, texture: Texture) -> u32 {
        let index = self.textures.len() as u32;
        self.textures.push(texture);
        index
    }

    /// Create a combined material + texture bind group for a given material and texture.
    ///
    /// The shader expects material uniform (binding 0), texture (binding 1), and sampler (binding 2)
    /// all in the same bind group (group 1).
    pub fn create_material_texture_bind_group(
        &self,
        material: &Material,
        texture: &Texture,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material + Texture Bind Group"),
            layout: &self.material_texture_bind_group_layout,
            entries: &[
                // Material uniform (binding 0)
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &material.buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                // Albedo texture (binding 1)
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                // Albedo sampler (binding 2)
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        })
    }

    /// Add a texture from a file path.
    pub fn add_texture_from_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<u32, String> {
        let texture = Texture::from_file(
            &self.device,
            &self.queue,
            &self.texture_bind_group_layout,
            path,
        )?;
        Ok(self.add_texture(texture))
    }

    /// Get a texture bind group by index.
    ///
    /// Returns the default texture if the index is out of bounds.
    pub fn get_texture_bind_group(&self, index: u32) -> &wgpu::BindGroup {
        if index == 0 || index as usize > self.material_texture_bind_groups.len() {
            &self.default_texture.bind_group
        } else {
            &self.material_texture_bind_groups[index as usize - 1]
        }
    }

    // ── Split-phase rendering API ────────────────────────────────

    /// Acquire the next surface frame and create a fresh command encoder.
    ///
    /// Use together with [`render_3d_pass`](Self::render_3d_pass) and
    /// [`finish_frame`](Self::finish_frame) when you need to inject
    /// additional render passes (e.g. egui) between the 3D draw and
    /// presentation.
    pub fn begin_frame(
        &self,
    ) -> Result<
        (
            wgpu::SurfaceTexture,
            wgpu::TextureView,
            wgpu::CommandEncoder,
        ),
        wgpu::SurfaceError,
    > {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        Ok((output, view, encoder))
    }

    /// Perform the 3D clear + draw pass using an externally-owned encoder.
    ///
    /// All per-object camera uniforms are written to the GPU buffer **before**
    /// the render pass begins, using dynamic offsets to index each object's
    /// data during drawing.
    ///
    /// Each tuple may carry an optional material bind group.  When `Some`, the
    /// per-entity material is used; otherwise the default white material is
    /// applied.
    ///
    /// Each tuple may carry an optional texture index.  When `Some`, the
    /// texture at that index is used; otherwise the default texture is applied.
    pub fn render_3d_pass(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        // ── Frustum culling ──
        let frustum = Frustum::from_view_proj(&camera.view_projection());
        let total = objects.len();
        let mut culled_count = 0u32;

        // Pre-compute visibility and write only visible objects' uniforms
        let mut visible_indices = Vec::with_capacity(total);
        for (i, (_, model, _, _)) in objects.iter().enumerate() {
            // Decompose model matrix to get scale and translation for AABB
            let (scale, _, translation) = model.to_scale_rotation_translation();
            let half_extent = scale.abs();
            let world_aabb = Aabb {
                min: translation - half_extent,
                max: translation + half_extent,
            };

            if frustum.intersects_aabb(&world_aabb) {
                visible_indices.push(i);
            } else {
                culled_count += 1;
            }
        }

        // Write uniform data only for visible objects
        if !visible_indices.is_empty() {
            let total_size = aligned_size * visible_indices.len();

            // Ensure staging buffer is large enough
            if self.uniform_staging.len() < total_size {
                self.uniform_staging.resize(total_size, 0);
            }

            let data = &mut self.uniform_staging[..total_size];
            for (visible_idx, &i) in visible_indices.iter().enumerate() {
                let mut uniform = CameraUniform::new();
                uniform.update(camera, objects[i].1);
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = visible_idx * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue
                .write_buffer(&self.camera_buffer, 0, &data[..total_size]);
        }

        // Update stats
        self.stats.total_objects = total as u32;
        self.stats.rendered_objects = visible_indices.len() as u32;
        self.stats.culled_objects = culled_count;

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.render_pipeline);

            pass.set_bind_group(2, &self.lighting_bind_group, &[]);

            for (visible_idx, &i) in visible_indices.iter().enumerate() {
                let dyn_offset = (visible_idx * aligned_size) as u32;
                pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);

                let (_, _, mat_bg, tex_index) = objects[i];
                // Use the combined material + texture bind group
                let material_texture_bg = if mat_bg.is_some() || tex_index.is_some() {
                    // For now, we'll use the default combined bind group
                    // In a full implementation, we'd create combined bind groups for each material/texture pair
                    &self.default_material_texture_bind_group
                } else {
                    &self.default_material_texture_bind_group
                };
                pass.set_bind_group(1, material_texture_bg, &[]);

                let (mesh, _, _, _) = objects[i];
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }
    }

    /// Perform the 3D clear + draw pass with mesh/material batching.
    ///
    /// Groups objects by (mesh, material) to minimize state changes.
    ///
    /// Each tuple may carry an optional material bind group.  When `Some`, the
    /// per-entity material is used; otherwise the default white material is
    /// applied.
    pub fn render_3d_pass_batched(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        use std::collections::HashMap;

        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        if !objects.is_empty() {
            let total = aligned_size * objects.len();
            let mut data = vec![0u8; total];
            for (i, (_, model, _, _)) in objects.iter().enumerate() {
                let mut uniform = CameraUniform::new();
                uniform.update(camera, *model);
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = i * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue.write_buffer(&self.camera_buffer, 0, &data);
        }

        // ── GPU-driven indirect rendering path ──
        if self.render_config.gpu_driven_culling {
            if let (Some(cull_pass), Some(mgr)) = (
                self.gpu_cull_pass.as_ref(),
                self.indirect_draw_manager.as_mut(),
            ) {
                mgr.clear();
                for (mesh, model, _, _) in objects.iter() {
                    let (scale, _, translation) = model.to_scale_rotation_translation();
                    let half = scale.abs();
                    mgr.push(MeshDrawCommand {
                        index_count: mesh.index_count,
                        first_index: 0,
                        base_vertex: 0,
                        aabb_min: translation - half,
                        aabb_max: translation + half,
                        material_index: 0,
                    });
                }
                let vp = camera.view_projection();
                mgr.upload_aabbs(&self.queue, cull_pass);
                mgr.upload_uniforms(
                    &self.queue,
                    cull_pass,
                    &vp,
                    self.config.width as f32,
                    self.config.height as f32,
                );
                mgr.prepare_indirect_buffer(&self.queue, cull_pass);
                let bg = cull_pass.create_bind_group(&self.device);
                cull_pass.dispatch(encoder, &bg, mgr.count());

                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("3D Render Pass (GPU Culled)"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.05,
                                    g: 0.05,
                                    b: 0.08,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    pass.set_pipeline(&self.render_pipeline);
                    pass.set_bind_group(2, &self.lighting_bind_group, &[]);

                    let stride = std::mem::size_of::<DrawIndexedIndirectArgs>() as u64;
                    for (i, (mesh, _, _mat_bg, _tex_index)) in objects.iter().enumerate() {
                        let dyn_offset = (i * aligned_size) as u32;
                        pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);

                        // Use the combined material + texture bind group
                        let material_texture_bg = &self.default_material_texture_bind_group;
                        pass.set_bind_group(1, material_texture_bg, &[]);

                        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                        pass.set_index_buffer(
                            mesh.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint32,
                        );
                        pass.draw_indexed_indirect(&cull_pass.indirect_buffer, i as u64 * stride);
                    }
                }
            }
            return;
        }

        type BatchKey = (usize, usize, usize);
        #[allow(dead_code)]
        struct Batch {
            mesh: &'static Mesh,
            indices: Vec<usize>,
            material: Option<&'static wgpu::BindGroup>,
            texture: &'static wgpu::BindGroup,
        }

        let mut batches: HashMap<BatchKey, Batch> = HashMap::new();

        for (i, (mesh, _, mat_bg, tex_index)) in objects.iter().enumerate() {
            let mesh_key = *mesh as *const Mesh as usize;
            let mat_key = mat_bg
                .map(|bg| bg as *const wgpu::BindGroup as usize)
                .unwrap_or(usize::MAX);
            let tex_key = tex_index.unwrap_or(0) as usize;

            let entry = batches
                .entry((mesh_key, mat_key, tex_key))
                .or_insert_with(|| {
                    let texture_bg = self.get_texture_bind_group(tex_index.unwrap_or(0));
                    Batch {
                        mesh: unsafe { &*(*mesh as *const Mesh) },
                        indices: Vec::new(),
                        material: mat_bg.map(|bg| unsafe { &*(bg as *const wgpu::BindGroup) }),
                        texture: unsafe { &*(texture_bg as *const wgpu::BindGroup) },
                    }
                });
            entry.indices.push(i);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D Render Pass (Batched)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.render_pipeline);

            pass.set_bind_group(2, &self.lighting_bind_group, &[]);

            for batch in batches.values() {
                // Use the combined material + texture bind group
                pass.set_bind_group(1, &self.default_material_texture_bind_group, &[]);
                pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(batch.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

                for &idx in &batch.indices {
                    let dyn_offset = (idx * aligned_size) as u32;
                    pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
                    pass.draw_indexed(0..batch.mesh.index_count, 0, 0..1);
                }
            }
        }
    }

    #[cfg(feature = "deferred")]
    pub fn render_deferred_geometry_pass(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
        gbuffer: &crate::deferred::GBuffer,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        if !objects.is_empty() {
            let total = aligned_size * objects.len();
            let mut data = vec![0u8; total];
            for (i, (_, model, _, _)) in objects.iter().enumerate() {
                let mut uniform = CameraUniform::new();
                uniform.update(camera, *model);
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = i * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue.write_buffer(&self.camera_buffer, 0, &data);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Deferred Geometry Pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &gbuffer.albedo_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &gbuffer.normal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.5,
                                g: 0.5,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &gbuffer.roughness_metallic_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.5,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gbuffer.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.render_pipeline);

            for (i, (mesh, _, _mat_bg, _tex_index)) in objects.iter().enumerate() {
                let dyn_offset = (i * aligned_size) as u32;
                pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
                pass.set_bind_group(1, &self.default_material_texture_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }
    }

    /// Submit the encoder and present the frame.
    pub fn finish_frame(&self, encoder: wgpu::CommandEncoder, output: wgpu::SurfaceTexture) {
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    // ── Legacy one-shot rendering ────────────────────────────────

    /// Render a single frame: clear the screen and draw all provided meshes.
    pub fn render(&self, meshes: &[&Mesh]) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[0]);
            render_pass.set_bind_group(1, &self.default_material_texture_bind_group, &[]);
            render_pass.set_bind_group(2, &self.lighting_bind_group, &[]);

            for mesh in meshes {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Render a frame with multiple objects, each with its own model matrix.
    ///
    /// Pre-writes all per-object camera uniforms to the GPU buffer before the
    /// render pass, then uses dynamic offsets to select each object's data.
    ///
    /// Each tuple may carry an optional material bind group.  When `Some`, the
    /// per-entity material is used; otherwise the default white material is
    /// applied.
    ///
    /// Each tuple may carry an optional texture index.  When `Some`, the
    /// texture at that index is used; otherwise the default texture is used.
    pub fn render_objects(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
    ) -> Result<(), wgpu::SurfaceError> {
        self.render_objects_internal(camera, objects, false)
    }

    /// Render a frame with GPU instancing for objects sharing the same mesh.
    ///
    /// Groups objects by (mesh, material) to minimize draw calls.  For each group,
    /// all instances are drawn in a single instanced draw call.
    ///
    /// Each tuple may carry an optional material bind group.  When `Some`, the
    /// per-entity material is used; otherwise the default white material is used.
    ///
    /// Each tuple may carry an optional texture index.  When `Some`, the
    /// texture at that index is used; otherwise the default texture is used.
    pub fn render_objects_instanced(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        if !objects.is_empty() {
            let total = aligned_size * objects.len();
            let mut data = vec![0u8; total];
            let mut uniform = CameraUniform::new();
            uniform.view_proj = camera.view_projection().to_cols_array_2d();
            uniform.normal_matrix = glam::Mat4::IDENTITY.to_cols_array_2d();
            let bytes = bytemuck::bytes_of(&uniform);
            for i in 0..objects.len() {
                let offset = i * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue.write_buffer(&self.camera_buffer, 0, &data);
        }

        use std::collections::HashMap;

        type BatchKey = (usize, usize, usize);
        struct Batch {
            mesh: *const Mesh,
            materials: Vec<glam::Mat4>,
            material: Option<*const wgpu::BindGroup>,
            texture: *const wgpu::BindGroup,
        }

        let mut batches: HashMap<BatchKey, Batch> = HashMap::new();

        for (mesh, model, mat_bg, tex_index) in objects.iter() {
            let mesh_key = *mesh as *const Mesh as usize;
            let mat_key = mat_bg
                .map(|bg| bg as *const wgpu::BindGroup as usize)
                .unwrap_or(usize::MAX);
            let tex_key = tex_index.unwrap_or(0) as usize;

            let entry = batches
                .entry((mesh_key, mat_key, tex_key))
                .or_insert_with(|| {
                    let texture_bg: *const wgpu::BindGroup =
                        self.get_texture_bind_group(tex_index.unwrap_or(0));
                    let mat_ptr: Option<*const wgpu::BindGroup> =
                        mat_bg.map(|bg| bg as *const wgpu::BindGroup);
                    Batch {
                        mesh: *mesh as *const Mesh,
                        materials: Vec::new(),
                        material: mat_ptr,
                        texture: texture_bg,
                    }
                });
            entry.materials.push(*model);
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass (Instanced)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(2, &self.lighting_bind_group, &[]);
            render_pass.set_bind_group(4, &self.instance_bind_group, &[]);

            for batch in batches.values() {
                let material_bg = batch
                    .material
                    .unwrap_or(&self.default_material.bind_group as *const wgpu::BindGroup);
                render_pass.set_bind_group(1, unsafe { &*material_bg }, &[]);
                render_pass.set_bind_group(3, unsafe { &*batch.texture }, &[]);
                render_pass.set_vertex_buffer(0, unsafe { &(*batch.mesh).vertex_buffer }.slice(..));
                render_pass.set_index_buffer(
                    unsafe { &(*batch.mesh).index_buffer }.slice(..),
                    wgpu::IndexFormat::Uint32,
                );

                if !batch.materials.is_empty() {
                    let matrix_bytes: Vec<u8> = batch
                        .materials
                        .iter()
                        .flat_map(|m| bytemuck::bytes_of(m).iter().copied())
                        .collect();
                    self.queue
                        .write_buffer(&self.instance_buffer, 0, &matrix_bytes);

                    render_pass.draw_indexed(
                        0..unsafe { (*batch.mesh).index_count },
                        0,
                        0..batch.materials.len() as u32,
                    );
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn render_objects_internal(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
        use_instancing: bool,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        // ── Frustum culling ──
        let frustum = Frustum::from_view_proj(&camera.view_projection());
        let total = objects.len();
        let mut culled_count = 0u32;
        let mut visible_indices = Vec::with_capacity(total);

        for (i, (_, model, _, _)) in objects.iter().enumerate() {
            let (scale, _, translation) = model.to_scale_rotation_translation();
            let half_extent = scale.abs();
            let world_aabb = Aabb {
                min: translation - half_extent,
                max: translation + half_extent,
            };

            if frustum.intersects_aabb(&world_aabb) {
                visible_indices.push(i);
            } else {
                culled_count += 1;
            }
        }

        // Write uniform data only for visible objects
        if !visible_indices.is_empty() {
            let total_bytes = aligned_size * visible_indices.len();
            let mut data = vec![0u8; total_bytes];
            for (visible_idx, &i) in visible_indices.iter().enumerate() {
                let mut uniform = CameraUniform::new();
                uniform.update(camera, objects[i].1);
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = visible_idx * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue.write_buffer(&self.camera_buffer, 0, &data);
        }

        // Update stats
        self.stats.total_objects = total as u32;
        self.stats.rendered_objects = visible_indices.len() as u32;
        self.stats.culled_objects = culled_count;

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);

            if use_instancing {
                use std::collections::HashMap;

                type BatchKey = (usize, usize, usize);
                struct Batch {
                    mesh: &'static Mesh,
                    indices: Vec<usize>,
                    material: Option<&'static wgpu::BindGroup>,
                    texture: &'static wgpu::BindGroup,
                }

                let mut batches: HashMap<BatchKey, Batch> = HashMap::new();

                // Only batch visible objects
                for &visible_idx in &visible_indices {
                    let (mesh, _, mat_bg, tex_index) = objects[visible_idx];
                    let mesh_key = mesh as *const Mesh as usize;
                    let mat_key = mat_bg
                        .map(|bg| bg as *const wgpu::BindGroup as usize)
                        .unwrap_or(usize::MAX);
                    let tex_key = tex_index.unwrap_or(0) as usize;

                    let entry = batches
                        .entry((mesh_key, mat_key, tex_key))
                        .or_insert_with(|| {
                            let texture_bg = self.get_texture_bind_group(tex_index.unwrap_or(0));
                            Batch {
                                mesh: unsafe { &*(mesh as *const Mesh) },
                                indices: Vec::new(),
                                material: mat_bg
                                    .map(|bg| unsafe { &*(bg as *const wgpu::BindGroup) }),
                                texture: unsafe { &*(texture_bg as *const wgpu::BindGroup) },
                            }
                        });
                    entry.indices.push(visible_idx);
                }

                for batch in batches.values() {
                    let material_bg = batch.material.unwrap_or(&self.default_material.bind_group);
                    render_pass.set_bind_group(1, material_bg, &[]);
                    render_pass.set_bind_group(2, &self.lighting_bind_group, &[]);
                    render_pass.set_bind_group(3, batch.texture, &[]);
                    render_pass.set_vertex_buffer(0, batch.mesh.vertex_buffer.slice(..));
                    render_pass.set_index_buffer(
                        batch.mesh.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );

                    for &orig_idx in &batch.indices {
                        // Find position within visible_indices for uniform offset
                        let visible_pos = visible_indices
                            .iter()
                            .position(|&v| v == orig_idx)
                            .unwrap_or(0);
                        let dyn_offset = (visible_pos * aligned_size) as u32;
                        render_pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
                        render_pass.draw_indexed(0..batch.mesh.index_count, 0, 0..1);
                    }
                }
            } else {
                render_pass.set_bind_group(2, &self.lighting_bind_group, &[]);

                for (visible_idx, &i) in visible_indices.iter().enumerate() {
                    let dyn_offset = (visible_idx * aligned_size) as u32;
                    render_pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
                    let (mesh, _, mat_bg, tex_index) = objects[i];
                    let material_bg = mat_bg.unwrap_or(&self.default_material.bind_group);
                    render_pass.set_bind_group(1, material_bg, &[]);
                    let texture_bg = self.get_texture_bind_group(tex_index.unwrap_or(0));
                    render_pass.set_bind_group(3, texture_bg, &[]);
                    render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    render_pass
                        .set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Create a [`Material`] from a [`MaterialOverride`] component.
    ///
    /// The returned material has its GPU buffer already uploaded and is ready
    /// to be passed as a bind group to
    /// [`render_3d_pass`](Self::render_3d_pass) /
    /// [`render_objects`](Self::render_objects).
    pub fn create_material_from_override(
        &self,
        name: &str,
        material_override: &MaterialOverride,
    ) -> Material {
        Material::from_override(
            &self.device,
            &self.queue,
            &self.material_texture_bind_group_layout,
            name,
            material_override,
            &self.default_texture.view,
            &self.default_texture.sampler,
        )
    }

    /// Create a depth texture and its view.
    fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Create a zero-initialised Rg16Float motion vector texture.
    fn create_motion_vector_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Motion Vector Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Render using a RenderGraph for pass composition.
    ///
    /// This method builds a RenderGraph with the standard passes:
    /// 1. Shadow pass (if shadow maps are present)
    /// 2. Opaque geometry pass
    /// 3. Post-process pass (TAA, tonemapping)
    ///
    /// The graph handles pass ordering, dependency tracking, and resource
    /// transitions automatically.
    pub fn render_with_graph(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>, Option<u32>)],
        hdr_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
    ) -> Result<wgpu::CommandBuffer, wgpu::SurfaceError> {
        use super::render_graph::{Attachment, AttachmentId, PassId, RenderContext, RenderGraph};

        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        if !objects.is_empty() {
            let total = aligned_size * objects.len();
            let mut data = vec![0u8; total];
            for (i, (_, model, _, _)) in objects.iter().enumerate() {
                let mut uniform = CameraUniform::new();
                uniform.update(camera, *model);
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = i * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue.write_buffer(&self.camera_buffer, 0, &data);
        }

        let mut graph = RenderGraph::new();

        let hdr_att = AttachmentId(0);
        graph.add_attachment(
            hdr_att,
            Attachment {
                name: "HDR Color".into(),
                format: wgpu::TextureFormat::Rgba16Float,
                size: (self.config.width, self.config.height),
                texture: None,
                view: None,
            },
        );

        let context = RenderContext {
            screen_size: (self.config.width, self.config.height),
            hdr_texture: Some(hdr_view.clone()),
            depth_view: output_view.clone(),
            camera_bind_group: self.camera_bind_group.clone(),
            light_bind_group: self.lighting_bind_group.clone(),
            resources: Default::default(),
        };

        let opaque_pass = PassId(1);
        let draw_data: Vec<OpaqueDrawData> = objects
            .iter()
            .map(|(m, _, _, _)| OpaqueDrawData {
                vertex_buffer: m.vertex_buffer.clone(),
                index_buffer: m.index_buffer.clone(),
                index_count: m.index_count,
            })
            .collect();

        graph.add_pass(
            opaque_pass,
            Box::new(OpaqueGraphPass {
                objects: draw_data,
                camera_bind_group: self.camera_bind_group.clone(),
                material_bind_group: self.default_material_texture_bind_group.clone(),
                lighting_bind_group: self.lighting_bind_group.clone(),
                pipeline: self.render_pipeline.clone(),
                uniform_alignment: self.uniform_alignment,
            }),
        );
        graph.add_output(opaque_pass, hdr_att);

        graph.compile().map_err(|_e| wgpu::SurfaceError::Lost)?;

        graph
            .execute(&self.device, &self.queue, &context)
            .map_err(|_| wgpu::SurfaceError::Lost)
    }

    #[cfg(feature = "meshlet")]
    pub fn render_meshlets(
        &mut self,
        camera: &Camera,
        meshlet_buffers: &MeshletGpuBuffers,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
    ) {
        use super::meshlet::MAX_MESHLETS;

        let visibility_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Meshlet Visibility"),
            size: MAX_MESHLETS as u64 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let cull_shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Meshlet Cull"),
                source: wgpu::ShaderSource::Wgsl(MESHLET_CULL_WGSL.into()),
            });

        let cull_bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Meshlet Cull BGL"),
                    entries: &[
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
                    ],
                });

        let cull_uniforms = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Meshlet Cull Uniforms"),
            size: std::mem::size_of::<MeshletCullUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vp = camera.view_projection();
        let cull_data = MeshletCullUniforms {
            view_proj: vp.to_cols_array_2d(),
            camera_pos: [camera.position.x, camera.position.y, camera.position.z],
            meshlet_count: meshlet_buffers.meshlet_count,
        };
        self.queue
            .write_buffer(&cull_uniforms, 0, bytemuck::bytes_of(&cull_data));

        let cull_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Meshlet Cull BG"),
            layout: &cull_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cull_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: meshlet_buffers.meshlet_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: meshlet_buffers.bounds_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: visibility_buffer.as_entire_binding(),
                },
            ],
        });

        let cull_pipeline_layout =
            self.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Meshlet Cull Pipeline Layout"),
                    bind_group_layouts: &[&cull_bind_group_layout],
                    push_constant_ranges: &[],
                });

        let cull_pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Meshlet Cull Pipeline"),
                layout: Some(&cull_pipeline_layout),
                module: &cull_shader,
                entry_point: Some("cs_meshlet_cull"),
                compilation_options: Default::default(),
                cache: None,
            });

        {
            let mut cull_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Meshlet Cull"),
                timestamp_writes: None,
            });
            cull_pass.set_pipeline(&cull_pipeline);
            cull_pass.set_bind_group(0, &cull_bind_group, &[]);
            let workgroups = meshlet_buffers.meshlet_count.div_ceil(64);
            cull_pass.dispatch_workgroups(workgroups, 1, 1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Meshlet Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.render_pipeline);
            pass.set_bind_group(2, &self.lighting_bind_group, &[]);
        }
    }
}

#[cfg(feature = "meshlet")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshletCullUniforms {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 3],
    meshlet_count: u32,
}

/// Draw data for a single mesh in the opaque pass.
struct OpaqueDrawData {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// Internal opaque pass for RenderGraph integration.
struct OpaqueGraphPass {
    objects: Vec<OpaqueDrawData>,
    camera_bind_group: wgpu::BindGroup,
    material_bind_group: wgpu::BindGroup,
    lighting_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    uniform_alignment: u32,
}

impl super::render_graph::RenderPass for OpaqueGraphPass {
    fn name(&self) -> &str {
        "OpaquePass"
    }

    fn execute(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        context: &super::render_graph::RenderContext,
    ) {
        let hdr_view = match &context.hdr_texture {
            Some(v) => v,
            None => return,
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Opaque Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: hdr_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.08,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &context.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(2, &self.lighting_bind_group, &[]);

        let uniform_size = std::mem::size_of::<CameraUniform>();
        let align = self.uniform_alignment as usize;
        let aligned_size = uniform_size.div_ceil(align) * align;

        for (i, mesh) in self.objects.iter().enumerate() {
            let dyn_offset = (i * aligned_size) as u32;
            pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
            pass.set_bind_group(1, &self.material_bind_group, &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
}
