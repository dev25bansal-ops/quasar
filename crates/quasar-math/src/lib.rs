//! # Quasar Math
//!
//! Re-exports and extends [`glam`] with engine-specific types like
//! [`Transform`] and [`Color`].

pub mod color;
pub mod transform;

// Re-export glam types for convenience.
pub use glam::{vec2, vec3, vec4, Affine3A, EulerRot, Mat3, Mat4, Quat, Vec2, Vec3, Vec4};

pub use color::Color;
pub use transform::{GlobalTransform, Transform};
