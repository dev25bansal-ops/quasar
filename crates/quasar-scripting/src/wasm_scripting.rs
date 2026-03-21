//! WASM scripting backend via Wasmtime.
//!
//! Provides a `WasmScriptEngine` that can load and execute WASM modules as
//! game scripts. Implements the same `ScriptingBridge` trait as the Lua backend
//! for unified scripting across both runtimes.

#[cfg(feature = "wasm")]
mod inner {
    use std::path::Path;

    use wasmtime::*;

    /// Host API functions exposed to WASM guest modules.
    ///
    /// Each method corresponds to an imported function the WASM module can call.
    pub trait WasmHostApi: Send + Sync + 'static {
        fn get_transform(&self, entity_id: u32) -> [f32; 10]; // px,py,pz, rx,ry,rz,rw, sx,sy,sz
        fn set_transform(&mut self, entity_id: u32, data: &[f32; 10]);
        fn spawn_entity(&mut self) -> u32;
        fn despawn_entity(&mut self, entity_id: u32);
        fn get_component_f32(&self, entity_id: u32, component_name: &str, field_name: &str) -> f32;
        fn set_component_f32(
            &mut self,
            entity_id: u32,
            component_name: &str,
            field_name: &str,
            value: f32,
        );
        fn log_message(&self, level: u32, message: &str);
    }

    /// State stored per-WASM instance for host callbacks.
    struct HostState {
        api: Box<dyn WasmHostApi>,
    }

    /// A WASM script engine backed by Wasmtime.
    pub struct WasmScriptEngine {
        engine: Engine,
        modules: Vec<(String, Module)>,
    }

    impl WasmScriptEngine {
        pub fn new() -> Result<Self, anyhow::Error> {
            let mut config = Config::new();
            config.wasm_bulk_memory(true);
            config.wasm_multi_value(true);
            let engine = Engine::new(&config)?;
            Ok(Self {
                engine,
                modules: Vec::new(),
            })
        }

        /// Compile and register a WASM module from bytes.
        pub fn load_module(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<(), anyhow::Error> {
            let module = Module::new(&self.engine, wasm_bytes)?;
            self.modules.push((name.to_string(), module));
            log::info!("Loaded WASM module: {}", name);
            Ok(())
        }

        /// Compile and register a WASM module from a file.
        pub fn load_module_file(&mut self, name: &str, path: &Path) -> Result<(), anyhow::Error> {
            let wasm_bytes = std::fs::read(path)?;
            self.load_module(name, &wasm_bytes)
        }

        /// Instantiate a loaded module with host API bindings and call its `on_update` export.
        pub fn call_update(
            &self,
            module_name: &str,
            api: Box<dyn WasmHostApi>,
            dt: f32,
        ) -> Result<(), anyhow::Error> {
            let module = self
                .modules
                .iter()
                .find(|(n, _)| n == module_name)
                .map(|(_, m)| m)
                .ok_or_else(|| anyhow::anyhow!("WASM module '{}' not found", module_name))?;

            let mut store = Store::new(&self.engine, HostState { api });
            let mut linker = Linker::new(&self.engine);

            // Register host functions.
            linker.func_wrap(
                "env",
                "log_info",
                |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
                    let mem = caller.get_export("memory").and_then(|e| e.into_memory());
                    if let Some(mem) = mem {
                        let data = mem.data(&caller);
                        if let Some(slice) = data.get(ptr as usize..(ptr as usize + len as usize)) {
                            if let Ok(msg) = std::str::from_utf8(slice) {
                                caller.data().api.log_message(0, msg);
                            }
                        }
                    }
                },
            )?;

            linker.func_wrap(
                "env",
                "spawn_entity",
                |mut caller: Caller<'_, HostState>| -> i32 {
                    caller.data_mut().api.spawn_entity() as i32
                },
            )?;

            linker.func_wrap(
                "env",
                "despawn_entity",
                |mut caller: Caller<'_, HostState>, entity_id: i32| {
                    caller.data_mut().api.despawn_entity(entity_id as u32);
                },
            )?;

            linker.func_wrap(
                "env",
                "get_transform_x",
                |caller: Caller<'_, HostState>, entity_id: i32| -> f32 {
                    caller.data().api.get_transform(entity_id as u32)[0]
                },
            )?;

            linker.func_wrap(
                "env",
                "get_transform_y",
                |caller: Caller<'_, HostState>, entity_id: i32| -> f32 {
                    caller.data().api.get_transform(entity_id as u32)[1]
                },
            )?;

            linker.func_wrap(
                "env",
                "get_transform_z",
                |caller: Caller<'_, HostState>, entity_id: i32| -> f32 {
                    caller.data().api.get_transform(entity_id as u32)[2]
                },
            )?;

            linker.func_wrap(
                "env",
                "set_position",
                |mut caller: Caller<'_, HostState>, entity_id: i32, x: f32, y: f32, z: f32| {
                    let mut data = caller.data().api.get_transform(entity_id as u32);
                    data[0] = x;
                    data[1] = y;
                    data[2] = z;
                    caller.data_mut().api.set_transform(entity_id as u32, &data);
                },
            )?;

            let instance = linker.instantiate(&mut store, module)?;

            // Call the on_update export if it exists.
            if let Ok(func) = instance.get_typed_func::<f32, ()>(&mut store, "on_update") {
                func.call(&mut store, dt)?;
            }

            Ok(())
        }

        /// Get the list of loaded module names.
        pub fn module_names(&self) -> Vec<&str> {
            self.modules.iter().map(|(n, _)| n.as_str()).collect()
        }
    }
}

#[cfg(feature = "wasm")]
pub use inner::*;

// ── Unified ScriptingBridge trait ────────────────────────────────

/// Unified scripting bridge interface for both Lua and WASM backends.
///
/// Allows the engine to treat Lua and WASM scripts interchangeably
/// for common operations.
pub trait ScriptingBridge: Send + Sync {
    /// Execute a named function (e.g. `on_update`) in the script with a delta time.
    fn call_update(&mut self, dt: f32) -> Result<(), String>;

    /// Execute a named function with custom arguments serialized as JSON.
    fn call_function(&mut self, name: &str, args_json: &str) -> Result<String, String>;

    /// Reload the script from its source.
    fn reload(&mut self) -> Result<(), String>;

    /// Return the backend name (e.g. "lua" or "wasm").
    fn backend_name(&self) -> &'static str;
}
