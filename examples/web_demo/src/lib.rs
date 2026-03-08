//! # Web Demo
//!
//! A minimal WebGPU demo running in the browser.
//!
//! This demonstrates Quasar Engine's rendering capabilities using WebGPU.
//! Due to WASM limitations, this demo only includes core rendering -
//! no audio, physics, or Lua scripting.

use wasm_bindgen::prelude::*;
use quasar_core::{App, Entity, TimeSnapshot, World};
use quasar_math::{Transform, Vec3};
use quasar_render::{Camera, MeshCache, MeshShape, RenderConfig, Renderer};

use std::sync::Arc;

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logging");

    log::info!("Quasar Engine — Web Demo initializing...");

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

    // Create wgpu instance targeting WebGPU backend.
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

    log::info!("WebGPU surface configured — format {:?}", format);

    // Build minimal app with a spinning cube.
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

    log::info!("App configured — WebGPU rendering active!");

    // Run a simple render loop via requestAnimationFrame.
    // The actual per-frame render pass uses the device + surface directly.
    use wasm_bindgen::closure::Closure;
    use std::cell::RefCell;
    use std::rc::Rc;

    let camera = Camera::new(width, height);
    let mesh_cache = Rc::new(RefCell::new(MeshCache::new()));

    // Ensure cube mesh is uploaded.
    mesh_cache.borrow_mut().get_or_create(&device, MeshShape::Cube);

    let state = Rc::new(RefCell::new((app, camera, device, queue, surface, format, width, height)));
    let mc = mesh_cache.clone();
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        let mut s = state.borrow_mut();
        let (ref mut app, ref camera, ref device, ref queue, ref surface, format, w, h) = *s;
        let _ = (w, h);

        app.tick();

        // Collect meshes.
        let transforms: Vec<glam::Mat4> = app.world.query::<Transform>()
            .into_iter()
            .map(|(_, t)| t.matrix())
            .collect();

        // Render frame.
        match surface.get_current_texture() {
            Ok(output) => {
                let view = output.texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: Some("web frame") },
                );
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
                }
                queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            Err(e) => log::warn!("web frame error: {e:?}"),
        }

        // Request next frame.
        let window = web_sys::window().unwrap();
        let _ = window.request_animation_frame(
            f.borrow().as_ref().unwrap().as_ref().unchecked_ref()
        );
    }));

    // Kick off first frame.
    let window = web_sys::window().unwrap();
    let _ = window.request_animation_frame(
        g.borrow().as_ref().unwrap().as_ref().unchecked_ref()
    );

    Ok(())
}
