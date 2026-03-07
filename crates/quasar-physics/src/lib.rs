//! # Quasar Physics
//!
//! Physics simulation powered by [Rapier3D](https://rapier.rs/).
//!
//! Provides rigid body dynamics, collision detection, ray-casting, and
//! a plugin that synchronises Rapier transforms ↔ ECS [`Transform`] components.

pub mod character_controller;
pub mod collider;
pub mod events;
pub mod joints;
pub mod plugin;
pub mod rigidbody;
pub mod world;

pub use character_controller::{
    CharacterControllerComponent, CharacterControllerConfig, CharacterMovementResult,
};
pub use collider::{ColliderComponent, ColliderShape};
pub use events::{
    CollisionEvent, CollisionEventType, CollisionStartEvent, CollisionStopEvent, SensorEnterEvent,
    SensorExitEvent,
};
pub use joints::{JointComponent, JointKind};
pub use plugin::{PhysicsPlugin, PhysicsResource};
pub use rigidbody::{BodyType, RigidBodyComponent};
pub use world::PhysicsWorld;

/// Re-export rapier types for advanced users.
pub use rapier3d;
