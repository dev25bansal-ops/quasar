//! Terrain System — heightmap-based terrain with LOD and splatmap texturing.
//!
//! Provides:
//! - `TerrainConfig` — describes the terrain (size, resolution, heightmap data).
//! - `TerrainMesh` — generates and holds the GPU vertex/index buffers.
//! - `TerrainLod` — camera-distance based LOD with multiple detail levels.
//! - `TerrainSplatmap` — up to 4-layer texture blending via a weightmap.
//! - `HeightFieldCollider` — generates rapier3d `HeightField` collider data.

use glam::Vec3;

/// Maximum terrain LOD levels.
pub const MAX_TERRAIN_LODS: usize = 4;

/// Maximum texture layers in the splatmap.
pub const MAX_SPLAT_LAYERS: usize = 4;

/// Configuration for a terrain patch.
#[derive(Debug, Clone)]
pub struct TerrainConfig {
    /// World-space width (X axis).
    pub width: f32,
    /// World-space depth (Z axis).
    pub depth: f32,
    /// Maximum height (Y axis) when the heightmap value is 1.0.
    pub max_height: f32,
    /// Number of vertices along each axis at the highest LOD.
    pub resolution: u32,
    /// Row-major heightmap data, values in [0, 1].
    /// Length must be `resolution * resolution`.
    pub heightmap: Vec<f32>,
}

impl TerrainConfig {
    /// Sample the height at normalized (u, v) in [0, 1].
    pub fn sample_height(&self, u: f32, v: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let fx = u * (self.resolution - 1) as f32;
        let fz = v * (self.resolution - 1) as f32;
        let ix = (fx as u32).min(self.resolution - 2);
        let iz = (fz as u32).min(self.resolution - 2);
        let tx = fx - ix as f32;
        let tz = fz - iz as f32;
        let r = self.resolution;

        let h00 = self.heightmap[(iz * r + ix) as usize];
        let h10 = self.heightmap[(iz * r + ix + 1) as usize];
        let h01 = self.heightmap[((iz + 1) * r + ix) as usize];
        let h11 = self.heightmap[((iz + 1) * r + ix + 1) as usize];

        let h = h00 * (1.0 - tx) * (1.0 - tz)
            + h10 * tx * (1.0 - tz)
            + h01 * (1.0 - tx) * tz
            + h11 * tx * tz;

        h * self.max_height
    }

    /// Get the world-space position for grid coordinates.
    pub fn world_pos(&self, gx: u32, gz: u32) -> Vec3 {
        let u = gx as f32 / (self.resolution - 1) as f32;
        let v = gz as f32 / (self.resolution - 1) as f32;
        Vec3::new(
            u * self.width - self.width * 0.5,
            self.sample_height(u, v),
            v * self.depth - self.depth * 0.5,
        )
    }
}

/// A single terrain LOD level.
#[derive(Debug)]
pub struct TerrainLodLevel {
    /// Step size — every Nth vertex is included.
    pub step: u32,
    /// Squared distance threshold from camera beyond which this LOD activates.
    pub distance_sq: f32,
    /// Generated vertex positions (x, y, z) + normals (nx, ny, nz) + UV (u, v).
    pub vertices: Vec<[f32; 8]>,
    /// Triangle indices.
    pub indices: Vec<u32>,
}

/// Terrain mesh with multiple LOD levels.
pub struct TerrainMesh {
    pub lods: Vec<TerrainLodLevel>,
}

impl TerrainMesh {
    /// Generate LOD levels from a config.
    ///
    /// `lod_distances` are the camera distances at which each successive LOD
    /// is used.  The first entry is the max distance for LOD 0 (highest detail).
    pub fn generate(config: &TerrainConfig, lod_distances: &[f32]) -> Self {
        let num_lods = lod_distances.len().min(MAX_TERRAIN_LODS);
        let mut lods = Vec::with_capacity(num_lods);

        for lod_idx in 0..num_lods {
            let step = 1u32 << lod_idx; // 1, 2, 4, 8 …
            let dist_sq = lod_distances[lod_idx] * lod_distances[lod_idx];

            let mut vertices = Vec::new();
            let mut indices = Vec::new();

            let res = config.resolution;
            let mut gz = 0u32;
            let mut row_idx = 0u32;
            while gz < res {
                let mut gx = 0u32;
                let mut col_idx = 0u32;
                while gx < res {
                    let pos = config.world_pos(gx, gz);
                    let u = gx as f32 / (res - 1) as f32;
                    let v = gz as f32 / (res - 1) as f32;

                    // Approximate normal via central differences.
                    let eps = 1.0 / (res - 1) as f32;
                    let hx0 = config.sample_height((u - eps).max(0.0), v);
                    let hx1 = config.sample_height((u + eps).min(1.0), v);
                    let hz0 = config.sample_height(u, (v - eps).max(0.0));
                    let hz1 = config.sample_height(u, (v + eps).min(1.0));
                    let dx = (hx1 - hx0) / (2.0 * eps * config.width);
                    let dz = (hz1 - hz0) / (2.0 * eps * config.depth);
                    let normal = Vec3::new(-dx, 1.0, -dz).normalize();

                    vertices.push([pos.x, pos.y, pos.z, normal.x, normal.y, normal.z, u, v]);

                    gx += step;
                    col_idx += 1;
                }

                let cols = col_idx;
                if row_idx > 0 {
                    // Generate triangles for this row-pair.
                    for c in 0..cols.saturating_sub(1) {
                        let tl = (row_idx - 1) * cols + c;
                        let tr = tl + 1;
                        let bl = row_idx * cols + c;
                        let br = bl + 1;
                        indices.push(tl);
                        indices.push(bl);
                        indices.push(tr);
                        indices.push(tr);
                        indices.push(bl);
                        indices.push(br);
                    }
                }

                gz += step;
                row_idx += 1;
            }

            lods.push(TerrainLodLevel {
                step,
                distance_sq: dist_sq,
                vertices,
                indices,
            });
        }

        Self { lods }
    }

    /// Select the appropriate LOD level based on squared camera distance.
    pub fn select_lod(&self, camera_distance_sq: f32) -> usize {
        for (i, lod) in self.lods.iter().enumerate() {
            if camera_distance_sq < lod.distance_sq {
                return i;
            }
        }
        self.lods.len().saturating_sub(1)
    }
}

/// Splatmap descriptor — 4-channel RGBA weight texture for terrain layers.
#[derive(Debug, Clone)]
pub struct TerrainSplatmap {
    /// Width and height of the splatmap texture.
    pub resolution: u32,
    /// RGBA pixel data (row-major), each channel is the weight for one layer.
    pub data: Vec<[u8; 4]>,
}

impl TerrainSplatmap {
    /// Create a uniform splatmap (all weight on layer 0).
    pub fn uniform(resolution: u32) -> Self {
        Self {
            resolution,
            data: vec![[255, 0, 0, 0]; (resolution * resolution) as usize],
        }
    }
}

/// Component that ties a terrain to its physics HeightField.
///
/// This is intentionally separate from the render terrain so that crates
/// without a physics dependency can still use the terrain renderer.
#[derive(Debug, Clone)]
pub struct HeightFieldColliderDesc {
    /// Number of rows (Z segments + 1) in the heightfield.
    pub nrows: usize,
    /// Number of columns (X segments + 1) in the heightfield.
    pub ncols: usize,
    /// Row-major heights.
    pub heights: Vec<f32>,
    /// World-space scale (x, y, z).
    pub scale: [f32; 3],
}

impl HeightFieldColliderDesc {
    /// Build from a `TerrainConfig`.
    pub fn from_config(config: &TerrainConfig) -> Self {
        let n = config.resolution as usize;
        let mut heights = Vec::with_capacity(n * n);
        for z in 0..n {
            for x in 0..n {
                let u = x as f32 / (n - 1) as f32;
                let v = z as f32 / (n - 1) as f32;
                heights.push(config.sample_height(u, v));
            }
        }
        Self {
            nrows: n,
            ncols: n,
            heights,
            scale: [config.width, 1.0, config.depth],
        }
    }
}

/// WGSL snippet for splatmap sampling.
pub const TERRAIN_SPLATMAP_WGSL: &str = r#"
// Blend four terrain texture layers using splatmap weights.
// `weights` is the RGBA value from the splatmap texture.
fn terrain_blend(
    layer0: vec4<f32>,
    layer1: vec4<f32>,
    layer2: vec4<f32>,
    layer3: vec4<f32>,
    weights: vec4<f32>,
) -> vec4<f32> {
    let w = weights / max(dot(weights, vec4<f32>(1.0)), 0.0001);
    return layer0 * w.r + layer1 * w.g + layer2 * w.b + layer3 * w.a;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_config(res: u32) -> TerrainConfig {
        TerrainConfig {
            width: 100.0,
            depth: 100.0,
            max_height: 10.0,
            resolution: res,
            heightmap: vec![0.5; (res * res) as usize],
        }
    }

    #[test]
    fn sample_height_interpolation() {
        let cfg = flat_config(4);
        let h = cfg.sample_height(0.5, 0.5);
        assert!((h - 5.0).abs() < 0.01, "h = {}", h);
    }

    #[test]
    fn generate_lods() {
        let cfg = flat_config(9);
        let mesh = TerrainMesh::generate(&cfg, &[100.0, 200.0, 400.0]);
        assert_eq!(mesh.lods.len(), 3);
        // LOD 0 should have the most vertices.
        assert!(mesh.lods[0].vertices.len() > mesh.lods[1].vertices.len());
    }

    #[test]
    fn heightfield_from_config() {
        let cfg = flat_config(5);
        let hf = HeightFieldColliderDesc::from_config(&cfg);
        assert_eq!(hf.heights.len(), 25);
    }

    #[test]
    fn lod_selection() {
        let cfg = flat_config(9);
        let mesh = TerrainMesh::generate(&cfg, &[100.0, 200.0, 400.0]);
        assert_eq!(mesh.select_lod(50.0), 0);
        assert_eq!(mesh.select_lod(150.0), 1);
        assert_eq!(mesh.select_lod(1000.0), 2);
    }
}
