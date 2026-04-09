//! Debug draw trait and line types for physics visualization.
//!
//! This module provides an abstraction layer so the render crate can
//! visualize debug information (colliders, joints, etc.) without
//! depending on any specific physics implementation.
//!
//! # Architecture
//!
//! - `DebugLine` — a simple POD line segment in world space
//! - `DebugDraw` — a trait that physics engines implement to produce lines
//! - `DebugDrawColors` — conventional colors for different debug elements
//!
//! # Example
//!
//! ```ignore
//! use quasar_core::debug_draw::{DebugDraw, DebugLine, DebugDrawColors};
//!
//! // Physics crate implements DebugDraw:
//! impl DebugDraw for PhysicsWorld {
//!     fn generate_debug_lines(&self, config: &DebugDrawConfig) -> Vec<DebugLine> {
//!         // ...generate lines from collider state...
//!     }
//! }
//!
//! // Render crate consumes the trait output:
//! let lines = world.generate_debug_lines(&config);
//! debug_renderer.render(&lines);
//! ```

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// DebugLine
// ---------------------------------------------------------------------------

/// A single debug line segment in world space.
///
/// This is a simple, serializable POD type so any system can produce
/// debug lines and any renderer can consume them without coupling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DebugLine {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub color: [f32; 4],
}

impl DebugLine {
    /// Create a new debug line from raw arrays.
    pub fn new(start: [f32; 3], end: [f32; 3], color: [f32; 4]) -> Self {
        Self { start, end, color }
    }

    /// Create a debug line from `glam::Vec3` positions.
    pub fn from_vec3(start: Vec3, end: Vec3, color: [f32; 4]) -> Self {
        Self {
            start: start.to_array(),
            end: end.to_array(),
            color,
        }
    }
}

// ---------------------------------------------------------------------------
// DebugDrawColors
// ---------------------------------------------------------------------------

/// Colors used by debug visualization systems.
///
/// Provides sensible defaults so implementations don't need to hard-code
/// colors for colliders, AABBs, joints, etc.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DebugDrawColors {
    /// Color for solid colliders (default: green).
    pub collider: [f32; 4],
    /// Color for AABB wireframes (default: yellow, translucent).
    pub aabb: [f32; 4],
    /// Color for joint connections (default: blue).
    pub joint: [f32; 4],
    /// Color for contact points (default: red).
    pub contact: [f32; 4],
    /// Color for trigger volumes (default: magenta, translucent).
    pub trigger: [f32; 4],
}

impl Default for DebugDrawColors {
    fn default() -> Self {
        Self {
            collider: [0.0, 1.0, 0.0, 1.0],
            aabb: [1.0, 1.0, 0.0, 0.5],
            joint: [0.0, 0.5, 1.0, 1.0],
            contact: [1.0, 0.0, 0.0, 1.0],
            trigger: [1.0, 0.0, 1.0, 0.6],
        }
    }
}

// ---------------------------------------------------------------------------
// DebugDrawConfig
// ---------------------------------------------------------------------------

/// Configuration controlling which debug elements are drawn.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DebugDrawConfig {
    pub draw_colliders: bool,
    pub draw_aabbs: bool,
    pub draw_joints: bool,
    pub draw_contacts: bool,
    pub draw_triggers: bool,
    pub colors: DebugDrawColors,
}

impl Default for DebugDrawConfig {
    fn default() -> Self {
        Self {
            draw_colliders: true,
            draw_aabbs: false,
            draw_joints: true,
            draw_contacts: true,
            draw_triggers: true,
            colors: DebugDrawColors::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// DebugDraw trait
// ---------------------------------------------------------------------------

/// Trait for any system that can produce debug visualization lines.
///
/// Implement this trait on your physics world (or any other system) so
/// the render crate can consume debug output without a direct dependency.
///
/// # Example
///
/// ```ignore
/// use quasar_core::debug_draw::{DebugDraw, DebugDrawConfig, DebugLine};
///
/// impl DebugDraw for MyPhysicsWorld {
///     fn generate_debug_lines(&self, config: &DebugDrawConfig) -> Vec<DebugLine> {
///         let mut lines = Vec::new();
///         if config.draw_colliders {
///             for collider in &self.colliders {
///                 // generate wireframe lines...
///             }
///         }
///         lines
///     }
/// }
/// ```
pub trait DebugDraw {
    /// Generate debug lines for the current state of the system.
    fn generate_debug_lines(&self, config: &DebugDrawConfig) -> Vec<DebugLine>;
}
