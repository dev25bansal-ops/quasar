//! Inverse Kinematics (IK) System
//!
//! Provides IK solvers for procedural animation:
//! - `TwoBoneIK`: Fast solver for arms/legs (shoulder-elbow-wrist chains)
//! - `FabrikIK`: FABRIK algorithm for arbitrary length chains
//! - `CCDIK`: Cyclic Coordinate Descent for multi-joint chains

use quasar_math::{Quat, Vec3};

/// A bone joint in an IK chain.
#[derive(Debug, Clone, Copy)]
pub struct IKJoint {
    pub position: Vec3,
    pub rotation: Quat,
    pub length: f32,
}

impl IKJoint {
    pub fn new(position: Vec3, rotation: Quat, length: f32) -> Self {
        Self {
            position,
            rotation,
            length,
        }
    }
}

/// Configuration for IK solver.
#[derive(Debug, Clone, Copy)]
pub struct IKConfig {
    pub max_iterations: usize,
    pub tolerance: f32,
    pub pole_angle: f32,
}

impl Default for IKConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            tolerance: 0.001,
            pole_angle: 0.0,
        }
    }
}

/// Result of IK solve.
#[derive(Debug, Clone, Copy)]
pub struct IKResult {
    pub success: bool,
    pub iterations: usize,
    pub end_effector_position: Vec3,
}

/// Two-bone IK solver (optimized for arms/legs).
pub struct TwoBoneIK;

impl TwoBoneIK {
    /// Solve two-bone IK for a shoulder-elbow-wrist chain.
    pub fn solve(
        shoulder_pos: Vec3,
        elbow_pos: Vec3,
        wrist_pos: Vec3,
        target: Vec3,
        pole: Vec3,
    ) -> (Vec3, Vec3) {
        let upper_len = (elbow_pos - shoulder_pos).length();
        let lower_len = (wrist_pos - elbow_pos).length();
        let target_dist = (target - shoulder_pos).length();
        let reach = upper_len + lower_len;

        if target_dist > reach * 0.9999 {
            let dir = (target - shoulder_pos).normalize();
            let new_elbow = shoulder_pos + dir * upper_len;
            let new_wrist = new_elbow + dir * lower_len;
            return (new_elbow, new_wrist);
        }

        let target_dist = target_dist
            .min(reach * 0.9999)
            .max((upper_len - lower_len).abs() + 0.001);
        let cos_angle = (upper_len * upper_len + target_dist * target_dist - lower_len * lower_len)
            / (2.0 * upper_len * target_dist);
        let cos_angle = cos_angle.clamp(-1.0, 1.0);
        let elbow_angle = cos_angle.acos();
        let to_target = (target - shoulder_pos).normalize();
        let axis = (to_target.cross(pole - shoulder_pos)).normalize();

        if axis.is_finite() {
            let rotation = Quat::from_axis_angle(axis, elbow_angle);
            let elbow_dir = rotation * to_target;
            let new_elbow = shoulder_pos + elbow_dir * upper_len;
            let to_target_from_elbow = (target - new_elbow).normalize();
            let new_wrist = new_elbow + to_target_from_elbow * lower_len;
            (new_elbow, new_wrist)
        } else {
            (elbow_pos, target)
        }
    }
}

/// FABRIK (Forward And Backward Reaching Inverse Kinematics) solver.
pub struct FabrikIK {
    joints: Vec<IKJoint>,
    config: IKConfig,
}

impl FabrikIK {
    pub fn new(joints: Vec<IKJoint>, config: IKConfig) -> Self {
        Self { joints, config }
    }

    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    pub fn get_joint(&self, index: usize) -> Option<&IKJoint> {
        self.joints.get(index)
    }

    pub fn set_joint(&mut self, index: usize, joint: IKJoint) {
        if index < self.joints.len() {
            self.joints[index] = joint;
        }
    }

    pub fn total_length(&self) -> f32 {
        self.joints
            .windows(2)
            .map(|w| (w[1].position - w[0].position).length())
            .sum()
    }

    pub fn solve(&mut self, target: Vec3) -> IKResult {
        if self.joints.is_empty() {
            return IKResult {
                success: false,
                iterations: 0,
                end_effector_position: Vec3::ZERO,
            };
        }

        let root = self.joints[0].position;
        let total_len = self.total_length();
        let target_dist = (target - root).length();

        if target_dist > total_len {
            for i in 1..self.joints.len() {
                let dir = (target - self.joints[i - 1].position).normalize();
                self.joints[i].position =
                    self.joints[i - 1].position + dir * self.joints[i - 1].length;
            }
            return IKResult {
                success: true,
                iterations: 1,
                end_effector_position: self.joints.last().map(|j| j.position).unwrap_or(Vec3::ZERO),
            };
        }

        let mut iterations = 0;

        for _ in 0..self.config.max_iterations {
            iterations += 1;

            if let Some(last) = self.joints.last_mut() {
                last.position = target;
            }

            for i in (1..self.joints.len()).rev() {
                let dir = (self.joints[i - 1].position - self.joints[i].position).normalize();
                self.joints[i - 1].position =
                    self.joints[i].position + dir * self.joints[i - 1].length;
            }

            self.joints[0].position = root;

            for i in 1..self.joints.len() {
                let dir = (self.joints[i].position - self.joints[i - 1].position).normalize();
                self.joints[i].position =
                    self.joints[i - 1].position + dir * self.joints[i - 1].length;
            }

            if let Some(last) = self.joints.last() {
                if (last.position - target).length() < self.config.tolerance {
                    return IKResult {
                        success: true,
                        iterations,
                        end_effector_position: last.position,
                    };
                }
            }
        }

        IKResult {
            success: false,
            iterations,
            end_effector_position: self.joints.last().map(|j| j.position).unwrap_or(Vec3::ZERO),
        }
    }

    pub fn apply_pole_constraint(&mut self, pole_target: Vec3) {
        if self.joints.len() < 3 {
            return;
        }

        let root = self.joints[0].position;
        let tip = self.joints.last().map(|j| j.position).unwrap_or(root);

        for i in 1..self.joints.len() - 1 {
            let joint_pos = self.joints[i].position;

            let plane_normal = (tip - root).normalize();
            let to_joint = joint_pos - root;
            let to_pole = pole_target - root;

            let joint_proj = to_joint - plane_normal * to_joint.dot(plane_normal);
            let pole_proj = to_pole - plane_normal * to_pole.dot(plane_normal);

            if joint_proj.length_squared() > 0.0001 && pole_proj.length_squared() > 0.0001 {
                let joint_dir = joint_proj.normalize();
                let pole_dir = pole_proj.normalize();

                let cross = joint_dir.cross(pole_dir);
                if cross.length_squared() > 0.0001 {
                    let axis = plane_normal;
                    let angle =
                        joint_dir.dot(pole_dir).clamp(-1.0, 1.0).acos() * cross.dot(axis).signum();
                    if angle.abs() > 0.0001 {
                        let rotation = Quat::from_axis_angle(axis, angle);
                        self.joints[i].position = root + rotation * to_joint;
                    }
                }
            }
        }
    }
}

/// CCD (Cyclic Coordinate Descent) IK solver.
pub struct CCDIK {
    joints: Vec<IKJoint>,
    config: IKConfig,
}

impl CCDIK {
    pub fn new(joints: Vec<IKJoint>, config: IKConfig) -> Self {
        Self { joints, config }
    }

    pub fn solve(&mut self, target: Vec3) -> IKResult {
        if self.joints.len() < 2 {
            return IKResult {
                success: false,
                iterations: 0,
                end_effector_position: self.joints.last().map(|j| j.position).unwrap_or(Vec3::ZERO),
            };
        }

        let mut iterations = 0;

        for _ in 0..self.config.max_iterations {
            iterations += 1;

            for i in (0..self.joints.len() - 1).rev() {
                let effector_pos = self.joints.last().map(|j| j.position).unwrap();

                if (effector_pos - target).length() < self.config.tolerance {
                    return IKResult {
                        success: true,
                        iterations,
                        end_effector_position: effector_pos,
                    };
                }

                let joint_pos = self.joints[i].position;
                let to_effector = (effector_pos - joint_pos).normalize();
                let to_target = (target - joint_pos).normalize();

                let dot = to_effector.dot(to_target).clamp(-1.0, 1.0);
                if dot < 0.9999 {
                    let axis = to_effector.cross(to_target);
                    if axis.length_squared() > 0.0001 {
                        let axis = axis.normalize();
                        let angle = dot.acos();
                        let rotation = Quat::from_axis_angle(axis, angle);

                        for j in (i + 1)..self.joints.len() {
                            let offset = self.joints[j].position - joint_pos;
                            self.joints[j].position = joint_pos + rotation * offset;
                            self.joints[j].rotation = rotation * self.joints[j].rotation;
                        }
                    }
                }
            }

            let effector_pos = self.joints.last().map(|j| j.position).unwrap();
            if (effector_pos - target).length() < self.config.tolerance {
                return IKResult {
                    success: true,
                    iterations,
                    end_effector_position: effector_pos,
                };
            }
        }

        IKResult {
            success: false,
            iterations,
            end_effector_position: self.joints.last().map(|j| j.position).unwrap_or(Vec3::ZERO),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quasar_math::Vec3;

    #[test]
    fn two_bone_ik_reaches_target() {
        let shoulder = Vec3::ZERO;
        let elbow = Vec3::X;
        let wrist = Vec3::new(2.0, 0.0, 0.0);
        let target = Vec3::new(1.5, 0.5, 0.0);
        let pole = Vec3::Y;

        let (new_elbow, new_wrist) = TwoBoneIK::solve(shoulder, elbow, wrist, target, pole);

        let elbow_dist = (new_elbow - shoulder).length();
        assert!((elbow_dist - 1.0).abs() < 0.01);

        let wrist_dist = (new_wrist - new_elbow).length();
        assert!((wrist_dist - 1.0).abs() < 0.01);
    }

    #[test]
    fn two_bone_ik_stretched() {
        let shoulder = Vec3::ZERO;
        let elbow = Vec3::X;
        let wrist = Vec3::new(2.0, 0.0, 0.0);
        let target = Vec3::new(10.0, 0.0, 0.0);
        let pole = Vec3::Y;

        let (new_elbow, new_wrist) = TwoBoneIK::solve(shoulder, elbow, wrist, target, pole);

        assert!((new_elbow - shoulder).length() - 1.0 < 0.01);
        assert!((new_wrist - new_elbow).length() - 1.0 < 0.01);
    }

    #[test]
    fn fabrik_ik_basic_solve() {
        let joints = vec![
            IKJoint::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
            IKJoint::new(Vec3::X, Quat::IDENTITY, 1.0),
            IKJoint::new(Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, 0.0),
        ];
        let mut solver = FabrikIK::new(joints, IKConfig::default());
        let target = Vec3::new(1.5, 0.5, 0.0);
        let result = solver.solve(target);

        assert!(result.success);
        assert!(result.iterations > 0);
        assert!((result.end_effector_position - target).length() < 0.1);
    }

    #[test]
    fn fabrik_ik_unreachable() {
        let joints = vec![
            IKJoint::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
            IKJoint::new(Vec3::X, Quat::IDENTITY, 1.0),
        ];
        let mut solver = FabrikIK::new(joints, IKConfig::default());
        let target = Vec3::new(10.0, 0.0, 0.0);
        let result = solver.solve(target);

        assert!(result.success);
    }

    #[test]
    fn ccd_ik_basic_solve() {
        let joints = vec![
            IKJoint::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
            IKJoint::new(Vec3::X, Quat::IDENTITY, 1.0),
            IKJoint::new(Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, 0.0),
        ];
        let mut solver = CCDIK::new(joints, IKConfig::default());
        let target = Vec3::new(1.0, 1.0, 0.0);
        let result = solver.solve(target);

        assert!((result.end_effector_position - target).length() < 0.5);
    }

    #[test]
    fn ik_config_default() {
        let config = IKConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert!((config.tolerance - 0.001).abs() < f32::EPSILON);
        assert!((config.pole_angle).abs() < f32::EPSILON);
    }

    #[test]
    fn ik_joint_new() {
        let joint = IKJoint::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY, 5.0);
        assert!((joint.position - Vec3::new(1.0, 2.0, 3.0)).length() < 0.001);
        assert!((joint.length - 5.0).abs() < 0.001);
    }
}
