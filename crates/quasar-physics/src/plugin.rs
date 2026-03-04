//! Physics plugin — integrates the physics world into the ECS runtime.
//!
//! # How it works
//!
//! 1. In **PostUpdate**, the plugin steps the Rapier simulation.
//! 2. After stepping, it reads the new positions/rotations from Rapier and
//!    writes them back into the ECS `Transform` components.
//! 3. Collision events are collected and piped into the [`Events`] bus.

use quasar_core::ecs::{Entity, System, World};
use quasar_core::Events;
use quasar_math::Transform;

use crate::collider::ColliderComponent;
use crate::events::{CollisionEvent, CollisionEventType};
use crate::rigidbody::RigidBodyComponent;
use crate::world::PhysicsWorld;

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

pub struct PhysicsStepSystem;

impl System for PhysicsStepSystem {
    fn name(&self) -> &str {
        "physics_step"
    }

    fn run(&mut self, world: &mut World) {
        if let Some(resource) = world.resource_mut::<PhysicsResource>() {
            resource.physics.step();
        }

        let handles: Vec<(Entity, rapier3d::prelude::RigidBodyHandle)> = world
            .query::<RigidBodyComponent>()
            .map(|(e, rbc)| (e, rbc.handle))
            .collect();

        if handles.is_empty() {
            return;
        }

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
        app.world.insert_resource(PhysicsResource::new());
        app.world.insert_resource(Events::new());

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

        log::info!("PhysicsPlugin loaded — Rapier3D simulation active");
    }
}
