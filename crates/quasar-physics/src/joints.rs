//! Physics joints — wrappers for Rapier3D joints / constraints.
//!
//! Provides ECS-friendly joint components that are automatically
//! synchronized with the underlying Rapier impulse-joint set.

use rapier3d::prelude::*;

// ---------------------------------------------------------------------------
// Joint configuration types
// ---------------------------------------------------------------------------

/// Describes the type and parameters of a physics joint.
#[derive(Debug, Clone)]
pub enum JointKind {
    /// Fixed joint — no relative movement between bodies.
    Fixed {
        /// Local frame anchor on body A (position offset).
        anchor_a: [f32; 3],
        /// Local frame anchor on body B (position offset).
        anchor_b: [f32; 3],
    },
    /// Revolute (hinge) joint — one rotational degree of freedom.
    Revolute {
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
        /// Rotation axis in local space of body A.
        axis: [f32; 3],
        /// Optional angular limits `(min, max)` in radians.
        limits: Option<(f32, f32)>,
    },
    /// Prismatic (slider) joint — one translational degree of freedom.
    Prismatic {
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
        /// Translation axis in local space of body A.
        axis: [f32; 3],
        /// Optional linear limits `(min, max)`.
        limits: Option<(f32, f32)>,
    },
    /// Spherical (ball) joint — three rotational DOFs.
    Spherical {
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
    },
    /// Spring-damper (distance) joint — maintains a target distance.
    Spring {
        anchor_a: [f32; 3],
        anchor_b: [f32; 3],
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    },
}

/// ECS component representing a joint between two rigid bodies.
///
/// The system will create the Rapier joint on the next tick and populate
/// `handle` with the resulting `ImpulseJointHandle`.
#[derive(Debug, Clone)]
pub struct JointComponent {
    /// RigidBody handle of body A (the "parent" side of the joint).
    pub body_a: RigidBodyHandle,
    /// RigidBody handle of body B.
    pub body_b: RigidBodyHandle,
    /// Joint configuration.
    pub kind: JointKind,
    /// Populated by the joint sync system after creation.
    pub handle: Option<ImpulseJointHandle>,
}

impl JointComponent {
    pub fn new(body_a: RigidBodyHandle, body_b: RigidBodyHandle, kind: JointKind) -> Self {
        Self {
            body_a,
            body_b,
            kind,
            handle: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers — build Rapier GenericJoint from our JointKind
// ---------------------------------------------------------------------------

fn na_point(p: [f32; 3]) -> nalgebra::Point3<f32> {
    nalgebra::point![p[0], p[1], p[2]]
}

fn na_unit(v: [f32; 3]) -> nalgebra::Unit<nalgebra::Vector3<f32>> {
    nalgebra::Unit::new_normalize(nalgebra::vector![v[0], v[1], v[2]])
}

pub(crate) fn build_rapier_joint(kind: &JointKind) -> GenericJoint {
    match kind {
        JointKind::Fixed { anchor_a, anchor_b } => {
            let mut j = FixedJointBuilder::new()
                .local_anchor1(na_point(*anchor_a))
                .local_anchor2(na_point(*anchor_b))
                .build();
            j.set_contacts_enabled(false);
            j.into()
        }
        JointKind::Revolute {
            anchor_a,
            anchor_b,
            axis,
            limits,
        } => {
            let mut j = RevoluteJointBuilder::new(na_unit(*axis))
                .local_anchor1(na_point(*anchor_a))
                .local_anchor2(na_point(*anchor_b))
                .build();
            if let Some((min, max)) = limits {
                j.set_limits([*min, *max]);
            }
            j.set_contacts_enabled(false);
            j.into()
        }
        JointKind::Prismatic {
            anchor_a,
            anchor_b,
            axis,
            limits,
        } => {
            let mut j = PrismaticJointBuilder::new(na_unit(*axis))
                .local_anchor1(na_point(*anchor_a))
                .local_anchor2(na_point(*anchor_b))
                .build();
            if let Some((min, max)) = limits {
                j.set_limits([*min, *max]);
            }
            j.set_contacts_enabled(false);
            j.into()
        }
        JointKind::Spherical { anchor_a, anchor_b } => {
            let mut j = SphericalJointBuilder::new()
                .local_anchor1(na_point(*anchor_a))
                .local_anchor2(na_point(*anchor_b))
                .build();
            j.set_contacts_enabled(false);
            j.into()
        }
        JointKind::Spring {
            anchor_a,
            anchor_b,
            rest_length,
            stiffness,
            damping,
        } => {
            let mut j = SpringJointBuilder::new(*rest_length, *stiffness, *damping)
                .local_anchor1(na_point(*anchor_a))
                .local_anchor2(na_point(*anchor_b))
                .build();
            j.set_contacts_enabled(false);
            j.into()
        }
    }
}

// ---------------------------------------------------------------------------
// Joint motor configuration
// ---------------------------------------------------------------------------

/// Motor mode (velocity target or position target).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotorMode {
    /// Drive the joint DOF to a target velocity.
    Velocity,
    /// Drive the joint DOF to a target position.
    Position,
}

/// Motor parameters for a single joint DOF.
#[derive(Debug, Clone, Copy)]
pub struct JointMotor {
    pub mode: MotorMode,
    /// Target value (radians for revolute, metres for prismatic).
    pub target: f32,
    /// Maximum force/torque the motor can exert.
    pub max_force: f32,
    /// Stiffness (spring constant) — relevant for position mode.
    pub stiffness: f32,
    /// Damping coefficient.
    pub damping: f32,
}

impl Default for JointMotor {
    fn default() -> Self {
        Self {
            mode: MotorMode::Velocity,
            target: 0.0,
            max_force: f32::MAX,
            stiffness: 0.0,
            damping: 0.0,
        }
    }
}

impl JointMotor {
    pub fn velocity(target: f32, max_force: f32) -> Self {
        Self {
            mode: MotorMode::Velocity,
            target,
            max_force,
            stiffness: 0.0,
            damping: 0.0,
        }
    }

    pub fn position(target: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            mode: MotorMode::Position,
            target,
            max_force: f32::MAX,
            stiffness,
            damping,
        }
    }
}

/// Apply motor settings to an existing impulse joint in the physics world.
///
/// `axis_index`: 0 for single-DOF joints (revolute, prismatic).
/// For prismatic joints the motor acts on JointAxis::LinX,
/// for revolute joints it acts on JointAxis::AngX.
pub fn apply_motor_to_joint(
    joint_set: &mut ImpulseJointSet,
    handle: ImpulseJointHandle,
    motor: &JointMotor,
    is_prismatic: bool,
) {
    if let Some(joint) = joint_set.get_mut(handle) {
        let axis = if is_prismatic {
            JointAxis::LinX
        } else {
            JointAxis::AngX
        };
        match motor.mode {
            MotorMode::Velocity => {
                joint
                    .data
                    .set_motor(axis, motor.target, 0.0, 0.0, motor.max_force);
            }
            MotorMode::Position => {
                joint
                    .data
                    .set_motor(axis, motor.target, 0.0, motor.stiffness, motor.damping);
            }
        }
    }
}

/// Convenience: apply a velocity motor to a joint in the physics world.
pub fn set_joint_motor_velocity(
    world: &mut crate::world::PhysicsWorld,
    handle: ImpulseJointHandle,
    target_velocity: f32,
    max_force: f32,
    is_prismatic: bool,
) {
    let motor = JointMotor::velocity(target_velocity, max_force);
    apply_motor_to_joint(&mut world.impulse_joints, handle, &motor, is_prismatic);
}

/// Convenience: apply a position motor (spring) to a joint.
pub fn set_joint_motor_position(
    world: &mut crate::world::PhysicsWorld,
    handle: ImpulseJointHandle,
    target_position: f32,
    stiffness: f32,
    damping: f32,
    is_prismatic: bool,
) {
    let motor = JointMotor::position(target_position, stiffness, damping);
    apply_motor_to_joint(&mut world.impulse_joints, handle, &motor, is_prismatic);
}
