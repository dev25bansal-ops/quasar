//! Frustum culling and render batching utilities.
//!
//! Provides an axis-aligned bounding box (AABB) against a view-projection
//! frustum test and a helper to sort objects by mesh identity for batching.

use glam::{Mat4, Vec3, Vec4};

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

    /// Large plane AABB (10×0.01×10).
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

/// Six frustum planes extracted from a view-projection matrix.
///
/// Each plane is stored as `(nx, ny, nz, d)` where the plane equation is
/// `nx*x + ny*y + nz*z + d >= 0` for the *inside* half-space.
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

    /// Test an AABB against the frustum.
    ///
    /// Returns `true` if the AABB is at least partially inside the frustum.
    pub fn intersects_aabb(&self, aabb: &Aabb) -> bool {
        for plane in &self.planes {
            // Find the positive vertex — the corner most in the direction of the
            // plane normal.
            let px = if plane.x >= 0.0 {
                aabb.max.x
            } else {
                aabb.min.x
            };
            let py = if plane.y >= 0.0 {
                aabb.max.y
            } else {
                aabb.min.y
            };
            let pz = if plane.z >= 0.0 {
                aabb.max.z
            } else {
                aabb.min.z
            };

            if plane.x * px + plane.y * py + plane.z * pz + plane.w < 0.0 {
                // Entire AABB is outside this plane → outside frustum.
                return false;
            }
        }
        true
    }
}

/// Sort a slice of `(mesh_key, model_matrix, material_bind_group)` tuples by
/// `mesh_key` so that objects sharing the same mesh are adjacent.
///
/// This minimises vertex/index buffer switches during rendering.
pub fn sort_by_mesh<K: Ord, T>(objects: &mut [(K, T)]) {
    objects.sort_unstable_by(|a, b| a.0.cmp(&b.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_cube_inside_identity_frustum() {
        // Identity VP — NDC cube [-1,1].
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
}
