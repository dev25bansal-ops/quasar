//! Query interface for accessing component data across entities.
//!
//! Provides a typed, composable query system that iterates over entities
//! matching a given component tuple. Supports 1–4 component queries,
//! optional components via `Option<&T>`, and filter markers `With<T>` /
//! `Without<T>` / `Changed<T>` / `Added<T>` / `Removed<T>`.

use super::component::Component;
use super::entity::Entity;
use super::World;
use std::any::TypeId;
use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// WorldQuery trait — items that can be fetched from the World
// ---------------------------------------------------------------------------

/// Trait implemented by things that can be fetched from the World for each entity.
pub trait WorldQuery {
    type Item<'w>;
    /// Return the TypeIds of required components.
    fn type_ids() -> Vec<TypeId>;
    /// Fetch data for a specific entity index from the world. Returns None if missing.
    ///
    /// # Safety
    /// Caller must ensure world reference is valid for the lifetime.
    fn fetch<'w>(world: &'w World, entity_index: u32) -> Option<Self::Item<'w>>;
}

/// Fetch a single immutable component reference.
impl<T: Component> WorldQuery for &T {
    type Item<'w> = &'w T;
    fn type_ids() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
    fn fetch<'w>(world: &'w World, entity_index: u32) -> Option<Self::Item<'w>> {
        world.storage::<T>()?.data.get(&entity_index)
    }
}

/// Fetch an optional immutable component reference.
impl<T: Component> WorldQuery for Option<&T> {
    type Item<'w> = Option<&'w T>;
    fn type_ids() -> Vec<TypeId> {
        vec![] // optional — no required types
    }
    fn fetch<'w>(world: &'w World, entity_index: u32) -> Option<Self::Item<'w>> {
        Some(world.storage::<T>().and_then(|s| s.data.get(&entity_index)))
    }
}

// Tuple impls: (A,), (A, B), (A, B, C), (A, B, C, D)

impl<A: WorldQuery> WorldQuery for (A,) {
    type Item<'w> = (A::Item<'w>,);
    fn type_ids() -> Vec<TypeId> {
        A::type_ids()
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((A::fetch(world, idx)?,))
    }
}

impl<A: WorldQuery, B: WorldQuery> WorldQuery for (A, B) {
    type Item<'w> = (A::Item<'w>, B::Item<'w>);
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((A::fetch(world, idx)?, B::fetch(world, idx)?))
    }
}

impl<A: WorldQuery, B: WorldQuery, C: WorldQuery> WorldQuery for (A, B, C) {
    type Item<'w> = (A::Item<'w>, B::Item<'w>, C::Item<'w>);
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((
            A::fetch(world, idx)?,
            B::fetch(world, idx)?,
            C::fetch(world, idx)?,
        ))
    }
}

impl<A: WorldQuery, B: WorldQuery, C: WorldQuery, D: WorldQuery> WorldQuery for (A, B, C, D) {
    type Item<'w> = (A::Item<'w>, B::Item<'w>, C::Item<'w>, D::Item<'w>);
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids.extend(D::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((
            A::fetch(world, idx)?,
            B::fetch(world, idx)?,
            C::fetch(world, idx)?,
            D::fetch(world, idx)?,
        ))
    }
}

// ---------------------------------------------------------------------------
// QueryFilter — additional filters (With, Without, Changed, Added, Removed)
// ---------------------------------------------------------------------------

/// Trait for query filters applied during iteration.
pub trait QueryFilter {
    fn matches(world: &World, entity_index: u32) -> bool;
}

/// No filter — matches all entities.
impl QueryFilter for () {
    fn matches(_world: &World, _entity_index: u32) -> bool {
        true
    }
}

/// Filter: entity must also have component `T`.
pub struct FilterWith<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for FilterWith<T> {
    fn matches(world: &World, entity_index: u32) -> bool {
        if let Some(components) = world.entity_component_list(entity_index) {
            components.binary_search(&TypeId::of::<T>()).is_ok()
        } else {
            false
        }
    }
}

/// Filter: entity must NOT have component `T`.
pub struct FilterWithout<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for FilterWithout<T> {
    fn matches(world: &World, entity_index: u32) -> bool {
        if let Some(components) = world.entity_component_list(entity_index) {
            components.binary_search(&TypeId::of::<T>()).is_err()
        } else {
            true
        }
    }
}

/// Filter: component `T` was changed since stored `since_tick`.
pub struct FilterChanged<T: Component> {
    _marker: PhantomData<T>,
    pub since_tick: u64,
}

impl<T: Component> FilterChanged<T> {
    pub fn new(since_tick: u64) -> Self {
        Self {
            _marker: PhantomData,
            since_tick,
        }
    }
}

/// Filter: component `T` was added this tick (change_tick == current_tick of storage).
pub struct FilterAdded<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for FilterAdded<T> {
    fn matches(world: &World, entity_index: u32) -> bool {
        if let Some(storage) = world.storage::<T>() {
            if let Some(&tick) = storage.change_ticks.get(&entity_index) {
                return tick == storage.current_tick;
            }
        }
        false
    }
}

/// Filter: component `T` was recently removed (tracked in removal log).
pub struct FilterRemoved<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for FilterRemoved<T> {
    fn matches(world: &World, entity_index: u32) -> bool {
        world.was_removed::<T>(entity_index)
    }
}

// Tuple filter impls: (F1, F2), (F1, F2, F3)
impl<F1: QueryFilter, F2: QueryFilter> QueryFilter for (F1, F2) {
    fn matches(world: &World, entity_index: u32) -> bool {
        F1::matches(world, entity_index) && F2::matches(world, entity_index)
    }
}

impl<F1: QueryFilter, F2: QueryFilter, F3: QueryFilter> QueryFilter for (F1, F2, F3) {
    fn matches(world: &World, entity_index: u32) -> bool {
        F1::matches(world, entity_index)
            && F2::matches(world, entity_index)
            && F3::matches(world, entity_index)
    }
}

// ---------------------------------------------------------------------------
// QueryState — the main typed query struct
// ---------------------------------------------------------------------------

/// Typed query over entities. Collects matching entity indices then fetches
/// component data via `WorldQuery`.
pub struct QueryState<Q: WorldQuery, F: QueryFilter = ()> {
    _q: PhantomData<Q>,
    _f: PhantomData<F>,
}

impl<Q: WorldQuery, F: QueryFilter> QueryState<Q, F> {
    pub fn new() -> Self {
        Self {
            _q: PhantomData,
            _f: PhantomData,
        }
    }

    /// Iterate, returning (Entity, Q::Item) for each matching entity.
    pub fn iter<'w>(&self, world: &'w World) -> QueryIter<'w, Q, F> {
        let required = Q::type_ids();
        let mut matching = Vec::new();

        for (&idx, components) in world.entity_components_iter() {
            let has_all = required.iter().all(|tid| components.binary_search(tid).is_ok());
            if has_all && F::matches(world, idx) {
                matching.push(idx);
            }
        }

        QueryIter {
            world,
            indices: matching,
            pos: 0,
            _q: PhantomData,
            _f: PhantomData,
        }
    }

    /// Collect into a Vec for convenience.
    pub fn collect<'w>(&self, world: &'w World) -> Vec<(Entity, Q::Item<'w>)> {
        self.iter(world).collect()
    }
}

// ---------------------------------------------------------------------------
// QueryIter — iterator type
// ---------------------------------------------------------------------------

pub struct QueryIter<'w, Q: WorldQuery, F: QueryFilter = ()> {
    world: &'w World,
    indices: Vec<u32>,
    pos: usize,
    _q: PhantomData<Q>,
    _f: PhantomData<F>,
}

impl<'w, Q: WorldQuery, F: QueryFilter> Iterator for QueryIter<'w, Q, F> {
    type Item = (Entity, Q::Item<'w>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.pos < self.indices.len() {
            let idx = self.indices[self.pos];
            self.pos += 1;
            if let Some(item) = Q::fetch(self.world, idx) {
                let generation = self.world.generation_of(idx);
                return Some((Entity::new(idx, generation), item));
            }
        }
        None
    }
}

// Keep backward-compatible Query<T> alias
pub struct Query<T: Component> {
    _marker: PhantomData<T>,
}

#[allow(dead_code)]
impl<T: Component> Query<T> {
    pub fn iter(world: &World) -> QueryIter<'_, &T, ()> {
        let state = QueryState::<&T, ()>::new();
        state.iter(world)
    }
}
