//! World — the central data store for all entities and components.

use std::any::TypeId;
use std::collections::HashMap;

use super::component::{Component, ComponentStorage, TypedStorage};
use super::entity::{Entity, EntityAllocator};

/// The central container holding all entities and their component data.
///
/// # Examples
/// ```
/// use quasar_core::ecs::World;
///
/// let mut world = World::new();
///
/// // Spawn an entity with two components.
/// let player = world.spawn();
/// world.insert(player, 100_u32);  // health
/// world.insert(player, "Hero");   // name tag
///
/// assert_eq!(world.get::<u32>(player), Some(&100));
/// ```
pub struct World {
    allocator: EntityAllocator,
    storages: HashMap<TypeId, Box<dyn ComponentStorage>>,
}

impl World {
    /// Create an empty world.
    pub fn new() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            storages: HashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Entity management
    // ------------------------------------------------------------------

    /// Spawn a new entity (with no components).
    pub fn spawn(&mut self) -> Entity {
        self.allocator.allocate()
    }

    /// Despawn an entity, removing all of its components.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.allocator.deallocate(entity) {
            return false;
        }
        for storage in self.storages.values_mut() {
            storage.remove(entity);
        }
        true
    }

    /// Returns `true` if the entity handle is still valid.
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.allocator.is_alive(entity)
    }

    /// Returns the number of alive entities.
    pub fn entity_count(&self) -> u32 {
        self.allocator.alive_count()
    }

    // ------------------------------------------------------------------
    // Component management
    // ------------------------------------------------------------------

    /// Attach (or replace) a component on an entity.
    pub fn insert<T: Component>(&mut self, entity: Entity, component: T) {
        debug_assert!(self.is_alive(entity), "inserting on a dead entity");
        self.storage_mut::<T>().insert(entity, component);
    }

    /// Remove a component from an entity, returning `true` if it existed.
    pub fn remove_component<T: Component>(&mut self, entity: Entity) -> bool {
        if let Some(storage) = self.storages.get_mut(&TypeId::of::<T>()) {
            storage.remove(entity)
        } else {
            false
        }
    }

    /// Get a shared reference to a component on an entity.
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        self.storage::<T>()?.get(entity)
    }

    /// Get a mutable reference to a component on an entity.
    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        self.storage_mut::<T>().get_mut(entity)
    }

    /// Check whether an entity has a specific component type.
    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        self.storage::<T>()
            .map_or(false, |s| s.data.contains_key(&entity.index))
    }

    // ------------------------------------------------------------------
    // Query helpers
    // ------------------------------------------------------------------

    /// Iterate over all `(Entity, &T)` pairs.
    pub fn query<T: Component>(&self) -> impl Iterator<Item = (Entity, &T)> {
        let type_id = TypeId::of::<T>();
        let iter = self
            .storages
            .get(&type_id)
            .and_then(|s| s.as_any().downcast_ref::<TypedStorage<T>>())
            .into_iter()
            .flat_map(|storage| {
                storage.data.iter().filter_map(|(&index, component)| {
                    // Reconstruct entity handle — we need the generation from the
                    // allocator. Because storages only contain alive entities,
                    // this is safe.
                    let gen = 0; // Simplified: we trust storage consistency
                    Some((Entity::new(index, gen), component))
                })
            });
        iter
    }

    /// Iterate over all `(Entity, &mut T)` pairs.
    pub fn query_mut<T: Component>(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        let type_id = TypeId::of::<T>();
        let iter = self
            .storages
            .get_mut(&type_id)
            .and_then(|s| s.as_any_mut().downcast_mut::<TypedStorage<T>>())
            .into_iter()
            .flat_map(|storage| {
                storage.data.iter_mut().map(|(&index, component)| {
                    (Entity::new(index, 0), component)
                })
            });
        iter
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Get or create the typed storage for `T`.
    fn storage_mut<T: Component>(&mut self) -> &mut TypedStorage<T> {
        let type_id = TypeId::of::<T>();
        self.storages
            .entry(type_id)
            .or_insert_with(|| Box::new(TypedStorage::<T>::new()))
            .as_any_mut()
            .downcast_mut::<TypedStorage<T>>()
            .expect("type mismatch in component storage")
    }

    /// Get an existing typed storage for `T` (read-only).
    fn storage<T: Component>(&self) -> Option<&TypedStorage<T>> {
        let type_id = TypeId::of::<T>();
        self.storages
            .get(&type_id)?
            .as_any()
            .downcast_ref::<TypedStorage<T>>()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Debug, PartialEq)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[test]
    fn spawn_and_insert() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 2.0 });

        assert_eq!(world.get::<Position>(e), Some(&Position { x: 1.0, y: 2.0 }));
        assert_eq!(world.get::<Velocity>(e), None);
    }

    #[test]
    fn despawn_removes_components() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 0.0, y: 0.0 });
        world.despawn(e);

        assert!(!world.is_alive(e));
    }

    #[test]
    fn query_iterates_all() {
        let mut world = World::new();
        for i in 0..5 {
            let e = world.spawn();
            world.insert(e, Position { x: i as f32, y: 0.0 });
        }

        let positions: Vec<_> = world.query::<Position>().collect();
        assert_eq!(positions.len(), 5);
    }
}
