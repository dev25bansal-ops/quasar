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
                SharedShape::halfspace(nalgebra::Unit::new_normalize(nalgebra::vector![0.0, 1.0, 0.0]))
            }
            Self::Cylinder {
                half_height,
                radius,
            } => SharedShape::cylinder(*half_height, *radius),
            Self::Cone {
                half_height,
                radius,
            } => SharedShape::cone(*half_height, *radius),
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
}
