//! # Spinning Cube Demo
//!
//! A minimal example demonstrating the Quasar Engine using the high-level
//! [`run`] entry point — no manual winit boilerplate required.
//!
//! Run: `cargo run --example spinning_cube`

use quasar_engine::prelude::*;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Quasar Engine — Spinning Cube Demo");
    log::info!("Press ESC to exit, F12 to toggle editor");

    let mut app = App::new();

    // Spawn a cube entity.
    let cube = app.world.spawn();
    app.world.insert(cube, Transform::IDENTITY);
    app.world.insert(cube, MeshShape::Cube);

    // Register the spin system.
    app.add_system("spin_cube", |world: &mut World| {
        let dt = world
            .resource::<TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        for (_entity, transform) in world.query_mut::<Transform>() {
            transform.rotate(Vec3::Y, dt * 1.2);
            transform.rotate(Vec3::X, dt * 0.4);
        }
    });

    run(
        app,
        WindowConfig {
            title: "Quasar Engine — Spinning Cube".into(),
            width: 1280,
            height: 720,
            ..WindowConfig::default()
        },
    );
}
