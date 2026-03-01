//! # Spinning Cube Demo
//!
//! A minimal example demonstrating the Quasar Engine:
//! - Window creation with winit
//! - GPU rendering with wgpu
//! - A spinning, lit cube with per-face colors
//! - ECS world with Transform component
//!
//! Run: `cargo run --example spinning_cube`

use std::sync::Arc;

use glam::{Mat4, Vec3};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use quasar_core::{Time, World};
use quasar_math::Transform;
use quasar_render::{Camera, Mesh, MeshData, Renderer};
use quasar_window::Input;

/// Application state — created once the window is ready.
struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    mesh: Mesh,
    world: World,
    input: Input,
    time: Time,
    cube_entity: quasar_core::Entity,
}

/// The winit application handler.
struct QuasarApp {
    state: Option<AppState>,
}

impl QuasarApp {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for QuasarApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // Already initialized.
        }

        log::info!("Creating window and initializing renderer...");

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Quasar Engine — Spinning Cube")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("Failed to create window"),
        );

        let size = window.inner_size();
        let renderer = pollster::block_on(Renderer::new(window.clone(), size.width, size.height));

        let camera = Camera::new(size.width, size.height);
        let mesh = Mesh::from_data(&renderer.device, &MeshData::cube());

        // ECS: spawn a cube entity with a Transform.
        let mut world = World::new();
        let cube_entity = world.spawn();
        world.insert(cube_entity, Transform::IDENTITY);

        let input = Input::new();
        let time = Time::new();

        self.state = Some(AppState {
            window,
            renderer,
            camera,
            mesh,
            world,
            input,
            time,
            cube_entity,
        });

        log::info!(
            "Engine initialized — rendering at {} × {}",
            size.width,
            size.height
        );
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested — shutting down.");
                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    if event.state.is_pressed() {
                        state.input.key_pressed(key);
                        if key == KeyCode::Escape {
                            event_loop.exit();
                        }
                    } else {
                        state.input.key_released(key);
                    }
                }
            }

            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.renderer.resize(new_size.width, new_size.height);
                    state.camera.set_aspect(new_size.width, new_size.height);
                }
            }

            WindowEvent::RedrawRequested => {
                state.time.update();
                state.input.begin_frame();

                // ---- UPDATE: rotate the cube via ECS ----
                if let Some(transform) = state.world.get_mut::<Transform>(state.cube_entity) {
                    let dt = state.time.delta_seconds();
                    // Spin on Y axis (turntable) + slight tilt on X.
                    transform.rotate(Vec3::Y, dt * 1.2);
                    transform.rotate(Vec3::X, dt * 0.4);
                }

                // ---- RENDER ----
                let model = state
                    .world
                    .get::<Transform>(state.cube_entity)
                    .map(|t| t.matrix())
                    .unwrap_or(Mat4::IDENTITY);

                state.renderer.update_camera(&state.camera, model);

                match state.renderer.render(&[&state.mesh]) {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => {
                        let size = state.window.inner_size();
                        state.renderer.resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory!");
                        event_loop.exit();
                    }
                    Err(e) => {
                        log::warn!("Render error: {:?}", e);
                    }
                }

                state.window.request_redraw();
            }

            _ => {}
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("🚀 Quasar Engine — Spinning Cube Demo");
    log::info!("Press ESC to exit");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = QuasarApp::new();
    event_loop.run_app(&mut app).expect("Event loop error");
}
