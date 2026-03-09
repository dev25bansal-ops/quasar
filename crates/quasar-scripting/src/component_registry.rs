//! Component Registry — maps string component names to Rust component types
//! for Lua ↔ ECS bridging.
//!
//! Each registered component provides:
//! - A serializer that reads the component from the ECS and writes it as a Lua table.
//! - A deserializer that reads a Lua table and inserts the component into the ECS.
//! - A remover that removes the component from an entity.
//!
//! TypeId-based reflection enables zero-copy read-only access from Lua via
//! the `get_component_for_entity` / `set_component_for_entity` APIs.

use mlua::prelude::*;
use quasar_core::ecs::{Entity, World};
use std::any::TypeId;
use std::collections::HashMap;

/// A trait-object–friendly descriptor for one component type.
pub struct ComponentDescriptor {
    /// Human-readable name (e.g. `"Transform"`, `"RigidBody"`).
    pub name: &'static str,

    /// The Rust `TypeId` of the underlying component, enabling TypeId→name lookups.
    pub type_id: TypeId,

    /// Serialize all entities that have this component into the Lua table:
    /// `out_table[entity_index] = { field1 = ..., field2 = ... }`
    pub serialize_all: fn(lua: &Lua, world: &World) -> LuaResult<LuaTable>,

    /// Serialize a single entity's component into a Lua table.
    /// Returns `Ok(Nil)` if the entity doesn't have this component.
    pub serialize_one: fn(lua: &Lua, world: &World, entity: Entity) -> LuaResult<LuaValue>,

    /// Deserialize a Lua table and insert the component onto `entity`.
    /// Called when Lua does `quasar.add_component(entity_id, name, data)`.
    pub insert: fn(world: &mut World, entity: Entity, data: &LuaTable) -> LuaResult<()>,

    /// Update an existing component on `entity` from a Lua table.
    /// Only modifies fields present in the table, preserving unspecified fields.
    pub update: fn(world: &mut World, entity: Entity, data: &LuaTable) -> LuaResult<()>,

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

    /// Look up a descriptor by TypeId.
    pub fn get_by_type_id(&self, type_id: TypeId) -> Option<&ComponentDescriptor> {
        self.descriptors.values().find(|d| d.type_id == type_id)
    }

    /// Read a single entity's component and return it as a Lua value (table or nil).
    /// Zero-copy read-only access — the world is borrowed immutably.
    pub fn get_component_for_entity(
        &self,
        lua: &Lua,
        world: &World,
        entity: Entity,
        name: &str,
    ) -> LuaResult<LuaValue> {
        match self.get(name) {
            Some(desc) => (desc.serialize_one)(lua, world, entity),
            None => Ok(LuaValue::Nil),
        }
    }

    /// Write fields from a Lua table into an existing component on an entity.
    /// Only fields present in the table are modified (partial update).
    pub fn set_component_for_entity(
        &self,
        world: &mut World,
        entity: Entity,
        name: &str,
        data: &LuaTable,
    ) -> LuaResult<bool> {
        match self.get(name) {
            Some(desc) => {
                (desc.update)(world, entity, data)?;
                Ok(true)
            }
            None => Ok(false),
        }
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
        type_id: TypeId::of::<quasar_math::Transform>(),
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
        serialize_one: |lua, world, entity| {
            use quasar_math::Transform;
            match world.get::<Transform>(entity) {
                Some(t) => {
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
                    Ok(LuaValue::Table(entry))
                }
                None => Ok(LuaValue::Nil),
            }
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
        update: |world, entity, data| {
            use glam::{Quat, Vec3};
            use quasar_math::Transform;
            if let Some(existing) = world.get::<Transform>(entity).cloned() {
                let px: f32 = data.get("px").unwrap_or(existing.position.x);
                let py: f32 = data.get("py").unwrap_or(existing.position.y);
                let pz: f32 = data.get("pz").unwrap_or(existing.position.z);
                let rx: f32 = data.get("rx").unwrap_or(existing.rotation.x);
                let ry: f32 = data.get("ry").unwrap_or(existing.rotation.y);
                let rz: f32 = data.get("rz").unwrap_or(existing.rotation.z);
                let rw: f32 = data.get("rw").unwrap_or(existing.rotation.w);
                let sx: f32 = data.get("sx").unwrap_or(existing.scale.x);
                let sy: f32 = data.get("sy").unwrap_or(existing.scale.y);
                let sz: f32 = data.get("sz").unwrap_or(existing.scale.z);
                let t = Transform {
                    position: Vec3::new(px, py, pz),
                    rotation: Quat::from_xyzw(rx, ry, rz, rw).normalize(),
                    scale: Vec3::new(sx, sy, sz),
                };
                world.insert(entity, t);
            }
            Ok(())
        },
        remove: |world, entity| {
            world.remove_component::<quasar_math::Transform>(entity);
        },
    });

    reg
}
