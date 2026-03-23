//! 3D transform — position, rotation, and scale.

use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// A 3D transform representing position, rotation, and uniform scale.
///
/// Used as a component on entities to define where they exist in the world.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    /// World-space position.
    pub position: Vec3,
    /// Orientation as a unit quaternion.
    pub rotation: Quat,
    /// Non-uniform scale.
    pub scale: Vec3,
}

impl Transform {
    /// Identity transform — origin, no rotation, unit scale.
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    /// Create a transform at the given position.
    pub fn from_position(position: Vec3) -> Self {
        Self {
            position,
            ..Self::IDENTITY
        }
    }

    /// Create a transform with position and rotation.
    pub fn from_position_rotation(position: Vec3, rotation: Quat) -> Self {
        Self {
            position,
            rotation,
            ..Self::IDENTITY
        }
    }

    /// Create a transform with uniform scale.
    pub fn from_scale(scale: f32) -> Self {
        Self {
            scale: Vec3::splat(scale),
            ..Self::IDENTITY
        }
    }

    /// Compute the 4×4 model matrix (TRS order).
    #[inline]
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    /// Forward direction (−Z in right-handed coords).
    #[inline]
    pub fn forward(&self) -> Vec3 {
        self.rotation * Vec3::NEG_Z
    }

    /// Right direction (+X).
    #[inline]
    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    /// Up direction (+Y).
    #[inline]
    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    /// Rotate around an axis by an angle (radians).
    pub fn rotate(&mut self, axis: Vec3, angle: f32) {
        self.rotation = Quat::from_axis_angle(axis, angle) * self.rotation;
    }

    /// Translate by an offset in world space.
    pub fn translate(&mut self, offset: Vec3) {
        self.position += offset;
    }

    /// Look at a target position (Y-up).
    pub fn look_at(&mut self, target: Vec3, up: Vec3) {
        let forward = (target - self.position).normalize();
        if forward.length_squared() > 0.0001 {
            self.rotation = Quat::from_rotation_arc(Vec3::NEG_Z, forward);
            // Adjust for up vector
            let right = forward.cross(up).normalize();
            let corrected_up = right.cross(forward);
            self.rotation =
                Quat::from_mat4(&Mat4::look_to_rh(Vec3::ZERO, forward, corrected_up)).conjugate();
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Computed world-space transform after hierarchy propagation.
///
/// This is a read-only component written by the transform propagation system.
/// It combines the entity's local [`Transform`] with all ancestor transforms
/// in the scene graph to produce the final world-space matrix.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GlobalTransform {
    /// The final world-space 4×4 matrix.
    pub matrix: Mat4,
}

impl GlobalTransform {
    /// Identity global transform.
    pub const IDENTITY: Self = Self {
        matrix: Mat4::IDENTITY,
    };

    /// Create a global transform from a matrix.
    pub fn from_matrix(matrix: Mat4) -> Self {
        Self { matrix }
    }

    /// Extract the translation from the global matrix.
    #[inline]
    pub fn translation(&self) -> Vec3 {
        self.matrix.col(3).truncate()
    }
}

impl Default for GlobalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl From<Transform> for GlobalTransform {
    fn from(t: Transform) -> Self {
        Self { matrix: t.matrix() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matrix_is_identity() {
        let tf = Transform::IDENTITY;
        let mat = tf.matrix();
        let expected = Mat4::IDENTITY;
        for i in 0..16 {
            assert!(
                (mat.to_cols_array()[i] - expected.to_cols_array()[i]).abs() < 1e-6,
                "matrix element {} differs",
                i
            );
        }
    }

    #[test]
    fn from_position_sets_translation() {
        let tf = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(tf.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(tf.rotation, Quat::IDENTITY);
        assert_eq!(tf.scale, Vec3::ONE);
    }

    #[test]
    fn translate_adds_offset() {
        let mut tf = Transform::IDENTITY;
        tf.translate(Vec3::new(5.0, 0.0, 0.0));
        assert!((tf.position.x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_changes_forward() {
        let mut tf = Transform::IDENTITY;
        let original_forward = tf.forward();
        tf.rotate(Vec3::Y, std::f32::consts::FRAC_PI_2);
        let new_forward = tf.forward();
        // After 90° rotation around Y, forward should change significantly
        assert!((original_forward - new_forward).length() > 0.5);
    }

    #[test]
    fn scale_affects_matrix() {
        let tf = Transform::from_scale(2.0);
        let mat = tf.matrix();
        // The scale should be encoded in the matrix diagonal-ish
        let col0_len = Vec3::new(mat.x_axis.x, mat.x_axis.y, mat.x_axis.z).length();
        assert!((col0_len - 2.0).abs() < 1e-6);
    }

    #[test]
    fn forward_right_up_orthogonal() {
        let tf = Transform::IDENTITY;
        let f = tf.forward();
        let r = tf.right();
        let u = tf.up();
        // Should be orthogonal
        assert!(f.dot(r).abs() < 1e-6);
        assert!(f.dot(u).abs() < 1e-6);
        assert!(r.dot(u).abs() < 1e-6);
    }

    #[test]
    fn transform_default_is_identity() {
        let tf = Transform::default();
        assert_eq!(tf.position, Vec3::ZERO);
        assert_eq!(tf.rotation, Quat::IDENTITY);
        assert_eq!(tf.scale, Vec3::ONE);
    }

    #[test]
    fn transform_from_position_rotation() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let rot = Quat::from_rotation_x(std::f32::consts::FRAC_PI_4);
        let tf = Transform::from_position_rotation(pos, rot);

        assert_eq!(tf.position, pos);
        assert_eq!(tf.rotation, rot);
        assert_eq!(tf.scale, Vec3::ONE);
    }

    #[test]
    fn transform_clone() {
        let tf = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
        let cloned = tf.clone();
        assert_eq!(cloned.position, tf.position);
    }

    #[test]
    fn transform_copy() {
        let tf = Transform::from_position(Vec3::new(1.0, 2.0, 3.0));
        let copied = tf;
        assert_eq!(copied.position, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn global_transform_identity() {
        let gt = GlobalTransform::IDENTITY;
        assert_eq!(gt.matrix, Mat4::IDENTITY);
    }

    #[test]
    fn global_transform_default() {
        let gt = GlobalTransform::default();
        assert_eq!(gt.matrix, Mat4::IDENTITY);
    }

    #[test]
    fn global_transform_from_matrix() {
        let mat = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let gt = GlobalTransform::from_matrix(mat);
        assert_eq!(gt.matrix, mat);
    }

    #[test]
    fn global_transform_translation() {
        let mat = Mat4::from_translation(Vec3::new(10.0, 20.0, 30.0));
        let gt = GlobalTransform::from_matrix(mat);
        let trans = gt.translation();
        assert!((trans.x - 10.0).abs() < 1e-6);
        assert!((trans.y - 20.0).abs() < 1e-6);
        assert!((trans.z - 30.0).abs() < 1e-6);
    }

    #[test]
    fn global_transform_from_transform() {
        let tf = Transform::from_position(Vec3::new(5.0, 10.0, 15.0));
        let gt = GlobalTransform::from(tf);
        let trans = gt.translation();
        assert!((trans.x - 5.0).abs() < 1e-6);
        assert!((trans.y - 10.0).abs() < 1e-6);
        assert!((trans.z - 15.0).abs() < 1e-6);
    }

    #[test]
    fn global_transform_clone() {
        let gt = GlobalTransform::from_matrix(Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        let cloned = gt.clone();
        assert_eq!(cloned.matrix, gt.matrix);
    }

    #[test]
    fn transform_look_at() {
        let mut tf = Transform::IDENTITY;
        tf.look_at(Vec3::new(0.0, 0.0, -10.0), Vec3::Y);
        // Forward should point towards -Z
        let f = tf.forward();
        assert!((f - Vec3::NEG_Z).length() < 0.1);
    }

    #[test]
    fn transform_forward_identity() {
        let tf = Transform::IDENTITY;
        let f = tf.forward();
        assert!((f - Vec3::NEG_Z).length() < 1e-6);
    }

    #[test]
    fn transform_right_identity() {
        let tf = Transform::IDENTITY;
        let r = tf.right();
        assert!((r - Vec3::X).length() < 1e-6);
    }

    #[test]
    fn transform_up_identity() {
        let tf = Transform::IDENTITY;
        let u = tf.up();
        assert!((u - Vec3::Y).length() < 1e-6);
    }
}
