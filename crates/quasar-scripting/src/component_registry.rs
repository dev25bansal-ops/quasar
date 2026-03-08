//! Component Registry — maps string component names to Rust component types
//! for Lua ↔ ECS bridging.
//!
//! Each registered component provides:
//! - A serializer that reads the component from the ECS and writes it as a Lua table.
//! - A deserializer that reads a Lua table and inserts the component into the ECS.
//! - A remover that removes the component from an entity.

use mlua::prelude::*;
use quasar_core::ecs::{Entity, World};
use std::collections::HashMap;

/// A trait-object–friendly descriptor for one component type.
pub struct ComponentDescriptor {
    /// Human-readable name (e.g. `"Transform"`, `"RigidBody"`).
    pub name: &'static str,

    /// Serialize all entities that have this component into the Lua table:
    /// `out_table[entity_index] = { field1 = ..., field2 = ... }`
    pub serialize_all: fn(lua: &Lua, world: &World) -> LuaResult<LuaTable>,

    /// Deserialize a Lua table and insert the component onto `entity`.
    /// Called when Lua does `quasar.add_component(entity_id, name, data)`.
    pub insert: fn(world: &mut World, entity: Entity, data: &LuaTable) -> LuaResult<()>,

    /// Remove the component from `entity`.
    pub remove: fn(world: &mut World, entity: Entity),
}

/// Registry of all component types accessible from Lua scripts.
pub struct ComponentRegistry {
    descriptors: HashMap<&'static str, ComponentDescriptor>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            descriptors: HashMap::new(),
        }
    }

    /// Register a component descriptor.
    pub fn register(&mut self, desc: ComponentDescriptor) {
        self.descriptors.insert(desc.name, desc);
    }

    /// Look up a descriptor by name.
    pub fn get(&self, name: &str) -> Option<&ComponentDescriptor> {
        self.descriptors.get(name)
    }

    /// Iterate all registered component names.
    pub fn names(&self) -> impl Iterator<Item = &&'static str> {
        self.descriptors.keys()
    }

    /// Iterate all descriptors.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &ComponentDescriptor)> {
        self.descriptors.iter().map(|(k, v)| (*k, v))
    }

    /// Serialize every registered component for every entity into a nested
    /// Lua table: `{ ComponentName = { [entity_id] = { ... } } }`.
    pub fn serialize_all_to_lua(&self, lua: &Lua, world: &World) -> LuaResult<LuaTable> {
        let root = lua.create_table()?;
        for (name, desc) in &self.descriptors {
            let table = (desc.serialize_all)(lua, world)?;
            root.set(*name, table)?;
        }
        Ok(root)
    }
}

// ── Built-in component registrations ──────────────────────────────

/// Build a registry pre-populated with the engine's built-in components.
pub fn default_registry() -> ComponentRegistry {
    let mut reg = ComponentRegistry::new();

    // Transform
    reg.register(ComponentDescriptor {
        name: "Transform",
        serialize_all: |lua, world| {
            use quasar_math::Transform;
            let table = lua.create_table()?;
            for (entity, t) in world.query::<Transform>().into_iter() {
                let entry = lua.create_table()?;
                entry.set("px", t.position.x)?;
                entry.set("py", t.position.y)?;
                entry.set("pz", t.position.z)?;
                entry.set("rx", t.rotation.x)?;
                entry.set("ry", t.rotation.y)?;
                entry.set("rz", t.rotation.z)?;
                entry.set("rw", t.rotation.w)?;
                entry.set("sx", t.scale.x)?;
                entry.set("sy", t.scale.y)?;
                entry.set("sz", t.scale.z)?;
                table.set(entity.index(), entry)?;
            }
            Ok(table)
        },
        insert: |world, entity, data| {
            use glam::{Quat, Vec3};
            use quasar_math::Transform;
            let px: f32 = data.get("px").unwrap_or(0.0);
            let py: f32 = data.get("py").unwrap_or(0.0);
            let pz: f32 = data.get("pz").unwrap_or(0.0);
            let rx: f32 = data.get("rx").unwrap_or(0.0);
            let ry: f32 = data.get("ry").unwrap_or(0.0);
            let rz: f32 = data.get("rz").unwrap_or(0.0);
            let rw: f32 = data.get("rw").unwrap_or(1.0);
            let sx: f32 = data.get("sx").unwrap_or(1.0);
            let sy: f32 = data.get("sy").unwrap_or(1.0);
            let sz: f32 = data.get("sz").unwrap_or(1.0);
            let t = Transform {
                position: Vec3::new(px, py, pz),
                rotation: Quat::from_xyzw(rx, ry, rz, rw).normalize(),
                scale: Vec3::new(sx, sy, sz),
            };
            world.insert(entity, t);
            Ok(())
        },
        remove: |world, entity| {
            world.remove_component::<quasar_math::Transform>(entity);
        },
    });

    reg
}
