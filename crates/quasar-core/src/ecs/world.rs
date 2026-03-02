//! World — the central data store for all entities and components.

use std::any::{Any, TypeId};
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
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl World {
    /// Create an empty world.
    pub fn new() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            storages: HashMap::new(),
            resources: HashMap::new(),
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
            .is_some_and(|s| s.data.contains_key(&entity.index))
    }

    // ------------------------------------------------------------------
    // Resources (global singletons)
    // ------------------------------------------------------------------

    /// Insert a global resource (not tied to any entity).
    /// Replaces any existing resource of the same type.
    pub fn insert_resource<T: 'static + Send + Sync>(&mut self, resource: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(resource));
    }

    /// Get a shared reference to a global resource.
    pub fn resource<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|r| r.downcast_ref::<T>())
    }

    /// Get a mutable reference to a global resource.
    pub fn resource_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|r| r.downcast_mut::<T>())
    }

    /// Remove a global resource, returning it if it existed.
    pub fn remove_resource<T: 'static + Send + Sync>(&mut self) -> Option<T> {
        self.resources
            .remove(&TypeId::of::<T>())
            .and_then(|r| r.downcast::<T>().ok())
            .map(|b| *b)
    }

    /// Check whether a global resource of type `T` exists.
    pub fn has_resource<T: 'static + Send + Sync>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    // ------------------------------------------------------------------
    // Entity builder
    // ------------------------------------------------------------------

    /// Spawn a new entity and return a builder for attaching components.
    ///
    /// # Example
    /// ```ignore
    /// let entity = world.spawn_with()
    ///     .with(Position { x: 0.0, y: 0.0 })
    ///     .with(Velocity { dx: 1.0, dy: 0.0 })
    ///     .id();
    /// ```
    pub fn spawn_with(&mut self) -> EntityBuilder<'_> {
        let entity = self.allocator.allocate();
        EntityBuilder {
            world: self,
            entity,
        }
    }

    // ------------------------------------------------------------------
    // Query helpers
    // ------------------------------------------------------------------

    /// Iterate over all `(Entity, &T)` pairs.
    pub fn query<T: Component>(&self) -> impl Iterator<Item = (Entity, &T)> {
        let type_id = TypeId::of::<T>();
        let allocator = &self.allocator;
        let iter = self
            .storages
            .get(&type_id)
            .and_then(|s| s.as_any().downcast_ref::<TypedStorage<T>>())
            .into_iter()
            .flat_map(move |storage| {
                storage.data.iter().map(move |(&index, component)| {
                    let generation = allocator.generation_of(index);
                    (Entity::new(index, generation), component)
                })
            });
        iter
    }

    /// Iterate over all `(Entity, &mut T)` pairs using a callback.
    pub fn for_each_mut<T, F>(&mut self, mut f: F)
    where
        T: Component,
        F: FnMut(Entity, &mut T),
    {
        let type_id = TypeId::of::<T>();
        if let Some(storage) = self
            .storages
            .get_mut(&type_id)
            .and_then(|s| s.as_any_mut().downcast_mut::<TypedStorage<T>>())
        {
            let indices: Vec<u32> = storage.data.keys().copied().collect();

            for index in indices {
                let generation = self.allocator.generation_of(index);
                if let Some(component) = storage.data.get_mut(&index) {
                    f(Entity::new(index, generation), component);
                }
            }
        }
    }

    /// Query entities that have **both** components `A` and `B`.
    ///
    /// Iterates over the smaller storage and looks up the other, yielding
    /// `(Entity, &A, &B)` for every entity that has both.
    pub fn query2<A: Component, B: Component>(&self) -> impl Iterator<Item = (Entity, &A, &B)> {
        let sa = self.storage::<A>();
        let sb = self.storage::<B>();
        let allocator = &self.allocator;

        sa.into_iter().flat_map(move |storage_a| {
            storage_a.data.iter().filter_map(move |(&index, comp_a)| {
                sb.and_then(|storage_b| storage_b.data.get(&index))
                    .map(|comp_b| {
                        let generation = allocator.generation_of(index);
                        (Entity::new(index, generation), comp_a, comp_b)
                    })
            })
        })
    }

    /// Query entities that have components `A`, `B`, **and** `C`.
    pub fn query3<A: Component, B: Component, C: Component>(
        &self,
    ) -> impl Iterator<Item = (Entity, &A, &B, &C)> {
        let sa = self.storage::<A>();
        let sb = self.storage::<B>();
        let sc = self.storage::<C>();
        let allocator = &self.allocator;

        sa.into_iter().flat_map(move |storage_a| {
            storage_a.data.iter().filter_map(move |(&index, comp_a)| {
                let comp_b = sb.and_then(|s| s.data.get(&index))?;
                let comp_c = sc.and_then(|s| s.data.get(&index))?;
                let generation = allocator.generation_of(index);
                Some((Entity::new(index, generation), comp_a, comp_b, comp_c))
            })
        })
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

/// Fluent builder for spawning entities with components.
///
/// Created by [`World::spawn_with`].
pub struct EntityBuilder<'w> {
    world: &'w mut World,
    entity: Entity,
}

impl<'w> EntityBuilder<'w> {
    /// Attach a component to this entity.
    pub fn with<T: Component>(self, component: T) -> Self {
        self.world.insert(self.entity, component);
        self
    }

    /// Finish building and return the entity handle.
    pub fn id(self) -> Entity {
        self.entity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query;

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

    #[derive(Debug, PartialEq)]
    struct Health(u32);

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
            world.insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                },
            );
        }

        let positions: Vec<_> = world.query::<Position>().collect();
        assert_eq!(positions.len(), 5);
    }

    #[test]
    fn for_each_mut_modifies_components() {
        let mut world = World::new();
        for i in 0..5 {
            let e = world.spawn();
            world.insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                },
            );
        }

        world.for_each_mut(|_entity, pos: &mut Position| {
            pos.x += 10.0;
        });

        for i in 0..5 {
            let e = i as u32;
            let pos = world.get::<Position>(Entity::new(e, 0));
            assert_eq!(
                pos,
                Some(&Position {
                    x: (i as f32) + 10.0,
                    y: 0.0
                })
            );
        }
    }

    #[test]
    fn query2_intersects_components() {
        let mut world = World::new();

        // Entity with both Position and Velocity
        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });
        world.insert(e1, Velocity { dx: 3.0, dy: 4.0 });

        // Entity with only Position
        let e2 = world.spawn();
        world.insert(e2, Position { x: 5.0, y: 6.0 });

        // Entity with only Velocity
        let e3 = world.spawn();
        world.insert(e3, Velocity { dx: 7.0, dy: 8.0 });

        let results: Vec<_> = world.query2::<Position, Velocity>().collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, &Position { x: 1.0, y: 2.0 });
        assert_eq!(results[0].2, &Velocity { dx: 3.0, dy: 4.0 });
    }

    #[test]
    fn query3_intersects_three_components() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });
        world.insert(e1, Velocity { dx: 3.0, dy: 4.0 });
        world.insert(e1, Health(100));

        // Missing Health
        let e2 = world.spawn();
        world.insert(e2, Position { x: 5.0, y: 6.0 });
        world.insert(e2, Velocity { dx: 7.0, dy: 8.0 });

        let results: Vec<_> = world.query3::<Position, Velocity, Health>().collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].3, &Health(100));
    }

    #[test]
    fn query_macro_tuple_syntax() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });
        world.insert(e1, Velocity { dx: 3.0, dy: 4.0 });

        let e2 = world.spawn();
        world.insert(e2, Position { x: 5.0, y: 6.0 });
        world.insert(e2, Velocity { dx: 7.0, dy: 8.0 });

        let results: Vec<_> = query!(world, (Position, Velocity)).collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_macro_simple_syntax() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });
        world.insert(e1, Velocity { dx: 3.0, dy: 4.0 });
        world.insert(e1, Health(100));

        let results: Vec<_> = query!(world, Position, Velocity, Health).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].3, &Health(100));
    }

    #[test]
    fn resources_crud() {
        let mut world = World::new();

        // Insert
        world.insert_resource(42_u32);
        assert_eq!(world.resource::<u32>(), Some(&42));
        assert!(world.has_resource::<u32>());

        // Mutate
        *world.resource_mut::<u32>().unwrap() = 99;
        assert_eq!(world.resource::<u32>(), Some(&99));

        // Remove
        let removed = world.remove_resource::<u32>();
        assert_eq!(removed, Some(99));
        assert!(!world.has_resource::<u32>());
    }

    #[test]
    fn entity_builder_attaches_components() {
        let mut world = World::new();
        let e = world
            .spawn_with()
            .with(Position { x: 10.0, y: 20.0 })
            .with(Velocity { dx: 1.0, dy: 2.0 })
            .with(Health(50))
            .id();

        assert!(world.is_alive(e));
        assert_eq!(
            world.get::<Position>(e),
            Some(&Position { x: 10.0, y: 20.0 })
        );
        assert_eq!(
            world.get::<Velocity>(e),
            Some(&Velocity { dx: 1.0, dy: 2.0 })
        );
        assert_eq!(world.get::<Health>(e), Some(&Health(50)));
    }
}
