//! Rigid body component — links an ECS entity to a Rapier rigid body.

/// Describes the type of rigid body to create.
#[derive(Debug, Clone, Copy)]
pub enum RigidBodyType {
    /// Fully simulated — affected by forces and collisions.
    Dynamic,
    /// Stationary — collides but never moves.
    Fixed,
    /// Moves via velocity only — not affected by forces.
    KinematicVelocityBased,
}

/// Component linking an ECS entity to a Rapier rigid body.
pub struct RigidBodyHandle {
    pub handle: rapier3d::prelude::RigidBodyHandle,
}
