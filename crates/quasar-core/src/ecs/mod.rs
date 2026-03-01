//! Entity-Component-System framework for Quasar Engine.
//!
//! Provides a lightweight, type-safe ECS with:
//! - Generational entity IDs for safe reuse
//! - Type-erased component storage with `HashMap<Entity, T>` per type
//! - Query interface for iterating over entities with specific components
//! - System scheduling with ordered execution

mod entity;
mod component;
mod world;
mod query;
mod system;

pub use entity::Entity;
pub use component::Component;
pub use world::{World, EntityBuilder};
pub use query::{Query, QueryIter};
pub use system::{System, SystemStage, Schedule};
