//! Lua–ECS bridge — exposes engine APIs to Lua scripts.
//!
//! Registers functions under the `quasar` global table that allow Lua code
//! to interact with the ECS world (get/set transforms, spawn/despawn, input).
//!
//! **Data flow:**
//! Before each frame the plugin writes entity transforms and key state into
//! Lua globals (`quasar._transforms`, `quasar._pressed_keys`). Bridge
//! functions read from these tables. Mutations are queued in
//! `quasar._commands` and applied by the plugin after `on_update` returns.
//!
//! **Extended API (task 15):**
//! - `quasar.get_input(action_name)` — query ActionMap bindings
//! - `quasar.play_audio(path)` — queue audio playback
//! - `quasar.raycast(origin, dir, max_dist)` — physics raycast
//! - `quasar.get_component(entity, type_name)` — get component data
//! - `quasar.apply_force(entity, x, y, z)` — apply physics force

use mlua::prelude::*;

/// Register the ECS bridge functions into the Lua global `quasar` table.
///
/// Available Lua functions after registration:
/// - `quasar.log(msg)` — print an info log
/// - `quasar.dt()` — last frame delta time
/// - `quasar.time()` — total elapsed time
/// - `quasar.vec3(x, y, z)` — create a vec3 table
/// - `quasar.lerp(a, b, t)` — linear interpolation
/// - `quasar.clamp(val, min, max)` — clamp a number
/// - `quasar.get_position(entity_id)` — returns {x, y, z}
/// - `quasar.set_position(entity_id, x, y, z)` — queued for write-back
/// - `quasar.get_rotation(entity_id)` — returns {x, y, z, w} quaternion
/// - `quasar.set_rotation(entity_id, x, y, z, w)` — queued for write-back
/// - `quasar.get_scale(entity_id)` — returns {x, y, z}
/// - `quasar.set_scale(entity_id, x, y, z)` — queued for write-back
/// - `quasar.spawn()` — queue a new entity spawn, returns command index
/// - `quasar.despawn(entity_id)` — queue entity removal
/// - `quasar.is_key_pressed(key_name)` — check keyboard key
/// - `quasar.is_mouse_pressed(button_name)` — check mouse button
/// - `quasar.get_entity_count()` — how many entities have transforms
/// - `quasar.get_input(action_name)` — query ActionMap (returns value 0-1)
/// - `quasar.play_audio(path)` — queue audio playback
/// - `quasar.raycast(ox, oy, oz, dx, dy, dz, max_dist)` — physics raycast
/// - `quasar.get_component(entity_id, type_name)` — get component data
/// - `quasar.apply_force(entity_id, x, y, z)` — apply physics force
pub fn register_bridge(lua: &Lua) -> LuaResult<()> {
    let quasar: LuaTable = lua.globals().get("quasar")?;

    // Initialise internal tables that the plugin populates each frame.
    quasar.set("_transforms", lua.create_table()?)?;
    quasar.set("_pressed_keys", lua.create_table()?)?;
    quasar.set("_pressed_mouse", lua.create_table()?)?;
    quasar.set("_commands", lua.create_table()?)?;
    quasar.set("_action_values", lua.create_table()?)?;
    quasar.set("_raycast_results", lua.create_table()?)?;
    quasar.set("_audio_queue", lua.create_table()?)?;
    quasar.set("_physics_components", lua.create_table()?)?;

    // ── KeyCode constants table ─────────────────────────────────────
    // Expose all KeyCode variants so scripts can use quasar.KeyCode.KeyW
    let keycodes = lua.create_table()?;
    let key_names = [
        "KeyA",
        "KeyB",
        "KeyC",
        "KeyD",
        "KeyE",
        "KeyF",
        "KeyG",
        "KeyH",
        "KeyI",
        "KeyJ",
        "KeyK",
        "KeyL",
        "KeyM",
        "KeyN",
        "KeyO",
        "KeyP",
        "KeyQ",
        "KeyR",
        "KeyS",
        "KeyT",
        "KeyU",
        "KeyV",
        "KeyW",
        "KeyX",
        "KeyY",
        "KeyZ",
        "Digit0",
        "Digit1",
        "Digit2",
        "Digit3",
        "Digit4",
        "Digit5",
        "Digit6",
        "Digit7",
        "Digit8",
        "Digit9",
        "Numpad0",
        "Numpad1",
        "Numpad2",
        "Numpad3",
        "Numpad4",
        "Numpad5",
        "Numpad6",
        "Numpad7",
        "Numpad8",
        "Numpad9",
        "NumpadAdd",
        "NumpadSubtract",
        "NumpadMultiply",
        "NumpadDivide",
        "NumpadEnter",
        "NumpadDecimal",
        "NumpadEqual",
        "F1",
        "F2",
        "F3",
        "F4",
        "F5",
        "F6",
        "F7",
        "F8",
        "F9",
        "F10",
        "F11",
        "F12",
        "F13",
        "F14",
        "F15",
        "F16",
        "F17",
        "F18",
        "F19",
        "F20",
        "F21",
        "F22",
        "F23",
        "F24",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Insert",
        "Delete",
        "ShiftLeft",
        "ShiftRight",
        "ControlLeft",
        "ControlRight",
        "AltLeft",
        "AltRight",
        "SuperLeft",
        "SuperRight",
        "CapsLock",
        "NumLock",
        "ScrollLock",
        "Space",
        "Enter",
        "Tab",
        "Backspace",
        "Escape",
        "Minus",
        "Equal",
        "BracketLeft",
        "BracketRight",
        "Backslash",
        "Semicolon",
        "Quote",
        "Comma",
        "Period",
        "Slash",
    ];
    for name in key_names {
        keycodes.set(name, name)?;
    }
    quasar.set("KeyCode", keycodes)?;

    // ── Utilities ─────────────────────────────────────────────────

    // quasar.log(msg)
    let log_fn = lua.create_function(|_, msg: String| {
        log::info!("[lua] {}", msg);
        Ok(())
    })?;
    quasar.set("log", log_fn)?;

    // quasar.dt()
    let dt_fn = lua.create_function(|lua, ()| -> LuaResult<f32> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let dt: f32 = quasar.get("_dt").unwrap_or(0.016);
        Ok(dt)
    })?;
    quasar.set("dt", dt_fn)?;

    // quasar.time()
    let time_fn = lua.create_function(|lua, ()| -> LuaResult<f32> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let t: f32 = quasar.get("_time").unwrap_or(0.0);
        Ok(t)
    })?;
    quasar.set("time", time_fn)?;

    // quasar.vec3(x, y, z)
    let vec3_fn = lua.create_function(|lua, (x, y, z): (f32, f32, f32)| {
        let t = lua.create_table()?;
        t.set("x", x)?;
        t.set("y", y)?;
        t.set("z", z)?;
        Ok(t)
    })?;
    quasar.set("vec3", vec3_fn)?;

    // quasar.lerp(a, b, t)
    let lerp_fn = lua.create_function(|_, (a, b, t): (f32, f32, f32)| Ok(a + (b - a) * t))?;
    quasar.set("lerp", lerp_fn)?;

    // quasar.clamp(value, min, max)
    let clamp_fn =
        lua.create_function(|_, (value, min, max): (f32, f32, f32)| Ok(value.clamp(min, max)))?;
    quasar.set("clamp", clamp_fn)?;

    // quasar.distance(x1, y1, z1, x2, y2, z2) -> number
    let distance_fn = lua.create_function(
        |_, (x1, y1, z1, x2, y2, z2): (f32, f32, f32, f32, f32, f32)| {
            let dx = x2 - x1;
            let dy = y2 - y1;
            let dz = z2 - z1;
            Ok((dx * dx + dy * dy + dz * dz).sqrt())
        },
    )?;
    quasar.set("distance", distance_fn)?;

    // quasar.normalize(x, y, z) -> {x, y, z}
    let normalize_fn = lua.create_function(|lua, (x, y, z): (f32, f32, f32)| {
        let len = (x * x + y * y + z * z).sqrt();
        let t = lua.create_table()?;
        if len > 1e-8 {
            t.set("x", x / len)?;
            t.set("y", y / len)?;
            t.set("z", z / len)?;
        } else {
            t.set("x", 0.0f32)?;
            t.set("y", 0.0f32)?;
            t.set("z", 0.0f32)?;
        }
        Ok(t)
    })?;
    quasar.set("normalize", normalize_fn)?;

    // quasar.dot(x1, y1, z1, x2, y2, z2) -> number
    let dot_fn = lua.create_function(
        |_, (x1, y1, z1, x2, y2, z2): (f32, f32, f32, f32, f32, f32)| {
            Ok(x1 * x2 + y1 * y2 + z1 * z2)
        },
    )?;
    quasar.set("dot", dot_fn)?;

    // quasar.cross(x1, y1, z1, x2, y2, z2) -> {x, y, z}
    let cross_fn = lua.create_function(
        |lua, (x1, y1, z1, x2, y2, z2): (f32, f32, f32, f32, f32, f32)| {
            let t = lua.create_table()?;
            t.set("x", y1 * z2 - z1 * y2)?;
            t.set("y", z1 * x2 - x1 * z2)?;
            t.set("z", x1 * y2 - y1 * x2)?;
            Ok(t)
        },
    )?;
    quasar.set("cross", cross_fn)?;

    // quasar.random() -> number (0..1)
    let random_fn = lua.create_function(|_, ()| {
        // Simple xorshift-based PRNG seeded from frame counter.
        // For scripts this is adequate; not cryptographic.
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let mut x = seed.wrapping_add(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        Ok((x as f32) / (u32::MAX as f32))
    })?;
    quasar.set("random", random_fn)?;

    // quasar.random_range(min, max) -> number
    let random_range_fn = lua.create_function(|_, (min, max): (f32, f32)| {
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let mut x = seed.wrapping_add(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let t = (x as f32) / (u32::MAX as f32);
        Ok(min + (max - min) * t)
    })?;
    quasar.set("random_range", random_range_fn)?;

    // quasar.set_parent(child_id, parent_id) — queue scene-graph parenting
    let set_parent_fn = lua.create_function(|lua, (child_id, parent_id): (u32, u32)| {
        let cmd = lua.create_table()?;
        cmd.set("type", "set_parent")?;
        cmd.set("child", child_id)?;
        cmd.set("parent", parent_id)?;
        push_command(lua, cmd)?;
        Ok(())
    })?;
    quasar.set("set_parent", set_parent_fn)?;

    // quasar.look_at(entity_id, tx, ty, tz) — queue a look-at rotation
    let look_at_fn =
        lua.create_function(|lua, (entity_id, tx, ty, tz): (u32, f32, f32, f32)| {
            let cmd = lua.create_table()?;
            cmd.set("type", "look_at")?;
            cmd.set("entity", entity_id)?;
            cmd.set("tx", tx)?;
            cmd.set("ty", ty)?;
            cmd.set("tz", tz)?;
            push_command(lua, cmd)?;
            Ok(())
        })?;
    quasar.set("look_at", look_at_fn)?;

    // ── Transform getters (read from _transforms) ─────────────────

    // quasar.get_position(entity_id) -> {x, y, z} or nil
    let get_pos_fn = lua.create_function(|lua, entity_id: u32| -> LuaResult<LuaValue> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let transforms: LuaTable = quasar.get("_transforms")?;
        let entry: Option<LuaTable> = transforms.get(entity_id)?;
        match entry {
            Some(t) => {
                let result = lua.create_table()?;
                result.set("x", t.get::<f32>("px")?)?;
                result.set("y", t.get::<f32>("py")?)?;
                result.set("z", t.get::<f32>("pz")?)?;
                Ok(LuaValue::Table(result))
            }
            None => Ok(LuaValue::Nil),
        }
    })?;
    quasar.set("get_position", get_pos_fn)?;

    // quasar.get_rotation(entity_id) -> {x, y, z, w} or nil
    let get_rot_fn = lua.create_function(|lua, entity_id: u32| -> LuaResult<LuaValue> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let transforms: LuaTable = quasar.get("_transforms")?;
        let entry: Option<LuaTable> = transforms.get(entity_id)?;
        match entry {
            Some(t) => {
                let result = lua.create_table()?;
                result.set("x", t.get::<f32>("rx")?)?;
                result.set("y", t.get::<f32>("ry")?)?;
                result.set("z", t.get::<f32>("rz")?)?;
                result.set("w", t.get::<f32>("rw")?)?;
                Ok(LuaValue::Table(result))
            }
            None => Ok(LuaValue::Nil),
        }
    })?;
    quasar.set("get_rotation", get_rot_fn)?;

    // quasar.get_scale(entity_id) -> {x, y, z} or nil
    let get_scale_fn = lua.create_function(|lua, entity_id: u32| -> LuaResult<LuaValue> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let transforms: LuaTable = quasar.get("_transforms")?;
        let entry: Option<LuaTable> = transforms.get(entity_id)?;
        match entry {
            Some(t) => {
                let result = lua.create_table()?;
                result.set("x", t.get::<f32>("sx")?)?;
                result.set("y", t.get::<f32>("sy")?)?;
                result.set("z", t.get::<f32>("sz")?)?;
                Ok(LuaValue::Table(result))
            }
            None => Ok(LuaValue::Nil),
        }
    })?;
    quasar.set("get_scale", get_scale_fn)?;

    // quasar.get_entity_count() -> number
    let get_count_fn = lua.create_function(|lua, ()| -> LuaResult<u32> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let transforms: LuaTable = quasar.get("_transforms")?;
        Ok(transforms.len()? as u32)
    })?;
    quasar.set("get_entity_count", get_count_fn)?;

    // ── Transform setters (queue commands) ────────────────────────

    // Helper to push a command into quasar._commands
    fn push_command(lua: &Lua, cmd: LuaTable) -> LuaResult<()> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let commands: LuaTable = quasar.get("_commands")?;
        let len = commands.len()?;
        commands.set(len + 1, cmd)?;
        Ok(())
    }

    // quasar.set_position(entity_id, x, y, z)
    let set_pos_fn = lua.create_function(|lua, (entity_id, x, y, z): (u32, f32, f32, f32)| {
        let cmd = lua.create_table()?;
        cmd.set("type", "set_position")?;
        cmd.set("entity", entity_id)?;
        cmd.set("x", x)?;
        cmd.set("y", y)?;
        cmd.set("z", z)?;
        push_command(lua, cmd)?;
        Ok(())
    })?;
    quasar.set("set_position", set_pos_fn)?;

    // quasar.set_rotation(entity_id, x, y, z, w)
    let set_rot_fn =
        lua.create_function(|lua, (entity_id, x, y, z, w): (u32, f32, f32, f32, f32)| {
            let cmd = lua.create_table()?;
            cmd.set("type", "set_rotation")?;
            cmd.set("entity", entity_id)?;
            cmd.set("x", x)?;
            cmd.set("y", y)?;
            cmd.set("z", z)?;
            cmd.set("w", w)?;
            push_command(lua, cmd)?;
            Ok(())
        })?;
    quasar.set("set_rotation", set_rot_fn)?;

    // quasar.set_scale(entity_id, x, y, z)
    let set_scale_fn = lua.create_function(|lua, (entity_id, x, y, z): (u32, f32, f32, f32)| {
        let cmd = lua.create_table()?;
        cmd.set("type", "set_scale")?;
        cmd.set("entity", entity_id)?;
        cmd.set("x", x)?;
        cmd.set("y", y)?;
        cmd.set("z", z)?;
        push_command(lua, cmd)?;
        Ok(())
    })?;
    quasar.set("set_scale", set_scale_fn)?;

    // quasar.spawn() -> pushes a spawn command
    let spawn_fn = lua.create_function(|lua, ()| {
        let cmd = lua.create_table()?;
        cmd.set("type", "spawn")?;
        push_command(lua, cmd)?;
        Ok(())
    })?;
    quasar.set("spawn", spawn_fn)?;

    // quasar.despawn(entity_id)
    let despawn_fn = lua.create_function(|lua, entity_id: u32| {
        let cmd = lua.create_table()?;
        cmd.set("type", "despawn")?;
        cmd.set("entity", entity_id)?;
        push_command(lua, cmd)?;
        Ok(())
    })?;
    quasar.set("despawn", despawn_fn)?;

    // ── Input queries (read from _pressed_keys / _pressed_mouse) ──

    // quasar.is_key_pressed(key_name) -> bool
    let key_fn = lua.create_function(|lua, key_name: String| -> LuaResult<bool> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let keys: LuaTable = quasar.get("_pressed_keys")?;
        let pressed: bool = keys.get(key_name).unwrap_or(false);
        Ok(pressed)
    })?;
    quasar.set("is_key_pressed", key_fn)?;

    // quasar.is_mouse_pressed(button_name) -> bool ("left", "right", "middle")
    let mouse_fn = lua.create_function(|lua, button_name: String| -> LuaResult<bool> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let buttons: LuaTable = quasar.get("_pressed_mouse")?;
        let pressed: bool = buttons.get(button_name).unwrap_or(false);
        Ok(pressed)
    })?;
    quasar.set("is_mouse_pressed", mouse_fn)?;

    // ── Extended API (task 15) ─────────────────────────────────────

    // quasar.get_input(action_name) -> number (0.0-1.0)
    let get_input_fn = lua.create_function(|lua, action_name: String| -> LuaResult<f32> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let actions: LuaTable = quasar.get("_action_values")?;
        let value: f32 = actions.get(action_name.as_str()).unwrap_or(0.0);
        Ok(value)
    })?;
    quasar.set("get_input", get_input_fn)?;

    // quasar.play_audio(path)
    let play_audio_fn = lua.create_function(|lua, path: String| {
        let cmd = lua.create_table()?;
        cmd.set("type", "play_audio")?;
        cmd.set("path", path)?;
        push_command(lua, cmd)?;
        Ok(())
    })?;
    quasar.set("play_audio", play_audio_fn)?;

    // quasar.raycast(ox, oy, oz, dx, dy, dz, max_dist) -> table or nil
    let raycast_fn = lua.create_function(
        |lua,
         (ox, oy, oz, dx, dy, dz, max_dist): (f32, f32, f32, f32, f32, f32, f32)|
         -> LuaResult<LuaValue> {
            let quasar: LuaTable = lua.globals().get("quasar")?;
            let results: LuaTable = quasar.get("_raycast_results")?;

            let key = format!(
                "{:.2},{:.2},{:.2}|{:.2},{:.2},{:.2}|{:.2}",
                ox, oy, oz, dx, dy, dz, max_dist
            );
            match results.get::<Option<LuaTable>>(key.as_str())? {
                Some(hit) => {
                    let result = lua.create_table()?;
                    result.set("entity", hit.get::<u32>("entity")?)?;
                    result.set("distance", hit.get::<f32>("distance")?)?;
                    let point = hit.get::<LuaTable>("point")?;
                    let pt_result = lua.create_table()?;
                    pt_result.set("x", point.get::<f32>("x")?)?;
                    pt_result.set("y", point.get::<f32>("y")?)?;
                    pt_result.set("z", point.get::<f32>("z")?)?;
                    result.set("point", pt_result)?;
                    Ok(LuaValue::Table(result))
                }
                None => Ok(LuaValue::Nil),
            }
        },
    )?;
    quasar.set("raycast", raycast_fn)?;

    // quasar.get_component(entity_id, type_name) -> table or nil
    // Uses the component registry data populated by write_component_data()
    let get_component_fn = lua.create_function(
        |lua, (entity_id, type_name): (u32, String)| -> LuaResult<LuaValue> {
            let quasar: LuaTable = lua.globals().get("quasar")?;
            let component_data: LuaTable = quasar.get("_component_data")?;

            // component_data[type_name][entity_id]
            match component_data.get::<Option<LuaTable>>(type_name.as_str())? {
                Some(entities_table) => match entities_table.get::<Option<LuaTable>>(entity_id)? {
                    Some(comp) => Ok(LuaValue::Table(comp)),
                    None => Ok(LuaValue::Nil),
                },
                None => Ok(LuaValue::Nil),
            }
        },
    )?;
    quasar.set("get_component", get_component_fn)?;

    // quasar.apply_force(entity_id, x, y, z)
    let apply_force_fn =
        lua.create_function(|lua, (entity_id, x, y, z): (u32, f32, f32, f32)| {
            let cmd = lua.create_table()?;
            cmd.set("type", "apply_force")?;
            cmd.set("entity", entity_id)?;
            cmd.set("x", x)?;
            cmd.set("y", y)?;
            cmd.set("z", z)?;
            push_command(lua, cmd)?;
            Ok(())
        })?;
    quasar.set("apply_force", apply_force_fn)?;

    // quasar.get_velocity(entity_id) -> table or nil
    let get_velocity_fn = lua.create_function(|lua, entity_id: u32| -> LuaResult<LuaValue> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let components: LuaTable = quasar.get("_physics_components")?;

        let entity_key = entity_id.to_string();
        match components.get::<Option<LuaTable>>(entity_key.as_str())? {
            Some(entity_components) => {
                match entity_components.get::<Option<LuaTable>>("RigidBody")? {
                    Some(rb) => {
                        let result = lua.create_table()?;
                        result.set("x", rb.get::<f32>("vx")?)?;
                        result.set("y", rb.get::<f32>("vy")?)?;
                        result.set("z", rb.get::<f32>("vz")?)?;
                        Ok(LuaValue::Table(result))
                    }
                    None => Ok(LuaValue::Nil),
                }
            }
            None => Ok(LuaValue::Nil),
        }
    })?;
    quasar.set("get_velocity", get_velocity_fn)?;

    // ── ECS query API ──────────────────────────────────────────────

    // Internal table populated by the plugin each frame:
    // quasar._component_data[type_name] = { [entity_id] = { ... }, ... }
    quasar.set("_component_data", lua.create_table()?)?;

    // quasar.query(component_name) -> {{entity=id, ...fields}, ...}
    // Returns all entities that have the named component, with the
    // component's fields inlined into each result row plus an `entity` key.
    let query_fn = lua.create_function(|lua, component_name: String| -> LuaResult<LuaTable> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let component_data: LuaTable = quasar.get("_component_data")?;

        let result = lua.create_table()?;
        let mut idx = 1u32;

        if let Some(entities_table) =
            component_data.get::<Option<LuaTable>>(component_name.as_str())?
        {
            for pair in entities_table.pairs::<LuaValue, LuaTable>() {
                let (key, fields) = pair?;
                let row = lua.create_table()?;
                // Copy entity id.
                match key {
                    LuaValue::Integer(i) => row.set("entity", i)?,
                    LuaValue::String(s) => {
                        if let Ok(id) = s.to_str()?.parse::<u32>() {
                            row.set("entity", id)?;
                        }
                    }
                    _ => {}
                }
                // Copy all fields from the component.
                for pair in fields.pairs::<String, LuaValue>() {
                    let (k, v) = pair?;
                    row.set(k, v)?;
                }
                result.set(idx, row)?;
                idx += 1;
            }
        }

        Ok(result)
    })?;
    quasar.set("query", query_fn)?;

    // quasar.query_entities(component_name) -> {entity_id, entity_id, ...}
    // Returns just the entity ids (cheaper than full query).
    let query_entities_fn =
        lua.create_function(|lua, component_name: String| -> LuaResult<LuaTable> {
            let quasar: LuaTable = lua.globals().get("quasar")?;
            let component_data: LuaTable = quasar.get("_component_data")?;
            let result = lua.create_table()?;
            let mut idx = 1u32;

            if let Some(entities_table) =
                component_data.get::<Option<LuaTable>>(component_name.as_str())?
            {
                for pair in entities_table.pairs::<LuaValue, LuaValue>() {
                    let (key, _) = pair?;
                    result.set(idx, key)?;
                    idx += 1;
                }
            }

            Ok(result)
        })?;
    quasar.set("query_entities", query_entities_fn)?;

    // quasar.has_component(entity_id, component_name) -> bool
    let has_component_fn = lua.create_function(
        |lua, (entity_id, component_name): (u32, String)| -> LuaResult<bool> {
            let quasar: LuaTable = lua.globals().get("quasar")?;
            let component_data: LuaTable = quasar.get("_component_data")?;
            if let Some(entities_table) =
                component_data.get::<Option<LuaTable>>(component_name.as_str())?
            {
                let exists: Option<LuaTable> = entities_table.get(entity_id)?;
                Ok(exists.is_some())
            } else {
                Ok(false)
            }
        },
    )?;
    quasar.set("has_component", has_component_fn)?;

    // quasar.add_component(entity_id, component_name, data_table)
    // Queues an add-component command.
    let add_component_fn = lua.create_function(
        |lua, (entity_id, component_name, data): (u32, String, LuaTable)| {
            let cmd = lua.create_table()?;
            cmd.set("type", "add_component")?;
            cmd.set("entity", entity_id)?;
            cmd.set("component", component_name)?;
            cmd.set("data", data)?;
            push_command(lua, cmd)?;
            Ok(())
        },
    )?;
    quasar.set("add_component", add_component_fn)?;

    // quasar.remove_component(entity_id, component_name)
    let remove_component_fn =
        lua.create_function(|lua, (entity_id, component_name): (u32, String)| {
            let cmd = lua.create_table()?;
            cmd.set("type", "remove_component")?;
            cmd.set("entity", entity_id)?;
            cmd.set("component", component_name)?;
            push_command(lua, cmd)?;
            Ok(())
        })?;
    quasar.set("remove_component", remove_component_fn)?;

    // ── Event system ───────────────────────────────────────────────

    // Internal table: quasar._event_handlers[event_name] = { handler1, handler2, ... }
    quasar.set("_event_handlers", lua.create_table()?)?;

    // quasar.on_event(event_name, handler_fn)
    // Register a callback for a named event.
    let on_event_fn =
        lua.create_function(|lua, (event_name, handler): (String, LuaFunction)| {
            let quasar: LuaTable = lua.globals().get("quasar")?;
            let handlers: LuaTable = quasar.get("_event_handlers")?;
            let list: LuaTable = match handlers.get::<Option<LuaTable>>(event_name.as_str())? {
                Some(existing) => existing,
                None => {
                    let new_list = lua.create_table()?;
                    handlers.set(event_name.as_str(), new_list.clone())?;
                    new_list
                }
            };
            let len = list.len()?;
            list.set(len + 1, handler)?;
            Ok(())
        })?;
    quasar.set("on_event", on_event_fn)?;

    // quasar.emit_event(event_name, data_table_or_nil)
    // Fire an event, calling all registered handlers.
    let emit_event_fn = lua.create_function(|lua, (event_name, data): (String, LuaValue)| {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let handlers: LuaTable = quasar.get("_event_handlers")?;
        if let Some(list) = handlers.get::<Option<LuaTable>>(event_name.as_str())? {
            let len = list.len()?;
            for i in 1..=len {
                if let Ok(handler) = list.get::<LuaFunction>(i) {
                    if let Err(e) = handler.call::<()>(data.clone()) {
                        log::error!("[lua] Event '{}' handler error: {}", event_name, e);
                    }
                }
            }
        }
        Ok(())
    })?;
    quasar.set("emit_event", emit_event_fn)?;

    // ── Collision event registration API ─────────────────────────────

    // quasar.on_collision(handler_fn)
    // Register a callback for collision events. Handler receives {entity1, entity2, event_type}
    let on_collision_fn = lua.create_function(|lua, handler: LuaFunction| {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let handlers: LuaTable = quasar.get("_event_handlers")?;
        let list: LuaTable = match handlers.get::<Option<LuaTable>>("collision")? {
            Some(existing) => existing,
            None => {
                let new_list = lua.create_table()?;
                handlers.set("collision", new_list.clone())?;
                new_list
            }
        };
        let len = list.len()?;
        list.set(len + 1, handler)?;
        Ok(())
    })?;
    quasar.set("on_collision", on_collision_fn)?;

    // quasar.on_spawn(handler_fn)
    // Register a callback for entity spawn events. Handler receives {entity}
    let on_spawn_fn = lua.create_function(|lua, handler: LuaFunction| {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let handlers: LuaTable = quasar.get("_event_handlers")?;
        let list: LuaTable = match handlers.get::<Option<LuaTable>>("spawn")? {
            Some(existing) => existing,
            None => {
                let new_list = lua.create_table()?;
                handlers.set("spawn", new_list.clone())?;
                new_list
            }
        };
        let len = list.len()?;
        list.set(len + 1, handler)?;
        Ok(())
    })?;
    quasar.set("on_spawn", on_spawn_fn)?;

    // quasar.on_despawn(handler_fn)
    // Register a callback for entity despawn events. Handler receives {entity}
    let on_despawn_fn = lua.create_function(|lua, handler: LuaFunction| {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let handlers: LuaTable = quasar.get("_event_handlers")?;
        let list: LuaTable = match handlers.get::<Option<LuaTable>>("despawn")? {
            Some(existing) => existing,
            None => {
                let new_list = lua.create_table()?;
                handlers.set("despawn", new_list.clone())?;
                new_list
            }
        };
        let len = list.len()?;
        list.set(len + 1, handler)?;
        Ok(())
    })?;
    quasar.set("on_despawn", on_despawn_fn)?;

    // Initialize _events table for observer events forwarded each frame
    quasar.set("_events", lua.create_table()?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        lua.globals()
            .set("quasar", lua.create_table().unwrap())
            .unwrap();
        register_bridge(&lua).unwrap();
        lua
    }

    #[test]
    fn bridge_registers_functions() {
        let lua = setup_lua();

        // Test vec3
        let result: LuaTable = lua.load("return quasar.vec3(1, 2, 3)").eval().unwrap();
        assert_eq!(result.get::<f32>("x").unwrap(), 1.0);
        assert_eq!(result.get::<f32>("y").unwrap(), 2.0);
        assert_eq!(result.get::<f32>("z").unwrap(), 3.0);
    }

    #[test]
    fn bridge_lerp_works() {
        let lua = setup_lua();
        let result: f32 = lua.load("return quasar.lerp(0, 10, 0.5)").eval().unwrap();
        assert!((result - 5.0).abs() < 0.001);
    }

    #[test]
    fn bridge_clamp_works() {
        let lua = setup_lua();
        let result: f32 = lua.load("return quasar.clamp(15, 0, 10)").eval().unwrap();
        assert!((result - 10.0).abs() < 0.001);
    }

    #[test]
    fn bridge_get_position_from_transforms() {
        let lua = setup_lua();

        // Simulate plugin writing transform data.
        lua.load(
            r#"
            quasar._transforms[1] = {
                px = 10.0, py = 20.0, pz = 30.0,
                rx = 0.0, ry = 0.0, rz = 0.0, rw = 1.0,
                sx = 1.0, sy = 1.0, sz = 1.0,
            }
            "#,
        )
        .exec()
        .unwrap();

        let pos: LuaTable = lua.load("return quasar.get_position(1)").eval().unwrap();
        assert!((pos.get::<f32>("x").unwrap() - 10.0).abs() < 0.001);
        assert!((pos.get::<f32>("y").unwrap() - 20.0).abs() < 0.001);
        assert!((pos.get::<f32>("z").unwrap() - 30.0).abs() < 0.001);
    }

    #[test]
    fn bridge_set_position_queues_command() {
        let lua = setup_lua();

        lua.load("quasar.set_position(1, 5, 10, 15)")
            .exec()
            .unwrap();

        let commands: LuaTable = lua.load("return quasar._commands").eval().unwrap();
        let cmd: LuaTable = commands.get(1).unwrap();
        assert_eq!(cmd.get::<String>("type").unwrap(), "set_position");
        assert_eq!(cmd.get::<u32>("entity").unwrap(), 1);
        assert!((cmd.get::<f32>("x").unwrap() - 5.0).abs() < 0.001);
    }

    #[test]
    fn bridge_is_key_pressed_works() {
        let lua = setup_lua();

        lua.load(r#"quasar._pressed_keys["KeyW"] = true"#)
            .exec()
            .unwrap();

        let pressed: bool = lua
            .load(r#"return quasar.is_key_pressed("KeyW")"#)
            .eval()
            .unwrap();
        assert!(pressed);

        let not_pressed: bool = lua
            .load(r#"return quasar.is_key_pressed("KeyA")"#)
            .eval()
            .unwrap();
        assert!(!not_pressed);
    }

    #[test]
    fn bridge_spawn_and_despawn_queue_commands() {
        let lua = setup_lua();

        lua.load("quasar.spawn(); quasar.despawn(42)")
            .exec()
            .unwrap();

        let commands: LuaTable = lua.load("return quasar._commands").eval().unwrap();
        let spawn_cmd: LuaTable = commands.get(1).unwrap();
        assert_eq!(spawn_cmd.get::<String>("type").unwrap(), "spawn");

        let despawn_cmd: LuaTable = commands.get(2).unwrap();
        assert_eq!(despawn_cmd.get::<String>("type").unwrap(), "despawn");
        assert_eq!(despawn_cmd.get::<u32>("entity").unwrap(), 42);
    }

    #[test]
    fn bridge_get_input() {
        let lua = setup_lua();

        lua.load(r#"quasar._action_values["jump"] = 1.0"#)
            .exec()
            .unwrap();

        let value: f32 = lua
            .load(r#"return quasar.get_input("jump")"#)
            .eval()
            .unwrap();
        assert!((value - 1.0).abs() < 0.001);

        let missing: f32 = lua
            .load(r#"return quasar.get_input("nonexistent")"#)
            .eval()
            .unwrap();
        assert!((missing - 0.0).abs() < 0.001);
    }

    #[test]
    fn bridge_play_audio_queues_command() {
        let lua = setup_lua();

        lua.load(r#"quasar.play_audio("assets/sounds/jump.ogg")"#)
            .exec()
            .unwrap();

        let commands: LuaTable = lua.load("return quasar._commands").eval().unwrap();
        let cmd: LuaTable = commands.get(1).unwrap();
        assert_eq!(cmd.get::<String>("type").unwrap(), "play_audio");
        assert_eq!(cmd.get::<String>("path").unwrap(), "assets/sounds/jump.ogg");
    }

    #[test]
    fn bridge_apply_force_queues_command() {
        let lua = setup_lua();

        lua.load("quasar.apply_force(1, 10.0, 20.0, 30.0)")
            .exec()
            .unwrap();

        let commands: LuaTable = lua.load("return quasar._commands").eval().unwrap();
        let cmd: LuaTable = commands.get(1).unwrap();
        assert_eq!(cmd.get::<String>("type").unwrap(), "apply_force");
        assert_eq!(cmd.get::<u32>("entity").unwrap(), 1);
        assert!((cmd.get::<f32>("x").unwrap() - 10.0).abs() < 0.001);
        assert!((cmd.get::<f32>("y").unwrap() - 20.0).abs() < 0.001);
        assert!((cmd.get::<f32>("z").unwrap() - 30.0).abs() < 0.001);
    }

    #[test]
    fn bridge_raycast_returns_result() {
        let lua = setup_lua();

        lua.load(
            r#"
            quasar._raycast_results["0.00,0.00,0.00|1.00,0.00,0.00|100.00"] = {
                entity = 42,
                distance = 10.5,
                point = { x = 10.5, y = 0.0, z = 0.0 }
            }
        "#,
        )
        .exec()
        .unwrap();

        let result: LuaTable = lua
            .load(r#"return quasar.raycast(0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 100.0)"#)
            .eval()
            .unwrap();
        assert_eq!(result.get::<u32>("entity").unwrap(), 42);
        assert!((result.get::<f32>("distance").unwrap() - 10.5).abs() < 0.001);
    }

    #[test]
    fn bridge_get_component() {
        let lua = setup_lua();

        lua.load(
            r#"
            quasar._physics_components["1"] = {
                RigidBody = { vx = 1.0, vy = 2.0, vz = 3.0 }
            }
        "#,
        )
        .exec()
        .unwrap();

        let rb: LuaTable = lua
            .load(r#"return quasar.get_component(1, "RigidBody")"#)
            .eval()
            .unwrap();
        assert!((rb.get::<f32>("vx").unwrap() - 1.0).abs() < 0.001);
        assert!((rb.get::<f32>("vy").unwrap() - 2.0).abs() < 0.001);
        assert!((rb.get::<f32>("vz").unwrap() - 3.0).abs() < 0.001);
    }
}
