//! Core renderer — manages the wgpu device, surface, and draw calls.

use quasar_core::error::{QuasarError, QuasarResult};

use super::camera::{Camera, CameraUniform};
use super::material::{Material, MaterialOverride};
use super::mesh::Mesh;
use super::pipeline;
use super::texture::Texture;

/// Maximum number of objects that can be rendered in a single pass with
/// unique model matrices.
const MAX_RENDER_OBJECTS: usize = 4096;

/// The main GPU renderer for Quasar Engine.
///
/// Owns the wgpu device, queue, surface, and render pipeline. Provides a
/// high-level `draw` method that submits meshes for rendering each frame.
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub render_pipeline: wgpu::RenderPipeline,
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_uniform: CameraUniform,
    pub material_bind_group_layout: wgpu::BindGroupLayout,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    /// Default white material used when no material is specified.
    pub default_material: Material,
    /// Default 1×1 white texture used when no texture is specified.
    pub default_texture: Texture,
    /// Minimum uniform buffer offset alignment (bytes), from device limits.
    pub uniform_alignment: u32,
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
    ) -> QuasarResult<Self> {
        // Create wgpu instance with default backends.
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create the rendering surface from the window.
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| QuasarError::Render(format!("Failed to create surface: {e}")))?;

        // Request a GPU adapter compatible with our surface.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| QuasarError::Render("No suitable GPU adapter found".into()))?;

        log::info!("GPU adapter: {:?}", adapter.get_info().name);

        // Request the device and queue.
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Quasar Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| QuasarError::Render(format!("Failed to request device: {e}")))?;

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
        let (depth_texture, depth_view) = Self::create_depth_texture(&device, width, height);

        // -- Material + Texture bind group layouts --
        let material_bind_group_layout = Material::bind_group_layout(&device);
        let texture_bind_group_layout = Texture::bind_group_layout(&device);

        // -- Default material (white, roughness=0.5, metallic=0) --
        let default_material =
            Material::new(&device, &material_bind_group_layout, "Default");

        // -- Default 1×1 white texture --
        let default_texture =
            Texture::white(&device, &queue, &texture_bind_group_layout);

        // -- Render pipeline --
        let shader_source = include_str!("../../../assets/shaders/basic.wgsl");
        let render_pipeline = pipeline::create_render_pipeline(
            &device,
            format,
            &camera_bind_group_layout,
            &material_bind_group_layout,
            &texture_bind_group_layout,
            shader_source,
        );

        // Upload default material data.
        default_material.update(&queue);

        Ok(Self {
            device,
            queue,
            surface,
            config,
            render_pipeline,
            depth_texture,
            depth_view,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            camera_uniform,
            material_bind_group_layout,
            texture_bind_group_layout,
            default_material,
            default_texture,
            uniform_alignment,
        })
    }

    /// Handle window resize — reconfigure surface and depth buffer.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);

        let (depth_texture, depth_view) = Self::create_depth_texture(&self.device, width, height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
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

    // ── Split-phase rendering API ────────────────────────────────

    /// Acquire the next surface frame and create a fresh command encoder.
    ///
    /// Use together with [`render_3d_pass`](Self::render_3d_pass) and
    /// [`finish_frame`](Self::finish_frame) when you need to inject
    /// additional render passes (e.g. egui) between the 3D draw and
    /// presentation.
    pub fn begin_frame(
        &self,
    ) -> Result<(wgpu::SurfaceTexture, wgpu::TextureView, wgpu::CommandEncoder), wgpu::SurfaceError>
    {
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
    pub fn render_3d_pass(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>)],
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let align = self.uniform_alignment as usize;
        let uniform_size = std::mem::size_of::<CameraUniform>();
        let aligned_size = uniform_size.div_ceil(align) * align;

        // Pre-write all per-object uniforms to the buffer at aligned offsets.
        if !objects.is_empty() {
            let total = aligned_size * objects.len();
            let mut data = vec![0u8; total];
            for (i, (_, model, _)) in objects.iter().enumerate() {
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

            for (i, (mesh, _, mat_bg)) in objects.iter().enumerate() {
                let dyn_offset = (i * aligned_size) as u32;
                pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
                let material_bg = mat_bg.unwrap_or(&self.default_material.bind_group);
                pass.set_bind_group(1, material_bg, &[]);
                pass.set_bind_group(2, &self.default_texture.bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }
    }

    /// Submit the encoder and present the frame.
    pub fn finish_frame(
        &self,
        encoder: wgpu::CommandEncoder,
        output: wgpu::SurfaceTexture,
    ) {
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
            render_pass.set_bind_group(1, &self.default_material.bind_group, &[]);
            render_pass.set_bind_group(2, &self.default_texture.bind_group, &[]);

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
    pub fn render_objects(
        &mut self,
        camera: &Camera,
        objects: &[(&Mesh, glam::Mat4, Option<&wgpu::BindGroup>)],
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

        // Pre-write all per-object uniforms.
        if !objects.is_empty() {
            let total = aligned_size * objects.len();
            let mut data = vec![0u8; total];
            for (i, (_, model, _)) in objects.iter().enumerate() {
                let mut uniform = CameraUniform::new();
                uniform.update(camera, *model);
                let bytes = bytemuck::bytes_of(&uniform);
                let offset = i * aligned_size;
                data[offset..offset + uniform_size].copy_from_slice(bytes);
            }
            self.queue.write_buffer(&self.camera_buffer, 0, &data);
        }

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

            for (i, (mesh, _, mat_bg)) in objects.iter().enumerate() {
                let dyn_offset = (i * aligned_size) as u32;
                render_pass.set_bind_group(0, &self.camera_bind_group, &[dyn_offset]);
                let material_bg = mat_bg.unwrap_or(&self.default_material.bind_group);
                render_pass.set_bind_group(1, material_bg, &[]);
                render_pass.set_bind_group(2, &self.default_texture.bind_group, &[]);
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
            &self.material_bind_group_layout,
            name,
            material_override,
        )
    }

    /// Create a depth texture and its view.
    fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
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
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }
}
