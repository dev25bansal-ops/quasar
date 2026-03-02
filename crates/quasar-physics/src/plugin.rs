//! Physics plugin — integrates the physics world into the ECS runtime.
//!
//! # How it works
//!
//! 1. In **PostUpdate**, the plugin steps the Rapier simulation.
//! 2. After stepping, it reads the new positions/rotations from Rapier and
//!    writes them back into the ECS `Transform` components.
//!
//! Entities that should participate in physics must have:
//! - [`RigidBodyComponent`] — links to a Rapier rigid body.
//! - [`Transform`] — the spatial data read/written by the sync.

use quasar_core::ecs::{Entity, System, World};
use quasar_math::Transform;

use crate::rigidbody::RigidBodyComponent;
use crate::world::PhysicsWorld;

/// A resource wrapper so the physics world can live inside the ECS [`World`]
/// as a global resource via `insert_resource`.
pub struct PhysicsResource {
    pub physics: PhysicsWorld,
}

impl PhysicsResource {
    pub fn new() -> Self {
        Self {
            physics: PhysicsWorld::new(),
        }
    }

    pub fn with_gravity(gx: f32, gy: f32, gz: f32) -> Self {
        Self {
            physics: PhysicsWorld::with_gravity(gx, gy, gz),
        }
    }
}

impl Default for PhysicsResource {
    fn default() -> Self {
        Self::new()
    }
}

/// System that steps the physics simulation and syncs transforms back to ECS.
pub struct PhysicsStepSystem;

impl System for PhysicsStepSystem {
    fn name(&self) -> &str {
        "physics_step"
    }

    fn run(&mut self, world: &mut World) {
        // Step the physics simulation.
        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            resource.physics.step();
        }

        // Pass 1: collect (entity, body_handle) pairs from ECS.
        let handles: Vec<(Entity, rapier3d::prelude::RigidBodyHandle)> = world
            .query::<RigidBodyComponent>()
            .map(|(e, rbc)| (e, rbc.handle))
            .collect();

        if handles.is_empty() {
            return;
        }

        // Pass 2: read positions from physics resource (immutable borrow).
        let mut positions_to_write: Vec<(Entity, [f32; 3], [f32; 4])> = Vec::new();
        if let Some(resource) = world.resource::<PhysicsResource>() {
            for &(entity, handle) in &handles {
                if let Some(pos) = resource.physics.body_position(handle) {
                    if let Some(rot) = resource.physics.body_rotation(handle) {
                        positions_to_write.push((entity, pos, rot));
                    }
                }
            }
        }

        // Pass 3: write-back transforms — O(n) direct entity lookup.
        for (entity, pos, rot) in positions_to_write {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.position = glam::Vec3::new(pos[0], pos[1], pos[2]);
                transform.rotation = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
            }
        }
    }
}

/// Plugin that registers the physics step system into the PostUpdate stage.
pub struct PhysicsPlugin;

impl quasar_core::Plugin for PhysicsPlugin {
    fn name(&self) -> &str {
        "PhysicsPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        // Insert the physics resource as a World resource (not a component).
        app.world.insert_resource(PhysicsResource::new());

        // Register the step system in PostUpdate.
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(PhysicsStepSystem),
        );

        log::info!("PhysicsPlugin loaded — Rapier3D simulation active");
    }
}
