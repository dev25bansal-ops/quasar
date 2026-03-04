//! Entity-Component-System framework for Quasar Engine.
//!
//! Provides a lightweight, type-safe ECS with:
//! - Generational entity IDs for safe reuse
//! - Type-erased component storage with `HashMap<Entity, T>` per type
//! - Query interface for iterating over entities with specific components
//! - System scheduling with ordered execution
//! - Commands for deferred mutations (spawn/despawn between stages)

mod commands;
mod component;
mod entity;
mod query;
mod system;
mod world;

pub use commands::{Command, Commands, EntitySpawnBuilder};
pub use component::Component;
pub use entity::Entity;
pub use query::{Query, QueryIter};
pub use system::{Schedule, System, SystemStage};
pub use world::{EntityBuilder, World};

#[macro_export]
macro_rules! query {
    ($world:expr, ($A:ty, $B:ty $(,)?)) => {
        $world.query2::<$A, $B>()
    };
    ($world:expr, ($A:ty, $B:ty, $C:ty $(,)?)) => {
        $world.query3::<$A, $B, $C>()
    };
    ($world:expr, $A:ty, $B:ty $(,)?) => {
        $world.query2::<$A, $B>()
    };
    ($world:expr, $A:ty, $B:ty, $C:ty $(,)?) => {
        $world.query3::<$A, $B, $C>()
    };
}
