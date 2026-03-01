//! # Quasar Physics
//!
//! Physics simulation powered by [Rapier3D](https://rapier.rs/).
//!
//! Provides rigid body dynamics, collision detection, and joint constraints
//! integrated with the Quasar ECS.
//!
//! **Status**: Scaffolded — full implementation coming in Week 2.

pub mod world;
pub mod rigidbody;
pub mod collider;

/// Re-export rapier types commonly used by game code.
pub use rapier3d;
