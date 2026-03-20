//! Mobile application runner — drives the winit event loop on Android & iOS.
//!
//! Mirrors `quasar_engine::runner` but is tailored for mobile:
//! - Touch events are forwarded to [`TouchInput`] and [`GestureRecognizer`].
//! - No editor overlay (performance & screen-space constraints).
//! - Respects [`MobileConfig`] (safe-area insets, keep-screen-on, etc.).
//! - Initializes the GPU renderer on resume and drives the render loop.

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use quasar_core::App;
use quasar_render::{
    Camera, DirectionalLight, LightData, LightsUniform, MeshCache, MeshShape, RenderConfig,
    Renderer,
};
use quasar_window::{Input, QuasarWindow, WindowConfig};

use crate::{GestureRecognizer, MobileConfig, MobilePlatform, TouchInput};

/// Runtime state created once the window is available on the device.
struct MobileState {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    mesh_cache: MeshCache,
    touch: TouchInput,
    gestures: GestureRecognizer,
    #[allow(dead_code)]
    config: MobileConfig,
    elapsed: f32,
}

/// Winit `ApplicationHandler` for Android & iOS.
pub struct MobileRunner {
    app: App,
    window_config: Option<WindowConfig>,
    mobile_config: MobileConfig,
    state: Option<MobileState>,
}

impl MobileRunner {
    pub fn new(app: App, window_config: WindowConfig, mobile_config: MobileConfig) -> Self {
        Self {
            app,
            window_config: Some(window_config),
            mobile_config,
            state: None,
        }
    }
}

impl ApplicationHandler for MobileRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let config = self.window_config.take().unwrap_or_default();

        log::info!(
            "Mobile: creating window \"{}\" ({}×{}), platform: {:?}",
            config.title,
            config.width,
            config.height,
            MobilePlatform::current(),
        );

        let qw = QuasarWindow::new(WindowConfig {
            title: config.title.clone(),
            width: config.width,
            height: config.height,
            resizable: false,
            vsync: config.vsync,
            network: config.network.clone(),
        });

        let window = Arc::new(
            event_loop
                .create_window(qw.window_attributes())
                .expect("Failed to create mobile window"),
        );

        let size = window.inner_size();
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            size.width,
            size.height,
            RenderConfig::default(),
        ))
        .expect("Failed to initialise mobile GPU renderer");

        let camera = Camera::new(size.width, size.height);

        self.app.world.insert_resource(Input::new());

        self.state = Some(MobileState {
            window,
            renderer,
            camera,
            mesh_cache: MeshCache::new(),
            touch: TouchInput::new(),
            gestures: GestureRecognizer::default(),
            config: self.mobile_config.clone(),
            elapsed: 0.0,
        });

        log::info!(
            "Mobile renderer initialised — {}×{}",
            size.width,
            size.height
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Mobile close requested.");
                event_loop.exit();
            }

            WindowEvent::Touch(touch) => {
                let phase = match touch.phase {
                    winit::event::TouchPhase::Started => crate::TouchPhase::Started,
                    winit::event::TouchPhase::Moved => crate::TouchPhase::Moved,
                    winit::event::TouchPhase::Ended => crate::TouchPhase::Ended,
                    winit::event::TouchPhase::Cancelled => crate::TouchPhase::Cancelled,
                };
                state.touch.handle_touch(
                    touch.id,
                    phase,
                    touch.location.x as f32,
                    touch.location.y as f32,
                    touch
                        .force
                        .map(|f| match f {
                            winit::event::Force::Calibrated { force, .. } => force as f32,
                            winit::event::Force::Normalized(n) => n as f32,
                        })
                        .unwrap_or(1.0),
                );
                let _gestures = state.gestures.update(&state.touch, 0.016, state.elapsed);
            }

            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    state.renderer.resize(size.width, size.height);
                    state.camera.set_aspect(size.width, size.height);
                    log::info!("Mobile resize {}×{}", size.width, size.height);
                }
            }

            WindowEvent::RedrawRequested => {
                // Tick the application.
                self.app.tick();

                if let Some(state) = self.state.as_mut() {
                    state.touch.begin_frame();
                    state.elapsed += self.app.time.delta_seconds();

                    // Collect meshes from the world.
                    let shape_mats: Vec<(MeshShape, glam::Mat4)> = {
                        use quasar_math::Transform;
                        let with_shape: Vec<_> = self
                            .app
                            .world
                            .query2::<MeshShape, Transform>()
                            .into_iter()
                            .map(|(_, shape, t)| (*shape, t.matrix()))
                            .collect();
                        if !with_shape.is_empty() {
                            with_shape
                        } else {
                            self.app
                                .world
                                .query::<Transform>()
                                .into_iter()
                                .map(|(_, t)| (MeshShape::Cube, t.matrix()))
                                .collect()
                        }
                    };

                    // Ensure meshes are uploaded.
                    for (shape, _) in &shape_mats {
                        state
                            .mesh_cache
                            .get_or_create(&state.renderer.device, *shape);
                    }

                    // Default lights.
                    let mut lights = LightsUniform::default();
                    lights.lights[0] = LightData::from_directional(&DirectionalLight::default());
                    lights.count = 1;
                    state.renderer.queue.write_buffer(
                        &state.renderer.light_buffer,
                        0,
                        bytemuck::cast_slice(&[lights]),
                    );

                    // Render frame.
                    match state.renderer.begin_frame() {
                        Ok((output, view, mut encoder)) => {
                            let objects: Vec<(
                                &quasar_render::Mesh,
                                glam::Mat4,
                                Option<&wgpu::BindGroup>,
                                Option<u32>,
                            )> = shape_mats
                                .iter()
                                .filter_map(|(shape, model)| {
                                    state
                                        .mesh_cache
                                        .cache
                                        .get(shape)
                                        .map(|m| (m, *model, None, None))
                                })
                                .collect();

                            state.renderer.render_3d_pass_batched(
                                &state.camera,
                                &objects,
                                &view,
                                &mut encoder,
                            );

                            state
                                .renderer
                                .queue
                                .submit(std::iter::once(encoder.finish()));
                            output.present();
                        }
                        Err(wgpu::SurfaceError::Lost) => {
                            let size = state.window.inner_size();
                            state.renderer.resize(size.width, size.height);
                        }
                        Err(e) => log::warn!("Mobile render error: {e:?}"),
                    }

                    state.window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        log::info!("Mobile app suspended.");
        self.state = None;
    }
}

/// Convenience entry point: creates an event loop and runs the mobile runner.
pub fn run_mobile(app: App, window_config: WindowConfig, mobile_config: MobileConfig) {
    let event_loop = EventLoop::new().expect("Failed to create mobile event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut runner = MobileRunner::new(app, window_config, mobile_config);
    event_loop
        .run_app(&mut runner)
        .expect("Mobile event loop error");
}
