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
pub use collider::{ColliderComponent, ColliderShape, PendingCollider};
pub use events::{
    CollisionEvent, CollisionEventType, CollisionStartEvent, CollisionStopEvent, SensorEnterEvent,
    SensorExitEvent, TriggerEnterEvent, TriggerStayEvent, TriggerExitEvent, TriggerTracker,
};
pub use joints::{JointComponent, JointKind, JointMotor, MotorMode, apply_motor_to_joint, set_joint_motor_velocity, set_joint_motor_position};
pub use plugin::{PhysicsPlugin, PhysicsResource};
pub use rigidbody::{BodyType, RigidBodyComponent};
pub use world::PhysicsWorld;

/// Re-export rapier types for advanced users.
pub use rapier3d;
