//! Character controller - wraps Rapier KinematicCharacterController
//! for smooth player movement with ground detection and step climbing.

use crate::world::PhysicsWorld;
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

/// Configuration for a character controller.
#[derive(Debug, Clone)]
pub struct CharacterControllerConfig {
    pub height: f32,
    pub radius: f32,
    pub max_slope_climb_angle: f32,
    pub min_slope_slide_angle: f32,
    pub step_height: f32,
    pub snap_to_ground_distance: f32,
    pub slide: bool,
    pub offset: f32,
}

impl Default for CharacterControllerConfig {
    fn default() -> Self {
        Self {
            height: 1.6,
            radius: 0.3,
            max_slope_climb_angle: std::f32::consts::FRAC_PI_4,
            min_slope_slide_angle: std::f32::consts::FRAC_PI_6,
            step_height: 0.25,
            snap_to_ground_distance: 0.2,
            slide: true,
            offset: 0.01,
        }
    }
}

/// Result of a character movement step.
#[derive(Debug, Clone)]
pub struct CharacterMovementResult {
    pub effective_translation: [f32; 3],
    pub grounded: bool,
    pub ground_normal: Option<[f32; 3]>,
    pub hit_wall: bool,
}

/// ECS component for a kinematic character controller.
#[derive(Debug, Clone)]
pub struct CharacterControllerComponent {
    pub collider_handle: ColliderHandle,
    pub config: CharacterControllerConfig,
    pub grounded: bool,
    pub desired_velocity: [f32; 3],
    pub effective_velocity: [f32; 3],
    pub ground_normal: Option<[f32; 3]>,
}

impl CharacterControllerComponent {
    pub fn new(collider_handle: ColliderHandle) -> Self {
        Self {
            collider_handle,
            config: CharacterControllerConfig::default(),
            grounded: false,
            desired_velocity: [0.0; 3],
            effective_velocity: [0.0; 3],
            ground_normal: None,
        }
    }

    pub fn with_config(mut self, config: CharacterControllerConfig) -> Self {
        self.config = config;
        self
    }

    pub fn set_velocity(&mut self, velocity: [f32; 3]) {
        self.desired_velocity = velocity;
    }

    pub fn is_grounded(&self) -> bool {
        self.grounded
    }
    pub fn ground_normal(&self) -> Option<[f32; 3]> {
        self.ground_normal
    }
    pub fn effective_velocity(&self) -> [f32; 3] {
        self.effective_velocity
    }
}

impl PhysicsWorld {
    pub fn move_character(
        &mut self,
        body_handle: RigidBodyHandle,
        collider_handle: ColliderHandle,
        desired_translation: [f32; 3],
        config: &CharacterControllerConfig,
        dt: f32,
    ) -> CharacterMovementResult {
        if self.query_pipeline_dirty {
            self.query_pipeline.update(&self.colliders);
            self.query_pipeline_dirty = false;
        }

        let controller = KinematicCharacterController {
            max_slope_climb_angle: config.max_slope_climb_angle,
            min_slope_slide_angle: config.min_slope_slide_angle,
            offset: CharacterLength::Absolute(config.offset),
            slide: config.slide,
            snap_to_ground: Some(CharacterLength::Absolute(config.snap_to_ground_distance)),
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(config.step_height),
                min_width: CharacterLength::Absolute(config.radius * 0.5),
                include_dynamic_bodies: false,
            }),
            ..Default::default()
        };

        let desired = nalgebra::vector![
            desired_translation[0],
            desired_translation[1],
            desired_translation[2]
        ];

        let filter = QueryFilter::default().exclude_rigid_body(body_handle);

        let result = controller.move_shape(
            dt,
            &self.bodies,
            &self.colliders,
            &self.query_pipeline,
            self.colliders
                .get(collider_handle)
                .map(|c| c.shape())
                .unwrap_or(&*SharedShape::capsule_y(config.height * 0.5, config.radius)),
            &self
                .bodies
                .get(body_handle)
                .map(|rb| rb.position())
                .copied()
                .unwrap_or(Isometry::identity()),
            desired,
            filter,
            |_collision| {},
        );

        let grounded = result.grounded;
        let t = result.translation;

        if let Some(rb) = self.bodies.get_mut(body_handle) {
            let mut pos = *rb.position();
            pos.translation.vector += t;
            rb.set_next_kinematic_translation(pos.translation.vector);
        }

        CharacterMovementResult {
            effective_translation: [t.x, t.y, t.z],
            grounded,
            ground_normal: None,
            hit_wall: t.x.abs() < desired.x.abs() * 0.9 || t.z.abs() < desired.z.abs() * 0.9,
        }
    }
}
