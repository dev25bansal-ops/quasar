//! # Quasar Scripting
//!
//! Lua scripting integration via [`mlua`].
//!
//! Allows game logic to be authored in Lua/Luau scripts with access to the ECS,
//! input state, and engine utilities. Supports hot-reloading of scripts.
//!
//! ## Security
//!
//! By default, the scripting engine runs in sandboxed mode with restricted
//! standard libraries. Use `ScriptCapabilities::full()` with caution for
//! trusted scripts only.

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod bridge;
pub mod component_registry;
pub mod plugin;
pub mod wasm_scripting;
use crossbeam_channel::{unbounded, Receiver};
use mlua::prelude::*;
use mlua::LuaOptions;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Capabilities for sandboxing Lua scripts.
/// Controls what operations scripts are allowed to perform.
#[derive(Debug, Clone, Default)]
pub struct ScriptCapabilities {
    pub can_spawn_entities: bool,
    pub can_despawn_entities: bool,
    pub can_access_files: bool,
    pub can_apply_physics: bool,
    pub can_play_audio: bool,
    pub can_add_components: bool,
    pub can_remove_components: bool,
    pub sandbox_mode: bool,
}

impl ScriptCapabilities {
    pub fn full() -> Self {
        Self {
            can_spawn_entities: true,
            can_despawn_entities: true,
            can_access_files: true,
            can_apply_physics: true,
            can_play_audio: true,
            can_add_components: true,
            can_remove_components: true,
            sandbox_mode: false,
        }
    }

    pub fn restricted() -> Self {
        Self {
            can_spawn_entities: false,
            can_despawn_entities: false,
            can_access_files: false,
            can_apply_physics: false,
            can_play_audio: true,
            can_add_components: true,
            can_remove_components: false,
            sandbox_mode: true,
        }
    }

    pub fn readonly() -> Self {
        Self {
            can_spawn_entities: false,
            can_despawn_entities: false,
            can_access_files: false,
            can_apply_physics: false,
            can_play_audio: false,
            can_add_components: false,
            can_remove_components: false,
            sandbox_mode: true,
        }
    }
}

/// The scripting engine — manages a Lua VM and script execution.
///
/// Thread-safe wrapper around the Lua VM using internal mutex synchronization.
/// All Lua operations are serialized to prevent concurrent access.
pub struct ScriptEngine {
    lua: Mutex<Lua>,
    event_rx: Receiver<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    watched_files: Mutex<Vec<String>>,
    pub capabilities: ScriptCapabilities,
}

impl ScriptEngine {
    /// Create a new scripting engine with sandboxed standard libraries.
    ///
    /// The sandbox mode:
    /// - Disables `os` library (no system commands)
    /// - Disables `io` library (no file access)
    /// - Disables `debug` library (no introspection)
    /// - Disables `package` library (no module loading)
    pub fn new() -> LuaResult<Self> {
        Self::with_capabilities(ScriptCapabilities::restricted())
    }

    /// Create a scripting engine with custom capabilities.
    pub fn with_capabilities(capabilities: ScriptCapabilities) -> LuaResult<Self> {
        let lua = if capabilities.sandbox_mode {
            let safe_libs = mlua::StdLib::COROUTINE
                | mlua::StdLib::TABLE
                | mlua::StdLib::STRING
                | mlua::StdLib::UTF8
                | mlua::StdLib::MATH;
            Lua::new_with(safe_libs, LuaOptions::default()).map_err(|e| {
                mlua::Error::runtime(format!("Failed to create sandboxed Lua: {}", e))
            })?
        } else {
            Lua::new()
        };

        let quasar = lua.create_table()?;
        quasar.set("version", env!("CARGO_PKG_VERSION"))?;
        lua.globals().set("quasar", quasar)?;

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

        let (event_tx, event_rx) = unbounded();
        let watcher: RecommendedWatcher =
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                        if let Some(path) = event.paths.first() {
                            let _ = event_tx.send(path.clone());
                        }
                    }
                }
            })
            .map_err(|e| {
                log::error!("Failed to create file watcher: {}", e);
                mlua::Error::ExternalError(std::sync::Arc::new(e))
            })?;

        log::info!(
            "Lua scripting engine initialized (sandboxed: {})",
            capabilities.sandbox_mode
        );

        Ok(Self {
            lua: Mutex::new(lua),
            event_rx,
            watcher: Some(watcher),
            watched_files: Mutex::new(Vec::new()),
            capabilities,
        })
    }

    /// Create a minimal fallback engine without file watching.
    pub fn new_fallback() -> Self {
        let safe_libs = mlua::StdLib::COROUTINE
            | mlua::StdLib::TABLE
            | mlua::StdLib::STRING
            | mlua::StdLib::UTF8
            | mlua::StdLib::MATH;
        let lua = Lua::new_with(safe_libs, LuaOptions::default()).unwrap_or_else(|_| Lua::new());
        let (_, event_rx) = unbounded();

        let quasar = lua.create_table().ok();
        if let Some(q) = quasar {
            let _ = q.set("version", env!("CARGO_PKG_VERSION"));
            let _ = lua.globals().set("quasar", q);
        }

        log::warn!("Using fallback Lua engine without file watching");

        Self {
            lua: Mutex::new(lua),
            event_rx,
            watcher: None,
            watched_files: Mutex::new(Vec::new()),
            capabilities: ScriptCapabilities::restricted(),
        }
    }

    /// Set the capabilities for this scripting engine.
    pub fn set_capabilities(&mut self, caps: ScriptCapabilities) {
        self.capabilities = caps;
    }

    /// Check if a capability is enabled.
    pub fn can(&self, check: impl Fn(&ScriptCapabilities) -> bool) -> bool {
        check(&self.capabilities)
    }

    /// Execute a Lua script string.
    pub fn exec(&self, script: &str) -> LuaResult<()> {
        let lua = self
            .lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
        lua.load(script).exec()
    }

    /// Execute a Lua script and return the result as a value.
    pub fn eval<T: FromLua>(&self, script: &str) -> LuaResult<T> {
        let lua = self
            .lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
        lua.load(script).eval()
    }

    /// Execute a Lua script file.
    pub fn exec_file<P: AsRef<Path>>(&mut self, path: P) -> LuaResult<()> {
        if !self.capabilities.can_access_files {
            return Err(mlua::Error::runtime("File access denied by sandbox"));
        }

        let path_str = path.as_ref().to_string_lossy().to_string();
        let source = std::fs::read_to_string(&path_str)
            .map_err(|e| mlua::Error::ExternalError(std::sync::Arc::new(e)))?;

        {
            let mut watched = self
                .watched_files
                .lock()
                .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
            if !watched.contains(&path_str) {
                if let Some(ref mut watcher) = self.watcher {
                    let _ = watcher.watch(path.as_ref(), RecursiveMode::NonRecursive);
                }
                watched.push(path_str.clone());
            }
        }

        self.exec(&source)
    }

    /// Check if any tracked script files have been modified since last load.
    pub fn check_hot_reload(&self) -> Vec<String> {
        let mut changed = Vec::new();
        while let Ok(path) = self.event_rx.try_recv() {
            let path_str = path.to_string_lossy().to_string();
            let watched = self.watched_files.lock().ok();
            if let Some(w) = watched {
                if w.contains(&path_str) {
                    changed.push(path_str);
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
        let lua = self
            .lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
        lua.globals().set(name, value)
    }

    /// Get a global Lua variable.
    pub fn get_global<T: FromLua>(&self, name: &str) -> LuaResult<T> {
        let lua = self
            .lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
        lua.globals().get(name)
    }

    /// Call a global Lua function by name.
    pub fn call_function<A, R>(&self, name: &str, args: A) -> LuaResult<R>
    where
        A: IntoLuaMulti,
        R: FromLuaMulti,
    {
        let lua = self
            .lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
        let func: LuaFunction = lua.globals().get(name)?;
        func.call(args)
    }

    /// Register a Rust function as a global Lua function.
    pub fn register_function<F, A, R>(&self, name: &str, func: F) -> LuaResult<()>
    where
        F: Fn(&Lua, A) -> LuaResult<R> + Send + 'static,
        A: FromLuaMulti,
        R: IntoLuaMulti,
    {
        let lua = self
            .lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))?;
        let lua_func = lua.create_function(func)?;
        lua.globals().set(name, lua_func)
    }

    /// Get a reference to the underlying Lua state.
    ///
    /// # Warning
    /// This returns a MutexGuard. Hold it for as short as possible.
    pub fn lua(&self) -> Result<std::sync::MutexGuard<'_, Lua>, mlua::Error> {
        self.lua
            .lock()
            .map_err(|_| mlua::Error::runtime("Lock poisoned"))
    }

    /// Watch an entire directory for script changes (recursive).
    pub fn watch_directory<P: AsRef<Path>>(&mut self, dir: P) {
        if let Some(ref mut watcher) = self.watcher {
            let _ = watcher.watch(dir.as_ref(), RecursiveMode::Recursive);
            log::info!("Watching directory for script changes: {:?}", dir.as_ref());
        }
    }

    /// Reload all watched scripts, preserving Lua global state on error.
    pub fn hot_reload_safe(&mut self) -> Vec<(String, Result<(), String>)> {
        let changed = self.check_hot_reload();
        let mut results = Vec::new();
        for path in &changed {
            match std::fs::read_to_string(path) {
                Ok(source) => match self.lua.lock() {
                    Ok(lua) => match lua.load(&source).set_name(path).exec() {
                        Ok(()) => {
                            log::info!("Hot-reloaded script: {}", path);
                            results.push((path.clone(), Ok(())));
                        }
                        Err(e) => {
                            let msg = format!("{}", e);
                            log::error!("Hot-reload syntax/runtime error in {}: {}", path, msg);
                            results.push((path.clone(), Err(msg)));
                        }
                    },
                    Err(_) => {
                        results.push((path.clone(), Err("Lock poisoned".into())));
                    }
                },
                Err(e) => {
                    let msg = format!("IO error: {}", e);
                    log::error!("Hot-reload failed to read {}: {}", path, msg);
                    results.push((path.clone(), Err(msg)));
                }
            }
        }
        results
    }

    /// Get the list of currently watched script file paths.
    pub fn watched_files(&self) -> Vec<String> {
        self.watched_files
            .lock()
            .map(|w| w.clone())
            .unwrap_or_default()
    }

    /// Snapshot all global variables as a serializable table of strings.
    pub fn snapshot_globals(&self) -> Vec<(String, String)> {
        let mut snapshot = Vec::new();
        if let Ok(lua) = self.lua.lock() {
            if let Ok(globals) = lua
                .globals()
                .pairs::<String, LuaValue>()
                .collect::<Result<Vec<_>, _>>()
            {
                for (key, value) in globals {
                    match &value {
                        LuaValue::Number(n) => snapshot.push((key, n.to_string())),
                        LuaValue::Integer(i) => snapshot.push((key, i.to_string())),
                        LuaValue::String(s) => {
                            if let Ok(s) = s.to_str() {
                                snapshot.push((key, format!("\"{}\"", s)));
                            }
                        }
                        LuaValue::Boolean(b) => snapshot.push((key, b.to_string())),
                        _ => {}
                    }
                }
            }
        }
        snapshot
    }

    /// Restore globals from a snapshot produced by `snapshot_globals`.
    pub fn restore_globals(&self, snapshot: &[(String, String)]) {
        if let Ok(lua) = self.lua.lock() {
            for (key, value_str) in snapshot {
                if key == "quasar" || key == "log" || key == "_G" || key == "_VERSION" {
                    continue;
                }
                let script = format!("{} = {}", key, value_str);
                let _ = lua.load(&script).exec();
            }
        }
    }

    /// Hot-reload with state preservation.
    pub fn hot_reload_with_state(&mut self) -> Vec<(String, Result<(), String>)> {
        let snapshot = self.snapshot_globals();
        let results = self.hot_reload_safe();
        if results.iter().any(|(_, r)| r.is_ok()) {
            self.restore_globals(&snapshot);
        }
        results
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

pub use component_registry::{ComponentDescriptor, ComponentRegistry};
pub use plugin::{ScriptingPlugin, ScriptingResource};
pub use wasm_scripting::ScriptingBridge;
#[cfg(feature = "wasm")]
pub use wasm_scripting::{WasmHostApi, WasmScriptEngine};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_capabilities_full() {
        let caps = ScriptCapabilities::full();
        assert!(caps.can_spawn_entities);
        assert!(caps.can_despawn_entities);
        assert!(caps.can_access_files);
        assert!(!caps.sandbox_mode);
    }

    #[test]
    fn test_script_capabilities_restricted() {
        let caps = ScriptCapabilities::restricted();
        assert!(!caps.can_spawn_entities);
        assert!(!caps.can_access_files);
        assert!(caps.sandbox_mode);
    }

    #[test]
    fn test_script_capabilities_readonly() {
        let caps = ScriptCapabilities::readonly();
        assert!(!caps.can_spawn_entities);
        assert!(!caps.can_despawn_entities);
        assert!(!caps.can_access_files);
        assert!(!caps.can_play_audio);
        assert!(caps.sandbox_mode);
    }

    #[test]
    fn test_script_engine_new_creates_sandboxed_engine() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        assert!(engine.capabilities.sandbox_mode);
    }

    #[test]
    fn test_script_engine_with_capabilities_full() {
        let caps = ScriptCapabilities::full();
        let engine = ScriptEngine::with_capabilities(caps).expect("Failed to create engine");
        assert!(!engine.capabilities.sandbox_mode);
    }

    #[test]
    fn test_script_engine_exec_simple() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        let result = engine.exec("local x = 1 + 1");
        assert!(result.is_ok());
    }

    #[test]
    fn test_script_engine_eval() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        let result: i32 = engine.eval("return 2 + 2").expect("Eval failed");
        assert_eq!(result, 4);
    }

    #[test]
    fn test_script_engine_set_get_global() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        engine
            .set_global("test_var", 42)
            .expect("set_global failed");
        let value: i32 = engine.get_global("test_var").expect("get_global failed");
        assert_eq!(value, 42);
    }

    #[test]
    fn test_script_engine_register_function() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        engine
            .register_function("add_one", |_, x: i32| Ok(x + 1))
            .expect("register_function failed");
        let result: i32 = engine.eval("return add_one(5)").expect("eval failed");
        assert_eq!(result, 6);
    }

    #[test]
    fn test_script_engine_can_check_capability() {
        let mut engine = ScriptEngine::new().expect("Failed to create engine");
        engine.capabilities.can_spawn_entities = false;

        assert!(!engine.can(|c| c.can_spawn_entities));
        assert!(engine.can(|c| !c.can_spawn_entities));
    }

    #[test]
    fn test_script_engine_sandbox_blocks_os_library() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        engine.exec("test_os = os == nil").expect("exec failed");
        let test_os: bool = engine.get_global("test_os").expect("get_global failed");
        assert!(test_os, "os library should be nil in sandboxed mode");
    }

    #[test]
    fn test_script_engine_sandbox_blocks_io_library() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        engine.exec("test_io = io == nil").expect("exec failed");
        let test_io: bool = engine.get_global("test_io").expect("get_global failed");
        assert!(test_io, "io library should be nil in sandboxed mode");
    }

    #[test]
    fn test_script_engine_call_function() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        engine
            .exec("function multiply(a, b) return a * b end")
            .expect("exec failed");
        let result: i32 = engine
            .call_function("multiply", (3, 4))
            .expect("call_function failed");
        assert_eq!(result, 12);
    }

    #[test]
    fn test_script_engine_snapshot_globals() {
        let engine = ScriptEngine::new().expect("Failed to create engine");
        engine.set_global("my_num", 123).expect("set_global failed");
        engine
            .set_global("my_str", "hello")
            .expect("set_global failed");

        let snapshot = engine.snapshot_globals();
        assert!(snapshot.iter().any(|(k, _)| k == "my_num"));
        assert!(snapshot.iter().any(|(k, _)| k == "my_str"));
    }

    #[test]
    fn test_script_component_new() {
        let comp = ScriptComponent::new("scripts/player.lua");
        assert_eq!(comp.path, "scripts/player.lua");
        assert!(!comp.loaded);
    }

    #[test]
    fn test_engine_fallback_creates_restricted_engine() {
        let engine = ScriptEngine::new_fallback();
        assert!(engine.capabilities.sandbox_mode);
        assert!(!engine.capabilities.can_spawn_entities);
    }

    #[test]
    fn test_set_capabilities() {
        let mut engine = ScriptEngine::new().expect("Failed to create engine");
        let caps = ScriptCapabilities::full();
        engine.set_capabilities(caps);
        assert!(!engine.capabilities.sandbox_mode);
    }
}
