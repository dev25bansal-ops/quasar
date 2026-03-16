//! Character controller — wraps Rapier's `KinematicCharacterController`
//! for smooth player movement with ground detection, slope handling,
//! and step climbing.

use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

use crate::world::PhysicsWorld;

/// Configuration for a character controller.
#[derive(Debug, Clone)]
pub struct CharacterControllerConfig {
    /// Maximum slope angle the character can walk on (radians).
    pub max_slope_angle: f32,
    /// Extra offset used to detect ground contact.
    pub offset: f32,
    /// Maximum step height the character can climb.
    pub max_step_height: f32,
    /// Minimum distance to maintain from obstacles.
    pub min_distance: f32,
    /// If true, snaps to the ground when stepping off edges.
    pub snap_to_ground: Option<f32>,
    /// Whether to slide against walls rather than stop.
    pub slide: bool,
}

impl Default for CharacterControllerConfig {
    fn default() -> Self {
        Self {
            max_slope_angle: std::f32::consts::FRAC_PI_4, // 45 degrees
            offset: 0.01,
            max_step_height: 0.25,
            min_distance: 0.001,
            snap_to_ground: Some(0.2),
            slide: true,
        }
    }
}

/// Result of a character movement step.
#[derive(Debug, Clone)]
pub struct CharacterMovementResult {
    /// The effective translation that was applied.
    pub effective_translation: [f32; 3],
    /// Whether the character is currently on the ground.
    pub grounded: bool,
}

/// ECS component for a kinematic character controller.
///
/// Attach this to an entity that also has a `RigidBodyComponent`
/// (KinematicPositionBased) and a `ColliderComponent`.
///
/// Use `move_and_slide()` via the physics resource to apply movement.
#[derive(Debug, Clone)]
pub struct CharacterControllerComponent {
    /// The collider handle used for character shape-casting.
    pub collider_handle: ColliderHandle,
    /// Configuration for the controller.
    pub config: CharacterControllerConfig,
    /// Whether the character is currently grounded (updated each frame).
    pub grounded: bool,
    /// The effective velocity after the last move (for game logic).
    pub effective_velocity: [f32; 3],
}

impl CharacterControllerComponent {
    pub fn new(collider_handle: ColliderHandle) -> Self {
        Self {
            collider_handle,
            config: CharacterControllerConfig::default(),
            grounded: false,
            effective_velocity: [0.0; 3],
        }
    }

    pub fn with_config(mut self, config: CharacterControllerConfig) -> Self {
        self.config = config;
        self
    }

    /// Check whether the character is currently on the ground.
    pub fn is_grounded(&self) -> bool {
        self.grounded
    }
}

impl PhysicsWorld {
    /// Move a character controller using shape-casting, handling
    /// collisions, slopes, and steps automatically.
    ///
    /// Returns the effective translation and grounded state.
    pub fn move_character(
        &mut self,
        body_handle: RigidBodyHandle,
        collider_handle: ColliderHandle,
        desired_translation: [f32; 3],
        config: &CharacterControllerConfig,
        dt: f32,
    ) -> CharacterMovementResult {
        // Rebuild query pipeline if needed.
        if self.query_pipeline_dirty {
            self.query_pipeline.update(&self.colliders);
            self.query_pipeline_dirty = false;
        }

        let controller = KinematicCharacterController {
            max_slope_climb_angle: config.max_slope_angle,
            min_slope_slide_angle: config.max_slope_angle,
            offset: CharacterLength::Absolute(config.offset),
            slide: config.slide,
            snap_to_ground: config.snap_to_ground.map(CharacterLength::Absolute),
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(config.max_step_height),
                min_width: CharacterLength::Absolute(0.1),
                include_dynamic_bodies: false,
            }),
            ..Default::default()
        };

        let desired = nalgebra::vector![
            desired_translation[0],
            desired_translation[1],
            desired_translation[2]
        ];

        // Exclude the character's own collider from collision checks.
        let filter = QueryFilter::default().exclude_rigid_body(body_handle);

        let result = controller.move_shape(
            dt,
            &self.bodies,
            &self.colliders,
            &self.query_pipeline,
            self.colliders
                .get(collider_handle)
                .map(|c| c.shape())
                .unwrap_or(&*SharedShape::ball(0.5)),
            &self
                .bodies
                .get(body_handle)
                .map(|rb| rb.position())
                .copied()
                .unwrap_or(Isometry::identity()),
            desired,
            filter,
            |_| {},
        );

        let grounded = result.grounded;
        let t = result.translation;

        // Apply the effective translation to the kinematic body.
        if let Some(rb) = self.bodies.get_mut(body_handle) {
            let mut pos = *rb.position();
            pos.translation.vector += t;
            rb.set_next_kinematic_translation(pos.translation.vector);
        }

        CharacterMovementResult {
            effective_translation: [t.x, t.y, t.z],
            grounded,
        }
    }
}
