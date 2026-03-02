//! # Quasar Engine Showcase
//!
//! Demonstrates multiple engine features using the high-level [`run`] entry
//! point — no manual winit boilerplate:
//! - Multiple mesh primitives (cube, sphere, cylinder, plane)
//! - ECS world with Transform + MeshShape components
//! - Scene graph (parent-child) relationships
//! - Camera orbit via mouse (handled by the runner)
//!
//! Controls:
//!   Right-click + drag — orbit camera
//!   Scroll wheel — zoom
//!   F12 — toggle editor overlay
//!   ESC — exit

use quasar_engine::prelude::*;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Quasar Engine — Showcase Demo");
    log::info!("Controls: Right-click drag to orbit, scroll to zoom, F12 for editor, ESC to exit");

    let mut app = App::new();
    let mut scene = SceneGraph::new();

    // Ground plane.
    let ground = app.world.spawn();
    let mut ground_tf = Transform::IDENTITY;
    ground_tf.position = Vec3::new(0.0, -1.0, 0.0);
    app.world.insert(ground, ground_tf);
    app.world.insert(ground, MeshShape::Plane);
    scene.set_name(ground, "Ground");

    // Central pedestal (cylinder).
    let pedestal = app.world.spawn();
    let mut ped_tf = Transform::IDENTITY;
    ped_tf.position = Vec3::new(0.0, -0.25, 0.0);
    app.world.insert(pedestal, ped_tf);
    app.world.insert(pedestal, MeshShape::Cylinder { segments: 24 });
    scene.set_name(pedestal, "Pedestal");

    // Spinning cube on pedestal.
    let cube = app.world.spawn();
    let mut cube_tf = Transform::IDENTITY;
    cube_tf.position = Vec3::new(0.0, 1.0, 0.0);
    app.world.insert(cube, cube_tf);
    app.world.insert(cube, MeshShape::Cube);
    scene.set_name(cube, "SpinningCube");
    scene.set_parent(cube, pedestal);

    // Orbiting spheres.
    for i in 0..6 {
        let sphere = app.world.spawn();
        let angle = (i as f32) * std::f32::consts::TAU / 6.0;
        let radius = 3.0;
        let mut tf = Transform::IDENTITY;
        tf.position = Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());
        tf.scale = Vec3::splat(0.6 + 0.2 * (i as f32 / 6.0));
        app.world.insert(sphere, tf);
        app.world
            .insert(sphere, MeshShape::Sphere { sectors: 32, stacks: 16 });
        scene.set_name(sphere, format!("Sphere_{i}"));
    }

    // Outer ring of cubes.
    for i in 0..4 {
        let outer_cube = app.world.spawn();
        let angle = (i as f32) * std::f32::consts::TAU / 4.0 + 0.4;
        let radius = 5.0;
        let mut tf = Transform::IDENTITY;
        tf.position = Vec3::new(radius * angle.cos(), 0.5, radius * angle.sin());
        tf.scale = Vec3::splat(0.4);
        app.world.insert(outer_cube, tf);
        app.world.insert(outer_cube, MeshShape::Cube);
        scene.set_name(outer_cube, format!("OuterCube_{i}"));
    }

    app.world.insert_resource(scene);

    // Register animation system.
    app.add_system("showcase_animation", |world: &mut World| {
        let elapsed = world
            .resource::<TimeSnapshot>()
            .map(|t| t.elapsed_seconds)
            .unwrap_or(0.0);

        // Find named entities through the SceneGraph.
        let scene = match world.remove_resource::<SceneGraph>() {
            Some(s) => s,
            None => return,
        };

        // Spin the central cube.
        if let Some(cube) = scene.find_by_name("SpinningCube") {
            if let Some(tf) = world.get_mut::<Transform>(cube) {
                tf.rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    elapsed * 1.5,
                    elapsed * 0.4,
                    0.0,
                );
            }
        }

        // Bob the orbiting spheres.
        for i in 0..6 {
            let name = format!("Sphere_{i}");
            if let Some(entity) = scene.find_by_name(&name) {
                if let Some(tf) = world.get_mut::<Transform>(entity) {
                    let phase = (i as f32) * std::f32::consts::TAU / 6.0;
                    tf.position.y = 0.3 * (elapsed * 2.0 + phase).sin();
                    tf.rotation = Quat::from_rotation_y(elapsed * 0.8 + phase);
                }
            }
        }

        // Rotate outer cubes.
        for i in 0..4 {
            let name = format!("OuterCube_{i}");
            if let Some(entity) = scene.find_by_name(&name) {
                if let Some(tf) = world.get_mut::<Transform>(entity) {
                    let base_angle = (i as f32) * std::f32::consts::TAU / 4.0 + 0.4;
                    let orbit = base_angle + elapsed * 0.3;
                    tf.position.x = 5.0 * orbit.cos();
                    tf.position.z = 5.0 * orbit.sin();
                    tf.position.y = 0.5 + 0.2 * (elapsed * 1.5 + base_angle).sin();
                    tf.rotation = Quat::from_euler(
                        EulerRot::YXZ,
                        elapsed * 2.0,
                        elapsed * 1.0,
                        0.0,
                    );
                }
            }
        }

        // Put the scene graph back.
        world.insert_resource(scene);
    });

    run(
        app,
        WindowConfig {
            title: "Quasar Engine — Showcase".into(),
            width: 1280,
            height: 720,
            ..WindowConfig::default()
        },
    );
}
