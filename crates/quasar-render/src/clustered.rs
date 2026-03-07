//! Clustered forward rendering — bins lights into a 3D froxel grid
//! so the per-fragment shader only iterates lights that overlap each cluster.
//!
//! Grid dimensions: `CLUSTER_X × CLUSTER_Y × CLUSTER_Z` tiles covering
//! the view frustum. Each cluster stores indices into a global light list.

use crate::light::PointLight;

/// Number of cluster tiles along the X axis.
pub const CLUSTER_X: usize = 16;
/// Number of cluster tiles along the Y axis.
pub const CLUSTER_Y: usize = 9;
/// Number of cluster tiles along the Z (depth) axis — logarithmic slices.
pub const CLUSTER_Z: usize = 24;
/// Maximum number of lights per cluster before overflow.
pub const MAX_LIGHTS_PER_CLUSTER: usize = 128;
/// Total number of clusters.
pub const TOTAL_CLUSTERS: usize = CLUSTER_X * CLUSTER_Y * CLUSTER_Z;

/// Axis-aligned bounding box for a cluster in view space.
#[derive(Debug, Clone, Copy)]
pub struct ClusterAabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// A single cluster containing indices into the light list.
#[derive(Clone)]
pub struct Cluster {
    pub light_count: u32,
    pub light_indices: [u16; MAX_LIGHTS_PER_CLUSTER],
}

impl Default for Cluster {
    fn default() -> Self {
        Self {
            light_count: 0,
            light_indices: [0; MAX_LIGHTS_PER_CLUSTER],
        }
    }
}

/// The full 3D cluster grid.
pub struct LightClusterGrid {
    pub clusters: Vec<Cluster>,
    /// View-space AABBs for each cluster (rebuilt when camera changes).
    pub aabbs: Vec<ClusterAabb>,
    pub near: f32,
    pub far: f32,
    pub screen_width: u32,
    pub screen_height: u32,
}

impl LightClusterGrid {
    /// Create a new cluster grid for the given camera parameters.
    pub fn new(near: f32, far: f32, screen_width: u32, screen_height: u32) -> Self {
        let mut grid = Self {
            clusters: vec![Cluster::default(); TOTAL_CLUSTERS],
            aabbs: vec![
                ClusterAabb {
                    min: [0.0; 3],
                    max: [0.0; 3],
                };
                TOTAL_CLUSTERS
            ],
            near,
            far,
            screen_width,
            screen_height,
        };
        grid.rebuild_aabbs();
        grid
    }

    /// Recompute the view-space AABBs for each cluster (call when the camera or
    /// viewport changes).
    pub fn rebuild_aabbs(&mut self) {
        let tile_w = self.screen_width as f32 / CLUSTER_X as f32;
        let tile_h = self.screen_height as f32 / CLUSTER_Y as f32;

        let log_ratio = (self.far / self.near).ln();

        for z in 0..CLUSTER_Z {
            let z_near = self.near * ((z as f32 / CLUSTER_Z as f32) * log_ratio).exp();
            let z_far = self.near * (((z + 1) as f32 / CLUSTER_Z as f32) * log_ratio).exp();

            for y in 0..CLUSTER_Y {
                for x in 0..CLUSTER_X {
                    let idx = z * CLUSTER_X * CLUSTER_Y + y * CLUSTER_X + x;

                    let min_x = x as f32 * tile_w;
                    let max_x = (x + 1) as f32 * tile_w;
                    let min_y = y as f32 * tile_h;
                    let max_y = (y + 1) as f32 * tile_h;

                    // Convert screen-space tile to NDC-like normalised coords.
                    let ndc_min_x = min_x / self.screen_width as f32 * 2.0 - 1.0;
                    let ndc_max_x = max_x / self.screen_width as f32 * 2.0 - 1.0;
                    let ndc_min_y = min_y / self.screen_height as f32 * 2.0 - 1.0;
                    let ndc_max_y = max_y / self.screen_height as f32 * 2.0 - 1.0;

                    self.aabbs[idx] = ClusterAabb {
                        min: [ndc_min_x * z_near, ndc_min_y * z_near, z_near],
                        max: [ndc_max_x * z_far, ndc_max_y * z_far, z_far],
                    };
                }
            }
        }
    }

    /// Assign point lights to clusters. Call once per frame after
    /// view-space light positions are known.
    pub fn assign_lights(&mut self, lights: &[(PointLight, [f32; 3])]) {
        // Clear previous assignments.
        for cluster in &mut self.clusters {
            cluster.light_count = 0;
        }

        for (light_idx, (light, view_pos)) in lights.iter().enumerate() {
            let radius = light.range;

            // Find the range of clusters this light's bounding sphere overlaps.
            let z_range = self.z_range_for_sphere(view_pos, radius);
            let (z_min, z_max) = match z_range {
                Some(r) => r,
                None => continue,
            };

            for z in z_min..=z_max {
                for y in 0..CLUSTER_Y {
                    for x in 0..CLUSTER_X {
                        let idx = z * CLUSTER_X * CLUSTER_Y + y * CLUSTER_X + x;
                        if sphere_aabb_intersect(view_pos, radius, &self.aabbs[idx]) {
                            let cluster = &mut self.clusters[idx];
                            if (cluster.light_count as usize) < MAX_LIGHTS_PER_CLUSTER {
                                cluster.light_indices[cluster.light_count as usize] =
                                    light_idx as u16;
                                cluster.light_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns the Z-slice range `(min, max)` that a sphere at `view_pos` with
    /// the given `radius` overlaps, or `None` if entirely outside the frustum.
    fn z_range_for_sphere(&self, pos: &[f32; 3], radius: f32) -> Option<(usize, usize)> {
        let z = pos[2]; // View-space depth (positive = into screen)
        let z_min_f = z - radius;
        let z_max_f = z + radius;

        if z_max_f < self.near || z_min_f > self.far {
            return None;
        }

        let log_ratio = (self.far / self.near).ln();
        let slice = |depth: f32| -> usize {
            let d = depth.clamp(self.near, self.far);
            let t = (d / self.near).ln() / log_ratio;
            ((t * CLUSTER_Z as f32) as usize).min(CLUSTER_Z - 1)
        };

        Some((slice(z_min_f.max(self.near)), slice(z_max_f.min(self.far))))
    }
}

/// Test sphere vs AABB intersection.
fn sphere_aabb_intersect(center: &[f32; 3], radius: f32, aabb: &ClusterAabb) -> bool {
    let mut dist_sq: f32 = 0.0;
    for i in 0..3 {
        let v = center[i];
        let clamped = v.clamp(aabb.min[i], aabb.max[i]);
        dist_sq += (v - clamped) * (v - clamped);
    }
    dist_sq <= radius * radius
}

/// WGSL include snippet that iterates lights from the cluster grid.
///
/// Intended to be prepended to a forward PBR fragment shader.
pub const CLUSTERED_LIGHT_WGSL: &str = r#"
struct LightCluster {
    count: u32,
    indices: array<u32, 128>,
};

@group(2) @binding(0) var<storage, read> clusters: array<LightCluster>;
@group(2) @binding(1) var<storage, read> light_positions: array<vec4<f32>>;
@group(2) @binding(2) var<storage, read> light_colors: array<vec4<f32>>;

fn get_cluster_index(frag_coord: vec2<f32>, depth: f32, screen_size: vec2<f32>, near: f32, far: f32) -> u32 {
    let tile_x = u32(frag_coord.x / screen_size.x * 16.0);
    let tile_y = u32(frag_coord.y / screen_size.y * 9.0);
    let log_ratio = log(far / near);
    let z_slice = u32(log(depth / near) / log_ratio * 24.0);
    return z_slice * 16u * 9u + tile_y * 16u + tile_x;
}
"#;
