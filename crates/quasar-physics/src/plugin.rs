//! Physics plugin — integrates the physics world into the ECS runtime.
//!
//! # How it works
//!
//! 1. In **PostUpdate**, the plugin steps the Rapier simulation using a fixed timestep.
//! 2. After stepping, it reads the new positions/rotations from Rapier and
//!    writes them back into the ECS `Transform` components.
//! 3. Collision events are collected and routed through `App.events`.
//! 4. Transform mutations from ECS are written back to physics bodies before each step.

use quasar_core::ecs::{Entity, System, World};
use quasar_core::Events;
use quasar_math::Transform;

use crate::collider::ColliderComponent;
use crate::events::{CollisionEvent, CollisionEventType};
use crate::rigidbody::RigidBodyComponent;
use crate::world::PhysicsWorld;

/// Fixed physics timestep in seconds (60 Hz).
pub const PHYSICS_FIXED_DT: f32 = 1.0 / 60.0;

/// Resource holding physics accumulator state for fixed timestep.
pub struct PhysicsResource {
    pub physics: PhysicsWorld,
    /// Time accumulator for fixed timestep physics.
    pub accumulator: f32,
}

impl PhysicsResource {
    pub fn new() -> Self {
        Self {
            physics: PhysicsWorld::new(),
            accumulator: 0.0,
        }
    }

    pub fn with_gravity(gx: f32, gy: f32, gz: f32) -> Self {
        Self {
            physics: PhysicsWorld::with_gravity(gx, gy, gz),
            accumulator: 0.0,
        }
    }
}

impl Default for PhysicsResource {
    fn default() -> Self {
        Self::new()
    }
}

/// System that writes ECS Transform changes back to physics bodies.
///
/// This should run BEFORE physics step to capture any teleportations
/// or scripted movements.
pub struct TransformWritebackSystem;

impl System for TransformWritebackSystem {
    fn name(&self) -> &str {
        "transform_writeback"
    }

    fn run(&mut self, world: &mut World) {
        let handles: Vec<(rapier3d::prelude::RigidBodyHandle, Transform)> = world
            .query2::<RigidBodyComponent, Transform>()
            .map(|(_, rb, tf)| (rb.handle, *tf))
            .collect();

        if handles.is_empty() {
            return;
        }

        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            for (handle, tf) in handles {
                resource
                    .physics
                    .set_body_position(handle, tf.position.into());
                resource
                    .physics
                    .set_body_rotation(handle, tf.rotation.into());
            }
        }
    }
}

/// System that steps physics with fixed timestep accumulator.
///
/// Uses the accumulator pattern:
/// ```ignore
/// accumulator += delta_time;
/// while accumulator >= FIXED_DT {
///     physics.step();
///     accumulator -= FIXED_DT;
/// }
/// ```
pub struct PhysicsStepSystem;

impl System for PhysicsStepSystem {
    fn name(&self) -> &str {
        "physics_step"
    }

    fn run(&mut self, world: &mut World) {
        let delta = world
            .resource::<quasar_core::TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        let handles: Vec<(Entity, rapier3d::prelude::RigidBodyHandle)> = world
            .query::<RigidBodyComponent>()
            .map(|(e, rbc)| (e, rbc.handle))
            .collect();

        if handles.is_empty() {
            return;
        }

        // Fixed timestep physics with accumulator
        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            resource.accumulator += delta;

            while resource.accumulator >= PHYSICS_FIXED_DT {
                resource.physics.step();
                resource.accumulator -= PHYSICS_FIXED_DT;
            }
        }

        // Read physics positions back to transforms
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

        for (entity, pos, rot) in positions_to_write {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.position = glam::Vec3::new(pos[0], pos[1], pos[2]);
                transform.rotation = glam::Quat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
            }
        }
    }
}

pub struct CollisionEventSystem {
    sender: crossbeam::channel::Sender<rapier3d::prelude::CollisionEvent>,
    receiver: crossbeam::channel::Receiver<rapier3d::prelude::CollisionEvent>,
}

impl CollisionEventSystem {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        Self { sender, receiver }
    }

    pub fn channel(&self) -> crossbeam::channel::Sender<rapier3d::prelude::CollisionEvent> {
        self.sender.clone()
    }
}

impl Default for CollisionEventSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for CollisionEventSystem {
    fn name(&self) -> &str {
        "collision_events"
    }

    fn run(&mut self, world: &mut World) {
        let collider_to_entity: std::collections::HashMap<
            rapier3d::prelude::ColliderHandle,
            Entity,
        > = world
            .query::<ColliderComponent>()
            .map(|(e, collider)| (collider.handle, e))
            .collect();

        // Use Events from world resource (which is synced from App.events)
        if let Some(events) = world.resource_mut::<Events>() {
            use rapier3d::prelude::CollisionEvent as RapierCollisionEvent;

            while let Ok(collision_event) = self.receiver.try_recv() {
                match collision_event {
                    RapierCollisionEvent::Started(collider1, collider2, _) => {
                        if let (Some(&entity1), Some(&entity2)) = (
                            collider_to_entity.get(&collider1),
                            collider_to_entity.get(&collider2),
                        ) {
                            events.send(CollisionEvent {
                                entity1,
                                entity2,
                                event_type: CollisionEventType::Started,
                                contact_point: None,
                                normal: None,
                            });
                        }
                    }
                    RapierCollisionEvent::Stopped(collider1, collider2, _) => {
                        if let (Some(&entity1), Some(&entity2)) = (
                            collider_to_entity.get(&collider1),
                            collider_to_entity.get(&collider2),
                        ) {
                            events.send(CollisionEvent {
                                entity1,
                                entity2,
                                event_type: CollisionEventType::Stopped,
                                contact_point: None,
                                normal: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

pub struct PhysicsPlugin {
    enable_collision_events: bool,
}

impl PhysicsPlugin {
    pub fn new() -> Self {
        Self {
            enable_collision_events: true,
        }
    }

    pub fn without_collision_events() -> Self {
        Self {
            enable_collision_events: false,
        }
    }
}

impl Default for PhysicsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl quasar_core::Plugin for PhysicsPlugin {
    fn name(&self) -> &str {
        "PhysicsPlugin"
    }

    fn build(&self, app: &mut quasar_core::App) {
        // Insert physics resource with accumulator
        app.world.insert_resource(PhysicsResource::new());

        // PreUpdate: Write transforms back to physics (before step)
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PreUpdate,
            Box::new(TransformWritebackSystem),
        );

        // PostUpdate: Step physics with fixed timestep
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::PostUpdate,
            Box::new(PhysicsStepSystem),
        );

        if self.enable_collision_events {
            app.schedule.add_system(
                quasar_core::ecs::SystemStage::PostUpdate,
                Box::new(CollisionEventSystem::new()),
            );
        }

        log::info!(
            "PhysicsPlugin loaded — Rapier3D simulation active (fixed timestep {}Hz)",
            (1.0 / PHYSICS_FIXED_DT) as u32
        );
    }
}
