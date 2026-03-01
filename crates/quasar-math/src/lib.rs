//! # Quasar Math
//!
//! Re-exports and extends [`glam`] with engine-specific types like
//! [`Transform`] and [`Color`].

pub mod transform;
pub mod color;

// Re-export glam types for convenience.
pub use glam::{
    Mat3, Mat4, Quat, Vec2, Vec3, Vec4,
    EulerRot, Affine3A,
    vec2, vec3, vec4,
};

pub use transform::{Transform, GlobalTransform};
pub use color::Color;
