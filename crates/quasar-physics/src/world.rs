//! Physics world — wraps the Rapier3D simulation pipeline.
//!
//! Provides high-level methods for adding/removing bodies and colliders,
//! stepping the simulation, and synchronising transforms back to the ECS.

use rapier3d::prelude::*;

use crate::collider::ColliderShape;
use crate::rigidbody::BodyType;

/// A full physics simulation backed by Rapier3D.
pub struct PhysicsWorld {
    pub gravity: nalgebra::Vector3<f32>,
    pub integration_parameters: IntegrationParameters,
    pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
}

impl PhysicsWorld {
    /// Create a new physics world with Earth-like gravity (−9.81 m/s² on Y).
    pub fn new() -> Self {
        Self {
            gravity: nalgebra::vector![0.0, -9.81, 0.0],
            integration_parameters: IntegrationParameters::default(),
            pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
        }
    }

    /// Create a physics world with custom gravity.
    pub fn with_gravity(gx: f32, gy: f32, gz: f32) -> Self {
        let mut world = Self::new();
        world.gravity = nalgebra::vector![gx, gy, gz];
        world
    }

    // ------------------------------------------------------------------
    // Rigid-body management
    // ------------------------------------------------------------------

    /// Add a rigid body and return its handle.
    pub fn add_body(
        &mut self,
        body_type: BodyType,
        position: [f32; 3],
    ) -> RigidBodyHandle {
        let builder = match body_type {
            BodyType::Dynamic => RigidBodyBuilder::dynamic(),
            BodyType::Fixed => RigidBodyBuilder::fixed(),
            BodyType::KinematicPositionBased => RigidBodyBuilder::kinematic_position_based(),
            BodyType::KinematicVelocityBased => RigidBodyBuilder::kinematic_velocity_based(),
        };
        let rb = builder
            .translation(nalgebra::vector![position[0], position[1], position[2]])
            .build();
        self.bodies.insert(rb)
    }

    /// Remove a rigid body (and any attached colliders).
    pub fn remove_body(&mut self, handle: RigidBodyHandle) {
        self.bodies.remove(
            handle,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    /// Get the position of a rigid body as `[x, y, z]`.
    pub fn body_position(&self, handle: RigidBodyHandle) -> Option<[f32; 3]> {
        self.bodies.get(handle).map(|rb| {
            let t = rb.translation();
            [t.x, t.y, t.z]
        })
    }

    /// Get the rotation of a rigid body as `[x, y, z, w]` quaternion.
    pub fn body_rotation(&self, handle: RigidBodyHandle) -> Option<[f32; 4]> {
        self.bodies.get(handle).map(|rb| {
            let r = rb.rotation();
            [r.i, r.j, r.k, r.w]
        })
    }

    /// Apply a force to a dynamic rigid body.
    pub fn apply_force(&mut self, handle: RigidBodyHandle, force: [f32; 3]) {
        if let Some(rb) = self.bodies.get_mut(handle) {
            rb.add_force(nalgebra::vector![force[0], force[1], force[2]], true);
        }
    }

    /// Apply an impulse (instant velocity change) to a rigid body.
    pub fn apply_impulse(&mut self, handle: RigidBodyHandle, impulse: [f32; 3]) {
        if let Some(rb) = self.bodies.get_mut(handle) {
            rb.apply_impulse(nalgebra::vector![impulse[0], impulse[1], impulse[2]], true);
        }
    }

    /// Set linear velocity directly.
    pub fn set_linear_velocity(&mut self, handle: RigidBodyHandle, vel: [f32; 3]) {
        if let Some(rb) = self.bodies.get_mut(handle) {
            rb.set_linvel(nalgebra::vector![vel[0], vel[1], vel[2]], true);
        }
    }

    // ------------------------------------------------------------------
    // Collider management
    // ------------------------------------------------------------------

    /// Attach a collider to a rigid body.
    pub fn add_collider(
        &mut self,
        parent: RigidBodyHandle,
        shape: &ColliderShape,
        restitution: f32,
        friction: f32,
    ) -> ColliderHandle {
        let shared = shape.to_rapier();
        let collider = ColliderBuilder::new(shared)
            .restitution(restitution)
            .friction(friction)
            .build();
        self.colliders.insert_with_parent(collider, parent, &mut self.bodies)
    }

    /// Attach a collider without a parent body (static geometry).
    pub fn add_static_collider(
        &mut self,
        shape: &ColliderShape,
        position: [f32; 3],
    ) -> ColliderHandle {
        let shared = shape.to_rapier();
        let collider = ColliderBuilder::new(shared)
            .translation(nalgebra::vector![position[0], position[1], position[2]])
            .build();
        self.colliders.insert(collider)
    }

    /// Remove a collider.
    pub fn remove_collider(&mut self, handle: ColliderHandle) {
        self.colliders.remove(
            handle,
            &mut self.island_manager,
            &mut self.bodies,
            true,
        );
    }

    // ------------------------------------------------------------------
    // Ray-casting
    // ------------------------------------------------------------------

    /// Cast a ray into the physics world.  Returns `Some((collider, toi))` on hit.
    pub fn cast_ray(
        &self,
        origin: [f32; 3],
        direction: [f32; 3],
        max_toi: f32,
    ) -> Option<(ColliderHandle, f32)> {
        let ray = Ray::new(
            nalgebra::point![origin[0], origin[1], origin[2]],
            nalgebra::vector![direction[0], direction[1], direction[2]],
        );
        let filter = QueryFilter::default();
        let mut query_pipeline = QueryPipeline::new();
        query_pipeline.update(&self.colliders);
        query_pipeline
            .cast_ray(&self.bodies, &self.colliders, &ray, max_toi, true, filter)
            .map(|(handle, toi)| (handle, toi))
    }

    // ------------------------------------------------------------------
    // Simulation
    // ------------------------------------------------------------------

    /// Step the physics simulation by one tick.
    pub fn step(&mut self) {
        self.pipeline.step(
            &self.gravity.into(),
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
    }

    /// Step with a custom delta-time by temporarily overriding `dt`.
    pub fn step_with_dt(&mut self, dt: f32) {
        let prev = self.integration_parameters.dt;
        self.integration_parameters.dt = dt;
        self.step();
        self.integration_parameters.dt = prev;
    }

    /// Return the number of bodies in the world.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Return the number of colliders in the world.
    pub fn collider_count(&self) -> usize {
        self.colliders.len()
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_body_and_step() {
        let mut world = PhysicsWorld::new();
        let handle = world.add_body(BodyType::Dynamic, [0.0, 10.0, 0.0]);
        // Attach a collider so the body has mass (zero-mass bodies ignore gravity).
        world.add_collider(handle, &ColliderShape::Sphere { radius: 0.5 }, 0.5, 0.5);

        // Step a few ticks — the body should fall.
        for _ in 0..60 {
            world.step();
        }

        let pos = world.body_position(handle).unwrap();
        assert!(pos[1] < 10.0, "dynamic body should fall under gravity");
    }

    #[test]
    fn apply_impulse_changes_velocity() {
        let mut world = PhysicsWorld::new();
        let handle = world.add_body(BodyType::Dynamic, [0.0, 0.0, 0.0]);
        // Attach a collider so the body has mass (impulses need mass to work).
        world.add_collider(handle, &ColliderShape::Sphere { radius: 0.5 }, 0.5, 0.5);
        world.apply_impulse(handle, [100.0, 0.0, 0.0]);
        world.step();

        let pos = world.body_position(handle).unwrap();
        assert!(pos[0] > 0.0, "body should move in +X after impulse");
    }

    #[test]
    fn ray_cast_hits_collider() {
        let mut world = PhysicsWorld::new();
        let body = world.add_body(BodyType::Fixed, [0.0, 0.0, -5.0]);
        world.add_collider(body, &ColliderShape::Sphere { radius: 1.0 }, 0.5, 0.5);
        // Need to step once for broad-phase to register.
        world.step();

        let hit = world.cast_ray([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], 100.0);
        assert!(hit.is_some(), "ray should hit the sphere");
    }

    #[test]
    fn body_count_tracks() {
        let mut world = PhysicsWorld::new();
        assert_eq!(world.body_count(), 0);
        let h = world.add_body(BodyType::Dynamic, [0.0, 0.0, 0.0]);
        assert_eq!(world.body_count(), 1);
        world.remove_body(h);
        assert_eq!(world.body_count(), 0);
    }
}
