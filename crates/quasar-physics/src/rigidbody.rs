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
/// Attach this (plus a `ColliderComponent`) to an entity and the physics
/// plugin will automatically synchronise its `Transform` each frame.
#[derive(Debug, Clone, Copy)]
pub struct RigidBodyComponent {
    /// Handle into the `PhysicsWorld` body set.
    pub handle: rapier3d::prelude::RigidBodyHandle,
    /// The type of body.
    pub body_type: BodyType,
}

impl RigidBodyComponent {
    pub fn new(handle: rapier3d::prelude::RigidBodyHandle, body_type: BodyType) -> Self {
        Self { handle, body_type }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_body_type_equality() {
        assert_eq!(BodyType::Dynamic, BodyType::Dynamic);
        assert_eq!(BodyType::Fixed, BodyType::Fixed);
        assert_ne!(BodyType::Dynamic, BodyType::Fixed);
    }

    #[test]
    fn test_body_type_variants() {
        let dynamic = BodyType::Dynamic;
        let fixed = BodyType::Fixed;
        let kinematic_pos = BodyType::KinematicPositionBased;
        let kinematic_vel = BodyType::KinematicVelocityBased;

        assert_ne!(dynamic, fixed);
        assert_ne!(kinematic_pos, kinematic_vel);
    }

    #[test]
    fn test_rigid_body_component_new() {
        let handle = rapier3d::prelude::RigidBodyHandle::from_raw_parts(0, 0);
        let component = RigidBodyComponent::new(handle, BodyType::Dynamic);

        assert_eq!(component.handle, handle);
        assert_eq!(component.body_type, BodyType::Dynamic);
    }

    #[test]
    fn test_rigid_body_component_fixed() {
        let handle = rapier3d::prelude::RigidBodyHandle::from_raw_parts(1, 0);
        let component = RigidBodyComponent::new(handle, BodyType::Fixed);

        assert_eq!(component.body_type, BodyType::Fixed);
    }

    #[test]
    fn test_rigid_body_component_kinematic() {
        let handle = rapier3d::prelude::RigidBodyHandle::from_raw_parts(2, 0);
        let component = RigidBodyComponent::new(handle, BodyType::KinematicPositionBased);

        assert_eq!(component.body_type, BodyType::KinematicPositionBased);
    }

    #[test]
    fn test_rigid_body_component_clone() {
        let handle = rapier3d::prelude::RigidBodyHandle::from_raw_parts(3, 0);
        let component = RigidBodyComponent::new(handle, BodyType::Dynamic);
        let cloned = component.clone();

        assert_eq!(cloned.handle, component.handle);
        assert_eq!(cloned.body_type, component.body_type);
    }

    #[test]
    fn test_rigid_body_component_copy() {
        let handle = rapier3d::prelude::RigidBodyHandle::from_raw_parts(4, 0);
        let component = RigidBodyComponent::new(handle, BodyType::Dynamic);
        let copied = component;

        assert_eq!(copied.handle, handle);
    }
}
