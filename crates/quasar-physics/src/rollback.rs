//! Deterministic physics rollback - snapshot and restore for netcode.
//!
//! Used for GGPO-style rollback: save state at confirmed tick, then restore
//! and re-simulate when a mismatch is detected.

use crate::world::PhysicsWorld;
use rapier3d::prelude::*;

/// Maximum rollback frames (8-10 frames = 134ms at 60Hz).
pub const MAX_ROLLBACK_FRAMES: usize = 10;

/// Serialised state of a single rigid body.
#[derive(Clone)]
pub struct RigidBodyState {
    pub handle: RigidBodyHandle,
    pub position: Isometry<f32>,
    pub linvel: nalgebra::Vector3<f32>,
    pub angvel: nalgebra::Vector3<f32>,
    pub body_type: RigidBodyType,
    pub is_enabled: bool,
    pub ccd_enabled: bool,
}

/// Serialised state of a single collider.
#[derive(Clone)]
pub struct ColliderState {
    pub handle: ColliderHandle,
    pub position: Isometry<f32>,
    pub parent: Option<RigidBodyHandle>,
    pub is_sensor: bool,
}

/// Serialised state of a single impulse joint.
#[derive(Clone)]
pub struct JointState {
    pub handle: ImpulseJointHandle,
    pub body1: RigidBodyHandle,
    pub body2: RigidBodyHandle,
}

/// A complete snapshot of the physics world at a given simulation tick.
#[derive(Clone)]
pub struct PhysicsSnapshot {
    pub tick: u64,
    pub bodies: Vec<RigidBodyState>,
    pub colliders: Vec<ColliderState>,
    pub joints: Vec<JointState>,
}

impl PhysicsSnapshot {
    pub fn snapshot(world: &PhysicsWorld, tick: u64) -> Self {
        let bodies: Vec<RigidBodyState> = world
            .bodies
            .iter()
            .map(|(handle, rb)| RigidBodyState {
                handle,
                position: *rb.position(),
                linvel: *rb.linvel(),
                angvel: *rb.angvel(),
                body_type: rb.body_type(),
                is_enabled: rb.is_enabled(),
                ccd_enabled: rb.is_ccd_enabled(),
            })
            .collect();

        let colliders: Vec<ColliderState> = world
            .colliders
            .iter()
            .map(|(handle, col)| ColliderState {
                handle,
                position: *col.position(),
                parent: col.parent(),
                is_sensor: col.is_sensor(),
            })
            .collect();

        let joints: Vec<JointState> = world
            .impulse_joints
            .iter()
            .map(|(handle, joint)| JointState {
                handle,
                body1: joint.body1,
                body2: joint.body2,
            })
            .collect();

        Self {
            tick,
            bodies,
            colliders,
            joints,
        }
    }

    pub fn restore(&self, world: &mut PhysicsWorld) {
        for bs in &self.bodies {
            if let Some(rb) = world.bodies.get_mut(bs.handle) {
                rb.set_position(bs.position, true);
                rb.set_linvel(bs.linvel, true);
                rb.set_angvel(bs.angvel, true);
                rb.set_enabled(bs.is_enabled);
                if bs.ccd_enabled {
                    rb.enable_ccd(true);
                }
            }
        }

        for cs in &self.colliders {
            if let Some(col) = world.colliders.get_mut(cs.handle) {
                col.set_position(cs.position);
            }
        }
    }
}

/// Rollback manager for handling prediction errors.
pub struct RollbackManager {
    history: Vec<PhysicsSnapshot>,
    max_frames: usize,
}

impl RollbackManager {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            max_frames: MAX_ROLLBACK_FRAMES,
        }
    }

    pub fn record(&mut self, world: &PhysicsWorld, tick: u64) {
        let snapshot = PhysicsSnapshot::snapshot(world, tick);
        self.history.push(snapshot);
        while self.history.len() > self.max_frames {
            self.history.remove(0);
        }
    }

    pub fn rollback_to(&mut self, world: &mut PhysicsWorld, tick: u64) -> bool {
        let snapshot = self.history.iter().rev().find(|s| s.tick <= tick);
        if let Some(s) = snapshot {
            s.clone().restore(world);
            self.history.retain(|s| s.tick <= tick);
            true
        } else {
            false
        }
    }

    pub fn latest_tick(&self) -> Option<u64> {
        self.history.last().map(|s| s.tick)
    }
}

impl Default for RollbackManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_rollback_frames() {
        assert_eq!(MAX_ROLLBACK_FRAMES, 10);
    }

    #[test]
    fn test_rigid_body_state_clone() {
        let state = RigidBodyState {
            handle: RigidBodyHandle::from_raw_parts(0, 0),
            position: Isometry::new(
                nalgebra::vector![0.0, 0.0, 0.0],
                nalgebra::vector![0.0, 0.0, 0.0],
            ),
            linvel: nalgebra::vector![1.0, 2.0, 3.0],
            angvel: nalgebra::vector![0.1, 0.2, 0.3],
            body_type: RigidBodyType::Dynamic,
            is_enabled: true,
            ccd_enabled: false,
        };
        let cloned = state.clone();
        assert_eq!(cloned.handle, state.handle);
    }

    #[test]
    fn test_collider_state_clone() {
        let state = ColliderState {
            handle: ColliderHandle::from_raw_parts(0, 0),
            position: Isometry::new(
                nalgebra::vector![0.0, 0.0, 0.0],
                nalgebra::vector![0.0, 0.0, 0.0],
            ),
            parent: Some(RigidBodyHandle::from_raw_parts(1, 0)),
            is_sensor: false,
        };
        let cloned = state.clone();
        assert_eq!(cloned.handle, state.handle);
        assert!(cloned.parent.is_some());
    }

    #[test]
    fn test_joint_state_clone() {
        let state = JointState {
            handle: ImpulseJointHandle::from_raw_parts(0, 0),
            body1: RigidBodyHandle::from_raw_parts(1, 0),
            body2: RigidBodyHandle::from_raw_parts(2, 0),
        };
        let cloned = state.clone();
        assert_eq!(cloned.handle, state.handle);
        assert_eq!(cloned.body1, state.body1);
        assert_eq!(cloned.body2, state.body2);
    }

    #[test]
    fn test_physics_snapshot_clone() {
        let snapshot = PhysicsSnapshot {
            tick: 100,
            bodies: vec![],
            colliders: vec![],
            joints: vec![],
        };
        let cloned = snapshot.clone();
        assert_eq!(cloned.tick, 100);
    }

    #[test]
    fn test_rollback_manager_new() {
        let manager = RollbackManager::new();
        assert!(manager.history.is_empty());
        assert_eq!(manager.max_frames, MAX_ROLLBACK_FRAMES);
    }

    #[test]
    fn test_rollback_manager_default() {
        let manager = RollbackManager::default();
        assert!(manager.history.is_empty());
    }

    #[test]
    fn test_rollback_manager_latest_tick_empty() {
        let manager = RollbackManager::new();
        assert!(manager.latest_tick().is_none());
    }

    #[test]
    fn test_rollback_manager_latest_tick() {
        let mut manager = RollbackManager::new();
        manager.history.push(PhysicsSnapshot {
            tick: 100,
            bodies: vec![],
            colliders: vec![],
            joints: vec![],
        });
        assert_eq!(manager.latest_tick(), Some(100));
    }

    #[test]
    fn test_rollback_manager_rollback_to_empty() {
        let mut manager = RollbackManager::new();
        let mut world = PhysicsWorld::new();
        assert!(!manager.rollback_to(&mut world, 100));
    }
}
