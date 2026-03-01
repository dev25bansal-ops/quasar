//! Entity-Component-System framework for Quasar Engine.
//!
//! Provides a lightweight, type-safe ECS with:
//! - Generational entity IDs for safe reuse
//! - Type-erased component storage with `HashMap<Entity, T>` per type
//! - Query interface for iterating over entities with specific components
//! - System scheduling with ordered execution

mod component;
mod entity;
mod query;
mod system;
mod world;

pub use component::Component;
pub use entity::Entity;
pub use query::{Query, QueryIter};
pub use system::{Schedule, System, SystemStage};
pub use world::{EntityBuilder, World};
