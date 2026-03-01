//! Lua–ECS bridge — exposes engine APIs to Lua scripts.
//!
//! Registers functions under the `quasar` global table that allow Lua code
//! to interact with the ECS world (spawn entities, set transforms, etc.).

use mlua::prelude::*;

/// Register the ECS bridge functions into the Lua global `quasar` table.
///
/// Available Lua functions after registration:
/// - `quasar.log(msg)` — print an info log
/// - `quasar.dt()` — get the last frame delta time (set each frame by the plugin)
/// - `quasar.get_transform(entity_id)` — returns {x, y, z, rx, ry, rz, rw}
/// - `quasar.set_position(entity_id, x, y, z)` — queued for write-back
pub fn register_bridge(lua: &Lua) -> LuaResult<()> {
    let quasar: LuaTable = lua.globals().get("quasar")?;

    // quasar.log(msg)
    let log_fn = lua.create_function(|_, msg: String| {
        log::info!("[lua] {}", msg);
        Ok(())
    })?;
    quasar.set("log", log_fn)?;

    // quasar.dt() — returns cached delta time (set each frame by ScriptingSystem)
    let dt_fn = lua.create_function(|lua, ()| -> LuaResult<f32> {
        let quasar: LuaTable = lua.globals().get("quasar")?;
        let dt: f32 = quasar.get("_dt").unwrap_or(0.016);
        Ok(dt)
    })?;
    quasar.set("dt", dt_fn)?;

    // quasar.time() — total elapsed time
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
    let lerp_fn = lua.create_function(|_, (a, b, t): (f32, f32, f32)| {
        Ok(a + (b - a) * t)
    })?;
    quasar.set("lerp", lerp_fn)?;

    // quasar.clamp(value, min, max)
    let clamp_fn = lua.create_function(|_, (value, min, max): (f32, f32, f32)| {
        Ok(value.clamp(min, max))
    })?;
    quasar.set("clamp", clamp_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_registers_functions() {
        let lua = Lua::new();
        lua.globals()
            .set("quasar", lua.create_table().unwrap())
            .unwrap();
        register_bridge(&lua).unwrap();

        // Test vec3
        let result: LuaTable = lua
            .load("return quasar.vec3(1, 2, 3)")
            .eval()
            .unwrap();
        assert_eq!(result.get::<f32>("x").unwrap(), 1.0);
        assert_eq!(result.get::<f32>("y").unwrap(), 2.0);
        assert_eq!(result.get::<f32>("z").unwrap(), 3.0);
    }

    #[test]
    fn bridge_lerp_works() {
        let lua = Lua::new();
        lua.globals()
            .set("quasar", lua.create_table().unwrap())
            .unwrap();
        register_bridge(&lua).unwrap();

        let result: f32 = lua.load("return quasar.lerp(0, 10, 0.5)").eval().unwrap();
        assert!((result - 5.0).abs() < 0.001);
    }

    #[test]
    fn bridge_clamp_works() {
        let lua = Lua::new();
        lua.globals()
            .set("quasar", lua.create_table().unwrap())
            .unwrap();
        register_bridge(&lua).unwrap();

        let result: f32 = lua.load("return quasar.clamp(15, 0, 10)").eval().unwrap();
        assert!((result - 10.0).abs() < 0.001);
    }
}
