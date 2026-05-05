//! Entity-Component-System framework for Quasar Engine.
//!
//! Provides a lightweight, type-safe ECS with:
//! - Generational entity IDs for safe reuse
//! - Type-erased component storage with `HashMap<Entity, T>` per type
//! - Query interface for iterating over entities with specific components
//! - System scheduling with ordered execution
//! - Commands for deferred mutations (spawn/despawn between stages)
//! - Archetype-based storage for 5–50x query performance
//! - Parallel system execution with dependency graph
//!
//! # Architecture
//!
//! The ECS uses archetype-based storage:
//! - Entities are grouped by their component composition into "archetypes"
//! - Each archetype stores components in Structure-of-Arrays (SoA) columns
//! - Queries iterate over matching archetypes for cache-efficient access
//!
//! # Query API (v0.2.0+)
//!
//! **Recommended**: Use [`CachedArchetypeQueryState`] for all queries. It provides
//! zero-allocation, cache-friendly iteration by directly accessing archetype columns.
//!
//! ```rust,ignore
//! use quasar_core::ecs::*;
//!
//! // Create once, reuse across frames
//! let mut query: CachedArchetypeQueryState<(&Position, &Velocity), ()> =
//!     CachedArchetypeQueryState::new();
//!
//! for (entity, (pos, vel)) in query.iter(&world) {
//!     pos.x += vel.dx * dt;
//!     pos.y += vel.dy * dt;
//! }
//! ```
//!
//! **Deprecated**: `world.query::<T>()`, `world.query2::<A, B>()`, `QueryState::iter()`,
//! and `World::for_each_mut` are deprecated in favor of `CachedArchetypeQueryState`.
//! They allocate a `Vec` every call and iterate the entire `entity_components` HashMap
//! with binary search, resulting in ~95% slower performance.
//!
//! # Example
//!
//! ```rust,ignore
//! use quasar_core::ecs::*;
//!
//! // Define components
//! #[derive(Clone)]
//! struct Position { x: f32, y: f32 }
//!
//! #[derive(Clone)]
//! struct Velocity { dx: f32, dy: f32 }
//!
//! // Create world and spawn entities
//! let mut world = World::new();
//! let e = world.spawn();
//! world.insert(e, Position { x: 0.0, y: 0.0 });
//! world.insert(e, Velocity { dx: 1.0, dy: 0.0 });
//!
//! // Query for entities with both components (recommended way)
//! let mut query: CachedArchetypeQueryState<(&Position, &Velocity), ()> =
//!     CachedArchetypeQueryState::new();
//! for (e, (pos, vel)) in query.iter(&world) {
//!     println!("Entity {:?}: pos=({}, {}), vel=({}, {})",
//!              e, pos.x, pos.y, vel.dx, vel.dy);
//! }
//!
//! // Mutable iteration (recommended way)
//! let mut query: CachedArchetypeQueryState<&Position, ()> =
//!     CachedArchetypeQueryState::new();
//! for (e, pos) in query.iter(&world) {
//!     if let Some(p) = world.get_mut::<Position>(e) {
//!         // mutate p
//!     }
//! }
//! ```
//!
//! # Entity Lifecycle
//!
//! Entities are created with `World::spawn()` and destroyed with `World::despawn()`.
//! Entity IDs are generational - when an entity is despawned and its ID reused,
//! the generation increments to catch stale references.
//!
//! # Component Requirements
//!
//! Components must implement `Clone + Send + Sync + 'static`. This is automatically
//! satisfied for any type meeting these bounds thanks to a blanket implementation.
//!
//! # Parallel Queries
//!
//! For compute-heavy operations, use parallel iteration:
//!
//! ```rust,ignore
//! // Parallel iteration using rayon
//! world.par_for_each::<Position, _>(|e, pos| {
//!     // This closure runs in parallel across multiple threads
//!     process_position(pos);
//! });
//! ```

pub mod archetype;
mod commands;
mod component;
mod entity;
pub mod parallel;
mod query;
pub mod relation;
pub mod sparse_set;
mod system;
mod system_param;
mod world;

pub use archetype::{Archetype, ArchetypeGraph, ArchetypeId, ArchetypeSignature, ComponentTicks};
pub use commands::{Command, Commands, EntitySpawnBuilder};
pub use component::{Component, Mut};
pub use entity::Entity;
// Always re-export parallel types (they're always compiled, feature flag controls behavior).
pub use parallel::{
    read_set, system_node_with_access, write_set, ComponentAccess, ConflictGraph, DeclareAccess,
    ParallelBatch, ParallelSchedule, ReadWriteSet, SystemAccess, SystemGraph, SystemNode,
};
pub use query::{
    CachedArchetypeQueryIter, CachedArchetypeQueryState, FilterAdded, FilterChanged, FilterRemoved,
    FilterWith, FilterWithout, Query, Query2Iter, QueryFilter, QueryIter, QueryIterSingle,
    QueryMutRef, QueryRef, QueryState, QueryStateCache, QueryStateMut, QueryStateReadonly,
    Res, ResMut, ResMutRef, ResMutState, ResRef, ResState, SystemQuery, SystemQueryMut, WorldQuery,
    WorldQueryArchFetch,
};
pub use relation::{ChildOf, OwnedBy, Relation, RelationGraph};
pub use sparse_set::{SparseSet, SparseSetStorage};
pub use system::Schedule;
pub use system::{run_system_with, System, SystemExecutor, SystemStage, FnSystem};
pub use system_param::{
    Access, AccessKind, FnSystemWithParams, ParamSet, Read, SystemParam, SystemState, Write,
    system_fn,
};
pub use world::{
    Bundle, Children, EntityBuilder, ObserverEvent, ObserverKind, OnAdd, OnRemove, Parent,
    Prototype, World,
};

/// Marker type for change-detection queries.
/// Use with `World::query_changed::<T>(since_tick)` to find entities whose
/// component `T` was inserted or mutably accessed since a given tick.
pub struct Changed<T: Component>(std::marker::PhantomData<T>);

/// Marker for "entity must also have component W" filter.
pub struct With<T: Component>(std::marker::PhantomData<T>);

/// Marker for "entity must NOT have component W" filter.
pub struct Without<T: Component>(std::marker::PhantomData<T>);

/// Marker for "entity just had component T added" filter.
pub struct Added<T: Component>(std::marker::PhantomData<T>);

/// Marker for "entity just had component T removed" filter.
pub struct Removed<T: Component>(std::marker::PhantomData<T>);

/// Flush pending `Commands` from the world. Call between stages or after
/// system execution to apply deferred mutations.
pub fn flush_commands(world: &mut World) {
    if let Some(mut cmds) = world.remove_resource::<Commands>() {
        cmds.apply(world);
        world.insert_resource(cmds);
    }
}

#[macro_export]
macro_rules! query {
    ($world:expr, ($A:ty, $B:ty $(,)?)) => {
        $world.query2::<$A, $B>()
    };
    ($world:expr, ($A:ty, $B:ty, $C:ty $(,)?)) => {
        $world.query3::<$A, $B, $C>()
    };
    ($world:expr, ($A:ty, $B:ty, $C:ty, $D:ty $(,)?)) => {
        $world.query4::<$A, $B, $C, $D>()
    };
    ($world:expr, ($A:ty, $B:ty, $C:ty, $D:ty, $E:ty $(,)?)) => {
        $world.query5::<$A, $B, $C, $D, $E>()
    };
    ($world:expr, $A:ty, $B:ty $(,)?) => {
        $world.query2::<$A, $B>()
    };
    ($world:expr, $A:ty, $B:ty, $C:ty $(,)?) => {
        $world.query3::<$A, $B, $C>()
    };
    ($world:expr, $A:ty, $B:ty, $C:ty, $D:ty $(,)?) => {
        $world.query4::<$A, $B, $C, $D>()
    };
    ($world:expr, $A:ty, $B:ty, $C:ty, $D:ty, $E:ty $(,)?) => {
        $world.query5::<$A, $B, $C, $D, $E>()
    };
}
