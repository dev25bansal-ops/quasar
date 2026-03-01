//! # Quasar Engine Showcase
//!
//! Demonstrates multiple engine features in a single scene:
//! - Multiple mesh primitives (cube, sphere, cylinder, plane)
//! - ECS world with Transform components
//! - Scene graph (parent-child) relationships
//! - Camera orbit (arrow keys)
//! - Per-object model matrices
//!
//! Controls:
//!   Left/Right arrows — orbit camera
//!   Up/Down arrows — zoom in/out
//!   ESC — exit

use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use quasar_core::{Entity, SceneGraph, Time, World};
use quasar_math::Transform;
use quasar_render::{Camera, Mesh, MeshData, Renderer};
use quasar_window::Input;

/// Tag component to identify which mesh primitive an entity uses.
#[derive(Clone, Copy)]
enum MeshKind {
    Cube,
    Sphere,
    Cylinder,
    Plane,
}

struct AppState {
    window: Arc<Window>,
    renderer: Renderer,
    camera: Camera,
    input: Input,
    time: Time,
    world: World,
    scene: SceneGraph,
    // GPU meshes
    cube_mesh: Mesh,
    sphere_mesh: Mesh,
    cylinder_mesh: Mesh,
    plane_mesh: Mesh,
    // Entities
    entities: Vec<(Entity, MeshKind)>,
    // Camera orbit
    orbit_angle: f32,
    orbit_radius: f32,
    orbit_height: f32,
}

struct QuasarShowcase {
    state: Option<AppState>,
}

impl QuasarShowcase {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for QuasarShowcase {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        log::info!("Initializing Quasar Engine Showcase...");

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Quasar Engine — Showcase")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
                )
                .expect("Failed to create window"),
        );

        let size = window.inner_size();
        let renderer = pollster::block_on(Renderer::new(window.clone(), size.width, size.height));
        let camera = Camera::new(size.width, size.height);

        // Create GPU meshes.
        let cube_mesh = Mesh::from_data(&renderer.device, &MeshData::cube());
        let sphere_mesh = Mesh::from_data(&renderer.device, &MeshData::sphere(0.5, 32, 16));
        let cylinder_mesh = Mesh::from_data(&renderer.device, &MeshData::cylinder(0.3, 1.5, 24));
        let plane_mesh = Mesh::from_data(&renderer.device, &MeshData::plane(10.0));

        // Build the scene.
        let mut world = World::new();
        let mut scene = SceneGraph::new();
        let mut entities = Vec::new();

        // Ground plane.
        let ground = world.spawn();
        let mut ground_tf = Transform::IDENTITY;
        ground_tf.position = Vec3::new(0.0, -1.0, 0.0);
        world.insert(ground, ground_tf);
        scene.set_name(ground, "Ground");
        entities.push((ground, MeshKind::Plane));

        // Central pedestal (cylinder).
        let pedestal = world.spawn();
        let mut ped_tf = Transform::IDENTITY;
        ped_tf.position = Vec3::new(0.0, -0.25, 0.0);
        world.insert(pedestal, ped_tf);
        scene.set_name(pedestal, "Pedestal");
        entities.push((pedestal, MeshKind::Cylinder));

        // Spinning cube on top of pedestal.
        let cube = world.spawn();
        let mut cube_tf = Transform::IDENTITY;
        cube_tf.position = Vec3::new(0.0, 1.0, 0.0);
        world.insert(cube, cube_tf);
        scene.set_name(cube, "SpinningCube");
        scene.set_parent(cube, pedestal);
        entities.push((cube, MeshKind::Cube));

        // Orbiting spheres around the center.
        for i in 0..6 {
            let sphere = world.spawn();
            let angle = (i as f32) * std::f32::consts::TAU / 6.0;
            let radius = 3.0;
            let mut tf = Transform::IDENTITY;
            tf.position = Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());
            tf.scale = Vec3::splat(0.6 + 0.2 * (i as f32 / 6.0));
            world.insert(sphere, tf);
            scene.set_name(sphere, format!("Sphere_{}", i));
            entities.push((sphere, MeshKind::Sphere));
        }

        // Second ring — cubes at outer orbit.
        for i in 0..4 {
            let outer_cube = world.spawn();
            let angle = (i as f32) * std::f32::consts::TAU / 4.0 + 0.4;
            let radius = 5.0;
            let mut tf = Transform::IDENTITY;
            tf.position = Vec3::new(radius * angle.cos(), 0.5, radius * angle.sin());
            tf.scale = Vec3::splat(0.4);
            world.insert(outer_cube, tf);
            scene.set_name(outer_cube, format!("OuterCube_{}", i));
            entities.push((outer_cube, MeshKind::Cube));
        }

        let input = Input::new();
        let time = Time::new();

        log::info!(
            "Scene loaded: {} entities, {} with children",
            entities.len(),
            entities.iter().filter(|(e, _)| scene.has_children(*e)).count()
        );

        self.state = Some(AppState {
            window,
            renderer,
            camera,
            input,
            time,
            world,
            scene,
            cube_mesh,
            sphere_mesh,
            cylinder_mesh,
            plane_mesh,
            entities,
            orbit_angle: 0.0,
            orbit_radius: 6.0,
            orbit_height: 3.5,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

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
                let dt = state.time.delta_seconds();

                // Camera orbit controls.
                if state.input.is_pressed(KeyCode::ArrowLeft) {
                    state.orbit_angle += dt * 1.5;
                }
                if state.input.is_pressed(KeyCode::ArrowRight) {
                    state.orbit_angle -= dt * 1.5;
                }
                if state.input.is_pressed(KeyCode::ArrowUp) {
                    state.orbit_radius = (state.orbit_radius - dt * 3.0).max(2.0);
                }
                if state.input.is_pressed(KeyCode::ArrowDown) {
                    state.orbit_radius = (state.orbit_radius + dt * 3.0).min(15.0);
                }

                // Update camera position.
                state.camera.position = Vec3::new(
                    state.orbit_radius * state.orbit_angle.cos(),
                    state.orbit_height,
                    state.orbit_radius * state.orbit_angle.sin(),
                );
                state.camera.target = Vec3::new(0.0, 0.5, 0.0);

                // Animate entities.
                let elapsed = state.time.elapsed_seconds();

                // Spin the central cube.
                if let Some(cube_entity) = state.scene.find_by_name("SpinningCube") {
                    if let Some(tf) = state.world.get_mut::<Transform>(cube_entity) {
                        tf.rotation = Quat::from_euler(
                            glam::EulerRot::YXZ,
                            elapsed * 1.5,
                            elapsed * 0.4,
                            0.0,
                        );
                    }
                }

                // Bob the orbiting spheres up and down.
                for i in 0..6 {
                    let name = format!("Sphere_{}", i);
                    if let Some(entity) = state.scene.find_by_name(&name) {
                        if let Some(tf) = state.world.get_mut::<Transform>(entity) {
                            let phase = (i as f32) * std::f32::consts::TAU / 6.0;
                            tf.position.y = 0.3 * (elapsed * 2.0 + phase).sin();
                            // Gentle spin.
                            tf.rotation = Quat::from_rotation_y(elapsed * 0.8 + phase);
                        }
                    }
                }

                // Rotate outer cubes.
                for i in 0..4 {
                    let name = format!("OuterCube_{}", i);
                    if let Some(entity) = state.scene.find_by_name(&name) {
                        if let Some(tf) = state.world.get_mut::<Transform>(entity) {
                            // Orbit slowly.
                            let base_angle = (i as f32) * std::f32::consts::TAU / 4.0 + 0.4;
                            let orbit = base_angle + elapsed * 0.3;
                            tf.position.x = 5.0 * orbit.cos();
                            tf.position.z = 5.0 * orbit.sin();
                            tf.position.y = 0.5 + 0.2 * (elapsed * 1.5 + base_angle).sin();
                            // Self-rotation.
                            tf.rotation = Quat::from_euler(
                                glam::EulerRot::YXZ,
                                elapsed * 2.0,
                                elapsed * 1.0,
                                0.0,
                            );
                        }
                    }
                }

                // Collect render objects: (mesh, model_matrix) pairs.
                let objects: Vec<(&Mesh, Mat4)> = state
                    .entities
                    .iter()
                    .filter_map(|(entity, kind)| {
                        let tf = state.world.get::<Transform>(*entity)?;
                        let mesh = match kind {
                            MeshKind::Cube => &state.cube_mesh,
                            MeshKind::Sphere => &state.sphere_mesh,
                            MeshKind::Cylinder => &state.cylinder_mesh,
                            MeshKind::Plane => &state.plane_mesh,
                        };
                        Some((mesh, tf.matrix()))
                    })
                    .collect();

                match state.renderer.render_objects(&state.camera, &objects) {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => {
                        let size = state.window.inner_size();
                        state.renderer.resize(size.width, size.height);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("GPU out of memory!");
                        event_loop.exit();
                    }
                    Err(e) => log::warn!("Render error: {:?}", e),
                }

                state.input.begin_frame();
                state.window.request_redraw();
            }

            _ => {}
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("🚀 Quasar Engine — Showcase Demo");
    log::info!("Controls: Arrow keys to orbit/zoom, ESC to exit");

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = QuasarShowcase::new();
    event_loop.run_app(&mut app).expect("Event loop error");
}
