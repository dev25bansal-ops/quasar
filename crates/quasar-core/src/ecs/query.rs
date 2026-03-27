//! Query interface for accessing component data across entities.
//!
//! Provides a typed, composable query system that iterates over entities
//! matching a given component tuple. Supports 1–4 component queries,
//! optional components via `Option<&T>`, and filter markers `With<T>` /
//! `Without<T>` / `Changed<T>` / `Added<T>` / `Removed<T>`.

use super::archetype::TypedColumn;
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

/// Fetch a single immutable component reference via archetype SoA.
impl<T: Component> WorldQuery for &T {
    type Item<'w> = &'w T;
    fn type_ids() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
    fn fetch<'w>(world: &'w World, entity_index: u32) -> Option<Self::Item<'w>> {
        let generation = world.generation_of(entity_index);
        world.get::<T>(super::entity::Entity::new(entity_index, generation))
    }
}

/// Fetch an optional immutable component reference via archetype SoA.
impl<T: Component> WorldQuery for Option<&T> {
    type Item<'w> = Option<&'w T>;
    fn type_ids() -> Vec<TypeId> {
        vec![] // optional — no required types
    }
    fn fetch<'w>(world: &'w World, entity_index: u32) -> Option<Self::Item<'w>> {
        let generation = world.generation_of(entity_index);
        Some(world.get::<T>(super::entity::Entity::new(entity_index, generation)))
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

impl<A: WorldQuery, B: WorldQuery, C: WorldQuery, D: WorldQuery, E: WorldQuery> WorldQuery
    for (A, B, C, D, E)
{
    type Item<'w> = (
        A::Item<'w>,
        B::Item<'w>,
        C::Item<'w>,
        D::Item<'w>,
        E::Item<'w>,
    );
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids.extend(D::type_ids());
        ids.extend(E::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((
            A::fetch(world, idx)?,
            B::fetch(world, idx)?,
            C::fetch(world, idx)?,
            D::fetch(world, idx)?,
            E::fetch(world, idx)?,
        ))
    }
}

impl<A: WorldQuery, B: WorldQuery, C: WorldQuery, D: WorldQuery, E: WorldQuery, F: WorldQuery>
    WorldQuery for (A, B, C, D, E, F)
{
    type Item<'w> = (
        A::Item<'w>,
        B::Item<'w>,
        C::Item<'w>,
        D::Item<'w>,
        E::Item<'w>,
        F::Item<'w>,
    );
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids.extend(D::type_ids());
        ids.extend(E::type_ids());
        ids.extend(F::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((
            A::fetch(world, idx)?,
            B::fetch(world, idx)?,
            C::fetch(world, idx)?,
            D::fetch(world, idx)?,
            E::fetch(world, idx)?,
            F::fetch(world, idx)?,
        ))
    }
}

impl<
        A: WorldQuery,
        B: WorldQuery,
        C: WorldQuery,
        D: WorldQuery,
        E: WorldQuery,
        F: WorldQuery,
        G: WorldQuery,
    > WorldQuery for (A, B, C, D, E, F, G)
{
    type Item<'w> = (
        A::Item<'w>,
        B::Item<'w>,
        C::Item<'w>,
        D::Item<'w>,
        E::Item<'w>,
        F::Item<'w>,
        G::Item<'w>,
    );
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids.extend(D::type_ids());
        ids.extend(E::type_ids());
        ids.extend(F::type_ids());
        ids.extend(G::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((
            A::fetch(world, idx)?,
            B::fetch(world, idx)?,
            C::fetch(world, idx)?,
            D::fetch(world, idx)?,
            E::fetch(world, idx)?,
            F::fetch(world, idx)?,
            G::fetch(world, idx)?,
        ))
    }
}

impl<
        A: WorldQuery,
        B: WorldQuery,
        C: WorldQuery,
        D: WorldQuery,
        E: WorldQuery,
        F: WorldQuery,
        G: WorldQuery,
        H: WorldQuery,
    > WorldQuery for (A, B, C, D, E, F, G, H)
{
    type Item<'w> = (
        A::Item<'w>,
        B::Item<'w>,
        C::Item<'w>,
        D::Item<'w>,
        E::Item<'w>,
        F::Item<'w>,
        G::Item<'w>,
        H::Item<'w>,
    );
    fn type_ids() -> Vec<TypeId> {
        let mut ids = A::type_ids();
        ids.extend(B::type_ids());
        ids.extend(C::type_ids());
        ids.extend(D::type_ids());
        ids.extend(E::type_ids());
        ids.extend(F::type_ids());
        ids.extend(G::type_ids());
        ids.extend(H::type_ids());
        ids
    }
    fn fetch<'w>(world: &'w World, idx: u32) -> Option<Self::Item<'w>> {
        Some((
            A::fetch(world, idx)?,
            B::fetch(world, idx)?,
            C::fetch(world, idx)?,
            D::fetch(world, idx)?,
            E::fetch(world, idx)?,
            F::fetch(world, idx)?,
            G::fetch(world, idx)?,
            H::fetch(world, idx)?,
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

/// When used as a `QueryFilter`, `FilterChanged<T>` uses the active system's
/// last-run tick (set by `World::begin_system`) to detect components that
/// changed since the current system last ran.
impl<T: Component> QueryFilter for FilterChanged<T> {
    fn matches(world: &World, entity_index: u32) -> bool {
        let since = world.active_system_last_run();
        world
            .change_tick_for(TypeId::of::<T>(), entity_index)
            .is_some_and(|tick| tick > since)
    }
}

/// Filter: component `T` was added this tick (change_tick == current world tick).
pub struct FilterAdded<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for FilterAdded<T> {
    fn matches(world: &World, entity_index: u32) -> bool {
        world
            .change_tick_for(TypeId::of::<T>(), entity_index)
            .is_some_and(|tick| tick == world.current_tick::<T>())
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

impl<F1: QueryFilter, F2: QueryFilter, F3: QueryFilter, F4: QueryFilter> QueryFilter
    for (F1, F2, F3, F4)
{
    fn matches(world: &World, entity_index: u32) -> bool {
        F1::matches(world, entity_index)
            && F2::matches(world, entity_index)
            && F3::matches(world, entity_index)
            && F4::matches(world, entity_index)
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

impl<Q: WorldQuery, F: QueryFilter> Default for QueryState<Q, F> {
    fn default() -> Self {
        Self::new()
    }
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
            let has_all = required
                .iter()
                .all(|tid| components.binary_search(tid).is_ok());
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

// ---------------------------------------------------------------------------
// Query2Iter — two-component zero-allocation iterator
// ---------------------------------------------------------------------------

/// Lazy iterator for two-component queries (A, B).
/// Backed by archetype columns, zero allocation.
pub struct Query2Iter<'w, A: Component, B: Component> {
    world: &'w World,
    type_a: TypeId,
    type_b: TypeId,
    archetype_ids: Vec<super::ArchetypeId>,
    current_arch: usize,
    current_row: usize,
    _marker: PhantomData<(A, B)>,
}

impl<'w, A: Component, B: Component> Query2Iter<'w, A, B> {
    pub fn new(world: &'w World) -> Self {
        let type_a = TypeId::of::<A>();
        let type_b = TypeId::of::<B>();
        let matching = world
            .archetype_graph()
            .find_with_components_ids(&[type_a, type_b]);
        Self {
            world,
            type_a,
            type_b,
            archetype_ids: matching,
            current_arch: 0,
            current_row: 0,
            _marker: PhantomData,
        }
    }

    /// Returns the number of entities matching this query.
    pub fn len(&self) -> usize {
        let mut count = 0;
        for &arch_id in &self.archetype_ids {
            if let Some(arch) = self.world.archetype_graph().get(arch_id) {
                if let (Some(&ca), Some(&cb)) = (
                    arch.type_to_column.get(&self.type_a),
                    arch.type_to_column.get(&self.type_b),
                ) {
                    if !arch.columns[ca].is_empty() && !arch.columns[cb].is_empty() {
                        count += arch.entities.len();
                    }
                }
            }
        }
        count
    }

    /// Returns true if the query matches no entities.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'w, A: Component, B: Component> Iterator for Query2Iter<'w, A, B> {
    type Item = (Entity, &'w A, &'w B);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_arch < self.archetype_ids.len() {
            let arch_id = self.archetype_ids[self.current_arch];
            if let Some(arch) = self.world.archetype_graph().get(arch_id) {
                if let (Some(&col_a_idx), Some(&col_b_idx)) = (
                    arch.type_to_column.get(&self.type_a),
                    arch.type_to_column.get(&self.type_b),
                ) {
                    if let (Some(col_a), Some(col_b)) = (
                        arch.columns[col_a_idx]
                            .as_any()
                            .downcast_ref::<TypedColumn<A>>(),
                        arch.columns[col_b_idx]
                            .as_any()
                            .downcast_ref::<TypedColumn<B>>(),
                    ) {
                        if self.current_row < arch.entities.len()
                            && self.current_row < col_a.data.len()
                            && self.current_row < col_b.data.len()
                        {
                            let entity = arch.entities[self.current_row];
                            let item_a = &col_a.data[self.current_row];
                            let item_b = &col_b.data[self.current_row];
                            self.current_row += 1;
                            return Some((entity, item_a, item_b));
                        }
                    }
                }
            }
            self.current_arch += 1;
            self.current_row = 0;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// QueryState — cached query metadata for zero-allocation queries
// ---------------------------------------------------------------------------

/// Cached query state that amortizes archetype matching across frames.
/// Stores matching archetype IDs and column offsets, only re-checking
/// when the archetype graph changes.
pub struct QueryStateCache {
    /// Last seen archetype graph generation (incremented on spawn/despawn)
    pub archetype_generation: u64,
    /// Cached matching archetype IDs
    pub archetype_ids: Vec<super::ArchetypeId>,
    /// Cached column offsets per archetype
    pub column_offsets: Vec<Vec<(super::ArchetypeId, usize)>>,
}

impl QueryStateCache {
    pub fn new() -> Self {
        Self {
            archetype_generation: 0,
            archetype_ids: Vec::new(),
            column_offsets: Vec::new(),
        }
    }

    /// Check if cache is stale and needs rebuild
    pub fn is_stale(&self, world: &World) -> bool {
        self.archetype_generation != world.archetype_graph().generation()
    }
}

impl Default for QueryStateCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// QueryIterSingle — single component lazy iterator
// ---------------------------------------------------------------------------

/// Lazy iterator for single-component queries.
/// Backed by archetype columns, zero allocation.
pub struct QueryIterSingle<'w, T: Component> {
    world: &'w World,
    type_id: TypeId,
    archetype_ids: Vec<super::ArchetypeId>,
    current_arch: usize,
    current_row: usize,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> QueryIterSingle<'w, T> {
    pub fn new(world: &'w World) -> Self {
        let type_id = TypeId::of::<T>();
        let matching = world.archetype_graph().find_with_components_ids(&[type_id]);
        Self {
            world,
            type_id,
            archetype_ids: matching,
            current_arch: 0,
            current_row: 0,
            _marker: PhantomData,
        }
    }

    /// Returns the number of entities matching this query.
    pub fn len(&self) -> usize {
        let mut count = 0;
        for &arch_id in &self.archetype_ids {
            if let Some(arch) = self.world.archetype_graph().get(arch_id) {
                if let Some(&col_idx) = arch.type_to_column.get(&self.type_id) {
                    if !arch.columns[col_idx].is_empty() {
                        count += arch.entities.len();
                    }
                }
            }
        }
        count
    }

    /// Returns true if the query matches no entities.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'w, T: Component> Iterator for QueryIterSingle<'w, T> {
    type Item = (Entity, &'w T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_arch < self.archetype_ids.len() {
            let arch_id = self.archetype_ids[self.current_arch];
            if let Some(arch) = self.world.archetype_graph().get(arch_id) {
                if let Some(&col_idx) = arch.type_to_column.get(&self.type_id) {
                    if let Some(col) = arch.columns[col_idx]
                        .as_any()
                        .downcast_ref::<TypedColumn<T>>()
                    {
                        if self.current_row < arch.entities.len()
                            && self.current_row < col.data.len()
                        {
                            let entity = arch.entities[self.current_row];
                            let item = &col.data[self.current_row];
                            self.current_row += 1;
                            return Some((entity, item));
                        }
                    }
                }
            }
            self.current_arch += 1;
            self.current_row = 0;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Mut<T> — mutable borrow guard with change tracking
// ---------------------------------------------------------------------------

/// A mutable borrow guard that tracks when a component is modified.
/// Automatically marks the column's change tick on drop.
#[allow(dead_code)]
pub struct Mut<'a, T: Component> {
    value: &'a mut T,
    change_tick: &'a mut u64,
    world_tick: u64,
}

impl<'a, T: Component> Mut<'a, T> {
    #[allow(dead_code)]
    pub fn new(value: &'a mut T, change_tick: &'a mut u64, world_tick: u64) -> Self {
        Self {
            value,
            change_tick,
            world_tick,
        }
    }

    /// Get a reference to the component.
    #[allow(dead_code)]
    pub fn get(&self) -> &T {
        self.value
    }

    /// Mark this component as changed. Called automatically on drop if modified.
    #[allow(dead_code)]
    pub fn set_changed(&mut self) {
        *self.change_tick = self.world_tick;
    }
}

impl<'a, T: Component> std::ops::Deref for Mut<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.value
    }
}

impl<'a, T: Component> std::ops::DerefMut for Mut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.set_changed();
        self.value
    }
}

impl<'a, T: Component> Drop for Mut<'a, T> {
    fn drop(&mut self) {
        // Change tick is already updated by deref_mut
    }
}

// ---------------------------------------------------------------------------
// CachedQueryState — amortized archetype matching
// ---------------------------------------------------------------------------

use std::cell::UnsafeCell;

/// Cached query state that avoids re-matching archetypes every frame.
/// Stores the matching archetype IDs and only re-checks when the
/// archetype graph generation changes.
#[allow(dead_code)]
pub struct CachedQueryState<T: Component> {
    archetype_ids: UnsafeCell<Vec<super::ArchetypeId>>,
    cached_generation: UnsafeCell<u64>,
    _marker: PhantomData<T>,
}

impl<T: Component> CachedQueryState<T> {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            archetype_ids: UnsafeCell::new(Vec::new()),
            cached_generation: UnsafeCell::new(0),
            _marker: PhantomData,
        }
    }

    /// Get matching archetypes, rebuilding the cache if stale.
    pub fn archetypes(&self, world: &World) -> &[super::ArchetypeId] {
        let graph = world.archetype_graph();
        let current_gen = graph.generation();

        // SAFETY: Single-threaded access during query iteration
        let cached_gen = unsafe { *self.cached_generation.get() };

        if current_gen != cached_gen {
            // Rebuild cache
            let type_id = TypeId::of::<T>();
            let matching = graph.find_with_components_ids(&[type_id]);

            // SAFETY: Single-threaded access
            unsafe {
                *self.archetype_ids.get() = matching;
                *self.cached_generation.get() = current_gen;
            }
        }

        // SAFETY: Single-threaded access
        unsafe { &*self.archetype_ids.get() }
    }

    /// Create an iterator over matching entities.
    #[allow(dead_code)]
    pub fn iter<'w>(&self, world: &'w World) -> QueryIterSingle<'w, T> {
        let archetype_ids = self.archetypes(world).to_vec();
        QueryIterSingle {
            world,
            type_id: TypeId::of::<T>(),
            archetype_ids,
            current_arch: 0,
            current_row: 0,
            _marker: PhantomData,
        }
    }
}

impl<T: Component> Default for CachedQueryState<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached query state for two-component queries.
#[allow(dead_code)]
pub struct CachedQueryState2<A: Component, B: Component> {
    archetype_ids: UnsafeCell<Vec<super::ArchetypeId>>,
    cached_generation: UnsafeCell<u64>,
    _marker: PhantomData<(A, B)>,
}

impl<A: Component, B: Component> CachedQueryState2<A, B> {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            archetype_ids: UnsafeCell::new(Vec::new()),
            cached_generation: UnsafeCell::new(0),
            _marker: PhantomData,
        }
    }

    /// Get matching archetypes, rebuilding the cache if stale.
    #[allow(dead_code)]
    pub fn archetypes(&self, world: &World) -> &[super::ArchetypeId] {
        let graph = world.archetype_graph();
        let current_gen = graph.generation();

        let cached_gen = unsafe { *self.cached_generation.get() };

        if current_gen != cached_gen {
            let type_a = TypeId::of::<A>();
            let type_b = TypeId::of::<B>();
            let matching = graph.find_with_components_ids(&[type_a, type_b]);

            unsafe {
                *self.archetype_ids.get() = matching;
                *self.cached_generation.get() = current_gen;
            }
        }

        unsafe { &*self.archetype_ids.get() }
    }

    /// Create an iterator over matching entities.
    #[allow(dead_code)]
    pub fn iter<'w>(&self, world: &'w World) -> Query2Iter<'w, A, B> {
        let archetype_ids = self.archetypes(world).to_vec();
        Query2Iter {
            world,
            type_a: TypeId::of::<A>(),
            type_b: TypeId::of::<B>(),
            archetype_ids,
            current_arch: 0,
            current_row: 0,
            _marker: PhantomData,
        }
    }
}

impl<A: Component, B: Component> Default for CachedQueryState2<A, B> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ZeroAllocationQueryIter — truly zero-allocation query iterator
// ---------------------------------------------------------------------------

/// Zero-allocation query iterator that iterates archetypes directly.
///
/// This iterator does not allocate when iterating. It stores a reference
/// to cached archetype IDs and iterates them lazily.
#[allow(dead_code)]
pub struct ZeroAllocQueryIter<'w, T: Component> {
    world: &'w World,
    type_id: TypeId,
    archetype_ids: &'w [super::ArchetypeId],
    current_arch: usize,
    current_row: usize,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> ZeroAllocQueryIter<'w, T> {
    /// Create a zero-allocation iterator from cached archetype IDs.
    pub fn new(world: &'w World, archetype_ids: &'w [super::ArchetypeId]) -> Self {
        Self {
            world,
            type_id: TypeId::of::<T>(),
            archetype_ids,
            current_arch: 0,
            current_row: 0,
            _marker: PhantomData,
        }
    }
}

impl<'w, T: Component> Iterator for ZeroAllocQueryIter<'w, T> {
    type Item = (Entity, &'w T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_arch < self.archetype_ids.len() {
            let arch_id = self.archetype_ids[self.current_arch];
            if let Some(arch) = self.world.archetype_graph().get(arch_id) {
                if let Some(&col_idx) = arch.type_to_column.get(&self.type_id) {
                    if let Some(col) = arch.columns[col_idx]
                        .as_any()
                        .downcast_ref::<TypedColumn<T>>()
                    {
                        if self.current_row < arch.entities.len()
                            && self.current_row < col.data.len()
                        {
                            let entity = arch.entities[self.current_row];
                            let item = &col.data[self.current_row];
                            self.current_row += 1;
                            return Some((entity, item));
                        }
                    }
                }
            }
            self.current_arch += 1;
            self.current_row = 0;
        }
        None
    }
}

/// Zero-allocation two-component query iterator.
#[allow(dead_code)]
pub struct ZeroAllocQuery2Iter<'w, A: Component, B: Component> {
    world: &'w World,
    type_a: TypeId,
    type_b: TypeId,
    archetype_ids: &'w [super::ArchetypeId],
    current_arch: usize,
    current_row: usize,
    _marker: PhantomData<(A, B)>,
}

impl<'w, A: Component, B: Component> ZeroAllocQuery2Iter<'w, A, B> {
    /// Create a zero-allocation iterator from cached archetype IDs.
    pub fn new(world: &'w World, archetype_ids: &'w [super::ArchetypeId]) -> Self {
        Self {
            world,
            type_a: TypeId::of::<A>(),
            type_b: TypeId::of::<B>(),
            archetype_ids,
            current_arch: 0,
            current_row: 0,
            _marker: PhantomData,
        }
    }
}

impl<'w, A: Component, B: Component> Iterator for ZeroAllocQuery2Iter<'w, A, B> {
    type Item = (Entity, &'w A, &'w B);

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_arch < self.archetype_ids.len() {
            let arch_id = self.archetype_ids[self.current_arch];
            if let Some(arch) = self.world.archetype_graph().get(arch_id) {
                if let (Some(&col_a_idx), Some(&col_b_idx)) = (
                    arch.type_to_column.get(&self.type_a),
                    arch.type_to_column.get(&self.type_b),
                ) {
                    if let (Some(col_a), Some(col_b)) = (
                        arch.columns[col_a_idx]
                            .as_any()
                            .downcast_ref::<TypedColumn<A>>(),
                        arch.columns[col_b_idx]
                            .as_any()
                            .downcast_ref::<TypedColumn<B>>(),
                    ) {
                        if self.current_row < arch.entities.len()
                            && self.current_row < col_a.data.len()
                            && self.current_row < col_b.data.len()
                        {
                            let entity = arch.entities[self.current_row];
                            let item_a = &col_a.data[self.current_row];
                            let item_b = &col_b.data[self.current_row];
                            self.current_row += 1;
                            return Some((entity, item_a, item_b));
                        }
                    }
                }
            }
            self.current_arch += 1;
            self.current_row = 0;
        }
        None
    }
}

impl<T: Component> CachedQueryState<T> {
    /// Create a zero-allocation iterator over matching entities.
    ///
    /// Unlike `iter()`, this method does not allocate any heap memory.
    #[allow(dead_code)]
    pub fn iter_zero_alloc<'w>(&'w self, world: &'w World) -> ZeroAllocQueryIter<'w, T> {
        ZeroAllocQueryIter::new(world, self.archetypes(world))
    }
}

impl<A: Component, B: Component> CachedQueryState2<A, B> {
    /// Create a zero-allocation iterator over matching entities.
    ///
    /// Unlike `iter()`, this method does not allocate any heap memory.
    #[allow(dead_code)]
    pub fn iter_zero_alloc<'w>(&'w self, world: &'w World) -> ZeroAllocQuery2Iter<'w, A, B> {
        ZeroAllocQuery2Iter::new(world, self.archetypes(world))
    }
}

// ---------------------------------------------------------------------------
// LazyArchetypeIter — iterates all archetypes without pre-filtering
// ---------------------------------------------------------------------------

/// Lazy iterator that filters archetypes on-the-fly without pre-allocation.
///
/// This is useful when the query is run infrequently or when the set of
/// matching archetypes changes frequently.
pub struct LazyArchetypeIter<'w, T: Component> {
    #[allow(dead_code)]
    world: &'w World,
    type_id: TypeId,
    arch_iter: std::collections::hash_map::Iter<'w, super::ArchetypeId, super::Archetype>,
    current_row: usize,
    current_arch: Option<&'w super::Archetype>,
    current_col: Option<&'w TypedColumn<T>>,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> LazyArchetypeIter<'w, T> {
    /// Create a lazy iterator that filters archetypes on-the-fly.
    pub fn new(world: &'w World) -> Self {
        Self {
            world,
            type_id: TypeId::of::<T>(),
            arch_iter: world.archetype_graph().archetypes_iter(),
            current_row: 0,
            current_arch: None,
            current_col: None,
            _marker: PhantomData,
        }
    }
}

impl<'w, T: Component> Iterator for LazyArchetypeIter<'w, T> {
    type Item = (Entity, &'w T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Try to get next item from current archetype
            if let (Some(arch), Some(col)) = (self.current_arch, self.current_col) {
                if self.current_row < arch.entities.len() && self.current_row < col.data.len() {
                    let entity = arch.entities[self.current_row];
                    let item = &col.data[self.current_row];
                    self.current_row += 1;
                    return Some((entity, item));
                }
            }

            // Move to next archetype
            self.current_row = 0;
            self.current_arch = None;
            self.current_col = None;

            // Find next matching archetype
            for (_id, arch) in &mut self.arch_iter {
                if !arch.signature.contains(&self.type_id) {
                    continue;
                }
                if let Some(&col_idx) = arch.type_to_column.get(&self.type_id) {
                    if let Some(col) = arch.columns[col_idx]
                        .as_any()
                        .downcast_ref::<TypedColumn<T>>()
                    {
                        if !col.data.is_empty() {
                            self.current_arch = Some(arch);
                            self.current_col = Some(col);
                            break;
                        }
                    }
                }
            }

            // If no archetype found, we're done
            if self.current_arch.is_none() {
                return None;
            }
        }
    }
}

// Add method to World for lazy iteration
impl World {
    /// Create a lazy iterator over all entities with component T.
    ///
    /// This iterator does not allocate and filters archetypes on-the-fly.
    /// For hot loops, prefer using `CachedQueryState` with `iter_zero_alloc`.
    pub fn query_iter<T: Component>(&self) -> LazyArchetypeIter<'_, T> {
        LazyArchetypeIter::new(self)
    }
}

#[cfg(test)]
mod zero_alloc_tests {
    use super::*;
    use crate::ecs::World;

    #[derive(Debug, Clone, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Velocity {
        dx: f32,
        dy: f32,
    }

    #[test]
    fn lazy_archetype_iter_single_component() {
        let mut world = World::new();

        // Spawn entities with Position
        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });

        let e2 = world.spawn();
        world.insert(e2, Position { x: 3.0, y: 4.0 });

        // Spawn entity without Position
        let e3 = world.spawn();
        world.insert(e3, Velocity { dx: 1.0, dy: 0.0 });

        let mut count = 0;
        for (_entity, _pos) in world.query_iter::<Position>() {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn lazy_archetype_iter_empty_world() {
        let world = World::new();
        let mut count = 0;
        for (_entity, _pos) in world.query_iter::<Position>() {
            count += 1;
        }
        assert_eq!(count, 0);
    }

    #[test]
    fn zero_alloc_query_iter_single_component() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 10.0, y: 20.0 });

        let e2 = world.spawn();
        world.insert(e2, Position { x: 30.0, y: 40.0 });

        let cache = CachedQueryState::<Position>::new();
        let iter = cache.iter_zero_alloc(&world);

        let positions: Vec<_> = iter.map(|(_, p)| (p.x, p.y)).collect();
        assert_eq!(positions.len(), 2);
        assert!(positions.contains(&(10.0, 20.0)));
        assert!(positions.contains(&(30.0, 40.0)));
    }

    #[test]
    fn zero_alloc_query_iter_two_components() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });
        world.insert(e1, Velocity { dx: 0.1, dy: 0.2 });

        let e2 = world.spawn();
        world.insert(e2, Position { x: 3.0, y: 4.0 });
        world.insert(e2, Velocity { dx: 0.3, dy: 0.4 });

        // Entity with only Position (should not match)
        let e3 = world.spawn();
        world.insert(e3, Position { x: 5.0, y: 6.0 });

        let cache = CachedQueryState2::<Position, Velocity>::new();
        let iter = cache.iter_zero_alloc(&world);

        let results: Vec<_> = iter.collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn cached_query_state_updates_on_new_archetype() {
        let mut world = World::new();
        let cache = CachedQueryState::<Position>::new();

        // Initial: empty
        let archetypes = cache.archetypes(&world);
        assert_eq!(archetypes.len(), 0);

        // Add entity with Position
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 1.0 });

        // Cache should update
        let archetypes = cache.archetypes(&world);
        assert!(archetypes.len() >= 1);
    }

    #[test]
    fn lazy_iter_after_despawn() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 1.0 });

        let e2 = world.spawn();
        world.insert(e2, Position { x: 2.0, y: 2.0 });

        // Despawn one
        world.despawn(e1);

        let positions: Vec<_> = world.query_iter::<Position>().collect();
        assert_eq!(positions.len(), 1);
    }
}
