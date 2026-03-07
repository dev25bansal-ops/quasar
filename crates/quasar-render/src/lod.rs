//! Level of Detail (LOD) system.
//!
//! Provides `LodGroup` — an ECS component that holds a list of
//! mesh variants and their maximum display distances.  The `LodSystem`
//! selects the appropriate mesh each frame based on the entity's
//! distance to the active camera.

use quasar_core::ecs::{Entity, System, World};
use quasar_math::Transform;

use crate::camera::Camera;
use crate::mesh::MeshShape;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// A single LOD level.
#[derive(Debug, Clone)]
pub struct LodLevel {
    /// The mesh to use at this level.
    pub mesh: MeshShape,
    /// Maximum distance (from camera) at which this LOD is visible.
    /// The first level whose `max_distance >= actual_distance` wins.
    pub max_distance: f32,
}

/// Component that enables automatic LOD switching.
///
/// Levels should be sorted from highest detail (smallest `max_distance`)
/// to lowest detail (largest `max_distance`).
#[derive(Debug, Clone)]
pub struct LodGroup {
    pub levels: Vec<LodLevel>,
}

impl LodGroup {
    pub fn new(levels: Vec<LodLevel>) -> Self {
        Self { levels }
    }

    /// Pick the appropriate `MeshShape` for a given distance.
    pub fn select(&self, distance: f32) -> Option<&MeshShape> {
        for level in &self.levels {
            if distance <= level.max_distance {
                return Some(&level.mesh);
            }
        }
        // Fall back to the last (coarsest) level if nothing matched.
        self.levels.last().map(|l| &l.mesh)
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// System that updates each entity's `MeshShape` based on camera distance.
pub struct LodSystem;

impl System for LodSystem {
    fn name(&self) -> &str {
        "lod"
    }

    fn run(&mut self, world: &mut World) {
        // Find the camera position.
        let cam_pos = {
            let cameras: Vec<glam::Vec3> = world
                .query2::<Camera, Transform>()
                .into_iter()
                .map(|(_, _cam, tf)| tf.position)
                .collect();
            match cameras.first() {
                Some(p) => *p,
                None => return,
            }
        };

        // Collect entities with LodGroup + Transform.
        let lod_entities: Vec<(Entity, LodGroup, glam::Vec3)> = world
            .query2::<LodGroup, Transform>()
            .into_iter()
            .map(|(e, lod, tf)| (e, lod.clone(), tf.position))
            .collect();

        for (entity, lod, pos) in lod_entities {
            let distance = cam_pos.distance(pos);
            if let Some(desired_mesh) = lod.select(distance) {
                if let Some(current) = world.get_mut::<MeshShape>(entity) {
                    // Only write if the variant actually changed.
                    if std::mem::discriminant(current) != std::mem::discriminant(desired_mesh) {
                        *current = desired_mesh.clone();
                    }
                }
            }
        }
    }
}
