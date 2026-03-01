//! 3D transform — position, rotation, and scale.

use glam::{Mat4, Quat, Vec3};

/// A 3D transform representing position, rotation, and uniform scale.
///
/// Used as a component on entities to define where they exist in the world.
#[derive(Debug, Clone, Copy)]
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
            self.rotation = Quat::from_mat4(&Mat4::look_to_rh(Vec3::ZERO, forward, corrected_up)).conjugate();
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
