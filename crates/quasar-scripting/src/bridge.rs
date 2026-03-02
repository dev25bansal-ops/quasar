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
pub fn register_bridge(lua: &Lua) -> LuaResult<()> {
    let quasar: LuaTable = lua.globals().get("quasar")?;

    // Initialise internal tables that the plugin populates each frame.
    quasar.set("_transforms", lua.create_table()?)?;
    quasar.set("_pressed_keys", lua.create_table()?)?;
    quasar.set("_pressed_mouse", lua.create_table()?)?;
    quasar.set("_commands", lua.create_table()?)?;

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
    let set_pos_fn =
        lua.create_function(|lua, (entity_id, x, y, z): (u32, f32, f32, f32)| {
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
    let set_scale_fn =
        lua.create_function(|lua, (entity_id, x, y, z): (u32, f32, f32, f32)| {
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

    // quasar.is_mouse_pressed(button_name) -> bool  ("left", "right", "middle")
    let mouse_fn = lua.create_function(|lua, button_name: String| -> LuaResult<bool> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let buttons: LuaTable = quasar.get("_pressed_mouse")?;
        let pressed: bool = buttons.get(button_name).unwrap_or(false);
        Ok(pressed)
    })?;
    quasar.set("is_mouse_pressed", mouse_fn)?;

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

        let commands: LuaTable = lua
            .load("return quasar._commands")
            .eval()
            .unwrap();
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
}
