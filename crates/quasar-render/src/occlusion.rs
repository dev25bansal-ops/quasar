//! Hierarchical Z-Buffer (Hi-Z) occlusion culling.
//!
//! After the depth pre-pass (or after the geometry pass in deferred mode) the
//! Hi-Z system builds a mip-chain of the depth buffer.  Each object's
//! screen-space bounding rect is tested against the appropriate mip level —
//! if the closest depth in the mip tile is nearer than the object, the object
//! is fully occluded and can be skipped.
//!
//! This implementation does the test on the CPU side for simplicity.
//! A GPU compute variant could be added later for higher throughput.

use glam::{Mat4, Vec3, Vec4};

/// Number of mip levels in the depth pyramid.
pub const HIZ_MIP_LEVELS: usize = 8;

/// Hi-Z depth pyramid — a conservative depth mip-chain.
pub struct HiZBuffer {
    /// Mip levels, index 0 = full-res, each subsequent level is half.
    pub mips: Vec<HiZMip>,
}

/// A single level of the Hi-Z mip-chain.
pub struct HiZMip {
    pub width: u32,
    pub height: u32,
    /// Depth values (row-major). Each value is the **maximum** (farthest)
    /// depth among the 2×2 parent texels — because in a reversed-Z setup the
    /// closer depth is *larger*, but we use a standard [0,1] depth where 0 is
    /// near and 1 is far, so storing the max is the *conservative* choice for
    /// occlusion: if the max (farthest) pixel is still closer than the object,
    /// the whole tile occludes it.
    pub data: Vec<f32>,
}

impl HiZBuffer {
    /// Build the Hi-Z pyramid from a full-resolution depth buffer.
    ///
    /// `src` is row-major f32 depth values for a `width × height` buffer.
    pub fn build(src: &[f32], width: u32, height: u32) -> Self {
        assert_eq!(src.len(), (width * height) as usize);

        let mut mips = Vec::with_capacity(HIZ_MIP_LEVELS);

        // Level 0 = copy of the original depth buffer.
        mips.push(HiZMip {
            width,
            height,
            data: src.to_vec(),
        });

        // Successive 2× downsample using max.
        for _ in 1..HIZ_MIP_LEVELS {
            let prev = mips.last().unwrap();
            let mw = (prev.width / 2).max(1);
            let mh = (prev.height / 2).max(1);
            let mut data = vec![0.0_f32; (mw * mh) as usize];

            for y in 0..mh {
                for x in 0..mw {
                    let sx = (x * 2) as usize;
                    let sy = (y * 2) as usize;
                    let pw = prev.width as usize;
                    let ph = prev.height as usize;

                    let s00 = prev.data[sy * pw + sx];
                    let s10 = if sx + 1 < pw {
                        prev.data[sy * pw + sx + 1]
                    } else {
                        s00
                    };
                    let s01 = if sy + 1 < ph {
                        prev.data[(sy + 1) * pw + sx]
                    } else {
                        s00
                    };
                    let s11 = if sx + 1 < pw && sy + 1 < ph {
                        prev.data[(sy + 1) * pw + sx + 1]
                    } else {
                        s00
                    };

                    // Max = farthest depth in the 2×2 block (standard Z: 0 near, 1 far).
                    data[(y * mw + x) as usize] = s00.max(s10).max(s01).max(s11);
                }
            }

            mips.push(HiZMip {
                width: mw,
                height: mh,
                data,
            });

            if mw == 1 && mh == 1 {
                break;
            }
        }

        Self { mips }
    }

    /// Test whether an axis-aligned bounding box (in world-space) is occluded.
    ///
    /// Returns `true` if the AABB is **visible** (not occluded), `false` if
    /// it is fully behind closer geometry and can be culled.
    pub fn is_visible(
        &self,
        aabb_min: Vec3,
        aabb_max: Vec3,
        view_proj: &Mat4,
        screen_width: f32,
        screen_height: f32,
    ) -> bool {
        // Project all 8 AABB corners to screen space.
        let corners = [
            Vec3::new(aabb_min.x, aabb_min.y, aabb_min.z),
            Vec3::new(aabb_max.x, aabb_min.y, aabb_min.z),
            Vec3::new(aabb_min.x, aabb_max.y, aabb_min.z),
            Vec3::new(aabb_max.x, aabb_max.y, aabb_min.z),
            Vec3::new(aabb_min.x, aabb_min.y, aabb_max.z),
            Vec3::new(aabb_max.x, aabb_min.y, aabb_max.z),
            Vec3::new(aabb_min.x, aabb_max.y, aabb_max.z),
            Vec3::new(aabb_max.x, aabb_max.y, aabb_max.z),
        ];

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;

        for c in &corners {
            let clip = *view_proj * Vec4::new(c.x, c.y, c.z, 1.0);
            if clip.w <= 0.0 {
                // Behind the camera — conservatively visible.
                return true;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            let ndc_z = clip.z / clip.w;

            // NDC to [0, screen_width/height]
            let sx = (ndc_x * 0.5 + 0.5) * screen_width;
            let sy = (0.5 - ndc_y * 0.5) * screen_height;

            min_x = min_x.min(sx);
            max_x = max_x.max(sx);
            min_y = min_y.min(sy);
            max_y = max_y.max(sy);
            min_z = min_z.min(ndc_z);
        }

        // Clamp screen rect
        let x0 = (min_x.floor() as i32).max(0) as u32;
        let y0 = (min_y.floor() as i32).max(0) as u32;
        let x1 = (max_x.ceil() as i32).max(0) as u32;
        let y1 = (max_y.ceil() as i32).max(0) as u32;

        if x0 >= x1 || y0 >= y1 {
            return false; // zero-area on screen
        }

        // Choose mip level based on bounding rect size.
        let rect_w = (x1 - x0) as f32;
        let rect_h = (y1 - y0) as f32;
        let max_dim = rect_w.max(rect_h);
        let mip = if max_dim <= 1.0 {
            0
        } else {
            (max_dim.log2().ceil() as usize).min(self.mips.len() - 1)
        };

        let level = &self.mips[mip];

        // Scale coordinates to the mip level.
        let scale_x = level.width as f32 / screen_width;
        let scale_y = level.height as f32 / screen_height;

        let mx0 = ((x0 as f32 * scale_x).floor() as u32).min(level.width.saturating_sub(1));
        let my0 = ((y0 as f32 * scale_y).floor() as u32).min(level.height.saturating_sub(1));
        let mx1 = ((x1 as f32 * scale_x).ceil() as u32).min(level.width);
        let my1 = ((y1 as f32 * scale_y).ceil() as u32).min(level.height);

        // Find the farthest depth in the mip region.
        let mut max_depth = 0.0_f32;
        for ry in my0..my1 {
            for rx in mx0..mx1 {
                let d = level.data[(ry * level.width + rx) as usize];
                max_depth = max_depth.max(d);
            }
        }

        // If the closest point of the AABB is farther than the farthest depth
        // in the hi-z tile, the object is occluded.
        // (standard Z: smaller Z = closer)
        if min_z > max_depth {
            return false; // occluded
        }

        true // visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiz_build_downsample() {
        let depth = vec![0.5_f32; 16]; // 4×4
        let hiz = HiZBuffer::build(&depth, 4, 4);
        assert!(hiz.mips.len() >= 3);
        assert_eq!(hiz.mips[0].width, 4);
        assert_eq!(hiz.mips[1].width, 2);
        assert_eq!(hiz.mips[2].width, 1);
    }

    #[test]
    fn hiz_occluded_behind_wall() {
        // A simple 4×4 depth buffer where everything is at depth 0.1 (very close).
        let depth = vec![0.1_f32; 16];
        let hiz = HiZBuffer::build(&depth, 4, 4);

        let vp = Mat4::IDENTITY;
        // An object at depth 0.5 — it's behind the wall at 0.1.
        assert!(!hiz.is_visible(
            Vec3::new(-0.1, -0.1, 0.5),
            Vec3::new(0.1, 0.1, 0.5),
            &vp,
            4.0,
            4.0,
        ));
    }
}
