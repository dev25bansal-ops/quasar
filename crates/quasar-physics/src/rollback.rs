//! Deterministic physics rollback - snapshot and restore for netcode.
//!
//! Used for GGPO-style rollback: save state at confirmed tick, then restore
//! and re-simulate when a mismatch is detected.

use rapier3d::prelude::*;
use crate::world::PhysicsWorld;

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
        
        Self { tick, bodies, colliders, joints }
    }
    
    pub fn restore(&self, world: &mut PhysicsWorld) {
        for bs in &self.bodies {
            if let Some(rb) = world.bodies.get_mut(bs.handle) {
                rb.set_position(bs.position, true);
                rb.set_linvel(bs.linvel, true);
                rb.set_angvel(bs.angvel, true);
                rb.set_enabled(bs.is_enabled);
                if bs.ccd_enabled { rb.enable_ccd(true); }
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
        Self { history: Vec::new(), max_frames: MAX_ROLLBACK_FRAMES }
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
    fn default() -> Self { Self::new() }
}
