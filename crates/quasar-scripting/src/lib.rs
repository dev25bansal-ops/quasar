//! # Quasar Scripting
//!
//! Lua scripting integration via [`mlua`].
//!
//! Allows game logic to be authored in Lua/Luau scripts with access to the ECS,
//! input state, and engine utilities. Supports hot-reloading of scripts.

pub mod bridge;
pub mod plugin;

use mlua::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

/// The scripting engine — manages a Lua VM and script execution.
pub struct ScriptEngine {
    lua: Lua,
    /// Track file modification times for hot-reload.
    file_timestamps: HashMap<String, SystemTime>,
}

// SAFETY: Our ECS is single-threaded. The Lua VM is only accessed from
// the main thread via the ScriptingSystem. These impls let us store
// ScriptEngine inside the ECS component storage.
unsafe impl Send for ScriptEngine {}
unsafe impl Sync for ScriptEngine {}

impl ScriptEngine {
    /// Create a new scripting engine with standard libraries loaded.
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();

        // Register the engine's Lua API namespace.
        let quasar = lua.create_table()?;
        quasar.set("version", env!("CARGO_PKG_VERSION"))?;
        lua.globals().set("quasar", quasar)?;

        // Register a basic logging API.
        let log_info = lua.create_function(|_, msg: String| {
            log::info!("[lua] {}", msg);
            Ok(())
        })?;
        let log_warn = lua.create_function(|_, msg: String| {
            log::warn!("[lua] {}", msg);
            Ok(())
        })?;
        let log_error = lua.create_function(|_, msg: String| {
            log::error!("[lua] {}", msg);
            Ok(())
        })?;

        let log_table = lua.create_table()?;
        log_table.set("info", log_info)?;
        log_table.set("warn", log_warn)?;
        log_table.set("error", log_error)?;
        lua.globals().set("log", log_table)?;

        log::info!("Lua scripting engine initialized");

        Ok(Self {
            lua,
            file_timestamps: HashMap::new(),
        })
    }

    /// Execute a Lua script string.
    pub fn exec(&self, script: &str) -> LuaResult<()> {
        self.lua.load(script).exec()
    }

    /// Execute a Lua script and return the result as a value.
    pub fn eval<T: FromLua>(&self, script: &str) -> LuaResult<T> {
        self.lua.load(script).eval()
    }

    /// Execute a Lua script file.
    pub fn exec_file<P: AsRef<Path>>(&mut self, path: P) -> LuaResult<()> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let source = std::fs::read_to_string(&path_str)
            .map_err(|e| mlua::Error::ExternalError(std::sync::Arc::new(e)))?;

        // Track modification time for hot-reload.
        if let Ok(metadata) = std::fs::metadata(&path_str) {
            if let Ok(modified) = metadata.modified() {
                self.file_timestamps.insert(path_str, modified);
            }
        }

        self.exec(&source)
    }

    /// Check if any tracked script files have been modified since last load.
    pub fn check_hot_reload(&self) -> Vec<String> {
        let mut changed = Vec::new();
        for (path, &last_modified) in &self.file_timestamps {
            if let Ok(metadata) = std::fs::metadata(path) {
                if let Ok(modified) = metadata.modified() {
                    if modified > last_modified {
                        changed.push(path.clone());
                    }
                }
            }
        }
        changed
    }

    /// Reload all scripts that have changed on disk.
    pub fn hot_reload(&mut self) -> Vec<String> {
        let changed = self.check_hot_reload();
        let mut reloaded = Vec::new();
        for path in &changed {
            match self.exec_file(path.as_str()) {
                Ok(()) => {
                    log::info!("Hot-reloaded script: {}", path);
                    reloaded.push(path.clone());
                }
                Err(e) => {
                    log::error!("Failed to hot-reload {}: {}", path, e);
                }
            }
        }
        reloaded
    }

    /// Set a global Lua variable.
    pub fn set_global<T: IntoLua>(&self, name: &str, value: T) -> LuaResult<()> {
        self.lua.globals().set(name, value)
    }

    /// Get a global Lua variable.
    pub fn get_global<T: FromLua>(&self, name: &str) -> LuaResult<T> {
        self.lua.globals().get(name)
    }

    /// Call a global Lua function by name.
    pub fn call_function<A, R>(&self, name: &str, args: A) -> LuaResult<R>
    where
        A: IntoLuaMulti,
        R: FromLuaMulti,
    {
        let func: LuaFunction = self.lua.globals().get(name)?;
        func.call(args)
    }

    /// Register a Rust function as a global Lua function.
    pub fn register_function<F, A, R>(&self, name: &str, func: F) -> LuaResult<()>
    where
        F: Fn(&Lua, A) -> LuaResult<R> + Send + 'static,
        A: FromLuaMulti,
        R: IntoLuaMulti,
    {
        let lua_func = self.lua.create_function(func)?;
        self.lua.globals().set(name, lua_func)
    }

    /// Get a reference to the underlying Lua state.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}

/// ECS component that attaches a Lua script to an entity.
#[derive(Debug, Clone)]
pub struct ScriptComponent {
    /// Path to the Lua script file.
    pub path: String,
    /// Whether the script has been loaded initially.
    pub loaded: bool,
}

impl ScriptComponent {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            loaded: false,
        }
    }
}

pub use plugin::{ScriptingPlugin, ScriptingResource};
