//! Collision events — provides event types for physics collisions.
//!
//! Rapier3D supports collision events (contact start/stop) that can be
//! piped into the engine's Events bus for game logic to react to
//! collisions without polling.

use quasar_core::ecs::Entity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionEntity {
    pub entity: Entity,
    pub collider_handle: rapier3d::prelude::ColliderHandle,
}

#[derive(Debug, Clone)]
pub struct CollisionStartEvent {
    pub entity1: CollisionEntity,
    pub entity2: CollisionEntity,
    pub contact_point: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct CollisionStopEvent {
    pub entity1: CollisionEntity,
    pub entity2: CollisionEntity,
}

#[derive(Debug, Clone)]
pub struct CollisionEvent {
    pub entity1: Entity,
    pub entity2: Entity,
    pub event_type: CollisionEventType,
    pub contact_point: Option<[f32; 3]>,
    pub normal: Option<[f32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionEventType {
    Started,
    Stopped,
}

pub struct SensorEnterEvent {
    pub sensor_entity: Entity,
    pub other_entity: Entity,
}

pub struct SensorExitEvent {
    pub sensor_entity: Entity,
    pub other_entity: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use quasar_core::ecs::World;

    #[test]
    fn collision_event_types() {
        let mut world = World::new();
        let entity1 = world.spawn();
        let entity2 = world.spawn();

        let event = CollisionEvent {
            entity1,
            entity2,
            event_type: CollisionEventType::Started,
            contact_point: Some([0.0, 0.0, 0.0]),
            normal: Some([0.0, 1.0, 0.0]),
        };
        assert_eq!(event.event_type, CollisionEventType::Started);
    }
}
