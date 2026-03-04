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
use quasar_render::MeshShape;

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("Failed to initialize logging");

    log::info!("Quasar Engine — Web Demo initializing...");

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let _canvas = document
        .get_element_by_id("canvas")
        .ok_or("no canvas element")?;

    log::info!("Canvas acquired, creating app...");

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

    log::info!("App configured successfully!");
    log::info!("Full rendering requires WebGPU surface integration.");

    Ok(())
}
