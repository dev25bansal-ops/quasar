//! Commands — deferred mutation pattern for safe world access.
//!
//! Systems often need to spawn/despawn entities or insert components,
//! but doing so directly would cause borrow checker issues when iterating.
//! Commands queue these operations and flush them between stages.

#![allow(clippy::type_complexity)]

use std::any::TypeId;

use super::archetype::{ColumnStorage, TypedColumn};
use super::{Component, Entity, World};

/// Helper to create a column factory for type T.
fn create_column<T: 'static + Send + Sync>() -> Box<dyn ColumnStorage> {
    Box::new(TypedColumn::<T>::new())
}

/// A single deferred command to be executed on the World.
pub enum Command {
    /// Spawn a new entity and optionally insert initial components.
    Spawn {
        /// Pre-serialized component data to insert, with column factories.
        components: Vec<(
            TypeId,
            Box<dyn std::any::Any + Send + Sync>,
            fn() -> Box<dyn ColumnStorage>,
        )>,
    },
    /// Despawn an entity.
    Despawn(Entity),
    /// Insert a component on an entity.
    Insert {
        entity: Entity,
        type_id: TypeId,
        component: Box<dyn std::any::Any + Send + Sync>,
        column_factory: fn() -> Box<dyn ColumnStorage>,
    },
    /// Remove a component from an entity.
    Remove { entity: Entity, type_id: TypeId },
    /// Insert a resource.
    InsertResource {
        type_id: TypeId,
        resource: Box<dyn std::any::Any + Send + Sync>,
    },
    /// Remove a resource.
    RemoveResource(TypeId),
}

/// A queue of commands that will be applied to the World.
///
/// Systems can obtain a `Commands` reference and push operations,
/// which are then flushed between stages.
pub struct Commands {
    queue: Vec<Command>,
}

impl Commands {
    pub fn new() -> Self {
        Self { queue: Vec::new() }
    }

    /// Queue spawning a new entity.
    pub fn spawn(&mut self) -> EntitySpawnBuilder<'_> {
        EntitySpawnBuilder {
            commands: self,
            components: Vec::new(),
        }
    }

    /// Queue spawning an entity with no components.
    pub fn spawn_empty(&mut self) -> Entity {
        let entity = Entity::new(self.queue.len() as u32, 0);
        self.queue.push(Command::Spawn {
            components: Vec::new(),
        });
        entity
    }

    /// Queue despawning an entity.
    pub fn despawn(&mut self, entity: Entity) {
        self.queue.push(Command::Despawn(entity));
    }

    /// Queue inserting a component on an entity.
    pub fn insert<T: Component + Send + Sync>(&mut self, entity: Entity, component: T) {
        self.queue.push(Command::Insert {
            entity,
            type_id: TypeId::of::<T>(),
            component: Box::new(component),
            column_factory: create_column::<T>,
        });
    }

    /// Queue removing a component from an entity.
    pub fn remove<T: Component + Send + Sync>(&mut self, entity: Entity) {
        self.queue.push(Command::Remove {
            entity,
            type_id: TypeId::of::<T>(),
        });
    }

    /// Queue inserting a resource.
    pub fn insert_resource<T: 'static + Send + Sync>(&mut self, resource: T) {
        self.queue.push(Command::InsertResource {
            type_id: TypeId::of::<T>(),
            resource: Box::new(resource),
        });
    }

    /// Queue removing a resource.
    pub fn remove_resource<T: 'static + Send + Sync>(&mut self) {
        self.queue.push(Command::RemoveResource(TypeId::of::<T>()));
    }

    /// Apply all queued commands to the world.
    pub fn apply(&mut self, world: &mut World) {
        for cmd in self.queue.drain(..) {
            match cmd {
                Command::Spawn { components } => {
                    let entity = world.spawn();
                    for (type_id, component, factory) in components {
                        world.insert_raw(entity, type_id, component, factory);
                    }
                }
                Command::Despawn(entity) => {
                    world.despawn(entity);
                }
                Command::Insert {
                    entity,
                    type_id,
                    component,
                    column_factory,
                } => {
                    world.insert_raw(entity, type_id, component, column_factory);
                }
                Command::Remove { entity, type_id } => {
                    world.remove_raw(entity, type_id);
                }
                Command::InsertResource { type_id, resource } => {
                    world.insert_resource_raw(type_id, resource);
                }
                Command::RemoveResource(type_id) => {
                    world.remove_resource_raw(type_id);
                }
            }
        }
    }

    /// Get the number of pending commands.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Check if there are no pending commands.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl Default for Commands {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for spawning an entity with components.
pub struct EntitySpawnBuilder<'a> {
    commands: &'a mut Commands,
    components: Vec<(
        TypeId,
        Box<dyn std::any::Any + Send + Sync>,
        fn() -> Box<dyn ColumnStorage>,
    )>,
}

impl<'a> EntitySpawnBuilder<'a> {
    /// Add a component to the entity being spawned.
    pub fn with<T: Component + Send + Sync>(mut self, component: T) -> Self {
        self.components
            .push((TypeId::of::<T>(), Box::new(component), create_column::<T>));
        self
    }

    /// Finish building and queue the spawn command.
    /// Returns a placeholder Entity that will be replaced when applied.
    pub fn id(self) -> Entity {
        let entity = Entity::new(self.commands.queue.len() as u32, 0);
        self.commands.queue.push(Command::Spawn {
            components: self.components,
        });
        entity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Clone)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Debug, PartialEq, Clone)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[test]
    fn spawn_entity_with_components() {
        let mut world = World::new();
        let mut cmds = Commands::new();

        cmds.spawn()
            .with(Position { x: 1.0, y: 2.0 })
            .with(Velocity { dx: 0.5, dy: 0.0 })
            .id();

        assert_eq!(world.entity_count(), 0);

        cmds.apply(&mut world);
        assert_eq!(world.entity_count(), 1);
    }

    #[test]
    fn insert_and_remove_components() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 0.0, y: 0.0 });

        // First ensure storage exists by inserting a dummy
        let temp = world.spawn();
        world.insert(temp, Velocity { dx: 0.0, dy: 0.0 });
        world.despawn(temp);

        let mut cmds = Commands::new();
        cmds.insert(entity, Velocity { dx: 1.0, dy: 2.0 });
        cmds.apply(&mut world);

        assert!(world.get::<Velocity>(entity).is_some());

        let mut cmds = Commands::new();
        cmds.remove::<Velocity>(entity);
        cmds.apply(&mut world);

        assert!(world.get::<Velocity>(entity).is_none());
    }

    #[test]
    fn despawn_entity() {
        let mut world = World::new();
        let entity = world.spawn();

        let mut cmds = Commands::new();
        cmds.despawn(entity);
        cmds.apply(&mut world);

        assert!(!world.is_alive(entity));
    }
}
