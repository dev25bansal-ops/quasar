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

// ── Trigger zone events ─────────────────────────────────────────

/// Fired when an entity first enters a trigger volume.
#[derive(Debug, Clone)]
pub struct TriggerEnterEvent {
    pub trigger_entity: Entity,
    pub other_entity: Entity,
    pub trigger_collider: rapier3d::prelude::ColliderHandle,
    pub other_collider: rapier3d::prelude::ColliderHandle,
}

/// Fired every tick while an entity remains inside a trigger volume.
#[derive(Debug, Clone)]
pub struct TriggerStayEvent {
    pub trigger_entity: Entity,
    pub other_entity: Entity,
}

/// Fired when an entity exits a trigger volume.
#[derive(Debug, Clone)]
pub struct TriggerExitEvent {
    pub trigger_entity: Entity,
    pub other_entity: Entity,
}

/// Tracks which entity pairs are currently overlapping with trigger volumes
/// so we can generate Enter/Stay/Exit events correctly.
#[derive(Debug, Clone, Default)]
pub struct TriggerTracker {
    /// Set of (trigger_entity, other_entity) currently overlapping.
    active_pairs: std::collections::HashSet<(Entity, Entity)>,
}

impl TriggerTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process this tick's overlapping pairs.
    /// Returns (enter_events, stay_events, exit_events).
    pub fn update(
        &mut self,
        current_pairs: &[(
            Entity,
            Entity,
            rapier3d::prelude::ColliderHandle,
            rapier3d::prelude::ColliderHandle,
        )],
    ) -> (
        Vec<TriggerEnterEvent>,
        Vec<TriggerStayEvent>,
        Vec<TriggerExitEvent>,
    ) {
        let mut enters = Vec::new();
        let mut stays = Vec::new();
        let mut exits = Vec::new();

        let mut new_active = std::collections::HashSet::new();

        for (trigger_e, other_e, trigger_c, other_c) in current_pairs {
            let key = (*trigger_e, *other_e);
            new_active.insert(key);

            if self.active_pairs.contains(&key) {
                stays.push(TriggerStayEvent {
                    trigger_entity: *trigger_e,
                    other_entity: *other_e,
                });
            } else {
                enters.push(TriggerEnterEvent {
                    trigger_entity: *trigger_e,
                    other_entity: *other_e,
                    trigger_collider: *trigger_c,
                    other_collider: *other_c,
                });
            }
        }

        // Anything in active_pairs but not in new_active has exited.
        for &(te, oe) in &self.active_pairs {
            if !new_active.contains(&(te, oe)) {
                exits.push(TriggerExitEvent {
                    trigger_entity: te,
                    other_entity: oe,
                });
            }
        }

        self.active_pairs = new_active;
        (enters, stays, exits)
    }
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
