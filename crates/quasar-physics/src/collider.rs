//! Collider component — defines collision shapes for physics entities.

use rapier3d::prelude::SharedShape;

/// ECS component linking an entity to a Rapier collider.
#[derive(Debug, Clone, Copy)]
pub struct ColliderComponent {
    pub handle: rapier3d::prelude::ColliderHandle,
}

impl ColliderComponent {
    pub fn new(handle: rapier3d::prelude::ColliderHandle) -> Self {
        Self { handle }
    }
}

/// High-level collider shape description.
///
/// Use these values when creating physics bodies; the physics world will
/// convert them into the underlying Rapier shared shapes.
#[derive(Debug, Clone)]
pub enum ColliderShape {
    /// Axis-aligned box with half-extents `[hx, hy, hz]`.
    Box { half_extents: [f32; 3] },
    /// Sphere with a given `radius`.
    Sphere { radius: f32 },
    /// Vertical capsule (Y-axis).
    Capsule { half_height: f32, radius: f32 },
    /// Infinite ground plane (useful for floors).
    HalfSpace,
    /// Cylinder with half-height and radius.
    Cylinder { half_height: f32, radius: f32 },
    /// Cone with half-height and radius.
    Cone { half_height: f32, radius: f32 },
    /// Height-field terrain collider.
    ///
    /// `heights` is a row-major grid of `nrows × ncols` elevation values.
    /// `scale` applies to the (x, y, z) axes of the resulting shape.
    HeightField {
        nrows: usize,
        ncols: usize,
        heights: Vec<f32>,
        scale: [f32; 3],
    },
}

impl ColliderShape {
    /// Convert to a Rapier `SharedShape`.
    pub fn to_rapier(&self) -> SharedShape {
        match self {
            Self::Box { half_extents } => {
                SharedShape::cuboid(half_extents[0], half_extents[1], half_extents[2])
            }
            Self::Sphere { radius } => SharedShape::ball(*radius),
            Self::Capsule {
                half_height,
                radius,
            } => SharedShape::capsule_y(*half_height, *radius),
            Self::HalfSpace => {
                SharedShape::halfspace(nalgebra::Unit::new_normalize(nalgebra::vector![
                    0.0, 1.0, 0.0
                ]))
            }
            Self::Cylinder {
                half_height,
                radius,
            } => SharedShape::cylinder(*half_height, *radius),
            Self::Cone {
                half_height,
                radius,
            } => SharedShape::cone(*half_height, *radius),
            Self::HeightField {
                nrows,
                ncols,
                heights,
                scale,
            } => {
                let matrix = nalgebra::DMatrix::from_row_slice(*nrows, *ncols, heights);
                SharedShape::heightfield(matrix, nalgebra::vector![scale[0], scale[1], scale[2]])
            }
        }
    }
}

/// ECS component: attach to an entity to request automatic collider creation.
///
/// The [`ColliderSyncSystem`](crate::plugin::ColliderSyncSystem) will pick up
/// entities with this component, create the Rapier collider, insert a
/// [`ColliderComponent`], and remove the `PendingCollider`.
#[derive(Debug, Clone)]
pub struct PendingCollider {
    pub shape: ColliderShape,
    /// Optional parent rigid body handle.  When `None` the collider is
    /// inserted as static geometry (no parent body).
    pub parent_body: Option<rapier3d::prelude::RigidBodyHandle>,
    /// Position for static colliders (ignored when `parent_body` is `Some`).
    pub position: [f32; 3],
    pub restitution: f32,
    pub friction: f32,
}

impl PendingCollider {
    /// Create a pending static collider (no rigid body parent).
    pub fn new_static(shape: ColliderShape, position: [f32; 3]) -> Self {
        Self {
            shape,
            parent_body: None,
            position,
            restitution: 0.3,
            friction: 0.5,
        }
    }

    /// Create a pending collider attached to a rigid body.
    pub fn with_body(shape: ColliderShape, body: rapier3d::prelude::RigidBodyHandle) -> Self {
        Self {
            shape,
            parent_body: Some(body),
            position: [0.0; 3],
            restitution: 0.3,
            friction: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_shape_converts() {
        let shape = ColliderShape::Box {
            half_extents: [1.0, 2.0, 3.0],
        };
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn sphere_shape_converts() {
        let shape = ColliderShape::Sphere { radius: 0.5 };
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn capsule_shape_converts() {
        let shape = ColliderShape::Capsule {
            half_height: 1.0,
            radius: 0.5,
        };
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn cylinder_shape_converts() {
        let shape = ColliderShape::Cylinder {
            half_height: 1.0,
            radius: 0.5,
        };
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn cone_shape_converts() {
        let shape = ColliderShape::Cone {
            half_height: 1.0,
            radius: 0.5,
        };
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn halfspace_shape_converts() {
        let shape = ColliderShape::HalfSpace;
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn heightfield_shape_converts() {
        let shape = ColliderShape::HeightField {
            nrows: 2,
            ncols: 2,
            heights: vec![0.0, 1.0, 1.0, 2.0],
            scale: [1.0, 1.0, 1.0],
        };
        let _rapier = shape.to_rapier();
    }

    #[test]
    fn collider_component_new() {
        let handle = rapier3d::prelude::ColliderHandle::from_raw_parts(0, 0);
        let component = ColliderComponent::new(handle);
        assert_eq!(component.handle, handle);
    }

    #[test]
    fn collider_component_clone() {
        let handle = rapier3d::prelude::ColliderHandle::from_raw_parts(1, 0);
        let component = ColliderComponent::new(handle);
        let cloned = component.clone();
        assert_eq!(cloned.handle, handle);
    }

    #[test]
    fn pending_collider_new_static() {
        let pending =
            PendingCollider::new_static(ColliderShape::Sphere { radius: 1.0 }, [0.0, 0.0, 0.0]);

        assert!(pending.parent_body.is_none());
        assert_eq!(pending.position, [0.0, 0.0, 0.0]);
        assert_eq!(pending.restitution, 0.3);
        assert_eq!(pending.friction, 0.5);
    }

    #[test]
    fn pending_collider_with_body() {
        let body_handle = rapier3d::prelude::RigidBodyHandle::from_raw_parts(0, 0);
        let pending = PendingCollider::with_body(
            ColliderShape::Box {
                half_extents: [1.0, 1.0, 1.0],
            },
            body_handle,
        );

        assert!(pending.parent_body.is_some());
        assert_eq!(pending.parent_body.unwrap(), body_handle);
    }

    #[test]
    fn collider_shape_clone() {
        let shape = ColliderShape::Sphere { radius: 1.0 };
        let cloned = shape.clone();

        if let ColliderShape::Sphere { radius } = cloned {
            assert_eq!(radius, 1.0);
        } else {
            panic!("Expected Sphere shape");
        }
    }
}
