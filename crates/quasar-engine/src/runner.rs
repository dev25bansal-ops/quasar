//! Application runner — owns the winit event loop and drives the engine.
//!
//! Instead of manually wiring `winit::ApplicationHandler`, users call
//! [`run`] with a configured [`App`] and a [`WindowConfig`]:
//!
//! ```ignore
//! use quasar_engine::prelude::*;
//!
//! let mut app = App::new();
//! app.add_plugin(PhysicsPlugin);
//! app.add_plugin(AudioPlugin);
//! run(app, WindowConfig::default());
//! ```

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use quasar_core::App;
use quasar_editor::{renderer::EditorRenderer, Editor};
use quasar_render::{Camera, Renderer};
use quasar_window::{Input, QuasarWindow, WindowConfig};

/// Runtime state created once the window is available.
struct RunnerState {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    editor: Editor,
    editor_renderer: EditorRenderer,
}

/// The winit `ApplicationHandler` that drives the Quasar engine loop.
struct QuasarRunner {
    /// The user-configured application (world, schedule, time, etc.).
    app: App,
    /// Window configuration — consumed on first `resumed()` call.
    config: Option<WindowConfig>,
    /// Lazily initialised once the event loop has a window.
    state: Option<RunnerState>,
}

impl QuasarRunner {
    fn new(app: App, config: WindowConfig) -> Self {
        Self {
            app,
            config: Some(config),
            state: None,
        }
    }
}

impl ApplicationHandler for QuasarRunner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // Already initialised.
        }

        let config = self.config.take().unwrap_or_default();

        log::info!(
            "Creating window \"{}\" ({}×{})",
            config.title,
            config.width,
            config.height
        );

        let qw = QuasarWindow::new(WindowConfig {
            title: config.title.clone(),
            width: config.width,
            height: config.height,
            resizable: config.resizable,
            vsync: config.vsync,
        });

        let window = Arc::new(
            event_loop
                .create_window(qw.window_attributes())
                .expect("Failed to create window"),
        );

        let size = window.inner_size();
        let renderer = pollster::block_on(Renderer::new(
            window.clone(),
            size.width,
            size.height,
        ));

        let camera = Camera::new(size.width, size.height);

        let editor = Editor::new();
        let editor_renderer = EditorRenderer::new(
            &window,
            &renderer.device,
            renderer.config.format,
        );

        // Insert Input as a world resource so user systems can read it.
        self.app.world.insert_resource(Input::new());

        self.state = Some(RunnerState {
            window,
            renderer,
            camera,
            editor,
            editor_renderer,
        });

        log::info!(
            "Engine initialised — rendering at {}×{}",
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

        // ── Let egui have first crack at the event ────────────────
        let egui_consumed =
            state.editor_renderer.handle_event(&state.window, &event);

        match event {
            // ── Close ─────────────────────────────────────────────
            WindowEvent::CloseRequested => {
                log::info!("Close requested — shutting down.");
                event_loop.exit();
            }

            // ── Keyboard ──────────────────────────────────────────
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    // F12 toggles the editor regardless.
                    if key == KeyCode::F12 && event.state.is_pressed() {
                        state.editor.toggle();
                    }

                    // Only forward to game input if egui didn't consume.
                    if !egui_consumed {
                        if let Some(input) = self.app.world.resource_mut::<Input>() {
                            if event.state.is_pressed() {
                                input.key_pressed(key);
                            } else {
                                input.key_released(key);
                            }
                        }
                        // ESC closes the application.
                        if key == KeyCode::Escape && event.state.is_pressed() {
                            event_loop.exit();
                        }
                    }
                }
            }

            // ── Mouse movement ────────────────────────────────────
            WindowEvent::CursorMoved { position, .. } => {
                if !egui_consumed {
                    if let Some(input) = self.app.world.resource_mut::<Input>() {
                        input.cursor_position = Some((position.x, position.y));
                    }
                }
            }

            // ── Mouse buttons ─────────────────────────────────────
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if !egui_consumed {
                    if let Some(input) = self.app.world.resource_mut::<Input>() {
                        let btn = quasar_window::MouseButton::from(button);
                        if btn_state.is_pressed() {
                            input.mouse_button_pressed(btn);
                        } else {
                            input.mouse_button_released(btn);
                        }
                    }
                }
            }

            // ── Mouse scroll ──────────────────────────────────────
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed {
                    if let Some(input) = self.app.world.resource_mut::<Input>() {
                        match delta {
                            winit::event::MouseScrollDelta::LineDelta(x, y) => {
                                input.mouse_scrolled(x, y);
                            }
                            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                input.mouse_scrolled(pos.x as f32, pos.y as f32);
                            }
                        }
                    }
                }
            }

            // ── Resize ────────────────────────────────────────────
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.renderer.resize(new_size.width, new_size.height);
                    state.camera.set_aspect(new_size.width, new_size.height);
                }
            }

            // ── Redraw (main frame tick) ──────────────────────────
            WindowEvent::RedrawRequested => {
                // Clear per-frame input state.
                if let Some(input) = self.app.world.resource_mut::<Input>() {
                    input.begin_frame();
                }

                // Run one full ECS frame (updates time, runs all systems).
                self.app.tick();

                // Collect meshes + model matrices from the world.
                let objects: Vec<(quasar_render::Mesh, glam::Mat4)> = {
                    use quasar_math::Transform;
                    use quasar_render::MeshData;

                    let transforms: Vec<(u32, glam::Mat4)> = self
                        .app
                        .world
                        .query::<Transform>()
                        .map(|(e, t)| (e.index(), t.matrix()))
                        .collect();

                    if !transforms.is_empty() {
                        let mesh_data = MeshData::cube();
                        transforms
                            .into_iter()
                            .map(|(_, model)| {
                                let mesh = quasar_render::Mesh::from_data(
                                    &state.renderer.device,
                                    &mesh_data,
                                );
                                (mesh, model)
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                };

                // ── Split-phase rendering: 3D → egui → present ───
                let frame_result = state.renderer.begin_frame();
                match frame_result {
                    Ok((output, view, mut encoder)) => {
                        // 3D pass.
                        let refs: Vec<(&quasar_render::Mesh, glam::Mat4)> =
                            objects.iter().map(|(m, mat)| (m, *mat)).collect();
                        state
                            .renderer
                            .render_3d_pass(&state.camera, &refs, &view, &mut encoder);

                        // egui pass (editor overlay).
                        let egui_commands = if state.editor.enabled {
                            // Build entity name list for the hierarchy panel.
                            let entity_names: Vec<(quasar_core::ecs::Entity, String)> = self
                                .app
                                .world
                                .query::<quasar_math::Transform>()
                                .map(|(e, _)| (e, format!("Entity {}", e.index())))
                                .collect();

                            state.editor_renderer.begin_frame(&state.window);
                            state.editor.ui(
                                &state.editor_renderer.egui_ctx,
                                &entity_names,
                            );

                            let size = state.window.inner_size();
                            let screen = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [size.width, size.height],
                                pixels_per_point: state.window.scale_factor() as f32,
                            };

                            Some(state.editor_renderer.end_frame_and_render(
                                &state.renderer.device,
                                &state.renderer.queue,
                                &view,
                                screen,
                                &state.window,
                            ))
                        } else {
                            None
                        };

                        // Submit 3D + egui command buffers, then present.
                        let mut buffers = vec![encoder.finish()];
                        if let Some(egui_buf) = egui_commands {
                            buffers.push(egui_buf);
                        }
                        state.renderer.queue.submit(buffers);
                        output.present();
                    }
                    Err(wgpu::SurfaceError::Lost) => {
                        let size = state.window.inner_size();
                        state.renderer.resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory!");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("Render error: {e:?}"),
                }

                state.window.request_redraw();
            }

            _ => {}
        }
    }
}

/// Launch the engine: create a window, initialise the renderer, and run the
/// ECS game loop until the window is closed.
///
/// This is the main entry point for Quasar applications. Configure your
/// [`App`] with plugins and systems, then call `run`:
///
/// ```ignore
/// use quasar_engine::prelude::*;
///
/// let mut app = App::new();
/// app.add_plugin(PhysicsPlugin);
/// run(app, WindowConfig::default());
/// ```
pub fn run(app: App, config: WindowConfig) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut runner = QuasarRunner::new(app, config);
    event_loop.run_app(&mut runner).expect("Event loop error");
}
