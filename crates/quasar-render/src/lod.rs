//! Level of Detail (LOD) system.
//!
//! Provides `LodGroup` — an ECS component that holds a list of
//! mesh variants and their maximum display distances.  The `LodSystem`
//! selects the appropriate mesh each frame based on the entity's
//! distance to the active camera.
//!
//! Cross-fade dithering: within a configurable transition band around
//! each LOD boundary the system outputs a `LodCrossFade` component
//! carrying the blend factor for a screen-space dither discard pattern.

use quasar_core::ecs::{Entity, System, World};
use quasar_math::Transform;

use crate::camera::Camera;
use crate::mesh::MeshShape;

/// Width (in distance units) of the cross-fade band around each LOD
/// transition boundary.  Entities within `[boundary - HALF, boundary + HALF]`
/// are dithered.
pub const LOD_CROSSFADE_BAND: f32 = 5.0;

// ---------------------------------------------------------------------------
// Components
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

/// Attached to entities currently in a LOD cross-fade transition.
///
/// `blend` goes from `0.0` (fully current LOD) to `1.0` (fully next LOD).
/// The renderer uses this to drive a screen-space dither pattern: fragments
/// whose dither threshold exceeds `blend` are discarded.
#[derive(Debug, Clone, Copy)]
pub struct LodCrossFade {
    /// 0.0 = fully outgoing LOD, 1.0 = fully incoming.
    pub blend: f32,
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

    /// Returns `(selected_index, blend_factor)` where `blend_factor` is
    /// `0.0` when well within the chosen band and approaches `1.0` as
    /// we near the next transition boundary.
    pub fn select_with_fade(&self, distance: f32) -> (usize, f32) {
        let half = LOD_CROSSFADE_BAND * 0.5;
        for (i, level) in self.levels.iter().enumerate() {
            if distance <= level.max_distance {
                let edge = level.max_distance;
                let fade = if distance > edge - half {
                    (distance - (edge - half)) / LOD_CROSSFADE_BAND
                } else {
                    0.0
                };
                return (i, fade.clamp(0.0, 1.0));
            }
        }
        (self.levels.len().saturating_sub(1), 0.0)
    }
}

// ---------------------------------------------------------------------------
// 4×4 Bayer dither threshold matrix (for use in shaders / CPU discard).
// ---------------------------------------------------------------------------

/// 4×4 Bayer ordered-dithering matrix, values normalised to [0, 1).
pub const BAYER_4X4: [f32; 16] = [
     0.0 / 16.0,  8.0 / 16.0,  2.0 / 16.0, 10.0 / 16.0,
    12.0 / 16.0,  4.0 / 16.0, 14.0 / 16.0,  6.0 / 16.0,
     3.0 / 16.0, 11.0 / 16.0,  1.0 / 16.0,  9.0 / 16.0,
    15.0 / 16.0,  7.0 / 16.0, 13.0 / 16.0,  5.0 / 16.0,
];

/// Look up the Bayer threshold for a screen-space pixel.
pub fn bayer_threshold(screen_x: u32, screen_y: u32) -> f32 {
    let bx = (screen_x % 4) as usize;
    let by = (screen_y % 4) as usize;
    BAYER_4X4[by * 4 + bx]
}

/// WGSL snippet that can be `#include`-d into any fragment shader to
/// implement LOD cross-fade dithering.
pub const LOD_CROSSFADE_WGSL: &str = r#"
// LOD cross-fade dithering.
// `blend` uniform should be set from LodCrossFade::blend.
// Call discard_crossfade(frag_coord.xy, blend) at the start of the fragment shader.
fn bayer4x4(coord: vec2<u32>) -> f32 {
    let m = array<f32, 16>(
         0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0,
        12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0,
         3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0,
        15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0,
    );
    return m[(coord.y % 4u) * 4u + (coord.x % 4u)];
}

fn discard_crossfade(frag_coord: vec2<f32>, blend: f32) {
    let threshold = bayer4x4(vec2<u32>(u32(frag_coord.x), u32(frag_coord.y)));
    if threshold >= blend { discard; }
}
"#;

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// System that updates each entity's `MeshShape` based on camera distance
/// and attaches [`LodCrossFade`] during transition bands.
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
            let (_idx, blend) = lod.select_with_fade(distance);

            if let Some(desired_mesh) = lod.select(distance) {
                if let Some(current) = world.get_mut::<MeshShape>(entity) {
                    if std::mem::discriminant(current) != std::mem::discriminant(desired_mesh) {
                        *current = desired_mesh.clone();
                    }
                }
            }

            // Attach / update cross-fade component.
            if blend > 0.0 {
                if let Some(cf) = world.get_mut::<LodCrossFade>(entity) {
                    cf.blend = blend;
                } else {
                    world.insert(entity, LodCrossFade { blend });
                }
            } else {
                world.remove_component::<LodCrossFade>(entity);
            }
        }
    }
}
