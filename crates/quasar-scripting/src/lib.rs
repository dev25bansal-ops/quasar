//! # Quasar Scripting
//!
//! Lua scripting integration via [`mlua`].
//!
//! Allows game logic to be authored in Lua scripts, hot-reloaded at runtime.
//!
//! **Status**: Scaffolded — full implementation coming in Week 3.

use mlua::prelude::*;

/// The scripting engine — manages a Lua VM and script execution.
pub struct ScriptEngine {
    lua: Lua,
}

impl ScriptEngine {
    /// Create a new scripting engine with standard libraries loaded.
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();

        // Register the engine's Lua API namespace.
        lua.globals().set("quasar", lua.create_table()?)?;

        log::info!("Lua scripting engine initialized (Lua 5.4)");

        Ok(Self { lua })
    }

    /// Execute a Lua script string.
    pub fn exec(&self, script: &str) -> LuaResult<()> {
        self.lua.load(script).exec()
    }

    /// Execute a Lua script file.
    pub fn exec_file(&self, path: &str) -> LuaResult<()> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| mlua::Error::ExternalError(std::sync::Arc::new(e)))?;
        self.exec(&source)
    }

    /// Get a reference to the underlying Lua state.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}
