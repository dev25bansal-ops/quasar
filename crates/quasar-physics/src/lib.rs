//! # Quasar Physics
//!
//! Physics simulation powered by [Rapier3D](https://rapier.rs/).
//!
//! Provides:
//! - **Rigid Body Dynamics**: Full rigid body simulation with Rapier3D
//! - **Collision Detection**: Broad phase and narrow phase collision
//! - **Character Controller**: Capsule-based character movement
//! - **Cloth Simulation**: Mass-spring cloth physics
//! - **Soft Body**: Deformable body simulation
//! - **Vehicle Physics**: Realistic vehicle dynamics
//! - **Rollback**: Network rollback for determinism

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod async_step;
pub mod character_controller;
pub mod cloth;
pub mod collider;
pub mod debug_draw;
pub mod events;
pub mod extras;
pub mod joints;
pub mod plugin;
pub mod rigidbody;
pub mod rollback;
pub mod soft_body;
pub mod vehicle;
pub mod world;

pub use async_step::{AsyncPhysicsStepper, InterpolationSnapshot, PhysicsCommand};
pub use character_controller::{
    CharacterControllerComponent, CharacterControllerConfig, CharacterMovementResult,
};
pub use cloth::{ClothConfig, ClothMesh, ClothParticle, DistanceConstraint};
pub use collider::{ColliderComponent, ColliderShape, PendingCollider};
pub use debug_draw::{DebugDrawColors, DebugDrawConfig, DebugLine};
pub use events::{
    CollisionEvent, CollisionEventType, CollisionStartEvent, CollisionStopEvent, SensorEnterEvent,
    SensorExitEvent, TriggerEnterEvent, TriggerExitEvent, TriggerStayEvent, TriggerTracker,
};
pub use extras::{
    CcdEnabled, CompoundColliderComponent, PendingCompoundCollider, PendingSensor, PhysicsMaterial,
    SensorComponent,
};
pub use joints::{
    apply_motor_to_joint, set_joint_motor_position, set_joint_motor_velocity, JointComponent,
    JointKind, JointMotor, MotorMode,
};
pub use plugin::{PhysicsPlugin, PhysicsResource};
pub use rigidbody::{BodyType, RigidBodyComponent};
pub use rollback::{ColliderState, JointState, PhysicsSnapshot, RigidBodyState, RollbackManager};
pub use soft_body::{SoftBody, SoftBodyConfig, SoftBodyParticle, Spring, Tetrahedron};
pub use vehicle::{SuspensionConfig, TireConfig, Vehicle, VehicleConfig, VehicleInput, Wheel};
pub use world::PhysicsWorld;

pub use rapier3d;
