//! Rigid body component — links an ECS entity to a Rapier rigid body.

/// Describes the type of rigid body to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    /// Fully simulated — affected by forces and collisions.
    Dynamic,
    /// Stationary — collides but never moves.
    Fixed,
    /// Moves via position — unaffected by forces but interacts with dynamic bodies.
    KinematicPositionBased,
    /// Moves via velocity — unaffected by forces but interacts with dynamic bodies.
    KinematicVelocityBased,
}

/// ECS component linking an entity to a Rapier rigid body.
///
/// Attach this (plus a [`ColliderComponent`]) to an entity and the physics
/// plugin will automatically synchronise its [`Transform`] each frame.
#[derive(Debug, Clone, Copy)]
pub struct RigidBodyComponent {
    /// Handle into the [`PhysicsWorld`] body set.
    pub handle: rapier3d::prelude::RigidBodyHandle,
    /// The type of body.
    pub body_type: BodyType,
}

impl RigidBodyComponent {
    pub fn new(handle: rapier3d::prelude::RigidBodyHandle, body_type: BodyType) -> Self {
        Self { handle, body_type }
    }
}
