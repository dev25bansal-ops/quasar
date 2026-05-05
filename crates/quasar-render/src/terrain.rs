//! Terrain System — heightmap-based terrain with LOD and splatmap texturing.
//!
//! Provides:
//! - `TerrainConfig` — describes the terrain (size, resolution, heightmap data).
//! - `TerrainMesh` — generates and holds the GPU vertex/index buffers.
//! - `TerrainLod` — camera-distance based LOD with multiple detail levels.
//! - `TerrainSplatmap` — up to 4-layer texture blending via a weightmap.
//! - `HeightFieldCollider` — generates rapier3d `HeightField` collider data.
//! - `TerrainData` — full serializable terrain data for the editor.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

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

        for (lod_idx, &dist) in lod_distances.iter().take(num_lods).enumerate() {
            let step = 1u32 << lod_idx; // 1, 2, 4, 8 …
            let dist_sq = dist * dist;

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

// ---------------------------------------------------------------------------
// Editor-Ready Terrain Data Structures (Serializable)
// ---------------------------------------------------------------------------

/// 2D vector for serialization (matches glam::Vec2 but serializable).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<glam::Vec2> for Vec2 {
    fn from(v: glam::Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl From<Vec2> for glam::Vec2 {
    fn from(v: Vec2) -> Self {
        Self::new(v.x, v.y)
    }
}

/// Terrain world-space bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainBounds {
    /// World-space origin (x, y, z).
    pub origin: [f32; 3],
    /// World-space size (width, height, depth).
    pub size: [f32; 3],
}

impl TerrainBounds {
    pub fn from_dimensions(width: f32, max_height: f32, depth: f32) -> Self {
        Self {
            origin: [-width * 0.5, 0.0, -depth * 0.5],
            size: [width, max_height, depth],
        }
    }
}

/// A foliage instance placed on the terrain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainFoliageInstance {
    /// World X position.
    pub x: f32,
    /// World Y position (height).
    pub y: f32,
    /// World Z position.
    pub z: f32,
    /// Rotation in degrees around Y axis.
    pub rotation_deg: f32,
    /// Uniform scale factor.
    pub scale: f32,
    /// Which foliage type this instance uses.
    pub foliage_type: u32,
}

/// Blend mode for terrain material layers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TerrainBlendMode {
    /// Linear interpolation based on splatmap weights.
    #[default]
    Linear,
    /// Height-based blending using texture height maps.
    HeightBased,
    /// Triplanar projection for seamless tiling.
    Triplanar,
    /// Normal-based blending for micro-detail variation.
    NormalBased,
}

/// A terrain material layer descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainMaterial {
    /// Human-readable name (e.g., "Grass", "Rock", "Sand").
    pub name: String,
    /// Path to the albedo/diffuse texture.
    pub texture_albedo: String,
    /// Path to the normal map texture.
    pub texture_normal: String,
    /// Path to the roughness texture.
    pub texture_roughness: String,
    /// Path to the height/displacement texture.
    pub texture_height: String,
    /// Texture tiling scale (u, v).
    pub tiling: Vec2,
    /// How this layer blends with others.
    pub blend_mode: TerrainBlendMode,
}

impl TerrainMaterial {
    /// Create a new material with default empty paths.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            texture_albedo: String::new(),
            texture_normal: String::new(),
            texture_roughness: String::new(),
            texture_height: String::new(),
            tiling: Vec2::new(1.0, 1.0),
            blend_mode: TerrainBlendMode::default(),
        }
    }

    /// Create a grass material preset.
    pub fn grass() -> Self {
        Self {
            name: "Grass".to_string(),
            texture_albedo: "textures/terrain/grass_albedo.png".to_string(),
            texture_normal: "textures/terrain/grass_normal.png".to_string(),
            texture_roughness: "textures/terrain/grass_roughness.png".to_string(),
            texture_height: "textures/terrain/grass_height.png".to_string(),
            tiling: Vec2::new(10.0, 10.0),
            blend_mode: TerrainBlendMode::HeightBased,
        }
    }

    /// Create a rock material preset.
    pub fn rock() -> Self {
        Self {
            name: "Rock".to_string(),
            texture_albedo: "textures/terrain/rock_albedo.png".to_string(),
            texture_normal: "textures/terrain/rock_normal.png".to_string(),
            texture_roughness: "textures/terrain/rock_roughness.png".to_string(),
            texture_height: "textures/terrain/rock_height.png".to_string(),
            tiling: Vec2::new(5.0, 5.0),
            blend_mode: TerrainBlendMode::HeightBased,
        }
    }

    /// Create a sand material preset.
    pub fn sand() -> Self {
        Self {
            name: "Sand".to_string(),
            texture_albedo: "textures/terrain/sand_albedo.png".to_string(),
            texture_normal: "textures/terrain/sand_normal.png".to_string(),
            texture_roughness: "textures/terrain/sand_roughness.png".to_string(),
            texture_height: "textures/terrain/sand_height.png".to_string(),
            tiling: Vec2::new(8.0, 8.0),
            blend_mode: TerrainBlendMode::Linear,
        }
    }
}

/// Complete terrain data structure for the editor.
/// Serializable to/from JSON for save/load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainData {
    /// Human-readable name.
    pub name: String,
    /// Heightmap resolution (e.g., 1024x1024).
    pub resolution: u32,
    /// World-space width.
    pub width: f32,
    /// World-space depth.
    pub depth: f32,
    /// Maximum world-space height.
    pub max_height: f32,
    /// Row-major height values, normalized to [0, 1].
    pub heightmap: Vec<f32>,
    /// Texture weights per vertex (RGBA, up to 4 layers).
    pub splatmap: Vec<[f32; 4]>,
    /// Placed foliage instances.
    pub foliage: Vec<TerrainFoliageInstance>,
    /// Material layer definitions.
    pub materials: Vec<TerrainMaterial>,
    /// World-space bounds.
    pub bounds: TerrainBounds,
    /// Metadata: creation timestamp.
    pub created_at: String,
    /// Metadata: last modification timestamp.
    pub modified_at: String,
}

impl TerrainData {
    /// Create a new flat terrain with default materials.
    pub fn new(name: &str, resolution: u32, width: f32, depth: f32, max_height: f32) -> Self {
        let heightmap = vec![0.0f32; (resolution * resolution) as usize];
        let splatmap = vec![[1.0, 0.0, 0.0, 0.0]; (resolution * resolution) as usize];
        let materials = vec![
            TerrainMaterial::grass(),
            TerrainMaterial::rock(),
            TerrainMaterial::sand(),
        ];
        let bounds = TerrainBounds::from_dimensions(width, max_height, depth);
        let now = chrono_timestamp();

        Self {
            name: name.to_string(),
            resolution,
            width,
            depth,
            max_height,
            heightmap,
            splatmap,
            foliage: Vec::new(),
            materials,
            bounds,
            created_at: now.clone(),
            modified_at: now,
        }
    }

    /// Create a terrain from an existing TerrainConfig.
    pub fn from_config(config: &TerrainConfig, name: &str) -> Self {
        let mut data = Self::new(
            name,
            config.resolution,
            config.width,
            config.depth,
            config.max_height,
        );
        data.heightmap = config.heightmap.clone();
        data
    }

    /// Convert to a TerrainConfig for mesh generation.
    pub fn to_config(&self) -> TerrainConfig {
        TerrainConfig {
            width: self.width,
            depth: self.depth,
            max_height: self.max_height,
            resolution: self.resolution,
            heightmap: self.heightmap.clone(),
        }
    }

    /// Sample height at normalized (u, v) in [0, 1].
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

    /// Get world-space position for grid coordinates.
    pub fn world_pos(&self, gx: u32, gz: u32) -> Vec3 {
        let u = gx as f32 / (self.resolution - 1) as f32;
        let v = gz as f32 / (self.resolution - 1) as f32;
        Vec3::new(
            u * self.width - self.width * 0.5,
            self.sample_height(u, v),
            v * self.depth - self.depth * 0.5,
        )
    }

    /// Normalize all splatmap weights.
    pub fn normalize_splatmap(&mut self) {
        for w in self.splatmap.iter_mut() {
            let sum: f32 = w.iter().sum();
            if sum > 0.0001 {
                for c in w.iter_mut() {
                    *c /= sum;
                }
            } else {
                w[0] = 1.0;
            }
        }
    }

    /// Save terrain data to a JSON file.
    pub fn save_json(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize terrain data: {}", e))?;
        fs::write(path, json).map_err(|e| format!("Failed to write terrain file: {}", e))?;
        log::info!("Saved terrain data to {:?}", path);
        Ok(())
    }

    /// Load terrain data from a JSON file.
    pub fn load_json(path: &Path) -> Result<Self, String> {
        let json =
            fs::read_to_string(path).map_err(|e| format!("Failed to read terrain file: {}", e))?;
        let data: TerrainData = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse terrain data: {}", e))?;
        log::info!("Loaded terrain data from {:?}", path);
        Ok(data)
    }

    /// Save heightmap as a raw binary file (f32 values, row-major).
    pub fn save_heightmap_raw(&self, path: &Path) -> Result<(), String> {
        let bytes: Vec<u8> = self
            .heightmap
            .iter()
            .flat_map(|&h| h.to_le_bytes())
            .collect();
        fs::write(path, bytes).map_err(|e| format!("Failed to write heightmap: {}", e))?;
        log::info!("Saved raw heightmap to {:?}", path);
        Ok(())
    }

    /// Load heightmap from a raw binary file (f32 values, row-major).
    pub fn load_heightmap_raw(&mut self, path: &Path, resolution: u32) -> Result<(), String> {
        let bytes = fs::read(path).map_err(|e| format!("Failed to read heightmap file: {}", e))?;
        let expected = (resolution * resolution * 4) as usize;
        if bytes.len() != expected {
            return Err(format!(
                "Heightmap file size mismatch: expected {} bytes, got {}",
                expected,
                bytes.len()
            ));
        }

        let mut heightmap = Vec::with_capacity((resolution * resolution) as usize);
        for chunk in bytes.chunks_exact(4) {
            let h = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            heightmap.push(h);
        }

        self.heightmap = heightmap;
        self.resolution = resolution;
        self.modified_at = chrono_timestamp();
        log::info!("Loaded raw heightmap from {:?}", path);
        Ok(())
    }

    /// Update the modification timestamp.
    pub fn touch(&mut self) {
        self.modified_at = chrono_timestamp();
    }
}

/// Simple timestamp without external chrono dependency.
fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Format as ISO-8601-ish: YYYY-MM-DDTHH:MM:SSZ
    // This is a simplified version; proper ISO would need a calendar library
    format!("{}Z", secs)
}

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
        // select_lod takes squared distances; thresholds are 100² = 10_000,
        // 200² = 40_000, 400² = 160_000.
        assert_eq!(mesh.select_lod(50.0 * 50.0), 0);
        assert_eq!(mesh.select_lod(150.0 * 150.0), 1);
        assert_eq!(mesh.select_lod(1000.0 * 1000.0), 2);
    }

    #[test]
    fn terrain_data_creation() {
        let data = TerrainData::new("Test", 64, 100.0, 100.0, 50.0);
        assert_eq!(data.heightmap.len(), 64 * 64);
        assert_eq!(data.splatmap.len(), 64 * 64);
        assert_eq!(data.materials.len(), 3);
        assert!(!data.name.is_empty());
    }

    #[test]
    fn terrain_data_sample() {
        let data = TerrainData::new("Test", 4, 100.0, 100.0, 10.0);
        let h = data.sample_height(0.5, 0.5);
        assert!((h - 0.0).abs() < 0.01, "h = {}", h);
    }

    #[test]
    fn terrain_data_to_config() {
        let data = TerrainData::new("Test", 32, 200.0, 200.0, 100.0);
        let config = data.to_config();
        assert_eq!(config.resolution, 32);
        assert_eq!(config.width, 200.0);
        assert_eq!(config.depth, 200.0);
        assert_eq!(config.max_height, 100.0);
    }

    #[test]
    fn terrain_data_splatmap_normalize() {
        let mut data = TerrainData::new("Test", 4, 100.0, 100.0, 50.0);
        data.splatmap[0] = [2.0, 3.0, 0.0, 0.0];
        data.normalize_splatmap();
        assert!((data.splatmap[0][0] - 0.4).abs() < 0.001);
        assert!((data.splatmap[0][1] - 0.6).abs() < 0.001);
    }

    #[test]
    fn terrain_material_presets() {
        let grass = TerrainMaterial::grass();
        assert_eq!(grass.name, "Grass");
        let rock = TerrainMaterial::rock();
        assert_eq!(rock.name, "Rock");
        let sand = TerrainMaterial::sand();
        assert_eq!(sand.name, "Sand");
    }

    #[test]
    fn terrain_data_json_roundtrip() {
        let mut data = TerrainData::new("Roundtrip", 8, 50.0, 50.0, 25.0);
        data.heightmap[0] = 0.5;
        data.splatmap[0] = [0.5, 0.5, 0.0, 0.0];

        let path = std::path::Path::new("/tmp/test_terrain.json");
        let save_result = data.save_json(path);
        // May fail on some systems due to /tmp not existing on Windows
        if save_result.is_ok() {
            let loaded = TerrainData::load_json(path).unwrap();
            assert_eq!(loaded.name, "Roundtrip");
            assert_eq!(loaded.heightmap[0], 0.5);
            assert!((loaded.splatmap[0][0] - 0.5).abs() < 0.001);
            let _ = std::fs::remove_file(path);
        }
    }
}
