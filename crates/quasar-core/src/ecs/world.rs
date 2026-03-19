//! World — the central data store for all entities and components.

#![allow(clippy::type_complexity)]

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use rustc_hash::FxHashMap;

use super::archetype::{
    ArchetypeGraph, ArchetypeId, ArchetypeSignature, ColumnStorage, TypedColumn,
};
use super::component::Component;
use super::entity::{Entity, EntityAllocator};
use super::relation::{ChildOf, Relation, RelationGraph};
use super::sparse_set::SparseSetStorage;

use smallvec::SmallVec;

/// Parent component — stores the parent entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent(pub Entity);

/// Children component — stores child entities inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Children(pub SmallVec<[Entity; 4]>);

/// Create a typed SoA column factory for type T.
fn create_typed_column<T: 'static + Send + Sync>() -> Box<dyn ColumnStorage> {
    Box::new(TypedColumn::<T>::new())
}

// ── World Observers ─────────────────────────────────────────────

/// Kinds of component lifecycle events that can be observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObserverKind {
    /// Fires when a component is added to an entity.
    OnAdd,
    /// Fires when a component is removed from an entity (including despawn).
    OnRemove,
}

/// Marker type for `world.observe::<OnAdd<T>>(...)`.
pub struct OnAdd<T: 'static>(std::marker::PhantomData<T>);

/// Marker type for `world.observe::<OnRemove<T>>(...)`.
pub struct OnRemove<T: 'static>(std::marker::PhantomData<T>);

/// Trait implemented by `OnAdd<T>` and `OnRemove<T>` to extract the event kind and component type.
pub trait ObserverEvent {
    fn kind() -> ObserverKind;
    fn component_type_id() -> TypeId;
}

impl<T: 'static> ObserverEvent for OnAdd<T> {
    fn kind() -> ObserverKind {
        ObserverKind::OnAdd
    }
    fn component_type_id() -> TypeId {
        TypeId::of::<T>()
    }
}

impl<T: 'static> ObserverEvent for OnRemove<T> {
    fn kind() -> ObserverKind {
        ObserverKind::OnRemove
    }
    fn component_type_id() -> TypeId {
        TypeId::of::<T>()
    }
}

// ── Bundle trait ─────────────────────────────────────────────────

/// A collection of components that can be inserted together.
///
/// Implement this for structs whose fields are all `Component` types.
/// Use `#[derive(Bundle)]` from `quasar_derive` for automatic generation.
///
/// ```ignore
/// #[derive(Bundle)]
/// struct PlayerBundle {
///     position: Position,
///     velocity: Velocity,
///     health: Health,
/// }
///
/// let e = world.spawn_bundle(PlayerBundle {
///     position: Position(0.0, 0.0, 0.0),
///     velocity: Velocity(1.0, 0.0, 0.0),
///     health: Health(100),
/// });
/// ```
pub trait Bundle: Send + Sync + 'static {
    /// Insert each field into `world` on `entity`.
    fn insert_into(self, world: &mut World, entity: Entity);
}

// Implement Bundle for single components.
impl<T: Component + Clone> Bundle for (T,) {
    fn insert_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.0);
    }
}

macro_rules! impl_bundle_tuple {
    ($($T:ident),+) => {
        #[allow(non_snake_case)]
        impl<$($T: Component + Clone),+> Bundle for ($($T,)+) {
            fn insert_into(self, world: &mut World, entity: Entity) {
                let ($($T,)+) = self;
                $(world.insert(entity, $T);)+
            }
        }
    };
}

impl_bundle_tuple!(A, B);
impl_bundle_tuple!(A, B, C);
impl_bundle_tuple!(A, B, C, D);
impl_bundle_tuple!(A, B, C, D, E);
impl_bundle_tuple!(A, B, C, D, E, F);
impl_bundle_tuple!(A, B, C, D, E, F, G);
impl_bundle_tuple!(A, B, C, D, E, F, G, H);

// ── Prototype ───────────────────────────────────────────────────

/// A reusable entity template. Store a scene-level "prefab without an entity"
/// that can stamp identical copies into the world cheaply.
///
/// ```ignore
/// let proto = Prototype::new().with(Position(0.0, 0.0, 0.0)).with(Health(100));
/// let e1 = proto.spawn(&mut world);
/// let e2 = proto.spawn(&mut world);
/// ```
pub struct Prototype {
    /// Each entry is (TypeId, factory_fn, component_bytes).
    components: Vec<(
        TypeId,
        fn() -> Box<dyn ColumnStorage>,
        Box<dyn Fn(&mut World, Entity) + Send + Sync>,
    )>,
}

impl Prototype {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Add a component to the prototype.
    pub fn with<T: Component + Clone>(mut self, value: T) -> Self {
        self.components.push((
            TypeId::of::<T>(),
            create_typed_column::<T>,
            Box::new(move |world: &mut World, entity: Entity| {
                world.insert(entity, value.clone());
            }),
        ));
        self
    }

    /// Spawn a new entity populated with cloned components.
    pub fn spawn(&self, world: &mut World) -> Entity {
        let entity = world.spawn();
        for (_tid, _factory, inserter) in &self.components {
            inserter(world, entity);
        }
        entity
    }
}

impl Default for Prototype {
    fn default() -> Self {
        Self::new()
    }
}

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
    resources: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// Archetype-based SoA storage — the single source of truth for component data.
    archetype_graph: ArchetypeGraph,
    /// Maps entity index to its archetype ID
    entity_archetype: FxHashMap<u32, ArchetypeId>,
    /// Maps entity index to its row within the archetype
    entity_row: FxHashMap<u32, usize>,
    /// Track which components each entity has (for archetype migration)
    entity_components: FxHashMap<u32, Vec<TypeId>>,
    /// Per-type removal log: tracks entity indices that had a component removed this frame.
    /// Cleared once per frame via `clear_removal_log()`.
    removal_log: HashMap<TypeId, Vec<u32>>,
    /// Global change-detection tick, incremented once per frame.
    current_tick: u64,
    /// Per-(TypeId, entity_index) change tick for change-detection queries.
    change_ticks: FxHashMap<TypeId, FxHashMap<u32, u64>>,
    /// Column factories for creating typed SoA columns from runtime TypeIds.
    column_factories: FxHashMap<TypeId, fn() -> Box<dyn ColumnStorage>>,
    /// Sparse-set storage for components that bypass archetype migration.
    sparse_storage: SparseSetStorage,
    /// Per-system last-run tick: system_name → tick when it last ran.
    /// Uses RwLock for thread-safe read access during parallel execution.
    system_last_run: RwLock<HashMap<String, u64>>,
    /// The last-run tick of the currently executing system.
    /// Used by `FilterChanged<T>` to detect changes since this system last ran.
    /// Defaults to 0 so that a never-before-run system sees all components as changed.
    /// Uses AtomicU64 for thread-safe access during parallel system execution.
    active_system_last_run: AtomicU64,
    /// Typed entity-to-entity relationship graph.
    relation_graph: RelationGraph,
    /// Observer callbacks: maps (ObserverKind, component TypeId) → list of callbacks.
    observers: HashMap<(ObserverKind, TypeId), Vec<Box<dyn Fn(Entity) + Send + Sync>>>,
}

impl World {
    /// Create an empty world.
    pub fn new() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            resources: FxHashMap::default(),
            archetype_graph: ArchetypeGraph::new(),
            entity_archetype: FxHashMap::default(),
            entity_row: FxHashMap::default(),
            entity_components: FxHashMap::default(),
            removal_log: HashMap::new(),
            current_tick: 0,
            change_ticks: FxHashMap::default(),
            column_factories: FxHashMap::default(),
            sparse_storage: SparseSetStorage::new(),
            system_last_run: RwLock::new(HashMap::new()),
            active_system_last_run: AtomicU64::new(0),
            relation_graph: RelationGraph::new(),
            observers: HashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Entity management
    // ------------------------------------------------------------------

    /// Spawn a new entity (with no components).
    pub fn spawn(&mut self) -> Entity {
        self.allocator.allocate()
    }

    /// Spawn a new entity and immediately insert a bundle of components.
    ///
    /// ```ignore
    /// let e = world.spawn_bundle((Position(0.0, 0.0, 0.0), Velocity(1.0, 0.0, 0.0)));
    /// ```
    pub fn spawn_bundle<B: Bundle>(&mut self, bundle: B) -> Entity {
        let entity = self.spawn();
        bundle.insert_into(self, entity);
        entity
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
                self.removal_log
                    .entry(type_id)
                    .or_default()
                    .push(entity.index());
            }
        }

        // Remove from archetype storage (swap-and-pop)
        if let Some(arch_id) = self.entity_archetype.remove(&entity.index()) {
            if let Some(arch) = self.archetype_graph.get_mut(arch_id) {
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

        // Clean up change ticks for this entity
        if let Some(components) = self.entity_components.remove(&entity.index()) {
            for type_id in &components {
                if let Some(ticks) = self.change_ticks.get_mut(type_id) {
                    ticks.remove(&entity.index());
                }
            }
        }

        // Remove from all sparse-set storage
        self.sparse_storage.remove_entity(entity);

        // Clean up all entity relationships
        self.relation_graph.remove_entity(entity);

        true
    }

    /// Despawn an entity **and** all entities transitively owned via [`OwnedBy`].
    pub fn despawn_recursive(&mut self, entity: Entity) {
        let owned = self.relation_graph.owned_recursive(entity);
        for e in owned {
            self.despawn(e);
        }
        self.despawn(entity);
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
    /// Stores data in archetype SoA columns for cache-efficient iteration.
    pub fn insert<T: Component + Clone>(&mut self, entity: Entity, component: T) {
        debug_assert!(self.is_alive(entity), "inserting on a dead entity");

        let type_id = TypeId::of::<T>();

        // Register column factory for this type (for insert_raw support)
        self.column_factories
            .entry(type_id)
            .or_insert(create_typed_column::<T>);

        // Track component for this entity
        let components = self.entity_components.entry(entity.index()).or_default();
        if !components.contains(&type_id) {
            components.push(type_id);
            components.sort();
        }

        // Record change tick
        self.change_ticks
            .entry(type_id)
            .or_default()
            .insert(entity.index(), self.current_tick);

        // Get or create archetype for this component set
        let mut sig = ArchetypeSignature::new();
        for tid in components.iter() {
            sig.add(*tid);
        }

        // Check if entity needs to migrate to new archetype
        let old_arch_id = self.entity_archetype.get(&entity.index()).copied();
        let needs_migration = if let Some(current_arch_id) = old_arch_id {
            if let Some(current_arch) = self.archetype_graph.get(current_arch_id) {
                !current_arch.signature.contains(&type_id)
            } else {
                true
            }
        } else {
            true
        };

        if needs_migration {
            let new_arch_id = self.archetype_graph.get_or_create(&sig);

            // ── Phase 1: extract SoA data from old archetype (if any) ──
            let extracted = if let Some(old_id) = old_arch_id {
                if let Some(old_arch) = self.archetype_graph.get_mut(old_id) {
                    if let Some((_freed_row, data)) = old_arch.remove_entity_extract_soa(entity) {
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
            if let Some(new_arch) = self.archetype_graph.get_mut(new_arch_id) {
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
                    let Some(&ci) = new_arch.type_to_column.get(&tid) else {
                        continue;
                    };
                    (*new_arch.columns[ci]).push_raw(value);
                }

                // Push the NEW component T into its SoA column
                new_arch.ensure_column::<T>();
                if let Some(&ci) = new_arch.type_to_column.get(&type_id) {
                    if let Some(col) = (*new_arch.columns[ci])
                        .as_any_mut()
                        .downcast_mut::<TypedColumn<T>>()
                    {
                        col.push(component);
                    }
                }
            } else {
                // Fallback: just record the archetype mapping
                self.entity_archetype.insert(entity.index(), new_arch_id);
                self.entity_row.insert(entity.index(), 0);
            }
        } else {
            // No migration — replace the existing value at the entity's row.
            if let Some(&arch_id) = self.entity_archetype.get(&entity.index()) {
                if let Some(arch) = self.archetype_graph.get_mut(arch_id) {
                    arch.ensure_column::<T>();
                    if let Some(&col_idx) = arch.type_to_column.get(&type_id) {
                        let ci: usize = col_idx;
                        if let Some(col) = (*arch.columns[ci])
                            .as_any_mut()
                            .downcast_mut::<TypedColumn<T>>()
                        {
                            if let Some(&row) = arch.entity_to_row.get(&entity.index()) {
                                if row < col.data.len() {
                                    col.data[row] = component;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Notify observers
        self.fire_observers(ObserverKind::OnAdd, type_id, entity);
    }

    /// Register a column factory for a component type.
    ///
    /// This is needed when restoring entities from snapshots where the component
    /// type may not have been seen before. The factory allows the archetype system
    /// to create typed SoA columns on demand.
    pub fn register_column_factory<T: Component + Clone>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.column_factories
            .entry(type_id)
            .or_insert(create_typed_column::<T>);
    }

    /// Register a column factory from a TypeId and factory function.
    ///
    /// This is the type-erased version for use when the component type is not
    /// statically known (e.g., during undo/redo operations).
    pub fn register_column_factory_raw(
        &mut self,
        type_id: TypeId,
        factory: fn() -> Box<dyn ColumnStorage>,
    ) {
        self.column_factories.entry(type_id).or_insert(factory);
    }

    /// Get the column factory for a component type, if registered.
    pub fn get_column_factory(&self, type_id: TypeId) -> Option<fn() -> Box<dyn ColumnStorage>> {
        self.column_factories.get(&type_id).copied()
    }

    /// Remove a component from an entity, returning `true` if it existed.
    pub fn remove_component<T: Component>(&mut self, entity: Entity) -> bool {
        let type_id = TypeId::of::<T>();

        // Check if entity has the component
        let had_component =
            if let Some(components) = self.entity_components.get_mut(&entity.index()) {
                if let Ok(pos) = components.binary_search(&type_id) {
                    components.remove(pos);
                    true
                } else {
                    false
                }
            } else {
                false
            };

        if !had_component {
            return false;
        }

        // Log removal for Removed<T> query filter
        self.removal_log
            .entry(type_id)
            .or_default()
            .push(entity.index());

        // Clean up change tick
        if let Some(ticks) = self.change_ticks.get_mut(&type_id) {
            ticks.remove(&entity.index());
        }

        // Migrate entity to new archetype without this component
        if let Some(old_arch_id) = self.entity_archetype.get(&entity.index()).copied() {
            let mut new_sig = ArchetypeSignature::new();
            if let Some(components) = self.entity_components.get(&entity.index()) {
                for tid in components.iter() {
                    new_sig.add(*tid);
                }
            }

            // Extract data from old archetype
            let extracted = if let Some(old_arch) = self.archetype_graph.get_mut(old_arch_id) {
                if let Some((_freed_row, data)) = old_arch.remove_entity_extract_soa(entity) {
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
            };

            // Create new archetype and re-insert (minus the removed component)
            let new_arch_id = self.archetype_graph.get_or_create(&new_sig);
            if let Some(new_arch) = self.archetype_graph.get_mut(new_arch_id) {
                let row = new_arch.add_entity(entity);
                self.entity_archetype.insert(entity.index(), new_arch_id);
                self.entity_row.insert(entity.index(), row);

                for (tid, value, empty_col) in extracted {
                    if tid == type_id {
                        continue; // Skip the removed component
                    }
                    if !new_arch.type_to_column.contains_key(&tid) {
                        let idx = new_arch.columns.len();
                        new_arch.column_types.push(tid);
                        new_arch.columns.push(empty_col);
                        new_arch.type_to_column.insert(tid, idx);
                    }
                    let Some(&ci) = new_arch.type_to_column.get(&tid) else {
                        continue;
                    };
                    (*new_arch.columns[ci]).push_raw(value);
                }
            }
        }

        // Notify observers
        self.fire_observers(ObserverKind::OnRemove, type_id, entity);

        true
    }

    /// Get a shared reference to a component on an entity.
    /// Reads directly from archetype SoA storage.
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let type_id = TypeId::of::<T>();
        // Quick check: does this entity have the component?
        if let Some(components) = self.entity_components.get(&entity.index()) {
            if components.binary_search(&type_id).is_err() {
                return None;
            }
        } else {
            return None;
        }
        let &arch_id = self.entity_archetype.get(&entity.index())?;
        let arch = self.archetype_graph.get(arch_id)?;
        let &col_idx = arch.type_to_column.get(&type_id)?;
        let col = arch.columns[col_idx]
            .as_any()
            .downcast_ref::<TypedColumn<T>>()?;
        let &row = arch.entity_to_row.get(&entity.index())?;
        col.data.get(row)
    }

    /// Get a mutable reference to a component on an entity.
    /// Reads directly from archetype SoA storage and updates change tick
    /// both in the World-level map and the per-row column tick.
    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let type_id = TypeId::of::<T>();
        // Quick check: does this entity have the component?
        if let Some(components) = self.entity_components.get(&entity.index()) {
            if components.binary_search(&type_id).is_err() {
                return None;
            }
        } else {
            return None;
        }
        let tick = self.current_tick;
        // Update World-level change tick
        self.change_ticks
            .entry(type_id)
            .or_default()
            .insert(entity.index(), tick);

        let &arch_id = self.entity_archetype.get(&entity.index())?;
        let arch = self.archetype_graph.get_mut(arch_id)?;
        let col_idx = *arch.type_to_column.get(&type_id)?;
        let row = *arch.entity_to_row.get(&entity.index())?;
        let col = arch.columns[col_idx]
            .as_any_mut()
            .downcast_mut::<TypedColumn<T>>()?;
        // Stamp per-row column tick for change detection
        col.set_changed(row, tick);
        col.data.get_mut(row)
    }

    /// Check whether an entity has a specific component type.
    pub fn has<T: Component>(&self, entity: Entity) -> bool {
        if let Some(components) = self.entity_components.get(&entity.index()) {
            components.binary_search(&TypeId::of::<T>()).is_ok()
        } else {
            false
        }
    }

    // ------------------------------------------------------------------
    // Sparse-set component storage
    // ------------------------------------------------------------------

    /// Insert a component into sparse-set storage (bypasses archetype migration).
    pub fn insert_sparse<T: Component>(&mut self, entity: Entity, component: T) {
        debug_assert!(self.is_alive(entity), "inserting sparse on a dead entity");
        self.sparse_storage
            .get_or_create::<T>()
            .insert(entity, component);
    }

    /// Get a shared reference to a sparse-set component.
    pub fn get_sparse<T: Component>(&self, entity: Entity) -> Option<&T> {
        self.sparse_storage.get::<T>()?.get(entity)
    }

    /// Get a mutable reference to a sparse-set component.
    pub fn get_sparse_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        self.sparse_storage.get_mut::<T>()?.get_mut(entity)
    }

    /// Remove a sparse-set component from an entity.
    pub fn remove_sparse<T: Component>(&mut self, entity: Entity) -> Option<T> {
        self.sparse_storage.get_mut::<T>()?.remove(entity)
    }

    /// Check if an entity has a sparse-set component of type T.
    pub fn has_sparse<T: Component>(&self, entity: Entity) -> bool {
        self.sparse_storage.contains::<T>(entity)
    }

    /// Access the raw sparse-set storage.
    pub fn sparse_storage(&self) -> &SparseSetStorage {
        &self.sparse_storage
    }

    /// Access the raw sparse-set storage (mutable).
    pub fn sparse_storage_mut(&mut self) -> &mut SparseSetStorage {
        &mut self.sparse_storage
    }

    // ------------------------------------------------------------------
    // Entity relationships
    // ------------------------------------------------------------------

    /// Add a typed relation: `source R target`.
    pub fn add_relation<R: Relation>(&mut self, source: Entity, target: Entity) {
        self.relation_graph.add::<R>(source, target);
    }

    /// Remove a specific typed relation between source and target.
    pub fn remove_relation<R: Relation>(&mut self, source: Entity, target: Entity) {
        self.relation_graph.remove::<R>(source, target);
    }

    /// Check whether `source R target` exists.
    pub fn has_relation<R: Relation>(&self, source: Entity, target: Entity) -> bool {
        self.relation_graph.has::<R>(source, target)
    }

    /// Get all entities that `source` has relation `R` to.
    pub fn relation_targets<R: Relation>(&self, source: Entity) -> &[Entity] {
        self.relation_graph.targets::<R>(source)
    }

    /// Get all entities that have relation `R` **to** `target`.
    pub fn relation_sources<R: Relation>(&self, target: Entity) -> &[Entity] {
        self.relation_graph.sources::<R>(target)
    }

    /// Access the full relation graph.
    pub fn relations(&self) -> &RelationGraph {
        &self.relation_graph
    }

    /// Access the full relation graph (mutable).
    pub fn relations_mut(&mut self) -> &mut RelationGraph {
        &mut self.relation_graph
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
    // Entity hierarchy (Parent / Children)
    // ------------------------------------------------------------------

    /// Set `child`'s parent to `parent`, updating both `Parent` and `Children`
    /// components and the underlying `ChildOf` relation.
    pub fn set_parent(&mut self, child: Entity, parent: Entity) {
        // Remove from old parent's Children list if any
        if let Some(&Parent(old_parent)) = self.get::<Parent>(child) {
            if old_parent != parent {
                self.relation_graph.remove::<ChildOf>(child, old_parent);
                // Remove child from old parent's Children list
                if let Some(children) = self.get_mut::<Children>(old_parent) {
                    children.0.retain(|e| *e != child);
                }
            }
        }

        self.insert(child, Parent(parent));
        self.relation_graph.add::<ChildOf>(child, parent);

        // Add to new parent's Children list
        if self.has::<Children>(parent) {
            if let Some(children) = self.get_mut::<Children>(parent) {
                if !children.0.contains(&child) {
                    children.0.push(child);
                }
            }
        } else {
            self.insert(parent, Children(SmallVec::from_elem(child, 1)));
        }
    }

    /// Get the children of an entity (empty slice if none).
    pub fn children_of(&self, parent: Entity) -> &[Entity] {
        self.relation_graph.sources::<ChildOf>(parent)
    }

    /// Get the parent of an entity (if any).
    pub fn parent_of(&self, child: Entity) -> Option<Entity> {
        self.get::<Parent>(child).map(|p| p.0)
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

    /// Advance the change-detection tick.
    /// Call once per frame before running systems.
    pub fn tick_change_detection(&mut self) {
        self.current_tick += 1;
    }

    /// Return the current change-detection tick.
    pub fn current_tick<T: Component>(&self) -> u64 {
        self.current_tick
    }

    /// Set up the system context before running a system.
    ///
    /// Loads that system's `last_run_tick` into `active_system_last_run`
    /// so that `FilterChanged<T>` can determine what changed since this
    /// system last ran.
    pub fn begin_system(&mut self, name: &str) {
        let last_run = self
            .system_last_run
            .read()
            .unwrap()
            .get(name)
            .copied()
            .unwrap_or(0);
        self.active_system_last_run
            .store(last_run, Ordering::SeqCst);
    }

    /// Finalize after running a system: record the current tick as the
    /// system's last-run tick.
    pub fn end_system(&mut self, name: &str) {
        self.system_last_run
            .write()
            .unwrap()
            .insert(name.to_string(), self.current_tick);
        self.active_system_last_run.store(0, Ordering::SeqCst);
    }

    /// Return the last-run tick of the currently active system.
    /// Used by `FilterChanged<T>` and other change-detection filters.
    pub fn active_system_last_run(&self) -> u64 {
        self.active_system_last_run.load(Ordering::SeqCst)
    }

    /// Set the active_system_last_run directly (used by parallel scheduler).
    /// Thread-safe: can be called from multiple threads.
    pub fn set_active_system_last_run(&self, tick: u64) {
        self.active_system_last_run.store(tick, Ordering::SeqCst);
    }

    /// Get the last-run tick for a system by name.
    /// Thread-safe: can be called from multiple threads.
    pub fn get_system_last_run(&self, name: &str) -> u64 {
        self.system_last_run
            .read()
            .unwrap()
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    /// Iterate over all `(Entity, &T)` pairs via archetype SoA columns.
    pub fn query<T: Component>(&self) -> Vec<(Entity, &T)> {
        let type_id = TypeId::of::<T>();
        let matching = self.archetype_graph.find_with_components(&[type_id]);
        let mut results = Vec::new();
        for arch in &matching {
            let col_idx = match arch.type_to_column.get(&type_id) {
                Some(&idx) => idx,
                None => continue,
            };
            let col = match arch.columns[col_idx]
                .as_any()
                .downcast_ref::<TypedColumn<T>>()
            {
                Some(c) => c,
                None => continue,
            };
            let n = col.data.len().min(arch.entities.len());
            for i in 0..n {
                results.push((arch.entities[i], &col.data[i]));
            }
        }
        results
    }

    /// Iterate over all `(Entity, &mut T)` pairs using a callback.
    /// Stamps per-row change ticks for each entity visited.
    pub fn for_each_mut<T, F>(&mut self, mut f: F)
    where
        T: Component,
        F: FnMut(Entity, &mut T),
    {
        let type_id = TypeId::of::<T>();
        let tick = self.current_tick;
        let arch_ids = self.archetype_graph.find_with_components_ids(&[type_id]);
        for arch_id in arch_ids {
            if let Some(arch) = self.archetype_graph.get_mut(arch_id) {
                let col_idx = match arch.type_to_column.get(&type_id) {
                    Some(&idx) => idx,
                    None => continue,
                };
                // Split borrow: snapshot entity list before mutably accessing columns.
                let entities: Vec<Entity> = arch.entities.clone();
                if let Some(col) = arch.columns[col_idx]
                    .as_any_mut()
                    .downcast_mut::<TypedColumn<T>>()
                {
                    for (i, entity) in entities.iter().enumerate() {
                        if i < col.data.len() {
                            col.set_changed(i, tick);
                            f(*entity, &mut col.data[i]);
                        }
                    }
                }
            }
        }
    }

    /// Query entities that have **both** components `A` and `B` via archetype SoA.
    pub fn query2<A: Component, B: Component>(&self) -> Vec<(Entity, &A, &B)> {
        let type_a = TypeId::of::<A>();
        let type_b = TypeId::of::<B>();
        let matching = self.archetype_graph.find_with_components(&[type_a, type_b]);
        let mut results = Vec::new();
        for arch in &matching {
            let (Some(&ca), Some(&cb)) = (
                arch.type_to_column.get(&type_a),
                arch.type_to_column.get(&type_b),
            ) else {
                continue;
            };
            let (Some(col_a), Some(col_b)) = (
                arch.columns[ca].as_any().downcast_ref::<TypedColumn<A>>(),
                arch.columns[cb].as_any().downcast_ref::<TypedColumn<B>>(),
            ) else {
                continue;
            };
            let n = col_a
                .data
                .len()
                .min(col_b.data.len())
                .min(arch.entities.len());
            for i in 0..n {
                results.push((arch.entities[i], &col_a.data[i], &col_b.data[i]));
            }
        }
        results
    }

    /// Query entities that have components `A`, `B`, **and** `C` via archetype SoA.
    pub fn query3<A: Component, B: Component, C: Component>(&self) -> Vec<(Entity, &A, &B, &C)> {
        let type_a = TypeId::of::<A>();
        let type_b = TypeId::of::<B>();
        let type_c = TypeId::of::<C>();
        let matching = self
            .archetype_graph
            .find_with_components(&[type_a, type_b, type_c]);
        let mut results = Vec::new();
        for arch in &matching {
            let (Some(&ca), Some(&cb), Some(&cc)) = (
                arch.type_to_column.get(&type_a),
                arch.type_to_column.get(&type_b),
                arch.type_to_column.get(&type_c),
            ) else {
                continue;
            };
            let (Some(col_a), Some(col_b), Some(col_c)) = (
                arch.columns[ca].as_any().downcast_ref::<TypedColumn<A>>(),
                arch.columns[cb].as_any().downcast_ref::<TypedColumn<B>>(),
                arch.columns[cc].as_any().downcast_ref::<TypedColumn<C>>(),
            ) else {
                continue;
            };
            let n = [
                col_a.data.len(),
                col_b.data.len(),
                col_c.data.len(),
                arch.entities.len(),
            ]
            .into_iter()
            .min()
            .unwrap_or(0);
            for i in 0..n {
                results.push((
                    arch.entities[i],
                    &col_a.data[i],
                    &col_b.data[i],
                    &col_c.data[i],
                ));
            }
        }
        results
    }

    /// Query entities that have components `A`, `B`, `C`, **and** `D` via archetype SoA.
    pub fn query4<A: Component, B: Component, C: Component, D: Component>(
        &self,
    ) -> Vec<(Entity, &A, &B, &C, &D)> {
        let tids = [
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            TypeId::of::<C>(),
            TypeId::of::<D>(),
        ];
        let matching = self.archetype_graph.find_with_components(&tids);
        let mut results = Vec::new();
        for arch in &matching {
            let (Some(&ca), Some(&cb), Some(&cc), Some(&cd)) = (
                arch.type_to_column.get(&tids[0]),
                arch.type_to_column.get(&tids[1]),
                arch.type_to_column.get(&tids[2]),
                arch.type_to_column.get(&tids[3]),
            ) else {
                continue;
            };
            let (Some(a), Some(b), Some(c), Some(d)) = (
                arch.columns[ca].as_any().downcast_ref::<TypedColumn<A>>(),
                arch.columns[cb].as_any().downcast_ref::<TypedColumn<B>>(),
                arch.columns[cc].as_any().downcast_ref::<TypedColumn<C>>(),
                arch.columns[cd].as_any().downcast_ref::<TypedColumn<D>>(),
            ) else {
                continue;
            };
            let n = [
                a.data.len(),
                b.data.len(),
                c.data.len(),
                d.data.len(),
                arch.entities.len(),
            ]
            .into_iter()
            .min()
            .unwrap_or(0);
            for i in 0..n {
                results.push((
                    arch.entities[i],
                    &a.data[i],
                    &b.data[i],
                    &c.data[i],
                    &d.data[i],
                ));
            }
        }
        results
    }

    /// Query entities that have components `A` through `E` via archetype SoA.
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
        let matching = self.archetype_graph.find_with_components(&tids);
        let mut results = Vec::new();
        for arch in &matching {
            let (Some(&ca), Some(&cb), Some(&cc), Some(&cd), Some(&ce)) = (
                arch.type_to_column.get(&tids[0]),
                arch.type_to_column.get(&tids[1]),
                arch.type_to_column.get(&tids[2]),
                arch.type_to_column.get(&tids[3]),
                arch.type_to_column.get(&tids[4]),
            ) else {
                continue;
            };
            let (Some(a), Some(b), Some(c), Some(d), Some(e)) = (
                arch.columns[ca].as_any().downcast_ref::<TypedColumn<A>>(),
                arch.columns[cb].as_any().downcast_ref::<TypedColumn<B>>(),
                arch.columns[cc].as_any().downcast_ref::<TypedColumn<C>>(),
                arch.columns[cd].as_any().downcast_ref::<TypedColumn<D>>(),
                arch.columns[ce].as_any().downcast_ref::<TypedColumn<E>>(),
            ) else {
                continue;
            };
            let n = [
                a.data.len(),
                b.data.len(),
                c.data.len(),
                d.data.len(),
                e.data.len(),
                arch.entities.len(),
            ]
            .into_iter()
            .min()
            .unwrap_or(0);
            for i in 0..n {
                results.push((
                    arch.entities[i],
                    &a.data[i],
                    &b.data[i],
                    &c.data[i],
                    &d.data[i],
                    &e.data[i],
                ));
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
        let matching = self.archetype_graph.find_with_components(&[type_id]);
        let ticks = self.change_ticks.get(&type_id);
        let mut results = Vec::new();
        for arch in &matching {
            let col_idx = match arch.type_to_column.get(&type_id) {
                Some(&idx) => idx,
                None => continue,
            };
            let col = match arch.columns[col_idx]
                .as_any()
                .downcast_ref::<TypedColumn<T>>()
            {
                Some(c) => c,
                None => continue,
            };
            let n = col.data.len().min(arch.entities.len());
            for i in 0..n {
                let entity = arch.entities[i];
                let changed = ticks
                    .and_then(|m| m.get(&entity.index()))
                    .is_some_and(|&tick| tick > since_tick);
                if changed {
                    results.push((entity, &col.data[i]));
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
        let matching = self.archetype_graph.find_with_components(&[type_t, type_w]);
        let mut results = Vec::new();
        for arch in &matching {
            let col_idx = match arch.type_to_column.get(&type_t) {
                Some(&idx) => idx,
                None => continue,
            };
            let col = match arch.columns[col_idx]
                .as_any()
                .downcast_ref::<TypedColumn<T>>()
            {
                Some(c) => c,
                None => continue,
            };
            let n = col.data.len().min(arch.entities.len());
            for i in 0..n {
                results.push((arch.entities[i], &col.data[i]));
            }
        }
        results
    }

    /// Query entities with component `T` that do NOT have component `W`.
    pub fn query_without<T: Component, W: Component>(&self) -> Vec<(Entity, &T)> {
        let type_t = TypeId::of::<T>();
        let type_w = TypeId::of::<W>();
        let matching = self.archetype_graph.find_with_components(&[type_t]);
        let mut results = Vec::new();
        for arch in &matching {
            // Skip archetypes that also contain W
            if arch.signature.contains(&type_w) {
                continue;
            }
            let col_idx = match arch.type_to_column.get(&type_t) {
                Some(&idx) => idx,
                None => continue,
            };
            let col = match arch.columns[col_idx]
                .as_any()
                .downcast_ref::<TypedColumn<T>>()
            {
                Some(c) => c,
                None => continue,
            };
            let n = col.data.len().min(arch.entities.len());
            for i in 0..n {
                results.push((arch.entities[i], &col.data[i]));
            }
        }
        results
    }

    /// Query entities for component `T`, returning `Option<&U>` for an
    /// optional second component.
    pub fn query_optional<T: Component, U: Component>(&self) -> Vec<(Entity, &T, Option<&U>)> {
        let type_t = TypeId::of::<T>();
        let type_u = TypeId::of::<U>();
        let matching = self.archetype_graph.find_with_components(&[type_t]);
        let mut results = Vec::new();
        for arch in &matching {
            let col_t_idx = match arch.type_to_column.get(&type_t) {
                Some(&idx) => idx,
                None => continue,
            };
            let col_t = match arch.columns[col_t_idx]
                .as_any()
                .downcast_ref::<TypedColumn<T>>()
            {
                Some(c) => c,
                None => continue,
            };
            let col_u = arch
                .type_to_column
                .get(&type_u)
                .and_then(|&idx| arch.columns[idx].as_any().downcast_ref::<TypedColumn<U>>());
            let n = col_t.data.len().min(arch.entities.len());
            for i in 0..n {
                let u_val = col_u.and_then(|c| c.data.get(i));
                results.push((arch.entities[i], &col_t.data[i], u_val));
            }
        }
        results
    }

    /// Ergonomic typed query with composable filters.
    ///
    /// Returns a `QueryIter` that lazily yields `(Entity, Q::Item)` tuples,
    /// skipping entities that don't pass the filter `F`.
    ///
    /// ```ignore
    /// for (entity, (pos, vel)) in world.query_filtered::<(&Position, &Velocity), FilterChanged<Velocity>>() {
    ///     // only entities whose Velocity changed since this system last ran
    /// }
    /// ```
    pub fn query_filtered<Q, F>(&self) -> crate::ecs::query::QueryIter<'_, Q, F>
    where
        Q: crate::ecs::query::WorldQuery,
        F: crate::ecs::query::QueryFilter,
    {
        crate::ecs::query::QueryState::<Q, F>::new().iter(self)
    }

    // ------------------------------------------------------------------
    // Internals — helpers for query.rs
    // ------------------------------------------------------------------

    /// Look up the change tick for a specific entity/component pair.
    pub(crate) fn change_tick_for(&self, type_id: TypeId, entity_index: u32) -> Option<u64> {
        self.change_ticks.get(&type_id)?.get(&entity_index).copied()
    }

    /// Get the archetype graph (immutable) for query iteration.
    #[allow(dead_code)]
    pub(crate) fn archetype_graph(&self) -> &ArchetypeGraph {
        &self.archetype_graph
    }

    /// Look up which archetype an entity belongs to (if any).
    #[allow(dead_code)]
    pub(crate) fn entity_archetype_id(&self, entity_index: u32) -> Option<ArchetypeId> {
        self.entity_archetype.get(&entity_index).copied()
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
    ///
    /// Requires a column factory so the archetype system can create a
    /// typed SoA column if one doesn't already exist for this type.
    pub fn insert_raw(
        &mut self,
        entity: Entity,
        type_id: TypeId,
        component: Box<dyn std::any::Any + Send + Sync>,
        column_factory: fn() -> Box<dyn ColumnStorage>,
    ) {
        debug_assert!(self.is_alive(entity), "inserting on a dead entity");

        // Register column factory for this type
        self.column_factories
            .entry(type_id)
            .or_insert(column_factory);

        // Track component for this entity
        let components = self.entity_components.entry(entity.index()).or_default();
        let already_has = components.binary_search(&type_id).is_ok();
        if !already_has {
            components.push(type_id);
            components.sort();
        }

        // Build the new archetype signature
        let comp_list = components.clone();
        let mut sig = ArchetypeSignature::new();
        for &tid in &comp_list {
            sig.add(tid);
        }

        let new_arch_id = self.archetype_graph.get_or_create(&sig);

        // Handle archetype migration if entity was in a different archetype
        if let Some(&old_arch_id) = self.entity_archetype.get(&entity.index()) {
            if old_arch_id != new_arch_id {
                // Extract all SoA data from old archetype
                if let Some(old_arch) = self.archetype_graph.get_mut(old_arch_id) {
                    if let Some((_row, extracted)) = old_arch.remove_entity_extract_soa(entity) {
                        // Push entity + extracted data into new archetype
                        if let Some(new_arch) = self.archetype_graph.get_mut(new_arch_id) {
                            new_arch.add_entity(entity);
                            for (tid, value, empty_col) in extracted {
                                new_arch.ensure_column_from_factory(
                                    tid,
                                    self.column_factories.get(&tid).copied().unwrap_or_else(|| {
                                        let e = empty_col;
                                        let _ = e;
                                        // This should not happen for properly registered types
                                        || Box::new(crate::ecs::archetype::TypedColumn::<()>::new())
                                            as Box<dyn ColumnStorage>
                                    }),
                                );
                                if let Some(&col_idx) = new_arch.type_to_column.get(&tid) {
                                    new_arch.columns[col_idx].push_raw(value);
                                }
                            }
                            // Push the new component
                            new_arch.ensure_column_from_factory(type_id, column_factory);
                            if let Some(&col_idx) = new_arch.type_to_column.get(&type_id) {
                                new_arch.columns[col_idx].push_raw(component);
                            }
                        }
                    }
                }
            } else {
                // Same archetype — overwrite existing value
                if let Some(arch) = self.archetype_graph.get_mut(new_arch_id) {
                    if let Some(&row) = arch.entity_to_row.get(&entity.index()) {
                        arch.ensure_column_from_factory(type_id, column_factory);
                        if let Some(&col_idx) = arch.type_to_column.get(&type_id) {
                            // Swap-remove old value at row, push new, then swap back
                            if arch.columns[col_idx].len() > row {
                                arch.columns[col_idx].swap_remove_entry(row);
                            }
                            arch.columns[col_idx].push_raw(component);
                            // If row wasn't last, swap new value into correct position
                            let last = arch.columns[col_idx].len().saturating_sub(1);
                            if row != last && !arch.columns[col_idx].is_empty() {
                                arch.columns[col_idx].swap_remove_entry(row);
                                // The old "last" is now at row, push the swapped value back
                            }
                        }
                    }
                }
            }
        } else {
            // No previous archetype — first component for this entity
            if let Some(arch) = self.archetype_graph.get_mut(new_arch_id) {
                arch.add_entity(entity);
                arch.ensure_column_from_factory(type_id, column_factory);
                if let Some(&col_idx) = arch.type_to_column.get(&type_id) {
                    arch.columns[col_idx].push_raw(component);
                }
            }
        }

        self.entity_archetype.insert(entity.index(), new_arch_id);

        // Record change tick
        self.change_ticks
            .entry(type_id)
            .or_default()
            .insert(entity.index(), self.current_tick);
    }

    /// Remove a component by type from an entity.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> bool {
        self.remove_raw(entity, TypeId::of::<T>())
    }

    /// Remove a component using runtime type information (for Commands).
    pub fn remove_raw(&mut self, entity: Entity, type_id: TypeId) -> bool {
        // Update entity's component list
        if let Some(components) = self.entity_components.get_mut(&entity.index()) {
            if let Ok(pos) = components.binary_search(&type_id) {
                components.remove(pos);
                self.removal_log
                    .entry(type_id)
                    .or_default()
                    .push(entity.index());
            } else {
                return false;
            }
        } else {
            return false;
        }

        // Migrate entity to new archetype without the removed component
        let comp_list = self
            .entity_components
            .get(&entity.index())
            .cloned()
            .unwrap_or_default();
        let mut new_sig = ArchetypeSignature::new();
        for &tid in &comp_list {
            new_sig.add(tid);
        }
        let new_arch_id = self.archetype_graph.get_or_create(&new_sig);

        if let Some(&old_arch_id) = self.entity_archetype.get(&entity.index()) {
            if old_arch_id != new_arch_id {
                if let Some(old_arch) = self.archetype_graph.get_mut(old_arch_id) {
                    if let Some((_row, extracted)) = old_arch.remove_entity_extract_soa(entity) {
                        if let Some(new_arch) = self.archetype_graph.get_mut(new_arch_id) {
                            new_arch.add_entity(entity);
                            for (tid, value, _empty) in extracted {
                                if tid == type_id {
                                    continue; // Skip the removed component
                                }
                                if let Some(&factory) = self.column_factories.get(&tid) {
                                    new_arch.ensure_column_from_factory(tid, factory);
                                }
                                if let Some(&col_idx) = new_arch.type_to_column.get(&tid) {
                                    new_arch.columns[col_idx].push_raw(value);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.entity_archetype.insert(entity.index(), new_arch_id);

        // Clean up change ticks for removed component
        if let Some(ticks) = self.change_ticks.get_mut(&type_id) {
            ticks.remove(&entity.index());
        }

        true
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

    /// Propagate transforms down the entity hierarchy.
    ///
    /// For every entity with a `Transform` and `Parent`, computes the
    /// `GlobalTransform` by combining the parent's global matrix with
    /// the child's local transform. Entities without a parent get their
    /// `GlobalTransform` set directly from their local `Transform`.
    ///
    /// Uses a topological walk via the `ChildOf` relation so that parents
    /// are always processed before children.
    pub fn propagate_transforms(&mut self) {
        use quasar_math::transform::{GlobalTransform, Transform};

        // Phase 1: roots — entities with Transform but no Parent
        let roots: Vec<Entity> = self
            .query::<Transform>()
            .iter()
            .filter(|(e, _)| self.get::<Parent>(*e).is_none())
            .map(|(e, _)| *e)
            .collect();

        for root in &roots {
            if let Some(t) = self.get::<Transform>(*root).copied() {
                let gt = GlobalTransform::from(t);
                self.insert(*root, gt);
            }
        }

        // Phase 2: BFS children
        let mut queue: std::collections::VecDeque<Entity> = roots.into_iter().collect();

        while let Some(parent) = queue.pop_front() {
            let parent_matrix = self
                .get::<GlobalTransform>(parent)
                .map(|g| g.matrix)
                .unwrap_or(quasar_math::Mat4::IDENTITY);

            let children: Vec<Entity> = self.children_of(parent).to_vec();
            for child in children {
                if let Some(local) = self.get::<Transform>(child).copied() {
                    let gt = GlobalTransform::from_matrix(parent_matrix * local.matrix());
                    self.insert(child, gt);
                }
                queue.push_back(child);
            }
        }
    }

    // ------------------------------------------------------------------
    // Observers
    // ------------------------------------------------------------------

    /// Register an observer callback for a component lifecycle event.
    ///
    /// # Example
    /// ```ignore
    /// world.observe::<OnAdd<Health>>(|entity| {
    ///     println!("Entity {:?} gained Health", entity);
    /// });
    /// ```
    pub fn observe<E: ObserverEvent>(&mut self, callback: impl Fn(Entity) + Send + Sync + 'static) {
        let key = (E::kind(), E::component_type_id());
        self.observers
            .entry(key)
            .or_default()
            .push(Box::new(callback));
    }

    /// Fire observer callbacks for a specific event kind and component type.
    fn fire_observers(&self, kind: ObserverKind, type_id: TypeId, entity: Entity) {
        if let Some(callbacks) = self.observers.get(&(kind, type_id)) {
            for cb in callbacks {
                cb(entity);
            }
        }
    }

    // ------------------------------------------------------------------
    // Entity Cloning
    // ------------------------------------------------------------------

    /// Clone an entity, producing a new entity with copies of all components.
    ///
    /// Returns `None` if the source entity is not alive. Uses column-level
    /// bitwise copy for each component.
    pub fn clone_entity(&mut self, src: Entity) -> Option<Entity> {
        if !self.is_alive(src) {
            return None;
        }

        let components = self.entity_components.get(&src.index())?.clone();
        let new_entity = self.spawn();

        let src_arch_id = *self.entity_archetype.get(&src.index())?;

        // Collect raw bytes and factory for each component from the source archetype.
        // We copy bytes out so we can release the immutable borrow.
        struct ClonedComponent {
            type_id: TypeId,
            bytes: Vec<u8>,
            factory: fn() -> Box<dyn ColumnStorage>,
        }

        let mut cloned: Vec<ClonedComponent> = Vec::new();
        if let Some(src_arch) = self.archetype_graph.get(src_arch_id) {
            if let Some(&src_row) = src_arch.entity_to_row.get(&src.index()) {
                for &type_id in &components {
                    if let Some(&col_idx) = src_arch.type_to_column.get(&type_id) {
                        let col = &src_arch.columns[col_idx];
                        let elem_size = col.element_size();
                        if elem_size > 0 {
                            let ptr = col.raw_ptr();
                            let mut bytes = vec![0u8; elem_size];
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    ptr.add(src_row * elem_size),
                                    bytes.as_mut_ptr(),
                                    elem_size,
                                );
                            }
                            let factory = self
                                .column_factories
                                .get(&type_id)
                                .copied()
                                .unwrap_or(|| Box::new(TypedColumn::<()>::new()));
                            cloned.push(ClonedComponent {
                                type_id,
                                bytes,
                                factory,
                            });
                        }
                    }
                }
            }
        }

        // Build the destination archetype signature.
        let mut sig = ArchetypeSignature::new();
        for c in &cloned {
            sig.add(c.type_id);
        }

        let dest_arch_id = self.archetype_graph.get_or_create(&sig);

        // Push into destination archetype, creating columns as needed.
        if let Some(dest) = self.archetype_graph.get_mut(dest_arch_id) {
            dest.add_entity(new_entity);
            for c in &cloned {
                dest.ensure_column_from_factory(c.type_id, c.factory);
                if let Some(&col_idx) = dest.type_to_column.get(&c.type_id) {
                    dest.columns[col_idx].push_raw_bytes(&c.bytes, c.bytes.len());
                }
            }
        }

        // Update entity bookkeeping.
        self.entity_archetype
            .insert(new_entity.index(), dest_arch_id);
        self.entity_components
            .insert(new_entity.index(), components);

        Some(new_entity)
    }

    // -- Batch Operations --------------------------------------------

    /// Spawn multiple entities in a batch.
    /// Much faster than calling spawn() + insert() in a loop.
    pub fn spawn_batch<I, B>(&mut self, iter: I) -> Vec<Entity>
    where
        I: Iterator<Item = B> + ExactSizeIterator,
        B: Bundle,
    {
        let n = iter.len();
        if n == 0 {
            return Vec::new();
        }

        let mut entities = Vec::with_capacity(n);
        for _ in 0..n {
            entities.push(self.allocator.allocate());
        }

        for (entity, bundle) in entities.iter().copied().zip(iter) {
            bundle.insert_into(self, entity);
        }

        entities
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

        let results: Vec<_> = query!(world, Position, Velocity, Health)
            .into_iter()
            .collect();
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
            world.insert(
                e,
                Position {
                    x: i as f32,
                    y: 0.0,
                },
            );
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
            let matching = world.archetype_graph.find_with_components(&[
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
