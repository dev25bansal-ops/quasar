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

use quasar_core::ecs::{System, World};
use quasar_math::Transform;

use crate::rigidbody::RigidBodyComponent;
use crate::world::PhysicsWorld;

/// A resource wrapper so the physics world can live inside the ECS [`World`]
/// as a singleton component on a dedicated entity.
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
        // We store the PhysicsResource as a component on a dedicated "singleton" entity.
        // Find it via query.
        let mut positions_to_write: Vec<(u32, [f32; 3], [f32; 4])> = Vec::new();

        // Step physics and collect updated positions.
        {
            let mut iter = world.query_mut::<PhysicsResource>();
            if let Some((_entity, resource)) = iter.next() {
                resource.physics.step();

                // Collect body transforms for later write-back.
                for (_, rb) in resource.physics.bodies.iter() {
                    let t = rb.translation();
                    let r = rb.rotation();
                    // We use the body handle raw bits as a lookup key.
                    // (Bridge between rapier handle and our entity is via RigidBodyComponent)
                    let _ = (t, r); // positions collected below
                }
            }
        }

        // Sync: read RigidBodyComponent on each entity → look up Rapier body → write Transform.
        // We need two passes because we can't borrow PhysicsResource and Transform mutably at once.

        // Pass 1: collect (entity_index, body_handle) pairs.
        let handles: Vec<(u32, rapier3d::prelude::RigidBodyHandle)> = world
            .query::<RigidBodyComponent>()
            .map(|(e, rbc)| (e.index(), rbc.handle))
            .collect();

        // Pass 2: for each, read position from physics and write to Transform.
        if !handles.is_empty() {
            // Get physics resource once.
            if let Some(physics_world) = world
                .query::<PhysicsResource>()
                .next()
                .map(|(_, pr)| &pr.physics as *const PhysicsWorld)
            {
                for &(entity_idx, handle) in &handles {
                    // SAFETY: we only read from physics (immutable) and write to Transform.
                    let pw = unsafe { &*physics_world };
                    if let Some(pos) = pw.body_position(handle) {
                        if let Some(rot) = pw.body_rotation(handle) {
                            positions_to_write.push((entity_idx, pos, rot));
                        }
                    }
                }
            }
        }

        // Write-back pass.
        for (entity_idx, pos, rot) in positions_to_write {
            // Find the entity by index and update its Transform.
            for (entity, transform) in world.query_mut::<Transform>() {
                if entity.index() == entity_idx {
                    transform.position = glam::Vec3::new(pos[0], pos[1], pos[2]);
                    transform.rotation = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
                    break;
                }
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
        // Insert the physics resource as a component on a singleton entity.
        let singleton = app.world.spawn();
        app.world.insert(singleton, PhysicsResource::new());

        // Register the step system in PostUpdate.
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(PhysicsStepSystem),
        );

        log::info!("PhysicsPlugin loaded — Rapier3D simulation active");
    }
}
