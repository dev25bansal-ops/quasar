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

pub mod archetype;
mod commands;
mod component;
mod entity;
pub mod parallel;
mod query;
pub mod relation;
pub mod sparse_set;
mod system;
mod world;

pub use archetype::{
    Archetype, ArchetypeGraph, ArchetypeId, ArchetypeSignature,
};
pub use commands::{Command, Commands, EntitySpawnBuilder};
pub use component::Component;
pub use entity::Entity;
pub use parallel::{
    AccessMode, ComponentAccess, DeclareAccess, ParallelSchedule, ReadWriteSet, SystemAccess,
    SystemGraph, SystemNode, read_set, system_node_with_access, write_set,
};
pub use query::{
    FilterAdded, FilterChanged, FilterRemoved, FilterWith, FilterWithout, Query, QueryFilter,
    QueryIter, QueryState, WorldQuery,
};
pub use system::{Schedule, System, SystemStage};
pub use sparse_set::{SparseSet, SparseSetStorage};
pub use relation::{ChildOf, OwnedBy, Relation, RelationGraph};
pub use world::{Children, EntityBuilder, Parent, World};

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
