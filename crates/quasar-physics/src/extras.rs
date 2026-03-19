//! Additional physics components - sensors, CCD, materials, compound colliders.

use rapier3d::prelude::*;
use crate::world::PhysicsWorld;

/// Marker component enabling Continuous Collision Detection.
#[derive(Debug, Clone, Copy, Default)]
pub struct CcdEnabled;

/// Sensor component for trigger volumes.
#[derive(Debug, Clone)]
pub struct SensorComponent {
    pub handle: ColliderHandle,
    pub overlapping: bool,
}

impl SensorComponent {
    pub fn new(handle: ColliderHandle) -> Self {
        Self {
            handle,
            overlapping: false,
        }
    }
}

/// Pending sensor - attach to request automatic sensor creation.
#[derive(Debug, Clone)]
pub struct PendingSensor {
    pub shape: crate::collider::ColliderShape,
    pub parent_body: Option<RigidBodyHandle>,
    pub position: [f32; 3],
}

impl PendingSensor {
    pub fn new(shape: crate::collider::ColliderShape) -> Self {
        Self {
            shape,
            parent_body: None,
            position: [0.0; 3],
        }
    }
    
    pub fn with_body(mut self, body: RigidBodyHandle) -> Self {
        self.parent_body = Some(body);
        self
    }
}

/// Physics material for restitution and friction.
#[derive(Debug, Clone)]
pub struct PhysicsMaterial {
    pub restitution: f32,
    pub friction: f32,
}

impl Default for PhysicsMaterial {
    fn default() -> Self {
        Self { restitution: 0.3, friction: 0.5 }
    }
}

impl PhysicsMaterial {
    pub fn new(restitution: f32, friction: f32) -> Self {
        Self { restitution, friction }
    }
    
    pub fn ice() -> Self { Self { restitution: 0.1, friction: 0.02 } }
    pub fn rubber() -> Self { Self { restitution: 0.85, friction: 0.9 } }
    pub fn mud() -> Self { Self { restitution: 0.0, friction: 1.5 } }
    pub fn concrete() -> Self { Self { restitution: 0.3, friction: 0.7 } }
}

/// Component for compound colliders.
#[derive(Debug, Clone)]
pub struct CompoundColliderComponent {
    pub handle: ColliderHandle,
}

/// Pending compound collider.
#[derive(Debug, Clone)]
pub struct PendingCompoundCollider {
    pub shapes: Vec<(Isometry<f32>, crate::collider::ColliderShape)>,
    pub parent_body: RigidBodyHandle,
    pub material: PhysicsMaterial,
}

impl PendingCompoundCollider {
    pub fn new(parent_body: RigidBodyHandle) -> Self {
        Self {
            shapes: Vec::new(),
            parent_body,
            material: PhysicsMaterial::default(),
        }
    }
    
    pub fn add_shape(mut self, shape: crate::collider::ColliderShape, translation: [f32; 3]) -> Self {
        let iso = Isometry::new(
            nalgebra::vector![translation[0], translation[1], translation[2]],
            nalgebra::zero(),
        );
        self.shapes.push((iso, shape));
        self
    }
}

impl PhysicsWorld {
    pub fn add_sensor(
        &mut self,
        shape: &crate::collider::ColliderShape,
        parent_body: Option<RigidBodyHandle>,
        position: [f32; 3],
    ) -> ColliderHandle {
        let rapier_shape = shape.to_rapier();
        let iso = Isometry::new(
            nalgebra::vector![position[0], position[1], position[2]],
            nalgebra::zero(),
        );
        
        let builder = ColliderBuilder::new(rapier_shape)
            .sensor(true)
            .position(iso);
        
        match parent_body {
            Some(body) => self.colliders.insert_with_parent(builder.build(), body, &mut self.bodies),
            None => self.colliders.insert(builder.build()),
        }
    }
    
    pub fn add_compound_collider(
        &mut self,
        shapes: &[(Isometry<f32>, crate::collider::ColliderShape)],
        parent_body: RigidBodyHandle,
        material: &PhysicsMaterial,
    ) -> ColliderHandle {
        let rapier_shapes: Vec<(Isometry<f32>, SharedShape)> = shapes
            .iter()
            .map(|(iso, shape)| (*iso, shape.to_rapier()))
            .collect();
        
        let compound = SharedShape::compound(rapier_shapes);
        
        let builder = ColliderBuilder::new(compound)
            .restitution(material.restitution)
            .friction(material.friction);
        
        self.colliders.insert_with_parent(builder.build(), parent_body, &mut self.bodies)
    }
    
    pub fn enable_ccd(&mut self, body_handle: RigidBodyHandle, enable: bool) {
        if let Some(body) = self.bodies.get_mut(body_handle) {
            body.enable_ccd(enable);
        }
    }
}
