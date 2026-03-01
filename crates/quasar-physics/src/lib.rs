//! # Quasar Physics
//!
//! Physics simulation powered by [Rapier3D](https://rapier.rs/).
//!
//! Provides rigid body dynamics, collision detection, ray-casting, and
//! a plugin that synchronises Rapier transforms ↔ ECS [`Transform`] components.

pub mod collider;
pub mod plugin;
pub mod rigidbody;
pub mod world;

pub use collider::{ColliderComponent, ColliderShape};
pub use plugin::{PhysicsPlugin, PhysicsResource};
pub use rigidbody::{BodyType, RigidBodyComponent};
pub use world::PhysicsWorld;

/// Re-export rapier types for advanced users.
pub use rapier3d;
