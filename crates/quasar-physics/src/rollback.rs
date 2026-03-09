//! Deterministic physics rollback — snapshot and restore the full Rapier state.
//!
//! Used for netcode rollback: save state at a confirmed tick, then restore
//! and re-simulate when a mismatch is detected.

use rapier3d::prelude::*;

use crate::world::PhysicsWorld;

// ---------------------------------------------------------------------------
// Per-body / per-collider / per-joint state
// ---------------------------------------------------------------------------

/// Serialised state of a single rigid body.
#[derive(Clone)]
pub struct RigidBodyState {
    pub handle: RigidBodyHandle,
    pub position: Isometry<f32>,
    pub linvel: nalgebra::Vector3<f32>,
    pub angvel: nalgebra::Vector3<f32>,
    pub body_type: RigidBodyType,
    pub is_enabled: bool,
}

/// Serialised state of a single collider.
#[derive(Clone)]
pub struct ColliderState {
    pub handle: ColliderHandle,
    pub position: Isometry<f32>,
    pub parent: Option<RigidBodyHandle>,
}

/// Serialised state of a single impulse joint.
#[derive(Clone)]
pub struct JointState {
    pub handle: ImpulseJointHandle,
    pub body1: RigidBodyHandle,
    pub body2: RigidBodyHandle,
}

// ---------------------------------------------------------------------------
// Full snapshot
// ---------------------------------------------------------------------------

/// A complete snapshot of the physics world at a given simulation tick.
#[derive(Clone)]
pub struct PhysicsSnapshot {
    pub tick: u64,
    pub bodies: Vec<RigidBodyState>,
    pub colliders: Vec<ColliderState>,
    pub joints: Vec<JointState>,
}

impl PhysicsSnapshot {
    /// Capture the current state of `world` at the given `tick`.
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
            })
            .collect();

        let colliders: Vec<ColliderState> = world
            .colliders
            .iter()
            .map(|(handle, col)| ColliderState {
                handle,
                position: *col.position(),
                parent: col.parent(),
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

    /// Restore this snapshot onto `world`, resetting all body positions,
    /// velocities, and enabled states to the captured values.
    ///
    /// **Note:** This does not add/remove bodies — it assumes the same set of
    /// handles exist in both the snapshot and the world.  For full
    /// add/remove support, pair this with entity-level rollback.
    pub fn restore(&self, world: &mut PhysicsWorld) {
        for bs in &self.bodies {
            if let Some(rb) = world.bodies.get_mut(bs.handle) {
                rb.set_position(bs.position, true);
                rb.set_linvel(bs.linvel, true);
                rb.set_angvel(bs.angvel, true);
                if bs.is_enabled {
                    rb.set_enabled(true);
                } else {
                    rb.set_enabled(false);
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
