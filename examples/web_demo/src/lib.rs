//! # Web Demo
//!
//! A minimal WebGPU demo running in the browser.
//!
//! This demonstrates Quasar Engine''s rendering capabilities using WebGPU.
//! Due to WASM limitations, this demo only includes core rendering -
//! no audio, physics, or Lua scripting.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use quasar_core::{App, Entity, TimeSnapshot, World};
#[cfg(target_arch = "wasm32")]
use quasar_math::{Transform, Vec3};
#[cfg(target_arch = "wasm32")]
use quasar_render::{Camera, MeshCache, MeshShape, RenderConfig, Renderer};

#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
const SHADER_SOURCE: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = uniforms.mvp * vec4<f32>(position, 1.0);
    out.color = normal * 0.5 + vec3<f32>(0.5, 0.5, 0.5);
    return out;
}

@fragment
fn fs_main(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(color, 1.0);
}
"#;

#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
}

#[cfg(target_arch = "wasm32")]
fn create_cube_vertices() -> Vec<Vertex> {
    let positions = [
        [-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5,  0.5, -0.5], [-0.5,  0.5, -0.5],
        [-0.5, -0.5,  0.5], [0.5, -0.5,  0.5], [0.5,  0.5,  0.5], [-0.5,  0.5,  0.5],
    ];
    let normals = [
        [ 0.0,  0.0, -1.0],
        [ 0.0,  0.0,  1.0],
        [ 0.0, -1.0,  0.0],
        [ 0.0,  1.0,  0.0],
        [-1.0,  0.0,  0.0],
        [ 1.0,  0.0,  0.0],
    ];
    let faces: [(usize, usize, usize, usize); 6] = [
        (0, 1, 2, 3),
        (4, 7, 6, 5),
        (0, 4, 5, 1),
        (2, 6, 7, 3),
        (0, 3, 7, 4),
        (1, 5, 6, 2),
    ];
    let normal_indices = [0, 1, 2, 3, 4, 5];

    let mut vertices = Vec::with_capacity(36);
    for (fi, &(i0, i1, i2, i3)) in faces.iter().enumerate() {
        let n = normals[normal_indices[fi]];
        let v = [
            positions[i0], positions[i1], positions[i2], positions[i3],
        ];
        vertices.extend_from_slice(&[
            Vertex { position: v[0], normal: n },
            Vertex { position: v[1], normal: n },
            Vertex { position: v[2], normal: n },
            Vertex { position: v[0], normal: n },
            Vertex { position: v[2], normal: n },
            Vertex { position: v[3], normal: n },
        ]);
    }
    vertices
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logging");

    log::info!("Quasar Engine - Web Demo initializing...");

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas element")?;
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into()
        .map_err(|_| "element is not a canvas")?;

    let width = canvas.client_width().max(1) as u32;
    let height = canvas.client_height().max(1) as u32;

    log::info!("Canvas acquired ({}x{}), creating wgpu surface...", width, height);

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
        ..Default::default()
    });

    let surface_target = wgpu::SurfaceTarget::Canvas(canvas.clone());
    let surface = instance
        .create_surface(surface_target)
        .map_err(|e| JsValue::from_str(&format!("surface: {e}")))?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .ok_or("No suitable GPU adapter")?;

    log::info!("GPU adapter: {:?}", adapter.get_info().name);

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Quasar Web Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: Default::default(),
            },
            None,
        )
        .await
        .map_err(|e| JsValue::from_str(&format!("device: {e}")))?;

    let caps = surface.get_capabilities(&adapter);
    let format = caps.formats.iter().find(|f| f.is_srgb()).copied()
        .unwrap_or(caps.formats[0]);

    surface.configure(&device, &wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    });

    log::info!("WebGPU surface configured - format {:?}", format);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Cube Shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
    });

    let vertices = create_cube_vertices();
    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Cube Vertices"),
        size: (vertices.len() * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Uniforms"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
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
        label: Some("Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Cube Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: std::mem::size_of::<[f32; 3]>() as u64,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                    ],
                },
            ],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    let mut app = App::new();
    let cube = app.world.spawn();
    app.world.insert(cube, Transform::IDENTITY);
    app.world.insert(cube, MeshShape::Cube);

    app.add_system("spin_cube", |world: &mut World| {
        let dt = world
            .resource::<TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        world.for_each_mut(|_entity: Entity, transform: &mut Transform| {
            transform.rotate(Vec3::Y, dt * 1.2);
            transform.rotate(Vec3::X, dt * 0.4);
        });
    });

    log::info!("App configured - WebGPU rendering active!");

    use std::cell::RefCell;
    use std::rc::Rc;

    let state = Rc::new(RefCell::new((
        app,
        device,
        queue,
        surface,
        format,
        width,
        height,
        vertex_buffer,
        uniform_buffer,
        bind_group,
        pipeline,
        0.0f32,
    )));

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        let mut s = state.borrow_mut();
        let (
            ref mut app,
            ref device,
            ref queue,
            ref surface,
            _format,
            width,
            height,
            ref vertex_buffer,
            ref uniform_buffer,
            ref bind_group,
            ref pipeline,
            ref mut time,
        ) = *s;

        app.tick();
        *time += 1.0 / 60.0;

        let aspect = width as f32 / height as f32;
        let projection = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 2.0, 5.0),
            glam::Vec3::ZERO,
            glam::Vec3::Y,
        );
        let rotation = glam::Quat::from_rotation_y(*time * 1.2)
            * glam::Quat::from_rotation_x(*time * 0.4);
        let model = glam::Mat4::from_quat(rotation);
        let mvp = projection * view * model;

        let uniforms = Uniforms {
            mvp: mvp.to_cols_array_2d(),
        };
        queue.write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        match surface.get_current_texture() {
            Ok(output) => {
                let view = output.texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: Some("web frame") },
                );
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("web clear"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1, g: 0.1, b: 0.15, a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, bind_group, &[]);
                    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    pass.draw(0..36, 0..1);
                }
                queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            Err(e) => log::warn!("web frame error: {e:?}"),
        }

        let window = web_sys::window().unwrap();
        let _ = window.request_animation_frame(
            f.borrow().as_ref().unwrap().as_ref().unchecked_ref()
        );
    }));

    let window = web_sys::window().unwrap();
    let _ = window.request_animation_frame(
        g.borrow().as_ref().unwrap().as_ref().unchecked_ref()
    );

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start() -> Result<(), ()> {
    // web_demo is only available for wasm32 target
    Ok(())
}
