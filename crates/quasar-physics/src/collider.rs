//! Collider component — defines collision shapes for physics entities.

/// Component linking an ECS entity to a Rapier collider.
pub struct ColliderHandle {
    pub handle: rapier3d::prelude::ColliderHandle,
}

/// Common collider shapes.
#[derive(Debug, Clone)]
pub enum ColliderShape {
    Box { half_extents: [f32; 3] },
    Sphere { radius: f32 },
    Capsule { half_height: f32, radius: f32 },
}
