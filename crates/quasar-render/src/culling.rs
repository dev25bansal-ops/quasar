//! Frustum culling and render batching utilities.
//!
//! Provides an axis-aligned bounding box (AABB) against a view-projection
//! frustum test and a helper to sort objects by mesh identity for batching.

use glam::{Mat4, Vec3, Vec4};

// ---------------------------------------------------------------------------
// AABB
// ---------------------------------------------------------------------------

/// Axis-aligned bounding box described by its min and max corners.
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Unit cube AABB: [-0.5, 0.5] on each axis.
    pub const UNIT_CUBE: Self = Self {
        min: Vec3::new(-0.5, -0.5, -0.5),
        max: Vec3::new(0.5, 0.5, 0.5),
    };

    /// Unit sphere AABB: [-1, 1] on each axis.
    pub const UNIT_SPHERE: Self = Self {
        min: Vec3::new(-1.0, -1.0, -1.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    };

    /// Unit cylinder AABB: radius 0.5, height 1.0 centered at origin.
    pub const UNIT_CYLINDER: Self = Self {
        min: Vec3::new(-0.5, -0.5, -0.5),
        max: Vec3::new(0.5, 0.5, 0.5),
    };

    /// Large plane AABB (10x0.01x10).
    pub const UNIT_PLANE: Self = Self {
        min: Vec3::new(-5.0, -0.01, -5.0),
        max: Vec3::new(5.0, 0.01, 5.0),
    };

    /// Transform this AABB by a model matrix, returning the new world-space AABB.
    ///
    /// Uses the standard 8-corner transformation to produce a tight AABB.
    pub fn transformed(&self, model: &Mat4) -> Self {
        let corners = [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ];

        let mut new_min = Vec3::splat(f32::INFINITY);
        let mut new_max = Vec3::splat(f32::NEG_INFINITY);
        for c in &corners {
            let w = model.transform_point3(*c);
            new_min = new_min.min(w);
            new_max = new_max.max(w);
        }
        Self {
            min: new_min,
            max: new_max,
        }
    }
}

// ---------------------------------------------------------------------------
// Plane
// ---------------------------------------------------------------------------

/// A single frustum plane (normal + distance).
///
/// Plane equation: `normal · point + distance >= 0` for the inside half-space.
#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

// ---------------------------------------------------------------------------
// Frustum
// ---------------------------------------------------------------------------

/// Six frustum planes extracted from a view-projection matrix.
///
/// Each plane is stored as a `Vec4` where `(x, y, z)` is the normal and `w`
/// is the distance. The plane equation is `normal · point + distance >= 0`
/// for the *inside* half-space.
///
/// Plane order: left, right, bottom, top, near, far.
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    /// Extract frustum planes from a combined view-projection matrix.
    ///
    /// Uses the Gribb/Hartmann method.
    pub fn from_view_proj(vp: &Mat4) -> Self {
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);

        let mut planes = [
            r3 + r0, // left
            r3 - r0, // right
            r3 + r1, // bottom
            r3 - r1, // top
            r3 + r2, // near
            r3 - r2, // far
        ];

        // Normalize planes.
        for p in &mut planes {
            let len = Vec3::new(p.x, p.y, p.z).length();
            if len > 1e-8 {
                *p /= len;
            }
        }

        Self { planes }
    }

    /// Access the raw plane array (left, right, bottom, top, near, far).
    #[inline]
    pub fn planes(&self) -> &[Vec4; 6] {
        &self.planes
    }

    /// Test an AABB against the frustum.
    ///
    /// Returns `true` if the AABB is at least partially inside the frustum.
    ///
    /// Uses the "positive vertex" optimisation: for each plane we find the
    /// corner of the AABB that extends farthest in the direction of the plane
    /// normal.  If that corner is behind the plane, the entire AABB is outside.
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        for plane in &self.planes {
            let px = if plane.x >= 0.0 { aabb.max.x } else { aabb.min.x };
            let py = if plane.y >= 0.0 { aabb.max.y } else { aabb.min.y };
            let pz = if plane.z >= 0.0 { aabb.max.z } else { aabb.min.z };

            if plane.x * px + plane.y * py + plane.z * pz + plane.w < 0.0 {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Render statistics
// ---------------------------------------------------------------------------

/// Rendering statistics for the current frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    /// Total number of objects submitted to the render pipeline.
    pub total_objects: u32,
    /// Number of objects that passed frustum culling and were drawn.
    pub rendered_objects: u32,
    /// Number of objects culled by the frustum test.
    pub culled_objects: u32,
}

impl RenderStats {
    /// Reset all counters to zero.
    pub fn reset(&mut self) {
        self.total_objects = 0;
        self.rendered_objects = 0;
        self.culled_objects = 0;
    }
}

// ---------------------------------------------------------------------------
// Culling helpers
// ---------------------------------------------------------------------------

/// Cull a list of objects by their world-space AABB against the camera frustum.
///
/// Returns a `RenderStats` struct with the total, rendered, and culled counts,
/// along with a `Vec<bool>` mask indicating which objects are visible (`true`)
/// and which were culled (`false`).
///
/// Each object is represented as a `(local_aabb, model_matrix)` tuple.
pub fn cull_objects_slice(
    frustum: &Frustum,
    objects: &[(Aabb, Mat4)],
) -> (RenderStats, Vec<bool>) {
    let total = objects.len() as u32;
    let mut culled_count = 0u32;
    let mut visibility = Vec::with_capacity(objects.len());

    for (local_aabb, model) in objects {
        let world_aabb = local_aabb.transformed(model);
        let visible = frustum.intersects_aabb(&world_aabb);
        if !visible {
            culled_count += 1;
        }
        visibility.push(visible);
    }

    let stats = RenderStats {
        total_objects: total,
        rendered_objects: total - culled_count,
        culled_objects: culled_count,
    };
    (stats, visibility)
}

/// Sort a slice of `(mesh_key, model_matrix, material_bind_group)` tuples by
/// `mesh_key` so that objects sharing the same mesh are adjacent.
///
/// This minimises vertex/index buffer switches during rendering.
pub fn sort_by_mesh<K: Ord, T>(objects: &mut [(K, T)]) {
    objects.sort_unstable_by(|a, b| a.0.cmp(&b.0));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_cube_inside_identity_frustum() {
        // Identity VP -- NDC cube [-1,1].
        let frustum = Frustum::from_view_proj(&Mat4::IDENTITY);
        assert!(frustum.intersects_aabb(&Aabb::UNIT_CUBE));
    }

    #[test]
    fn far_aabb_outside_frustum() {
        let vp = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_proj(&vp);

        // An object 500 units away should be culled.
        let far_aabb = Aabb {
            min: Vec3::new(499.0, -0.5, -0.5),
            max: Vec3::new(500.0, 0.5, 0.5),
        };
        assert!(!frustum.intersects_aabb(&far_aabb));
    }

    #[test]
    fn near_aabb_inside_frustum() {
        let vp = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_proj(&vp);

        // Object at origin should be visible.
        assert!(frustum.intersects_aabb(&Aabb::UNIT_CUBE));
    }

    #[test]
    fn cull_objects_slice_counts_add_up() {
        let vp = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_proj(&vp);

        // 4 objects: 2 visible (near origin), 2 far away (culled)
        let objects: [(Aabb, Mat4); 4] = [
            // Visible -- at origin
            (Aabb::UNIT_CUBE, Mat4::IDENTITY),
            (Aabb::UNIT_CUBE, Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0))),
            // Culled -- 500 units away
            (Aabb::UNIT_CUBE, Mat4::from_translation(Vec3::new(500.0, 0.0, 0.0))),
            (Aabb::UNIT_CUBE, Mat4::from_translation(Vec3::new(-500.0, 0.0, 0.0))),
        ];
        let slice: Vec<_> = objects.iter().copied().collect();
        let (stats, visibility) = cull_objects_slice(&frustum, &slice);

        assert_eq!(stats.total_objects, 4);
        assert_eq!(stats.rendered_objects, 2);
        assert_eq!(stats.culled_objects, 2);
        assert_eq!(visibility, vec![true, true, false, false]);
    }

    #[test]
    fn render_stats_reset() {
        let mut stats = RenderStats {
            total_objects: 100,
            rendered_objects: 60,
            culled_objects: 40,
        };
        stats.reset();
        assert_eq!(stats.total_objects, 0);
        assert_eq!(stats.rendered_objects, 0);
        assert_eq!(stats.culled_objects, 0);
    }

    #[test]
    fn plane_extraction_from_perspective() {
        let vp = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 16.0 / 9.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let frustum = Frustum::from_view_proj(&vp);

        // All planes should be normalized (normal length ~1.0)
        for plane in frustum.planes() {
            let len = Vec3::new(plane.x, plane.y, plane.z).length();
            assert!((len - 1.0).abs() < 1e-5, "Plane not normalized: {plane:?}");
        }
    }

    #[test]
    fn aabb_partially_intersecting_frustum() {
        let vp = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_proj(&vp);

        // Large AABB that partially overlaps the frustum
        let large_aabb = Aabb {
            min: Vec3::new(-10.0, -10.0, -20.0),
            max: Vec3::new(10.0, 10.0, 20.0),
        };
        assert!(frustum.intersects_aabb(&large_aabb));
    }

    #[test]
    fn aabb_to_left_of_frustum() {
        let vp = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_proj(&vp);

        // Object far to the left of camera view
        let left_aabb = Aabb {
            min: Vec3::new(-50.0, -0.5, 3.0),
            max: Vec3::new(-49.0, 0.5, 4.0),
        };
        assert!(!frustum.intersects_aabb(&left_aabb));
    }
}
