//! Scripting plugin — runs Lua scripts every frame and handles hot-reload.

use quasar_core::ecs::{System, World};

use crate::bridge;
use crate::ScriptEngine;

/// Resource wrapper so the scripting engine lives in the ECS.
pub struct ScriptingResource {
    pub engine: ScriptEngine,
    /// Frame counter for hot-reload checks (every N frames).
    frame_counter: u64,
}

impl ScriptingResource {
    pub fn new() -> Self {
        let engine = ScriptEngine::new().expect("failed to create Lua scripting engine");
        bridge::register_bridge(engine.lua()).expect("failed to register Lua bridge");
        Self {
            engine,
            frame_counter: 0,
        }
    }
}

impl Default for ScriptingResource {
    fn default() -> Self {
        Self::new()
    }
}

/// System that calls `on_update(dt)` in Lua every frame and checks hot-reload.
pub struct ScriptingSystem;

impl System for ScriptingSystem {
    fn name(&self) -> &str {
        "scripting_update"
    }

    fn run(&mut self, world: &mut World) {
        // Get the scripting resource.
        let Some((_, resource)) = world.query_mut::<ScriptingResource>().next() else {
            return;
        };

        resource.frame_counter += 1;

        // Hot-reload check every 120 frames (~2 seconds at 60 fps).
        if resource.frame_counter % 120 == 0 {
            let _reloaded = resource.engine.hot_reload();
        }

        // Call the global `on_update(dt)` if it exists.
        let _ = resource
            .engine
            .call_function::<_, ()>("on_update", 0.016f32);
    }
}

/// Plugin that registers the scripting engine and update system.
pub struct ScriptingPlugin;

impl quasar_core::Plugin for ScriptingPlugin {
    fn name(&self) -> &str {
        "ScriptingPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        let singleton = app.world.spawn();
        app.world.insert(singleton, ScriptingResource::new());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(ScriptingSystem),
        );

        log::info!("ScriptingPlugin loaded — Lua scripting active");
    }
}
