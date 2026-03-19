//! Scripting plugin — runs Lua scripts every frame and handles hot-reload.
//!
//! Each frame the plugin:
//! 1. Serializes entity transforms and input state into Lua globals
//! 2. Calls the global `on_update(dt)` Lua function
//! 3. Reads back any queued commands and applies them to the ECS world

use glam::{Quat, Vec3};
use mlua::prelude::*;

use quasar_core::ecs::{Entity, System, World};
use quasar_core::Time;
use quasar_math::Transform;
use quasar_window::Input;

use crate::bridge;
use crate::component_registry::{self, ComponentRegistry};
use crate::{ScriptComponent, ScriptEngine};

/// Resource wrapper so the scripting engine lives in the ECS as a global resource.
pub struct ScriptingResource {
    pub engine: ScriptEngine,
    /// Registry of component types accessible from Lua.
    pub component_registry: ComponentRegistry,
    /// Registry key of the Lua table that maps entity_index → per-entity
    /// behaviour table (the table returned by each script file).
    entity_scripts_key: Option<mlua::RegistryKey>,
    /// Tracks which entity script paths have been registered with the file watcher.
    watched_entity_scripts: Vec<String>,
}

impl ScriptingResource {
    pub fn new() -> Self {
        let engine = ScriptEngine::new().unwrap_or_else(|e| {
            log::error!("failed to create Lua scripting engine: {}", e);
            panic!("failed to create Lua scripting engine: {}", e);
        });
        bridge::register_bridge(engine.lua()).unwrap_or_else(|e| {
            log::error!("failed to register Lua bridge: {}", e);
            panic!("failed to register Lua bridge: {}", e);
        });
        let entity_scripts_key = engine
            .lua()
            .create_table()
            .and_then(|t| engine.lua().create_registry_value(t))
            .ok();
        Self {
            engine,
            component_registry: component_registry::default_registry(),
            entity_scripts_key,
            watched_entity_scripts: Vec::new(),
        }
    }
}

impl Default for ScriptingResource {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a single command queued by Lua for the ECS.
enum ScriptCommand {
    SetPosition {
        entity_index: u32,
        value: Vec3,
    },
    SetRotation {
        entity_index: u32,
        value: Quat,
    },
    SetScale {
        entity_index: u32,
        value: Vec3,
    },
    Spawn,
    Despawn {
        entity_index: u32,
    },
    ApplyForce {
        entity_index: u32,
        force: Vec3,
    },
    PlayAudio {
        path: String,
    },
    AddComponent {
        entity_index: u32,
        component_name: String,
        data_key: mlua::RegistryKey,
    },
    RemoveComponent {
        entity_index: u32,
        component_name: String,
    },
}

/// System that calls `on_update(dt)` in Lua every frame and checks hot-reload.
pub struct ScriptingSystem;

impl ScriptingSystem {
    /// Serialize entity transforms into `quasar._transforms`.
    fn write_transforms(lua: &Lua, world: &World) {
        let Ok(quasar) = lua.globals().get::<LuaTable>("quasar") else {
            return;
        };
        let Ok(transforms) = lua.create_table() else {
            return;
        };

        for (entity, t) in world.query::<Transform>() {
            if let Ok(entry) = lua.create_table() {
                let _ = entry.set("px", t.position.x);
                let _ = entry.set("py", t.position.y);
                let _ = entry.set("pz", t.position.z);
                let _ = entry.set("rx", t.rotation.x);
                let _ = entry.set("ry", t.rotation.y);
                let _ = entry.set("rz", t.rotation.z);
                let _ = entry.set("rw", t.rotation.w);
                let _ = entry.set("sx", t.scale.x);
                let _ = entry.set("sy", t.scale.y);
                let _ = entry.set("sz", t.scale.z);
                let _ = transforms.set(entity.index(), entry);
            }
        }

        let _ = quasar.set("_transforms", transforms);
    }

    /// Populate `quasar._component_data` using the ComponentRegistry.
    fn write_component_data(lua: &Lua, world: &World) {
        let Ok(quasar) = lua.globals().get::<LuaTable>("quasar") else {
            return;
        };
        let Some(resource) = world.resource::<ScriptingResource>() else {
            return;
        };
        match resource.component_registry.serialize_all_to_lua(lua, world) {
            Ok(table) => {
                let _ = quasar.set("_component_data", table);
            }
            Err(e) => {
                log::error!("[scripting] Failed to serialize component data: {}", e);
            }
        }
    }

    /// Serialize pressed keys and mouse buttons into Lua.
    fn write_input(lua: &Lua, world: &World) {
        let Ok(quasar) = lua.globals().get::<LuaTable>("quasar") else {
            return;
        };

        if let Some(input) = world.resource::<Input>() {
            // Write pressed keys as a table { ["KeyW"] = true, ["Space"] = true, ... }
            if let Ok(keys_table) = lua.create_table() {
                // We expose a selection of commonly used keys.
                use winit::keyboard::KeyCode;
                static KEY_NAMES: &[(KeyCode, &str)] = &[
                    (KeyCode::KeyW, "KeyW"),
                    (KeyCode::KeyA, "KeyA"),
                    (KeyCode::KeyS, "KeyS"),
                    (KeyCode::KeyD, "KeyD"),
                    (KeyCode::KeyQ, "KeyQ"),
                    (KeyCode::KeyE, "KeyE"),
                    (KeyCode::KeyR, "KeyR"),
                    (KeyCode::KeyF, "KeyF"),
                    (KeyCode::Space, "Space"),
                    (KeyCode::ShiftLeft, "ShiftLeft"),
                    (KeyCode::ShiftRight, "ShiftRight"),
                    (KeyCode::ControlLeft, "ControlLeft"),
                    (KeyCode::ControlRight, "ControlRight"),
                    (KeyCode::AltLeft, "AltLeft"),
                    (KeyCode::AltRight, "AltRight"),
                    (KeyCode::Enter, "Enter"),
                    (KeyCode::Escape, "Escape"),
                    (KeyCode::Tab, "Tab"),
                    (KeyCode::ArrowUp, "ArrowUp"),
                    (KeyCode::ArrowDown, "ArrowDown"),
                    (KeyCode::ArrowLeft, "ArrowLeft"),
                    (KeyCode::ArrowRight, "ArrowRight"),
                    (KeyCode::Digit1, "Digit1"),
                    (KeyCode::Digit2, "Digit2"),
                    (KeyCode::Digit3, "Digit3"),
                    (KeyCode::Digit4, "Digit4"),
                    (KeyCode::Digit5, "Digit5"),
                    (KeyCode::Digit6, "Digit6"),
                    (KeyCode::Digit7, "Digit7"),
                    (KeyCode::Digit8, "Digit8"),
                    (KeyCode::Digit9, "Digit9"),
                    (KeyCode::Digit0, "Digit0"),
                ];

                for &(code, name) in KEY_NAMES {
                    if input.is_pressed(code) {
                        let _ = keys_table.set(name, true);
                    }
                }
                let _ = quasar.set("_pressed_keys", keys_table);
            }

            // Write pressed mouse buttons.
            if let Ok(mouse_table) = lua.create_table() {
                use quasar_window::MouseButton;
                if input.is_mouse_pressed(MouseButton::Left) {
                    let _ = mouse_table.set("left", true);
                }
                if input.is_mouse_pressed(MouseButton::Right) {
                    let _ = mouse_table.set("right", true);
                }
                if input.is_mouse_pressed(MouseButton::Middle) {
                    let _ = mouse_table.set("middle", true);
                }
                let _ = quasar.set("_pressed_mouse", mouse_table);
            }
        }
    }

    /// Read queued commands from `quasar._commands` and return them.
    fn read_commands(lua: &Lua) -> Vec<ScriptCommand> {
        let mut commands = Vec::new();

        let Ok(quasar) = lua.globals().get::<LuaTable>("quasar") else {
            return commands;
        };
        let Ok(cmd_table) = quasar.get::<LuaTable>("_commands") else {
            return commands;
        };

        let len = cmd_table.len().unwrap_or(0);
        for i in 1..=len {
            let Ok(cmd) = cmd_table.get::<LuaTable>(i) else {
                continue;
            };
            let Ok(cmd_type) = cmd.get::<String>("type") else {
                continue;
            };

            match cmd_type.as_str() {
                "set_position" => {
                    if let (Ok(eid), Ok(x), Ok(y), Ok(z)) = (
                        cmd.get::<u32>("entity"),
                        cmd.get::<f32>("x"),
                        cmd.get::<f32>("y"),
                        cmd.get::<f32>("z"),
                    ) {
                        commands.push(ScriptCommand::SetPosition {
                            entity_index: eid,
                            value: Vec3::new(x, y, z),
                        });
                    }
                }
                "set_rotation" => {
                    if let (Ok(eid), Ok(x), Ok(y), Ok(z), Ok(w)) = (
                        cmd.get::<u32>("entity"),
                        cmd.get::<f32>("x"),
                        cmd.get::<f32>("y"),
                        cmd.get::<f32>("z"),
                        cmd.get::<f32>("w"),
                    ) {
                        commands.push(ScriptCommand::SetRotation {
                            entity_index: eid,
                            value: Quat::from_xyzw(x, y, z, w),
                        });
                    }
                }
                "set_scale" => {
                    if let (Ok(eid), Ok(x), Ok(y), Ok(z)) = (
                        cmd.get::<u32>("entity"),
                        cmd.get::<f32>("x"),
                        cmd.get::<f32>("y"),
                        cmd.get::<f32>("z"),
                    ) {
                        commands.push(ScriptCommand::SetScale {
                            entity_index: eid,
                            value: Vec3::new(x, y, z),
                        });
                    }
                }
                "spawn" => {
                    commands.push(ScriptCommand::Spawn);
                }
                "despawn" => {
                    if let Ok(eid) = cmd.get::<u32>("entity") {
                        commands.push(ScriptCommand::Despawn { entity_index: eid });
                    }
                }
                "apply_force" => {
                    if let (Ok(eid), Ok(x), Ok(y), Ok(z)) = (
                        cmd.get::<u32>("entity"),
                        cmd.get::<f32>("x"),
                        cmd.get::<f32>("y"),
                        cmd.get::<f32>("z"),
                    ) {
                        commands.push(ScriptCommand::ApplyForce {
                            entity_index: eid,
                            force: Vec3::new(x, y, z),
                        });
                    }
                }
                "play_audio" => {
                    if let Ok(path) = cmd.get::<String>("path") {
                        commands.push(ScriptCommand::PlayAudio { path });
                    }
                }
                "add_component" => {
                    if let (Ok(eid), Ok(comp_name), Ok(data)) = (
                        cmd.get::<u32>("entity"),
                        cmd.get::<String>("component"),
                        cmd.get::<LuaTable>("data"),
                    ) {
                        // Store the data table in the Lua registry so it survives
                        // past the _commands table clear.
                        if let Ok(key) = lua.create_registry_value(data) {
                            commands.push(ScriptCommand::AddComponent {
                                entity_index: eid,
                                component_name: comp_name,
                                data_key: key,
                            });
                        }
                    }
                }
                "remove_component" => {
                    if let (Ok(eid), Ok(comp_name)) =
                        (cmd.get::<u32>("entity"), cmd.get::<String>("component"))
                    {
                        commands.push(ScriptCommand::RemoveComponent {
                            entity_index: eid,
                            component_name: comp_name,
                        });
                    }
                }
                _ => {
                    log::warn!("[lua] Unknown command type: {}", cmd_type);
                }
            }
        }

        // Clear the commands table for next frame.
        if let Ok(fresh) = lua.create_table() {
            let _ = quasar.set("_commands", fresh);
        }

        commands
    }

    /// Load and run per-entity scripts.
    ///
    /// Each entity's script file (pointed to by [`ScriptComponent::path`]) is
    /// expected to return a Lua table.  If the table contains an `on_start`
    /// (or legacy `on_init`) function it is called once after loading.
    /// Every frame, `on_update(entity_id, dt)` is called.
    /// When an entity is despawned or its script removed, `on_destroy(entity_id)`
    /// is called if present.
    fn run_entity_scripts(lua: &Lua, world: &mut World, dt: f32) {
        // Collect (entity_index, path, loaded) for entities with a ScriptComponent.
        let scripts: Vec<(u32, String, bool)> = world
            .query::<ScriptComponent>().into_iter().map(|(e, sc)| (e.index(), sc.path.clone(), sc.loaded))
            .collect();

        if scripts.is_empty() {
            return;
        }

        // Get or create the entity_scripts registry table.
        let registry_key = {
            let Some(resource) = world.resource::<ScriptingResource>() else {
                return;
            };
            match &resource.entity_scripts_key {
                Some(k) => lua.registry_value::<LuaTable>(k).ok(),
                None => None,
            }
        };
        let Some(entity_table) = registry_key else {
            return;
        };

        // Hot-reload: check which entity script files changed on disk.
        let changed_paths: Vec<String> = {
            let Some(resource) = world.resource::<ScriptingResource>() else {
                return;
            };
            resource.engine.check_hot_reload()
        };

        // Determine which entities need script re-evaluation.
        let mut reload_entities: Vec<(u32, String)> = Vec::new();
        if !changed_paths.is_empty() {
            for (eid, path, _) in &scripts {
                for cp in &changed_paths {
                    // Normalise comparison — match on file name ending.
                    if path.ends_with(cp.as_str()) || cp.ends_with(path.as_str()) || path == cp {
                        reload_entities.push((*eid, path.clone()));
                    }
                }
            }
        }

        // Re-evaluate changed entity scripts.
        for (eid, path) in &reload_entities {
            match std::fs::read_to_string(path) {
                Ok(source) => match lua.load(&source).eval::<LuaTable>() {
                    Ok(behaviour) => {
                        // Call on_destroy on the old behaviour if present.
                        if let Ok(old) = entity_table.get::<LuaTable>(*eid) {
                            if let Ok(destroy_fn) = old.get::<LuaFunction>("on_destroy") {
                                let _ = destroy_fn.call::<()>(*eid);
                            }
                        }
                        // Call on_start (or legacy on_init) on the new behaviour.
                        if let Ok(start_fn) = behaviour.get::<LuaFunction>("on_start") {
                            if let Err(e) = start_fn.call::<()>(*eid) {
                                log::error!("[lua] {}: on_start error: {}", path, e);
                            }
                        } else if let Ok(init_fn) = behaviour.get::<LuaFunction>("on_init") {
                            if let Err(e) = init_fn.call::<()>(*eid) {
                                log::error!("[lua] {}: on_init error: {}", path, e);
                            }
                        }
                        let _ = entity_table.set(*eid, behaviour);
                        log::info!("[lua] Hot-reloaded entity {} script: {}", eid, path);
                    }
                    Err(e) => {
                        log::error!("[lua] Failed to hot-reload {}: {}", path, e);
                    }
                },
                Err(e) => {
                    log::error!("[lua] Cannot read {}: {}", path, e);
                }
            }
        }

        for (eid, path, loaded) in &scripts {
            // ── First-time load ──────────────────────────────────
            if !loaded {
                // Register with file watcher if not already tracked.
                if let Some(resource) = world.resource_mut::<ScriptingResource>() {
                    if !resource.watched_entity_scripts.contains(path) {
                        let _ = resource.engine.exec_file(path.as_str());
                        resource.watched_entity_scripts.push(path.clone());
                    }
                }

                match std::fs::read_to_string(path) {
                    Ok(source) => {
                        match lua.load(&source).eval::<LuaTable>() {
                            Ok(behaviour) => {
                                // Call on_start (or legacy on_init) if present.
                                if let Ok(start_fn) = behaviour.get::<LuaFunction>("on_start") {
                                    if let Err(e) = start_fn.call::<()>(*eid) {
                                        log::error!("[lua] {}: on_start error: {}", path, e);
                                    }
                                } else if let Ok(init_fn) = behaviour.get::<LuaFunction>("on_init")
                                {
                                    if let Err(e) = init_fn.call::<()>(*eid) {
                                        log::error!("[lua] {}: on_init error: {}", path, e);
                                    }
                                }
                                let _ = entity_table.set(*eid, behaviour);
                            }
                            Err(e) => {
                                log::error!("[lua] Failed to load {}: {}", path, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[lua] Cannot read {}: {}", path, e);
                    }
                }

                // Mark as loaded — need mutable access.
                if let Some(sc) = world.get_mut::<ScriptComponent>({
                    let found: Option<Entity> = world
                        .query::<ScriptComponent>().into_iter().find(|(e, _)| e.index() == *eid)
                        .map(|(e, _)| e);
                    match found {
                        Some(e) => e,
                        None => continue,
                    }
                }) {
                    sc.loaded = true;
                }
            }

            // ── Per-frame update ─────────────────────────────────
            if let Ok(behaviour) = entity_table.get::<LuaTable>(*eid) {
                if let Ok(update_fn) = behaviour.get::<LuaFunction>("on_update") {
                    if let Err(e) = update_fn.call::<()>((*eid, dt)) {
                        log::error!("[lua] entity {}: on_update error: {}", eid, e);
                    }
                }
            }
        }
    }

    /// Call `on_destroy(entity_id)` for entities that are about to be despawned.
    ///
    /// Should be called before the entity is actually removed from the world.
    fn call_on_destroy(lua: &Lua, world: &World, entity_index: u32) {
        let registry_key = {
            let Some(resource) = world.resource::<ScriptingResource>() else {
                return;
            };
            match &resource.entity_scripts_key {
                Some(k) => lua.registry_value::<LuaTable>(k).ok(),
                None => None,
            }
        };
        let Some(entity_table) = registry_key else {
            return;
        };
        if let Ok(behaviour) = entity_table.get::<LuaTable>(entity_index) {
            if let Ok(destroy_fn) = behaviour.get::<LuaFunction>("on_destroy") {
                if let Err(e) = destroy_fn.call::<()>(entity_index) {
                    log::error!("[lua] entity {}: on_destroy error: {}", entity_index, e);
                }
            }
            // Remove from registry so it's not called again.
            let _ = entity_table.set(entity_index, mlua::Value::Nil);
        }
    }

    /// Apply queued commands to the world.
    ///
    /// We need an entity-index → Entity map for mutations on existing entities.
    fn apply_commands(lua: &Lua, world: &mut World, commands: Vec<ScriptCommand>) {
        // Build a map of entity_index → Entity for live entities with transforms.
        let entity_map: std::collections::HashMap<u32, Entity> = world
            .query::<Transform>().into_iter().map(|(e, _)| (e.index(), e))
            .collect();

        for cmd in commands {
            match cmd {
                ScriptCommand::SetPosition {
                    entity_index,
                    value,
                } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        if let Some(transform) = world.get_mut::<Transform>(entity) {
                            transform.position = value;
                        }
                    }
                }
                ScriptCommand::SetRotation {
                    entity_index,
                    value,
                } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        if let Some(transform) = world.get_mut::<Transform>(entity) {
                            transform.rotation = value;
                        }
                    }
                }
                ScriptCommand::SetScale {
                    entity_index,
                    value,
                } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        if let Some(transform) = world.get_mut::<Transform>(entity) {
                            transform.scale = value;
                        }
                    }
                }
                ScriptCommand::Spawn => {
                    let entity = world.spawn();
                    world.insert(entity, Transform::IDENTITY);
                    log::debug!("[lua] Spawned entity {:?}", entity);
                }
                ScriptCommand::Despawn { entity_index } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        // Call on_destroy before despawning.
                        Self::call_on_destroy(lua, world, entity_index);
                        world.despawn(entity);
                        log::debug!("[lua] Despawned entity {:?}", entity);
                    }
                }
                ScriptCommand::ApplyForce {
                    entity_index,
                    force,
                } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        // Get the rigid body handle, then apply force through the physics world.
                        if let Some(rigid_body) =
                            world.get::<quasar_physics::RigidBodyComponent>(entity)
                        {
                            let handle = rigid_body.handle;
                            if let Some(phys) =
                                world.resource_mut::<quasar_physics::PhysicsResource>()
                            {
                                phys.physics
                                    .apply_force(handle, [force.x, force.y, force.z]);
                                log::debug!(
                                    "[lua] Applied force {:?} to entity {}",
                                    force,
                                    entity_index
                                );
                            }
                        }
                    }
                }
                ScriptCommand::PlayAudio { path } => {
                    if let Some(audio) = world.resource_mut::<quasar_audio::AudioSystem>() {
                        if let Some(id) = audio.play(&path) {
                            log::debug!("[lua] Playing audio: {} (id: {})", path, id);
                        } else {
                            log::warn!("[lua] Failed to play audio: {}", path);
                        }
                    } else {
                        log::warn!("[lua] Audio system not available");
                    }
                }
                ScriptCommand::AddComponent {
                    entity_index,
                    component_name,
                    data_key,
                } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        let data_table: Option<LuaTable> = lua.registry_value(&data_key).ok();
                        if let Some(data) = data_table {
                            let inserted = {
                                let reg = &world
                                    .resource::<ScriptingResource>()
                                    .map(|r| &r.component_registry);
                                if let Some(reg) = reg {
                                    if let Some(desc) = reg.get(&component_name) {
                                        if let Err(e) = (desc.insert)(world, entity, &data) {
                                            log::error!(
                                                "[lua] Failed to add component '{}' to entity {}: {}",
                                                component_name, entity_index, e
                                            );
                                        }
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            };
                            if !inserted {
                                log::warn!(
                                    "[lua] Unknown component type '{}' — not in registry",
                                    component_name
                                );
                            }
                        }
                        let _ = lua.remove_registry_value(data_key);
                    }
                }
                ScriptCommand::RemoveComponent {
                    entity_index,
                    component_name,
                } => {
                    if let Some(&entity) = entity_map.get(&entity_index) {
                        let found = if let Some(resource) = world.resource::<ScriptingResource>() {
                            resource.component_registry.get(&component_name).is_some()
                        } else {
                            false
                        };
                        if found {
                            if let Some(resource) = world.resource::<ScriptingResource>() {
                                if let Some(desc) = resource.component_registry.get(&component_name)
                                {
                                    (desc.remove)(world, entity);
                                }
                            }
                        } else {
                            log::warn!(
                                "[lua] Cannot remove unknown component '{}'",
                                component_name
                            );
                        }
                    }
                }
            }
        }
    }
}

impl System for ScriptingSystem {
    fn name(&self) -> &str {
        "scripting_update"
    }

    fn run(&mut self, world: &mut World) {
        if !quasar_core::simulation_active(world) {
            return;
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() {
            p.begin_scope("scripting_update");
        }
        // Read delta time from the Time resource.
        let dt = world
            .resource::<Time>()
            .map(|t| t.delta_seconds())
            .unwrap_or(1.0 / 60.0);

        let total_time = world
            .resource::<Time>()
            .map(|t| t.elapsed_seconds())
            .unwrap_or(0.0);

        // ── Phase 1: Serialize world state into Lua ────────────────
        // We need a raw pointer to get the Lua state without holding a
        // &mut World borrow (we need World for queries too).
        // Safety: ScriptingSystem is single-threaded; we don't re-enter.
        let lua_ptr: *const mlua::Lua = {
            let Some(resource) = world.resource::<ScriptingResource>() else {
                return;
            };
            resource.engine.lua() as *const _
        };

        // SAFETY: We only use this pointer while `world` owns the resource.
        let lua = unsafe { &*lua_ptr };

        // Write dt, time, transforms, and input.
        if let Ok(quasar) = lua.globals().get::<LuaTable>("quasar") {
            let _ = quasar.set("_dt", dt);
            let _ = quasar.set("_time", total_time);
        }
        Self::write_transforms(lua, world);
        Self::write_input(lua, world);
        Self::write_component_data(lua, world);

        // ── Phase 2: Run Lua scripts ──────────────────────────────
        {
            let Some(resource) = world.resource_mut::<ScriptingResource>() else {
                return;
            };

            // Hot-reload check using file events.
            let _reloaded = resource.engine.hot_reload();

            // Call the global `on_update(dt)` if it exists.
            //
            // **DEPRECATED** — The global `on_update(dt)` pattern is deprecated.
            // Prefer attaching a `ScriptComponent` to an entity and defining
            // `on_update(entity_id, dt)` in the per-entity behaviour table.
            // If you need a "global" script, model it as a singleton entity
            // with a `ScriptComponent`.  The global callback will be removed
            // in a future release.
            if resource
                .engine
                .lua()
                .globals()
                .get::<LuaFunction>("on_update")
                .is_ok()
            {
                use std::sync::Once;
                static DEPRECATION_WARNING: Once = Once::new();
                DEPRECATION_WARNING.call_once(|| {
                    log::warn!(
                        "[scripting] Global `on_update(dt)` is DEPRECATED. \
                         Use per-entity ScriptComponent with on_update(entity_id, dt) instead. \
                         For global logic, attach a ScriptComponent to a singleton entity."
                    );
                });
                let _ = resource.engine.call_function::<_, ()>("on_update", dt);
            }
        }

        // ── Phase 2b: Per-entity script execution ─────────────────
        //
        // Entities with a ScriptComponent get their script loaded (once) and
        // then their per-entity `on_update(entity_id, dt)` called every frame.
        Self::run_entity_scripts(lua, world, dt);

        // ── Phase 3: Apply queued commands ────────────────────────
        let commands = Self::read_commands(lua);
        if !commands.is_empty() {
            Self::apply_commands(lua, world, commands);
        }
        if let Some(p) = world.resource_mut::<quasar_core::Profiler>() {
            p.end_scope("scripting_update");
        }
    }
}

/// Plugin that registers the scripting engine and update system.
pub struct ScriptingPlugin;

impl quasar_core::Plugin for ScriptingPlugin {
    fn name(&self) -> &str {
        "ScriptingPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        app.world.insert_resource(ScriptingResource::new());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(ScriptingSystem),
        );

        log::info!("ScriptingPlugin loaded — Lua scripting active");
    }
}
