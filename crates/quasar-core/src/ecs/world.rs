//! World — the central data store for all entities and components.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use super::archetype::{ArchetypeGraph, ArchetypeId, ArchetypeSignature, TypedColumn};
use super::component::{Component, ComponentStorage, TypedStorage};
use super::entity::{Entity, EntityAllocator};
use parking_lot::RwLock;

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
/// world.insert(player, 100_u32); // health
/// world.insert(player, "Hero"); // name tag
///
/// assert_eq!(world.get::<u32>(player), Some(&100));
/// ```
pub struct World {
    allocator: EntityAllocator,
    /// Legacy HashMap storage (kept for backward compatibility)
    storages: HashMap<TypeId, Box<dyn ComponentStorage>>,
    resources: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// Archetype-based storage for high-performance queries
    archetype_graph: RwLock<ArchetypeGraph>,
    /// Maps entity index to its archetype ID
    entity_archetype: HashMap<u32, ArchetypeId>,
    /// Maps entity index to its row within the archetype
    entity_row: HashMap<u32, usize>,
    /// Track which components each entity has (for archetype migration)
    entity_components: HashMap<u32, Vec<TypeId>>,
    /// Per-type removal log: tracks entity indices that had a component removed this frame.
    /// Cleared once per frame via `clear_removal_log()`.
    removal_log: HashMap<TypeId, Vec<u32>>,
}

impl World {
    /// Create an empty world.
    pub fn new() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            storages: HashMap::new(),
            resources: HashMap::new(),
            archetype_graph: RwLock::new(ArchetypeGraph::new()),
            entity_archetype: HashMap::new(),
            entity_row: HashMap::new(),
            entity_components: HashMap::new(),
            removal_log: HashMap::new(),
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
    /// Uses swap-and-pop removal from archetype storage and logs removals
    /// for `Removed<T>` query filters.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.allocator.deallocate(entity) {
            return false;
        }

        // Log removals for all components this entity had
        if let Some(components) = self.entity_components.get(&entity.index()) {
            for &type_id in components.iter() {
                self.removal_log.entry(type_id).or_default().push(entity.index());
            }
        }

        // Remove from archetype storage (swap-and-pop)
        if let Some(arch_id) = self.entity_archetype.remove(&entity.index()) {
            let graph = self.archetype_graph.get_mut();
            if let Some(arch) = graph.get_mut(arch_id) {
                if let Some(swapped_row) = arch.remove_entity(entity) {
                    // If a swap occurred, update the moved entity's row mapping
                    if swapped_row < arch.entities.len() {
                        let swapped_entity = arch.entities[swapped_row];
                        self.entity_row.insert(swapped_entity.index(), swapped_row);
                    }
                }
            }
        }

        self.entity_row.remove(&entity.index());
        self.entity_components.remove(&entity.index());

        // Also remove from legacy storage
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
    ///
    /// Populates both archetype SoA columns (for fast queries) and legacy
    /// HashMap storage (for backward-compatible `get`/`get_mut` access).
    pub fn insert<T: Component + Clone>(&mut self, entity: Entity, component: T) {
        debug_assert!(self.is_alive(entity), "inserting on a dead entity");

        let type_id = TypeId::of::<T>();

        // Track component for this entity
        let components = self.entity_components.entry(entity.index()).or_default();
        if !components.contains(&type_id) {
            components.push(type_id);
            components.sort();
        }

        // Get or create archetype for this component set
        let mut sig = ArchetypeSignature::new();
        for tid in components.iter() {
            sig.add(*tid);
        }

        // Check if entity needs to migrate to new archetype
        let old_arch_id = self.entity_archetype.get(&entity.index()).copied();
        let needs_migration =
            if let Some(current_arch_id) = old_arch_id {
                let graph = self.archetype_graph.read();
                if let Some(current_arch) = graph.get(current_arch_id) {
                    !current_arch.signature.contains(&type_id)
                } else {
                    true
                }
            } else {
                true
            };

        if needs_migration {
            let graph = self.archetype_graph.get_mut();
            // get_or_create returns an Arc clone — extract only the id, then drop
            // so the Arc refcount goes back to 1 (only the HashMap holds it).
            let new_arch_id = {
                let arc = graph.get_or_create(&sig);
                let id = arc.id;
                drop(arc);
                id
            };

            // ── Phase 1: extract SoA data from old archetype (if any) ──
            let extracted = if let Some(old_id) = old_arch_id {
                if let Some(old_arch) = graph.get_mut(old_id) {
                    if let Some((_freed_row, data)) =
                        old_arch.remove_entity_extract_soa(entity)
                    {
                        // Update entity_row for the entity that was swapped
                        // into the freed row (if any).
                        if _freed_row < old_arch.entities.len() {
                            let swapped = old_arch.entities[_freed_row];
                            self.entity_row.insert(swapped.index(), _freed_row);
                        }
                        data
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            // ── Phase 2: push into new archetype ──
            if let Some(new_arch) = graph.get_mut(new_arch_id) {
                let row = new_arch.add_entity(entity);
                self.entity_archetype.insert(entity.index(), new_arch_id);
                self.entity_row.insert(entity.index(), row);

                // Re-insert extracted columns from old archetype
                for (tid, value, empty_col) in extracted {
                    if !new_arch.type_to_column.contains_key(&tid) {
                        let idx = new_arch.columns.len();
                        new_arch.column_types.push(tid);
                        new_arch.columns.push(empty_col);
                        new_arch.type_to_column.insert(tid, idx);
                    }
                    let ci: usize = *new_arch.type_to_column.get(&tid).unwrap();
                    (*new_arch.columns[ci]).push_raw(value);
                }

                // Push the NEW component T into its SoA column
                new_arch.ensure_column::<T>();
                let ci: usize = *new_arch.type_to_column.get(&type_id).unwrap();
                if let Some(col) = (*new_arch.columns[ci])
                    .as_any_mut()
                    .downcast_mut::<TypedColumn<T>>()
                {
                    col.data.push(component.clone());
                }
            } else {
                // Fallback: just record the archetype mapping
                self.entity_archetype.insert(entity.index(), new_arch_id);
                self.entity_row.insert(entity.index(), 0);
            }
        } else {
            // No migration — replace the existing value at the entity's row.
            if let Some(&arch_id) = self.entity_archetype.get(&entity.index()) {
                let graph = self.archetype_graph.get_mut();
                if let Some(arch) = graph.get_mut(arch_id) {
                    arch.ensure_column::<T>();
                    if let Some(&col_idx) = arch.type_to_column.get(&type_id) {
                        let ci: usize = col_idx;
                        if let Some(col) = (*arch.columns[ci])
                            .as_any_mut()
                            .downcast_mut::<TypedColumn<T>>()
                        {
                            if let Some(&row) = arch.entity_to_row.get(&entity.index()) {
                                if row < col.data.len() {
                                    col.data[row] = component.clone();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also store in legacy storage (keeps fallback path working)
        self.storage_mut::<T>().insert(entity, component);
    }

    /// Remove a component from an entity, returning `true` if it existed.
    pub fn remove_component<T: Component>(&mut self, entity: Entity) -> bool {
        let type_id = TypeId::of::<T>();

        // Update entity's component list
        if let Some(components) = self.entity_components.get_mut(&entity.index()) {
            if let Ok(pos) = components.binary_search(&type_id) {
                components.remove(pos);
            }
        }

        // Remove from legacy storage
        let removed = if let Some(storage) = self.storages.get_mut(&type_id) {
            storage.remove(entity)
        } else {
            false
        };

        // Log removal for Removed<T> query filter
        if removed {
            self.removal_log.entry(type_id).or_default().push(entity.index());
        }
        removed
    }

    /// Get a shared reference to a component on an entity.
    /// Uses archetype metadata for a fast existence check before storage lookup.
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let type_id = TypeId::of::<T>();
        // Fast path: check archetype metadata to see if this entity has the component.
        if let Some(components) = self.entity_components.get(&entity.index()) {
            if components.binary_search(&type_id).is_err() {
                return None;
            }
        }
        self.storage::<T>()?.get(entity)
    }

    /// Get a mutable reference to a component on an entity.
    /// Uses legacy storage (primary storage for now).
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

    /// Advance the change-detection tick for all storages.
    /// Call once per frame before running systems.
    pub fn tick_change_detection(&mut self) {
        for storage in self.storages.values_mut() {
            storage.tick();
        }
    }

    /// Return the current change-detection tick for storage of type `T`.
    pub fn current_tick<T: Component>(&self) -> u64 {
        self.storage::<T>().map_or(0, |s| s.current_tick)
    }

    /// Iterate over all `(Entity, &T)` pairs.
    /// Uses archetype SoA columns first for cache-optimal iteration,
    /// falls back to legacy HashMap storage.
    pub fn query<T: Component>(&self) -> Vec<(Entity, &T)> {
        let type_id = TypeId::of::<T>();

        // Try SoA fast path via archetype columns.
        {
            let graph = self.archetype_graph.read();
            let matching = graph.find_with_components(&[type_id]);
            if !matching.is_empty() {
                let mut results = Vec::new();
                for arch in &matching {
                    let n = arch.column_len::<T>();
                    if n == 0 { continue; }
                    unsafe {
                        if let Some(ptr) = arch.column_ptr::<T>() {
                            let slice = std::slice::from_raw_parts(ptr, n);
                            for (i, val) in slice.iter().enumerate() {
                                if i < arch.entities.len() {
                                    results.push((arch.entities[i], val));
                                }
                            }
                        }
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }

        // Fallback: legacy HashMap storage.
        let storage = match self
            .storages
            .get(&type_id)
            .and_then(|s| s.as_any().downcast_ref::<TypedStorage<T>>())
        {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut results = Vec::with_capacity(storage.data.len());

        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_id).is_ok() {
                if let Some(component) = storage.data.get(&idx) {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), component));
                }
            }
        }

        results
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
    /// Fast path: if archetypes have SoA columns populated for both types,
    /// iterate via raw `column_ptr<T>()` slices — cache-optimal with no
    /// indirection. Falls back to legacy HashMap storage otherwise.
    pub fn query2<A: Component, B: Component>(&self) -> Vec<(Entity, &A, &B)> {
        let type_a = TypeId::of::<A>();
        let type_b = TypeId::of::<B>();

        // Try SoA fast path via archetype columns.
        {
            let graph = self.archetype_graph.read();
            let matching = graph.find_with_components(&[type_a, type_b]);
            if !matching.is_empty() {
                let mut results = Vec::new();
                for arch in &matching {
                    let n = arch.column_len::<A>();
                    if n == 0 || arch.column_len::<B>() == 0 {
                        continue;
                    }
                    // SAFETY: we hold an immutable borrow on the archetype graph
                    // via the read lock, and do not mutate archetype storage while
                    // the returned references are alive.
                    unsafe {
                        if let (Some(ptr_a), Some(ptr_b)) =
                            (arch.column_ptr::<A>(), arch.column_ptr::<B>())
                        {
                            let slice_a = std::slice::from_raw_parts(ptr_a, n);
                            let slice_b = std::slice::from_raw_parts(ptr_b, n);
                            for (i, (a, b)) in slice_a.iter().zip(slice_b.iter()).enumerate() {
                                if i < arch.entities.len() {
                                    results.push((arch.entities[i], a, b));
                                }
                            }
                        }
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }

        // Fallback: legacy HashMap storage path.
        let sa = self.storage::<A>();
        let sb = self.storage::<B>();

        if sa.is_none() || sb.is_none() {
            return Vec::new();
        }
        let sa = sa.unwrap();
        let sb = sb.unwrap();

        let mut results = Vec::new();

        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_a).is_ok()
                && components.binary_search(&type_b).is_ok()
            {
                if let (Some(a), Some(b)) = (sa.data.get(&idx), sb.data.get(&idx)) {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), a, b));
                }
            }
        }

        results
    }

    /// Query entities that have components `A`, `B`, **and** `C`.
    ///
    /// Fast path: if archetypes have SoA columns populated for all three
    /// types, iterate via raw `column_ptr<T>()` slices. Falls back to
    /// legacy HashMap storage otherwise.
    pub fn query3<A: Component, B: Component, C: Component>(
        &self,
    ) -> Vec<(Entity, &A, &B, &C)> {
        let type_a = TypeId::of::<A>();
        let type_b = TypeId::of::<B>();
        let type_c = TypeId::of::<C>();

        // Try SoA fast path via archetype columns.
        {
            let graph = self.archetype_graph.read();
            let matching = graph.find_with_components(&[type_a, type_b, type_c]);
            if !matching.is_empty() {
                let mut results = Vec::new();
                for arch in &matching {
                    let n = arch.column_len::<A>();
                    if n == 0 {
                        continue;
                    }
                    unsafe {
                        if let (Some(pa), Some(pb), Some(pc)) = (
                            arch.column_ptr::<A>(),
                            arch.column_ptr::<B>(),
                            arch.column_ptr::<C>(),
                        ) {
                            let sa = std::slice::from_raw_parts(pa, n);
                            let sb = std::slice::from_raw_parts(pb, n);
                            let sc = std::slice::from_raw_parts(pc, n);
                            for (i, ((a, b), c)) in
                                sa.iter().zip(sb).zip(sc).enumerate()
                            {
                                if i < arch.entities.len() {
                                    results.push((arch.entities[i], a, b, c));
                                }
                            }
                        }
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }

        // Fallback: legacy HashMap path.
        let sa = self.storage::<A>();
        let sb = self.storage::<B>();
        let sc = self.storage::<C>();

        if sa.is_none() || sb.is_none() || sc.is_none() {
            return Vec::new();
        }
        let sa = sa.unwrap();
        let sb = sb.unwrap();
        let sc = sc.unwrap();

        let mut results = Vec::new();

        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_a).is_ok()
                && components.binary_search(&type_b).is_ok()
                && components.binary_search(&type_c).is_ok()
            {
                if let (Some(a), Some(b), Some(c)) =
                    (sa.data.get(&idx), sb.data.get(&idx), sc.data.get(&idx))
                {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), a, b, c));
                }
            }
        }

        results
    }

    /// Query entities that have components `A`, `B`, `C`, **and** `D`.
    ///
    /// Fast path: archetypes with SoA columns for all four types.
    /// Falls back to HashMap storage otherwise.
    pub fn query4<A: Component, B: Component, C: Component, D: Component>(
        &self,
    ) -> Vec<(Entity, &A, &B, &C, &D)> {
        let tids = [
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            TypeId::of::<C>(),
            TypeId::of::<D>(),
        ];

        // Try SoA fast path via archetype columns.
        {
            let graph = self.archetype_graph.read();
            let matching = graph.find_with_components(&tids);
            if !matching.is_empty() {
                let mut results = Vec::new();
                for arch in &matching {
                    let n = arch.column_len::<A>();
                    if n == 0 { continue; }
                    unsafe {
                        if let (Some(pa), Some(pb), Some(pc), Some(pd)) = (
                            arch.column_ptr::<A>(),
                            arch.column_ptr::<B>(),
                            arch.column_ptr::<C>(),
                            arch.column_ptr::<D>(),
                        ) {
                            let sa = std::slice::from_raw_parts(pa, n);
                            let sb = std::slice::from_raw_parts(pb, n);
                            let sc = std::slice::from_raw_parts(pc, n);
                            let sd = std::slice::from_raw_parts(pd, n);
                            for i in 0..n.min(arch.entities.len()) {
                                results.push((arch.entities[i], &sa[i], &sb[i], &sc[i], &sd[i]));
                            }
                        }
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }

        // Fallback: legacy HashMap storage.
        let sa = self.storage::<A>();
        let sb = self.storage::<B>();
        let sc = self.storage::<C>();
        let sd = self.storage::<D>();

        if sa.is_none() || sb.is_none() || sc.is_none() || sd.is_none() {
            return Vec::new();
        }
        let (sa, sb, sc, sd) = (sa.unwrap(), sb.unwrap(), sc.unwrap(), sd.unwrap());

        let mut results = Vec::new();
        for (&idx, components) in &self.entity_components {
            if tids.iter().all(|tid| components.binary_search(tid).is_ok()) {
                if let (Some(a), Some(b), Some(c), Some(d)) = (
                    sa.data.get(&idx),
                    sb.data.get(&idx),
                    sc.data.get(&idx),
                    sd.data.get(&idx),
                ) {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), a, b, c, d));
                }
            }
        }
        results
    }

    /// Query entities that have components `A` through `E`.
    ///
    /// Fast path: archetypes with SoA columns for all five types.
    /// Falls back to HashMap storage otherwise.
    pub fn query5<A: Component, B: Component, C: Component, D: Component, E: Component>(
        &self,
    ) -> Vec<(Entity, &A, &B, &C, &D, &E)> {
        let tids = [
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            TypeId::of::<C>(),
            TypeId::of::<D>(),
            TypeId::of::<E>(),
        ];

        // Try SoA fast path via archetype columns.
        {
            let graph = self.archetype_graph.read();
            let matching = graph.find_with_components(&tids);
            if !matching.is_empty() {
                let mut results = Vec::new();
                for arch in &matching {
                    let n = arch.column_len::<A>();
                    if n == 0 { continue; }
                    unsafe {
                        if let (Some(pa), Some(pb), Some(pc), Some(pd), Some(pe)) = (
                            arch.column_ptr::<A>(),
                            arch.column_ptr::<B>(),
                            arch.column_ptr::<C>(),
                            arch.column_ptr::<D>(),
                            arch.column_ptr::<E>(),
                        ) {
                            let sa = std::slice::from_raw_parts(pa, n);
                            let sb = std::slice::from_raw_parts(pb, n);
                            let sc = std::slice::from_raw_parts(pc, n);
                            let sd = std::slice::from_raw_parts(pd, n);
                            let se = std::slice::from_raw_parts(pe, n);
                            for i in 0..n.min(arch.entities.len()) {
                                results.push((arch.entities[i], &sa[i], &sb[i], &sc[i], &sd[i], &se[i]));
                            }
                        }
                    }
                }
                if !results.is_empty() {
                    return results;
                }
            }
        }

        // Fallback: legacy HashMap storage.
        let sa = self.storage::<A>();
        let sb = self.storage::<B>();
        let sc = self.storage::<C>();
        let sd = self.storage::<D>();
        let se = self.storage::<E>();

        if sa.is_none() || sb.is_none() || sc.is_none() || sd.is_none() || se.is_none() {
            return Vec::new();
        }
        let (sa, sb, sc, sd, se) =
            (sa.unwrap(), sb.unwrap(), sc.unwrap(), sd.unwrap(), se.unwrap());

        let mut results = Vec::new();
        for (&idx, components) in &self.entity_components {
            if tids.iter().all(|tid| components.binary_search(tid).is_ok()) {
                if let (Some(a), Some(b), Some(c), Some(d), Some(e)) = (
                    sa.data.get(&idx),
                    sb.data.get(&idx),
                    sc.data.get(&idx),
                    sd.data.get(&idx),
                    se.data.get(&idx),
                ) {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), a, b, c, d, e));
                }
            }
        }
        results
    }

    // ------------------------------------------------------------------
    // Change-detection queries
    // ------------------------------------------------------------------

    /// Iterate over all `(Entity, &T)` where `T` was changed since `since_tick`.
    ///
    /// A component is considered "changed" if it was inserted or mutably
    /// accessed via `get_mut` since the given tick.
    pub fn query_changed<T: Component>(&self, since_tick: u64) -> Vec<(Entity, &T)> {
        let type_id = TypeId::of::<T>();

        let storage = match self
            .storages
            .get(&type_id)
            .and_then(|s| s.as_any().downcast_ref::<TypedStorage<T>>())
        {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut results = Vec::new();

        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_id).is_ok() {
                if storage.changed_since(idx, since_tick) {
                    if let Some(component) = storage.data.get(&idx) {
                        let generation = self.allocator.generation_of(idx);
                        results.push((Entity::new(idx, generation), component));
                    }
                }
            }
        }

        results
    }

    // ------------------------------------------------------------------
    // Filtered queries (With / Without / Optional)
    // ------------------------------------------------------------------

    /// Query entities with component `T` that also have component `W`.
    pub fn query_with<T: Component, W: Component>(&self) -> Vec<(Entity, &T)> {
        let type_t = TypeId::of::<T>();
        let type_w = TypeId::of::<W>();

        let storage = match self.storage::<T>() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut results = Vec::new();
        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_t).is_ok()
                && components.binary_search(&type_w).is_ok()
            {
                if let Some(component) = storage.data.get(&idx) {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), component));
                }
            }
        }
        results
    }

    /// Query entities with component `T` that do NOT have component `W`.
    pub fn query_without<T: Component, W: Component>(&self) -> Vec<(Entity, &T)> {
        let type_t = TypeId::of::<T>();
        let type_w = TypeId::of::<W>();

        let storage = match self.storage::<T>() {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut results = Vec::new();
        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_t).is_ok()
                && components.binary_search(&type_w).is_err()
            {
                if let Some(component) = storage.data.get(&idx) {
                    let generation = self.allocator.generation_of(idx);
                    results.push((Entity::new(idx, generation), component));
                }
            }
        }
        results
    }

    /// Query entities for component `T`, returning `Option<&U>` for an
    /// optional second component.
    pub fn query_optional<T: Component, U: Component>(
        &self,
    ) -> Vec<(Entity, &T, Option<&U>)> {
        let type_t = TypeId::of::<T>();

        let storage_t = match self.storage::<T>() {
            Some(s) => s,
            None => return Vec::new(),
        };
        let storage_u = self.storage::<U>();

        let mut results = Vec::new();
        for (&idx, components) in &self.entity_components {
            if components.binary_search(&type_t).is_ok() {
                if let Some(t_comp) = storage_t.data.get(&idx) {
                    let generation = self.allocator.generation_of(idx);
                    let u_comp = storage_u.and_then(|su| su.data.get(&idx));
                    results.push((Entity::new(idx, generation), t_comp, u_comp));
                }
            }
        }
        results
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// Get or create the typed storage for `T`.
    pub(crate) fn storage_mut<T: Component>(&mut self) -> &mut TypedStorage<T> {
        let type_id = TypeId::of::<T>();
        self.storages
            .entry(type_id)
            .or_insert_with(|| Box::new(TypedStorage::<T>::new()))
            .as_any_mut()
            .downcast_mut::<TypedStorage<T>>()
            .expect("type mismatch in component storage")
    }

    /// Get an existing typed storage for `T` (read-only).
    pub(crate) fn storage<T: Component>(&self) -> Option<&TypedStorage<T>> {
        let type_id = TypeId::of::<T>();
        self.storages
            .get(&type_id)?
            .as_any()
            .downcast_ref::<TypedStorage<T>>()
    }

    /// Get the component type list for an entity (for query filters).
    pub fn entity_component_list(&self, entity_index: u32) -> Option<&Vec<TypeId>> {
        self.entity_components.get(&entity_index)
    }

    /// Iterate over (entity_index, component_list) pairs.
    pub fn entity_components_iter(&self) -> impl Iterator<Item = (&u32, &Vec<TypeId>)> {
        self.entity_components.iter()
    }

    /// Get the generation for a given entity index.
    pub fn generation_of(&self, index: u32) -> u32 {
        self.allocator.generation_of(index)
    }

    /// Check if a component of type T was removed for a given entity index.
    pub fn was_removed<T: Component>(&self, entity_index: u32) -> bool {
        if let Some(removals) = self.removal_log.get(&TypeId::of::<T>()) {
            removals.contains(&entity_index)
        } else {
            false
        }
    }

    /// Clear the removal log. Should be called once per frame after systems run.
    pub fn clear_removal_log(&mut self) {
        self.removal_log.clear();
    }

    /// Insert a component using runtime type information (for Commands).
    pub fn insert_raw(
        &mut self,
        entity: Entity,
        type_id: TypeId,
        component: Box<dyn std::any::Any + Send + Sync>,
    ) {
        debug_assert!(self.is_alive(entity), "inserting on a dead entity");

        // Track component for this entity
        let components = self.entity_components.entry(entity.index()).or_default();
        if !components.contains(&type_id) {
            components.push(type_id);
            components.sort();
        }

        // We need to find or create storage, but we don't know the type.
        // For now, we'll need the caller to ensure storage exists.
        if let Some(storage) = self.storages.get_mut(&type_id) {
            storage.insert_raw(entity, component);
        } else {
            log::warn!("insert_raw: no storage for type, component dropped");
        }
    }

    /// Remove a component using runtime type information (for Commands).
    pub fn remove_raw(&mut self, entity: Entity, type_id: TypeId) -> bool {
        // Update entity's component list
        if let Some(components) = self.entity_components.get_mut(&entity.index()) {
            if let Ok(pos) = components.binary_search(&type_id) {
                components.remove(pos);
                // Log removal
                self.removal_log.entry(type_id).or_default().push(entity.index());
            }
        }

        if let Some(storage) = self.storages.get_mut(&type_id) {
            storage.remove(entity)
        } else {
            false
        }
    }

    /// Insert a resource using runtime type information (for Commands).
    pub fn insert_resource_raw(
        &mut self,
        type_id: TypeId,
        resource: Box<dyn std::any::Any + Send + Sync>,
    ) {
        self.resources.insert(type_id, resource);
    }

    /// Remove a resource using runtime type information (for Commands).
    pub fn remove_resource_raw(&mut self, type_id: TypeId) {
        self.resources.remove(&type_id);
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
    pub fn with<T: Component + Clone>(self, component: T) -> Self {
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

    #[derive(Debug, Clone, PartialEq)]
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

        let positions = world.query::<Position>();
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

        let results = world.query2::<Position, Velocity>();
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

        let results = world.query3::<Position, Velocity, Health>();
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

        let results: Vec<_> = query!(world, (Position, Velocity)).into_iter().collect();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_macro_simple_syntax() {
        let mut world = World::new();

        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 2.0 });
        world.insert(e1, Velocity { dx: 3.0, dy: 4.0 });
        world.insert(e1, Health(100));

        let results: Vec<_> = query!(world, Position, Velocity, Health).into_iter().collect();
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

    // ------------------------------------------------------------------
    // Comprehensive ECS unit tests
    // ------------------------------------------------------------------

    #[test]
    fn spawn_multiple_and_count() {
        let mut world = World::new();
        for _ in 0..10 {
            world.spawn();
        }
        assert_eq!(world.entity_count(), 10);
    }

    #[test]
    fn despawn_reduces_count() {
        let mut world = World::new();
        let e1 = world.spawn();
        let e2 = world.spawn();
        let e3 = world.spawn();
        assert_eq!(world.entity_count(), 3);
        world.despawn(e2);
        assert_eq!(world.entity_count(), 2);
        assert!(world.is_alive(e1));
        assert!(!world.is_alive(e2));
        assert!(world.is_alive(e3));
    }

    #[test]
    fn despawn_cleans_all_components() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 2.0 });
        world.insert(e, Velocity { dx: 3.0, dy: 4.0 });
        world.insert(e, Health(100));
        world.despawn(e);
        // After despawn, no query should return the entity.
        assert_eq!(world.query::<Position>().len(), 0);
        assert_eq!(world.query::<Velocity>().len(), 0);
        assert_eq!(world.query::<Health>().len(), 0);
    }

    #[test]
    fn insert_overwrite_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100));
        world.insert(e, Health(50));
        assert_eq!(world.get::<Health>(e), Some(&Health(50)));
    }

    #[test]
    fn remove_component_leaves_others() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 2.0 });
        world.insert(e, Health(100));
        assert!(world.remove_component::<Health>(e));
        assert!(world.get::<Health>(e).is_none());
        assert_eq!(world.get::<Position>(e), Some(&Position { x: 1.0, y: 2.0 }));
    }

    #[test]
    fn has_component_check() {
        let mut world = World::new();
        let e = world.spawn();
        assert!(!world.has::<Position>(e));
        world.insert(e, Position { x: 0.0, y: 0.0 });
        assert!(world.has::<Position>(e));
    }

    #[test]
    fn get_mut_modifies_in_place() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100));
        if let Some(h) = world.get_mut::<Health>(e) {
            h.0 += 50;
        }
        assert_eq!(world.get::<Health>(e), Some(&Health(150)));
    }

    #[test]
    fn commands_spawn_and_apply() {
        let mut world = World::new();
        let mut cmds = crate::ecs::Commands::new();
        cmds.spawn().with(Position { x: 5.0, y: 6.0 }).id();
        cmds.spawn().with(Health(200)).id();
        assert_eq!(world.entity_count(), 0);
        cmds.apply(&mut world);
        assert_eq!(world.entity_count(), 2);
    }

    #[test]
    fn commands_despawn_applies() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(1));
        let mut cmds = crate::ecs::Commands::new();
        cmds.despawn(e);
        cmds.apply(&mut world);
        assert!(!world.is_alive(e));
    }

    #[test]
    fn query_with_filter() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 0.0 });
        world.insert(e1, Health(10));
        let e2 = world.spawn();
        world.insert(e2, Position { x: 2.0, y: 0.0 });
        // e2 has Position but no Health
        let results = world.query_with::<Position, Health>();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, &Position { x: 1.0, y: 0.0 });
    }

    #[test]
    fn query_without_filter() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 0.0 });
        world.insert(e1, Health(10));
        let e2 = world.spawn();
        world.insert(e2, Position { x: 2.0, y: 0.0 });
        let results = world.query_without::<Position, Health>();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, &Position { x: 2.0, y: 0.0 });
    }

    #[test]
    fn query_optional_returns_both() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 0.0 });
        world.insert(e1, Health(10));
        let e2 = world.spawn();
        world.insert(e2, Position { x: 2.0, y: 0.0 });
        let results = world.query_optional::<Position, Health>();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn change_detection_tracks_mutations() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100));
        let tick_before = world.current_tick::<Health>();
        world.tick_change_detection();
        // No mutation yet — unchanged since tick_before
        let changed = world.query_changed::<Health>(tick_before);
        assert_eq!(changed.len(), 0);
        // Mutate
        if let Some(h) = world.get_mut::<Health>(e) {
            h.0 = 50;
        }
        let changed = world.query_changed::<Health>(tick_before);
        assert_eq!(changed.len(), 1);
    }

    #[test]
    fn typed_query_state_single_component() {
        use crate::ecs::query::QueryState;
        let mut world = World::new();
        for i in 0..3 {
            let e = world.spawn();
            world.insert(e, Position { x: i as f32, y: 0.0 });
        }
        let qs = QueryState::<&Position>::new();
        let results: Vec<_> = qs.iter(&world).collect();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn typed_query_state_two_components() {
        use crate::ecs::query::QueryState;
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Position { x: 1.0, y: 0.0 });
        world.insert(e1, Velocity { dx: 2.0, dy: 0.0 });
        let e2 = world.spawn();
        world.insert(e2, Position { x: 3.0, y: 0.0 });
        let qs = QueryState::<(&Position, &Velocity)>::new();
        let results: Vec<_> = qs.iter(&world).collect();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn removal_log_tracked() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100));
        world.remove_component::<Health>(e);
        assert!(world.was_removed::<Health>(e.index()));
        world.clear_removal_log();
        assert!(!world.was_removed::<Health>(e.index()));
    }

    #[test]
    fn generational_entity_reuse() {
        let mut world = World::new();
        let e1 = world.spawn();
        let idx = e1.index();
        world.despawn(e1);
        let e2 = world.spawn();
        assert_eq!(e2.index(), idx);
        assert_ne!(e1.generation(), e2.generation());
        assert!(!world.is_alive(e1));
        assert!(world.is_alive(e2));
    }

    #[test]
    fn soa_query2_uses_fast_path() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 2.0 });
        world.insert(e, Velocity { dx: 3.0, dy: 4.0 });

        // After insert, archetype SoA columns should be populated.
        {
            let graph = world.archetype_graph.read();
            let matching = graph.find_with_components(&[
                std::any::TypeId::of::<Position>(),
                std::any::TypeId::of::<Velocity>(),
            ]);
            assert!(!matching.is_empty(), "should find a matching archetype");
            let arch = matching[0];
            assert_eq!(
                arch.column_len::<Position>(),
                1,
                "SoA column for Position must have 1 entry"
            );
            assert_eq!(
                arch.column_len::<Velocity>(),
                1,
                "SoA column for Velocity must have 1 entry"
            );
        }

        // query2 should return data from the SoA fast path.
        let results = world.query2::<Position, Velocity>();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, &Position { x: 1.0, y: 2.0 });
        assert_eq!(results[0].2, &Velocity { dx: 3.0, dy: 4.0 });
    }
}
