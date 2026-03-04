//! # Scripting Demo
//!
//! Demonstrates the Quasar Lua scripting system:
//! - Loading and executing Lua scripts
//! - Hot-reloading when scripts change
//! - Calling Lua functions from Rust
//! - Registering Rust functions for Lua
//!
//! Controls:
//! R - reload script
//! Right-click + drag - orbit camera
//! Scroll - zoom
//! F12 - toggle editor
//! ESC - exit

use quasar_engine::prelude::*;
use quasar_math::Vec3;
use quasar_render::MeshShape;
use quasar_scripting::{ScriptingPlugin, ScriptingResource};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Quasar Engine — Scripting Demo");
    log::info!("Controls: R = reload script, Hot-reload is enabled");
    log::info!("Edit scripts/demo.lua to see changes!");

    let mut app = App::new();
    app.add_plugin(ScriptingPlugin);

    let mut scene = SceneGraph::new();

    // Create scene entities
    let ground = app.world.spawn();
    app.world
        .insert(ground, Transform::from_position(Vec3::new(0.0, -1.0, 0.0)));
    app.world.insert(ground, MeshShape::Plane);
    scene.set_name(ground, "Ground");

    // Spawn cubes that will be controlled by Lua
    for i in 0..5 {
        let cube = app.world.spawn();
        let x = (i as f32 - 2.0) * 2.0;
        app.world
            .insert(cube, Transform::from_position(Vec3::new(x, 0.0, 0.0)));
        app.world.insert(cube, MeshShape::Cube);
        scene.set_name(cube, format!("Cube_{}", i));
    }

    app.world.insert_resource(scene);

    // Register custom Rust functions for Lua
    {
        if let Some(scripting) = app.world.resource::<ScriptingResource>() {
            scripting
                .engine
                .register_function("get_time", |_lua, ()| Ok(0.0f64))
                .expect("Failed to register get_time");
            log::info!("Registered Lua API: get_time()");
        }
    }

    // System to handle script reloading
    app.add_system("script_reload", |world: &mut World| {
        let input = match world.resource::<quasar_window::Input>() {
            Some(i) => i,
            None => return,
        };

        let reload_key = input.just_pressed(winit::keyboard::KeyCode::KeyR);

        // Manual reload on R key
        if reload_key {
            if let Some(scripting) = world.resource_mut::<ScriptingResource>() {
                match scripting
                    .engine
                    .exec_file("examples/scripting_demo/scripts/demo.lua")
                {
                    Ok(()) => log::info!("Script reloaded manually"),
                    Err(e) => log::error!("Failed to reload script: {}", e),
                }
            }
        }

        // Auto hot-reload
        if let Some(scripting) = world.resource_mut::<ScriptingResource>() {
            let reloaded = scripting.engine.hot_reload();
            if !reloaded.is_empty() {
                log::info!("Hot-reloaded {} script(s)", reloaded.len());
            }
        }
    });

    // System to animate entities based on Lua script
    app.add_system("lua_animation", |world: &mut World| {
        let elapsed = world
            .resource::<TimeSnapshot>()
            .map(|t| t.elapsed_seconds)
            .unwrap_or(0.0);

        // Get animation parameters from Lua if available
        let (speed, amplitude) = if let Some(scripting) = world.resource::<ScriptingResource>() {
            let speed: f64 = scripting
                .engine
                .get_global("animation_speed")
                .unwrap_or(1.0);
            let amplitude: f64 = scripting
                .engine
                .get_global("animation_amplitude")
                .unwrap_or(0.5);
            (speed as f32, amplitude as f32)
        } else {
            (1.0, 0.5)
        };

        let scene = match world.remove_resource::<SceneGraph>() {
            Some(s) => s,
            None => return,
        };

        // Animate cubes using parameters from Lua
        for i in 0..5 {
            if let Some(entity) = scene.find_by_name(&format!("Cube_{}", i)) {
                if let Some(tf) = world.get_mut::<quasar_math::Transform>(entity) {
                    let phase = (i as f32) * std::f32::consts::TAU / 5.0;
                    tf.position.y = amplitude * (elapsed * speed + phase).sin();
                    tf.rotation = Quat::from_rotation_y(elapsed * speed);
                }
            }
        }

        world.insert_resource(scene);
    });

    // Load initial script
    {
        if let Some(scripting) = app.world.resource::<ScriptingResource>() {
            let setup_script = r#"
-- Demo Lua script for Quasar Engine
-- Edit this file and save to see hot-reload in action!

animation_speed = 1.5
animation_amplitude = 0.8

-- Log that we're loaded
log.info("Lua script loaded!")
log.info("Animation speed: " .. animation_speed)
log.info("Amplitude: " .. animation_amplitude)

-- You can define functions
function get_entity_position(name)
    -- This would call into the engine API
    return 0, 0, 0
end

function update_entity(name, x, y, z)
    -- This would update an entity's transform
    log.info("Would update " .. name .. " to " .. x .. "," .. y .. "," .. z)
end
"#;
            match scripting.engine.exec(setup_script) {
                Ok(()) => log::info!("Lua setup script executed"),
                Err(e) => log::error!("Lua error: {}", e),
            }
        }
    }

    run(
        app,
        WindowConfig {
            title: "Quasar Engine — Scripting Demo".into(),
            width: 1280,
            height: 720,
            ..WindowConfig::default()
        },
    );
}
