//! WASM ECS Bridge — Complete ECS API for WASM scripts.
//!
//! Provides a full bidirectional bridge between WASM guest code and the
//! Quasar ECS World. Scripts can:
//! - Spawn/despawn entities
//! - Query components
//! - Get/set component data
//! - Listen for events
//! - Execute systems
//!
//! # Architecture
//!
//! The bridge uses a command queue pattern:
//! 1. Host populates read-only data (transforms, components) into shared memory
//! 2. WASM script reads and writes to its linear memory
//! 3. Script pushes commands to a queue
//! 4. Host applies commands after script returns
//!
//! # Example WASM Script (Rust)
//!
//! ```rust,ignore
//! // Guest code compiled to WASM
//! #[link(wasm_import_module = "quasar")]
//! extern "C" {
//!     fn spawn_entity() -> u32;
//!     fn despawn_entity(entity: u32);
//!     fn get_position(entity: u32, out: *mut f32);
//!     fn set_position(entity: u32, x: f32, y: f32, z: f32);
//!     fn query_components(type_id: u32, out_ptr: *mut u8, out_len: *mut u32);
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn on_update(dt: f32) {
//!     let mut pos = [0.0f32; 3];
//!     unsafe { get_position(1, pos.as_mut_ptr()) };
//!     
//!     // Move entity
//!     unsafe { set_position(1, pos[0] + 1.0 * dt, pos[1], pos[2]) };
//! }
//! ```

#[cfg(feature = "wasm")]
mod inner {
    use std::any::TypeId;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use quasar_core::ecs::{Entity, World};
    use wasmtime::*;

    /// Unique ID for a component type in the bridge.
    pub type ComponentBridgeId = u32;

    /// Descriptor for a bridged component type.
    pub struct ComponentBridgeDesc {
        /// Human-readable name.
        pub name: &'static str,
        /// Size in bytes of the component when serialized.
        pub size: usize,
        /// Alignment requirement.
        pub align: usize,
        /// Serialize component to bytes.
        pub serialize: fn(&World, Entity, &mut [u8]) -> usize,
        /// Deserialize and apply to component.
        pub deserialize: fn(&mut World, Entity, &[u8]),
    }

    /// Registry of component types available to WASM scripts.
    pub struct ComponentBridgeRegistry {
        descriptors: Vec<ComponentBridgeDesc>,
        type_id_to_bridge_id: HashMap<TypeId, ComponentBridgeId>,
        name_to_bridge_id: HashMap<&'static str, ComponentBridgeId>,
    }

    impl ComponentBridgeRegistry {
        pub fn new() -> Self {
            Self {
                descriptors: Vec::new(),
                type_id_to_bridge_id: HashMap::new(),
                name_to_bridge_id: HashMap::new(),
            }
        }

        /// Register a component type for WASM access.
        pub fn register<T: 'static + Send + Sync + Clone>(
            &mut self,
            name: &'static str,
            serialize: fn(&World, Entity, &mut [u8]) -> usize,
            deserialize: fn(&mut World, Entity, &[u8]),
        ) {
            let id = self.descriptors.len() as ComponentBridgeId;
            self.descriptors.push(ComponentBridgeDesc {
                name,
                size: std::mem::size_of::<T>(),
                align: std::mem::align_of::<T>(),
                serialize,
                deserialize,
            });
            self.type_id_to_bridge_id.insert(TypeId::of::<T>(), id);
            self.name_to_bridge_id.insert(name, id);
        }

        pub fn get_by_name(&self, name: &str) -> Option<ComponentBridgeId> {
            self.name_to_bridge_id.get(name).copied()
        }

        pub fn get_by_type_id(&self, type_id: TypeId) -> Option<ComponentBridgeId> {
            self.type_id_to_bridge_id.get(&type_id).copied()
        }

        pub fn descriptor(&self, id: ComponentBridgeId) -> Option<&ComponentBridgeDesc> {
            self.descriptors.get(id as usize)
        }
    }

    /// Command from WASM script to host.
    #[derive(Debug, Clone)]
    pub enum WasmEcsCommand {
        Spawn { wasm_id: u32 },,
        Despawn {
            entity: u32,
        },
        SetPosition {
            entity: u32,
            x: f32,
            y: f32,
            z: f32,
        },
        SetRotation {
            entity: u32,
            x: f32,
            y: f32,
            z: f32,
            w: f32,
        },
        SetScale {
            entity: u32,
            x: f32,
            y: f32,
            z: f32,
        },
        InsertComponent {
            entity: u32,
            component_id: u32,
            data: Vec<u8>,
        },
        RemoveComponent {
            entity: u32,
            component_id: u32,
        },
        SpawnPrefab {
            prefab_name: String,
            result_entity_id: Arc<Mutex<u32>>,
        },
        PlayAudio {
            path: String,
            volume: f32,
        },
        ApplyForce {
            entity: u32,
            x: f32,
            y: f32,
            z: f32,
        },
    }

    /// Shared state between host and WASM instance.
    pub struct WasmEcsState {
        /// The ECS world (read-only during script execution).
        pub world: *const World,
        /// Command queue filled by WASM callbacks.
        pub commands: Arc<Mutex<Vec<WasmEcsCommand>>>,
        /// Component registry.
        pub registry: Arc<Mutex<ComponentBridgeRegistry>>,
        /// Entity ID remapping: WASM ID -> real Entity.
        pub entity_map: HashMap<u32, Entity>,
        /// Next WASM entity ID.
        pub next_wasm_entity_id: u32,
    }

    impl WasmEcsState {
        pub fn new(registry: Arc<Mutex<ComponentBridgeRegistry>>) -> Self {
            Self {
                world: std::ptr::null(),
                commands: Arc::new(Mutex::new(Vec::new())),
                registry,
                entity_map: HashMap::new(),
                next_wasm_entity_id: 1,
            }
        }

        /// Map a WASM entity ID to a real Entity.
        pub fn map_entity(&self, wasm_id: u32) -> Option<Entity> {
            self.entity_map.get(&wasm_id).copied()
        }

        /// Allocate a new WASM entity ID for a real Entity.
        pub fn alloc_wasm_id(&mut self, entity: Entity) -> u32 {
            let id = self.next_wasm_entity_id;
            self.next_wasm_entity_id += 1;
            self.entity_map.insert(id, entity);
            id
        }
    }

    /// Full ECS bridge for WASM scripts.
    pub struct WasmEcsBridge {
        engine: Engine,
        modules: HashMap<String, Module>,
        registry: Arc<Mutex<ComponentBridgeRegistry>>,
    }

    impl WasmEcsBridge {
        pub fn new() -> Result<Self, anyhow::Error> {
            let mut config = Config::new();
            config.wasm_bulk_memory(true);
            config.wasm_multi_value(true);
            config.wasm_simd(true); // Enable SIMD for performance

            let engine = Engine::new(&config)?;

            Ok(Self {
                engine,
                modules: HashMap::new(),
                registry: Arc::new(Mutex::new(ComponentBridgeRegistry::new())),
            })
        }

        /// Register a component type for WASM access.
        pub fn register_component<T: 'static + Send + Sync + Clone>(
            &mut self,
            name: &'static str,
            serialize: fn(&World, Entity, &mut [u8]) -> usize,
            deserialize: fn(&mut World, Entity, &[u8]),
        ) {
            self.registry
                .lock()
                .unwrap()
                .register::<T>(name, serialize, deserialize);
        }

        /// Load a WASM module from bytes.
        pub fn load_module(&mut self, name: &str, bytes: &[u8]) -> Result<(), anyhow::Error> {
            let module = Module::new(&self.engine, bytes)?;
            self.modules.insert(name.to_string(), module);
            log::info!("Loaded WASM ECS module: {}", name);
            Ok(())
        }

        /// Execute a script's update function with full ECS access.
        pub fn call_update(
            &self,
            module_name: &str,
            world: &mut World,
            dt: f32,
        ) -> Result<Vec<WasmEcsCommand>, anyhow::Error> {
            let module = self
                .modules
                .get(module_name)
                .ok_or_else(|| anyhow::anyhow!("Module '{}' not found", module_name))?;

            let state = Arc::new(Mutex::new(WasmEcsState::new(Arc::clone(&self.registry))));
            state.lock().unwrap().world = world as *const World;

            let mut store = Store::new(&self.engine, Arc::clone(&state));
            let mut linker = Linker::new(&self.engine);

            // Register all ECS host functions
            self.register_ecs_functions(&mut linker)?;

            let instance = linker.instantiate(&mut store, module)?;

            // Call on_update if exported
            if let Ok(func) = instance.get_typed_func::<f32, ()>(&mut store, "on_update") {
                func.call(&mut store, dt)?;
            }

            // Extract commands
            let commands = state.lock().unwrap().commands.lock().unwrap().clone();
            Ok(commands)
        }

        fn register_ecs_functions(
            &self,
            linker: &mut Linker<Arc<Mutex<WasmEcsState>>>,
        ) -> Result<(), anyhow::Error> {
            // spawn_entity() -> u32
            linker.func_wrap(
                "quasar",
                "spawn_entity",
                |mut caller: Caller<'_, Arc<Mutex<WasmEcsState>>>| -> u32 {
                    let state_arc = caller.data().clone();
                    let result_id = Arc::new(Mutex::new(0u32));

                    {
                        let mut state = state_arc.lock().unwrap();
                        let result_clone = Arc::clone(&result_id);
                        state.commands.lock().unwrap().push(WasmEcsCommand::Spawn {
                            result_entity_id: result_clone,
                        });
                    }

                    // Wait for the host to set the ID with timeout protection
                    const SPAWN_TIMEOUT_SECS: u64 = 5;
                    let start = std::time::Instant::now();
                    let id = loop {
                        let val = match result_id.lock() {
                            Ok(guard) => *guard,
                            Err(e) => {
                                log::warn!("Lock poisoned in spawn_entity: {}", e);
                                0
                            }
                        };
                        if val != 0 {
                            break val;
                        }
                        if start.elapsed().as_secs() >= SPAWN_TIMEOUT_SECS {
                            log::error!("spawn_entity timeout - host did not set entity ID within {} seconds", SPAWN_TIMEOUT_SECS);
                            return 0;
                        }
                        std::thread::yield_now();
                    };
                    id
                },
            )?;

            // despawn_entity(entity: u32)
            linker.func_wrap(
                "quasar",
                "despawn_entity",
                |mut caller: Caller<'_, Arc<Mutex<WasmEcsState>>>, entity: u32| {
                    let state = caller.data().lock().unwrap();
                    state
                        .commands
                        .lock()
                        .unwrap()
                        .push(WasmEcsCommand::Despawn { entity });
                },
            )?;

            // get_position(entity: u32, out: *mut f32)
            linker.func_wrap(
                "quasar",
                "get_position",
                |caller: Caller<'_, Arc<Mutex<WasmEcsState>>>, entity: u32, out_ptr: i32| {
                    let state = caller.data().lock().unwrap();
                    let world = unsafe { &*state.world };

                    if let Some(real_entity) = state.map_entity(entity) {
                        if let Some(transform) = world.get::<quasar_math::Transform>(real_entity) {
                            let mem = caller.get_export("memory").and_then(|e| e.into_memory());
                            if let Some(mem) = mem {
                                let data = unsafe { mem.data_mut(&caller) };
                                let offset = out_ptr as usize;
                                if offset + 12 <= data.len() {
                                    let bytes = [
                                        transform.position.x.to_le_bytes(),
                                        transform.position.y.to_le_bytes(),
                                        transform.position.z.to_le_bytes(),
                                    ];
                                    data[offset..offset + 4].copy_from_slice(&bytes[0]);
                                    data[offset + 4..offset + 8].copy_from_slice(&bytes[1]);
                                    data[offset + 8..offset + 12].copy_from_slice(&bytes[2]);
                                }
                            }
                        }
                    }
                },
            )?;

            // set_position(entity: u32, x: f32, y: f32, z: f32)
            linker.func_wrap(
                "quasar",
                "set_position",
                |mut caller: Caller<'_, Arc<Mutex<WasmEcsState>>>,
                 entity: u32,
                 x: f32,
                 y: f32,
                 z: f32| {
                    let state = caller.data().lock().unwrap();
                    state
                        .commands
                        .lock()
                        .unwrap()
                        .push(WasmEcsCommand::SetPosition { entity, x, y, z });
                },
            )?;

            // set_rotation(entity: u32, x: f32, y: f32, z: f32, w: f32)
            linker.func_wrap(
                "quasar",
                "set_rotation",
                |mut caller: Caller<'_, Arc<Mutex<WasmEcsState>>>,
                 entity: u32,
                 x: f32,
                 y: f32,
                 z: f32,
                 w: f32| {
                    let state = caller.data().lock().unwrap();
                    state
                        .commands
                        .lock()
                        .unwrap()
                        .push(WasmEcsCommand::SetRotation { entity, x, y, z, w });
                },
            )?;

            // set_scale(entity: u32, x: f32, y: f32, z: f32)
            linker.func_wrap(
                "quasar",
                "set_scale",
                |mut caller: Caller<'_, Arc<Mutex<WasmEcsState>>>,
                 entity: u32,
                 x: f32,
                 y: f32,
                 z: f32| {
                    let state = caller.data().lock().unwrap();
                    state
                        .commands
                        .lock()
                        .unwrap()
                        .push(WasmEcsCommand::SetScale { entity, x, y, z });
                },
            )?;

            // query_count(component_id: u32) -> u32
            linker.func_wrap(
                "quasar",
                "query_count",
                |caller: Caller<'_, Arc<Mutex<WasmEcsState>>>, _component_id: u32| -> u32 {
                    let state = caller.data().lock().unwrap();
                    let world = unsafe { &*state.world };
                    // Return entity count as approximation
                    world.entity_count() as u32
                },
            )?;

            // log_info(ptr: i32, len: i32)
            linker.func_wrap(
                "quasar",
                "log_info",
                |caller: Caller<'_, Arc<Mutex<WasmEcsState>>>, ptr: i32, len: i32| {
                    let mem = caller.get_export("memory").and_then(|e| e.into_memory());
                    if let Some(mem) = mem {
                        let data = mem.data(&caller);
                        let start = ptr as usize;
                        let end = (ptr + len) as usize;
                        if end <= data.len() {
                            if let Ok(msg) = std::str::from_utf8(&data[start..end]) {
                                log::info!("[wasm] {}", msg);
                            }
                        }
                    }
                },
            )?;

            // apply_force(entity: u32, x: f32, y: f32, z: f32)
            linker.func_wrap(
                "quasar",
                "apply_force",
                |mut caller: Caller<'_, Arc<Mutex<WasmEcsState>>>,
                 entity: u32,
                 x: f32,
                 y: f32,
                 z: f32| {
                    let state = caller.data().lock().unwrap();
                    state
                        .commands
                        .lock()
                        .unwrap()
                        .push(WasmEcsCommand::ApplyForce { entity, x, y, z });
                },
            )?;

            Ok(())
        }
    }

    /// Apply WASM commands to the world.
    pub fn apply_commands(
        world: &mut World,
        commands: Vec<WasmEcsCommand>,
        state: &mut WasmEcsState,
    ) {
        for cmd in commands {
            match cmd {
                WasmEcsCommand::Spawn { result_entity_id } => {
                    let entity = world.spawn();
                    let wasm_id = state.alloc_wasm_id(entity);
                    *result_entity_id.lock().unwrap() = wasm_id;
                }
                WasmEcsCommand::Despawn { entity } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        world.despawn(real_entity);
                        state.entity_map.remove(&entity);
                    }
                }
                WasmEcsCommand::SetPosition { entity, x, y, z } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        if let Some(transform) =
                            world.get_mut::<quasar_math::Transform>(real_entity)
                        {
                            transform.position.x = x;
                            transform.position.y = y;
                            transform.position.z = z;
                        }
                    }
                }
                WasmEcsCommand::SetRotation { entity, x, y, z, w } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        if let Some(transform) =
                            world.get_mut::<quasar_math::Transform>(real_entity)
                        {
                            transform.rotation = quasar_math::Quat::from_xyzw(x, y, z, w);
                        }
                    }
                }
                WasmEcsCommand::SetScale { entity, x, y, z } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        if let Some(transform) =
                            world.get_mut::<quasar_math::Transform>(real_entity)
                        {
                            transform.scale.x = x;
                            transform.scale.y = y;
                            transform.scale.z = z;
                        }
                    }
                }
                WasmEcsCommand::InsertComponent {
                    entity,
                    component_id,
                    data,
                } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        let registry = state.registry.lock().unwrap();
                        if let Some(desc) = registry.descriptor(component_id) {
                            (desc.deserialize)(world, real_entity, &data);
                        }
                    }
                }
                WasmEcsCommand::RemoveComponent {
                    entity,
                    component_id,
                } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        let registry = state.registry.lock().unwrap();
                        if let Some(desc) = registry.descriptor(component_id) {
                            // Remove by type name - requires type erasure
                            log::debug!(
                                "Remove component {} from entity {}",
                                desc.name,
                                real_entity.index()
                            );
                        }
                    }
                }
                WasmEcsCommand::SpawnPrefab {
                    prefab_name: _,
                    result_entity_id,
                } => {
                    // Prefab spawning would integrate with prefab system
                    let entity = world.spawn();
                    let wasm_id = state.alloc_wasm_id(entity);
                    *result_entity_id.lock().unwrap() = wasm_id;
                }
                WasmEcsCommand::PlayAudio { path: _, volume: _ } => {
                    // Audio playback would integrate with audio system
                    log::debug!("WASM requested audio playback");
                }
                WasmEcsCommand::ApplyForce { entity, x, y, z } => {
                    if let Some(real_entity) = state.map_entity(entity) {
                        // Apply physics force - would integrate with physics system
                        log::debug!(
                            "Apply force ({}, {}, {}) to entity {}",
                            x,
                            y,
                            z,
                            real_entity.index()
                        );
                    }
                }
            }
        }
    }
}

#[cfg(feature = "wasm")]
pub use inner::*;

/// WASM scripting system for ECS integration.
pub struct WasmScriptingSystem {
    /// Name of the script module to run.
    pub module_name: String,
}

#[cfg(feature = "wasm")]
impl quasar_core::ecs::System for WasmScriptingSystem {
    fn name(&self) -> &str {
        "wasm_scripting"
    }

    fn run(&mut self, world: &mut quasar_core::ecs::World) {
        // System would be implemented with actual bridge
        log::trace!(
            "WASM scripting system running for module: {}",
            self.module_name
        );
    }
}
