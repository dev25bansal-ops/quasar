//! Brush tool system for terrain editing.
//!
//! Provides:
//! - `BrushType` enum with raise, lower, smooth, flatten, paint, foliage variants
//! - `BrushSettings` with radius, strength, and falloff configuration
//! - Brush application to heightmaps, splatmaps, and foliage layers
//! - Undo/redo support via command pattern

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Brush Types
// ---------------------------------------------------------------------------

/// Types of terrain brush operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrushType {
    /// Raise terrain height.
    Raise { strength: f32 },
    /// Lower terrain height.
    Lower { strength: f32 },
    /// Smooth terrain by averaging neighbors.
    Smooth { strength: f32 },
    /// Flatten terrain to a target height.
    Flatten { height: f32 },
    /// Paint texture weights onto the splatmap.
    Paint { texture_index: u32, strength: f32 },
    /// Paint foliage instances.
    Foliage { foliage_type: u32, density: f32 },
    /// Erase foliage instances within radius.
    EraseFoliage { radius: f32 },
}

impl BrushType {
    /// Get the display name for this brush type.
    pub fn display_name(&self) -> &'static str {
        match self {
            BrushType::Raise { .. } => "Raise",
            BrushType::Lower { .. } => "Lower",
            BrushType::Smooth { .. } => "Smooth",
            BrushType::Flatten { .. } => "Flatten",
            BrushType::Paint { .. } => "Paint",
            BrushType::Foliage { .. } => "Foliage",
            BrushType::EraseFoliage { .. } => "Erase Foliage",
        }
    }
}

// ---------------------------------------------------------------------------
// Falloff Types
// ---------------------------------------------------------------------------

/// Falloff curve for brush influence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FalloffType {
    /// Linear falloff: 1 - t
    Linear,
    /// Smooth cubic falloff: 1 - 3t^2 + 2t^3
    Smooth,
    /// Sharp quadratic falloff: 1 - t^2
    Sharp,
    /// Gaussian falloff: exp(-t^2 / (2 * sigma^2))
    Gaussian,
}

impl FalloffType {
    /// Evaluate the falloff at a normalized distance t in [0, 1].
    pub fn evaluate(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            FalloffType::Linear => 1.0 - t,
            FalloffType::Smooth => {
                let t2 = t * t;
                1.0 - 3.0 * t2 + 2.0 * t2 * t
            }
            FalloffType::Sharp => 1.0 - t * t,
            FalloffType::Gaussian => {
                let sigma = 0.4f32;
                (-t * t / (2.0 * sigma * sigma)).exp()
            }
        }
    }
}

impl Default for FalloffType {
    fn default() -> Self {
        FalloffType::Smooth
    }
}

// ---------------------------------------------------------------------------
// Brush Settings
// ---------------------------------------------------------------------------

/// Configuration for the active brush.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrushSettings {
    pub brush_type: BrushType,
    pub radius: f32,
    pub strength: f32,
    pub falloff: FalloffType,
}

impl BrushSettings {
    /// Create new brush settings with defaults.
    pub fn new(brush_type: BrushType) -> Self {
        Self {
            brush_type,
            radius: 5.0,
            strength: 0.5,
            falloff: FalloffType::default(),
        }
    }

    /// Create a raise brush.
    pub fn raise(strength: f32, radius: f32) -> Self {
        Self {
            brush_type: BrushType::Raise { strength },
            radius,
            strength,
            falloff: FalloffType::default(),
        }
    }

    /// Create a lower brush.
    pub fn lower(strength: f32, radius: f32) -> Self {
        Self {
            brush_type: BrushType::Lower { strength },
            radius,
            strength,
            falloff: FalloffType::default(),
        }
    }

    /// Create a smooth brush.
    pub fn smooth(strength: f32, radius: f32) -> Self {
        Self {
            brush_type: BrushType::Smooth { strength },
            radius,
            strength,
            falloff: FalloffType::default(),
        }
    }

    /// Create a flatten brush.
    pub fn flatten(height: f32, radius: f32) -> Self {
        Self {
            brush_type: BrushType::Flatten { height },
            radius,
            strength: 0.5,
            falloff: FalloffType::default(),
        }
    }

    /// Create a paint brush.
    pub fn paint(texture_index: u32, strength: f32, radius: f32) -> Self {
        Self {
            brush_type: BrushType::Paint {
                texture_index,
                strength,
            },
            radius,
            strength,
            falloff: FalloffType::default(),
        }
    }

    /// Create a foliage brush.
    pub fn foliage(foliage_type: u32, density: f32, radius: f32) -> Self {
        Self {
            brush_type: BrushType::Foliage {
                foliage_type,
                density,
            },
            radius,
            strength: density,
            falloff: FalloffType::default(),
        }
    }

    /// Create an erase foliage brush.
    pub fn erase_foliage(radius: f32) -> Self {
        Self {
            brush_type: BrushType::EraseFoliage { radius },
            radius,
            strength: 1.0,
            falloff: FalloffType::default(),
        }
    }
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            brush_type: BrushType::Raise { strength: 0.5 },
            radius: 5.0,
            strength: 0.5,
            falloff: FalloffType::Smooth,
        }
    }
}

// ---------------------------------------------------------------------------
// Brush Application
// ---------------------------------------------------------------------------

/// Apply a brush stroke to a heightmap.
///
/// # Arguments
/// * `heightmap` - Mutable slice of height values (row-major)
/// * `resolution` - Width/height of the square heightmap
/// * `center_x` - Grid X coordinate of brush center
/// * `center_z` - Grid Z coordinate of brush center
/// * `settings` - Brush configuration
pub fn apply_brush_heightmap(
    heightmap: &mut [f32],
    resolution: u32,
    center_x: f32,
    center_z: f32,
    settings: &BrushSettings,
) {
    let radius_cells = settings.radius;
    let radius_sq = radius_cells * radius_cells;

    let min_x = (center_x - radius_cells).max(0.0).floor() as u32;
    let max_x = (center_x + radius_cells)
        .min(resolution as f32 - 1.0)
        .ceil() as u32;
    let min_z = (center_z - radius_cells).max(0.0).floor() as u32;
    let max_z = (center_z + radius_cells)
        .min(resolution as f32 - 1.0)
        .ceil() as u32;

    match &settings.brush_type {
        BrushType::Raise { strength } => {
            apply_heightmap_op(
                heightmap,
                resolution,
                min_x,
                max_x,
                min_z,
                max_z,
                center_x,
                center_z,
                radius_sq,
                settings.falloff,
                |h, influence| h + *strength * influence,
            );
        }
        BrushType::Lower { strength } => {
            apply_heightmap_op(
                heightmap,
                resolution,
                min_x,
                max_x,
                min_z,
                max_z,
                center_x,
                center_z,
                radius_sq,
                settings.falloff,
                |h, influence| h - *strength * influence,
            );
        }
        BrushType::Smooth { strength } => {
            apply_heightmap_smooth(
                heightmap,
                resolution,
                min_x,
                max_x,
                min_z,
                max_z,
                center_x,
                center_z,
                radius_sq,
                settings.falloff,
                *strength,
            );
        }
        BrushType::Flatten { height } => {
            apply_heightmap_op(
                heightmap,
                resolution,
                min_x,
                max_x,
                min_z,
                max_z,
                center_x,
                center_z,
                radius_sq,
                settings.falloff,
                |h, influence| h + (*height - h) * influence * settings.strength,
            );
        }
        _ => {}
    }
}

fn apply_heightmap_op<F>(
    heightmap: &mut [f32],
    resolution: u32,
    min_x: u32,
    max_x: u32,
    min_z: u32,
    max_z: u32,
    center_x: f32,
    center_z: f32,
    radius_sq: f32,
    falloff: FalloffType,
    mut op: F,
) where
    F: FnMut(f32, f32) -> f32,
{
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let dx = x as f32 - center_x;
            let dz = z as f32 - center_z;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > radius_sq {
                continue;
            }
            let dist = dist_sq.sqrt();
            let t = dist / radius_sq.sqrt();
            let influence = falloff.evaluate(t);

            let idx = (z * resolution + x) as usize;
            if idx < heightmap.len() {
                heightmap[idx] = op(heightmap[idx], influence);
            }
        }
    }
}

fn apply_heightmap_smooth(
    heightmap: &mut [f32],
    resolution: u32,
    min_x: u32,
    max_x: u32,
    min_z: u32,
    max_z: u32,
    center_x: f32,
    center_z: f32,
    radius_sq: f32,
    falloff: FalloffType,
    strength: f32,
) {
    // First pass: compute smoothed values
    let mut smoothed = vec![0.0f32; heightmap.len()];
    smoothed.copy_from_slice(heightmap);

    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let dx = x as f32 - center_x;
            let dz = z as f32 - center_z;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > radius_sq {
                continue;
            }

            let idx = (z * resolution + x) as usize;
            if idx >= heightmap.len() {
                continue;
            }

            // Average with 4-neighbors
            let mut sum = heightmap[idx];
            let mut count = 1.0f32;

            if x > 0 {
                sum += heightmap[(z * resolution + (x - 1)) as usize];
                count += 1.0;
            }
            if x < resolution - 1 {
                sum += heightmap[(z * resolution + (x + 1)) as usize];
                count += 1.0;
            }
            if z > 0 {
                sum += heightmap[((z - 1) * resolution + x) as usize];
                count += 1.0;
            }
            if z < resolution - 1 {
                sum += heightmap[((z + 1) * resolution + x) as usize];
                count += 1.0;
            }

            smoothed[idx] = sum / count;
        }
    }

    // Second pass: blend with falloff
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let dx = x as f32 - center_x;
            let dz = z as f32 - center_z;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > radius_sq {
                continue;
            }
            let dist = dist_sq.sqrt();
            let t = dist / radius_sq.sqrt();
            let influence = falloff.evaluate(t) * strength;

            let idx = (z * resolution + x) as usize;
            if idx < heightmap.len() {
                heightmap[idx] += (smoothed[idx] - heightmap[idx]) * influence;
            }
        }
    }
}

/// Apply a brush stroke to a splatmap.
///
/// # Arguments
/// * `splatmap` - Mutable slice of texture weight arrays (RGBA per vertex)
/// * `resolution` - Width/height of the square splatmap
/// * `center_x` - Grid X of brush center
/// * `center_z` - Grid Z of brush center
/// * `settings` - Brush configuration (must be Paint type)
pub fn apply_brush_splatmap(
    splatmap: &mut [[f32; 4]],
    resolution: u32,
    center_x: f32,
    center_z: f32,
    settings: &BrushSettings,
) {
    let BrushType::Paint {
        texture_index,
        strength,
    } = &settings.brush_type
    else {
        return;
    };

    let channel = (*texture_index).min(3) as usize;
    let radius_cells = settings.radius;
    let radius_sq = radius_cells * radius_cells;

    let min_x = (center_x - radius_cells).max(0.0).floor() as u32;
    let max_x = (center_x + radius_cells)
        .min(resolution as f32 - 1.0)
        .ceil() as u32;
    let min_z = (center_z - radius_cells).max(0.0).floor() as u32;
    let max_z = (center_z + radius_cells)
        .min(resolution as f32 - 1.0)
        .ceil() as u32;

    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let dx = x as f32 - center_x;
            let dz = z as f32 - center_z;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > radius_sq {
                continue;
            }
            let dist = dist_sq.sqrt();
            let t = dist / radius_sq.sqrt();
            let influence = settings.falloff.evaluate(t) * strength;

            let idx = (z * resolution + x) as usize;
            if idx >= splatmap.len() {
                continue;
            }

            // Add weight to the target channel, redistribute from others
            let current = splatmap[idx][channel];
            let delta = (1.0 - current) * influence;
            splatmap[idx][channel] += delta;

            // Redistribute remaining weight proportionally
            let remaining = 1.0 - splatmap[idx][channel];
            let mut other_sum = 0.0f32;
            for c in 0..4 {
                if c != channel {
                    other_sum += splatmap[idx][c];
                }
            }

            if other_sum > 0.0001 {
                for c in 0..4 {
                    if c != channel {
                        splatmap[idx][c] = splatmap[idx][c] / other_sum * remaining;
                    }
                }
            } else {
                // Evenly distribute remaining among other channels
                let share = remaining / 3.0;
                for c in 0..4 {
                    if c != channel {
                        splatmap[idx][c] = share;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Brush Preview
// ---------------------------------------------------------------------------

/// Generate a visual preview of the brush influence as a 2D grid.
///
/// Returns a flat vector of influence values [0, 1] for visualization.
pub fn generate_brush_preview(preview_resolution: u32, settings: &BrushSettings) -> Vec<f32> {
    let center = preview_resolution as f32 / 2.0;
    let radius_sq = settings.radius * settings.radius;
    let mut preview = vec![0.0f32; (preview_resolution * preview_resolution) as usize];

    for z in 0..preview_resolution {
        for x in 0..preview_resolution {
            let dx = x as f32 - center;
            let dz = z as f32 - center;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > radius_sq {
                continue;
            }
            let dist = dist_sq.sqrt();
            let t = dist / settings.radius;
            preview[(z * preview_resolution + x) as usize] = settings.falloff.evaluate(t);
        }
    }

    preview
}

// ---------------------------------------------------------------------------
// Undo History for Brush Strokes
// ---------------------------------------------------------------------------

/// A snapshot of terrain height data for undo.
#[derive(Debug, Clone)]
pub struct HeightmapSnapshot {
    pub resolution: u32,
    pub data: Vec<f32>,
}

/// A snapshot of splatmap data for undo.
#[derive(Debug, Clone)]
pub struct SplatmapSnapshot {
    pub resolution: u32,
    pub data: Vec<[f32; 4]>,
}

/// A single brush stroke that can be undone.
#[derive(Debug, Clone)]
pub struct BrushStroke {
    pub settings: BrushSettings,
    pub center_x: f32,
    pub center_z: f32,
    pub previous_heightmap: Option<HeightmapSnapshot>,
    pub previous_splatmap: Option<SplatmapSnapshot>,
}

impl BrushStroke {
    /// Create a new brush stroke with snapshots for undo.
    pub fn new(
        settings: BrushSettings,
        center_x: f32,
        center_z: f32,
        heightmap: Option<&[f32]>,
        splatmap: Option<&[[f32; 4]]>,
    ) -> Self {
        Self {
            settings,
            center_x,
            center_z,
            previous_heightmap: heightmap.map(|data| HeightmapSnapshot {
                resolution: data.len() as u32,
                data: data.to_vec(),
            }),
            previous_splatmap: splatmap.map(|data| SplatmapSnapshot {
                resolution: data.len() as u32,
                data: data.to_vec(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_linear() {
        assert!((FalloffType::Linear.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffType::Linear.evaluate(0.5) - 0.5).abs() < 0.001);
        assert!((FalloffType::Linear.evaluate(1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn falloff_smooth() {
        assert!((FalloffType::Smooth.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffType::Smooth.evaluate(1.0) - 0.0).abs() < 0.001);
        // At 0.5: 1 - 3*0.25 + 2*0.125 = 1 - 0.75 + 0.25 = 0.5
        assert!((FalloffType::Smooth.evaluate(0.5) - 0.5).abs() < 0.001);
    }

    #[test]
    fn falloff_sharp() {
        assert!((FalloffType::Sharp.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!((FalloffType::Sharp.evaluate(0.5) - 0.75).abs() < 0.001);
        assert!((FalloffType::Sharp.evaluate(1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn falloff_gaussian() {
        assert!((FalloffType::Gaussian.evaluate(0.0) - 1.0).abs() < 0.001);
        assert!(FalloffType::Gaussian.evaluate(1.0) < 0.1);
    }

    #[test]
    fn raise_brush() {
        let res = 32u32;
        let mut heightmap = vec![0.5f32; (res * res) as usize];
        let settings = BrushSettings::raise(0.1, 3.0);
        apply_brush_heightmap(&mut heightmap, res, 16.0, 16.0, &settings);

        // Center should be raised
        let center_idx = (16 * res + 16) as usize;
        assert!(heightmap[center_idx] > 0.5);
    }

    #[test]
    fn lower_brush() {
        let res = 32u32;
        let mut heightmap = vec![0.5f32; (res * res) as usize];
        let settings = BrushSettings::lower(0.1, 3.0);
        apply_brush_heightmap(&mut heightmap, res, 16.0, 16.0, &settings);

        let center_idx = (16 * res + 16) as usize;
        assert!(heightmap[center_idx] < 0.5);
    }

    #[test]
    fn smooth_brush() {
        let res = 32u32;
        let mut heightmap = vec![0.5f32; (res * res) as usize];
        // Create a spike
        heightmap[(16 * res + 16) as usize] = 1.0;
        let settings = BrushSettings::smooth(1.0, 3.0);
        apply_brush_heightmap(&mut heightmap, res, 16.0, 16.0, &settings);

        // Spike should be reduced
        let center_idx = (16 * res + 16) as usize;
        assert!(heightmap[center_idx] < 1.0);
    }

    #[test]
    fn flatten_brush() {
        let res = 32u32;
        let mut heightmap = vec![0.5f32; (res * res) as usize];
        let settings = BrushSettings::flatten(0.0, 5.0);
        apply_brush_heightmap(&mut heightmap, res, 16.0, 16.0, &settings);

        let center_idx = (16 * res + 16) as usize;
        assert!(heightmap[center_idx] < 0.5);
    }

    #[test]
    fn brush_preview_generated() {
        let settings = BrushSettings::raise(0.5, 5.0);
        let preview = generate_brush_preview(32, &settings);
        assert_eq!(preview.len(), 32 * 32);
        // Center should be max influence
        let center_idx = (16 * 32 + 16) as usize;
        assert!((preview[center_idx] - 1.0).abs() < 0.01);
    }

    #[test]
    fn brush_stroke_snapshot() {
        let heightmap = vec![0.5f32; 1024];
        let settings = BrushSettings::raise(0.5, 3.0);
        let stroke = BrushStroke::new(settings, 16.0, 16.0, Some(&heightmap), None);

        assert!(stroke.previous_heightmap.is_some());
        assert!(stroke.previous_splatmap.is_none());
        assert_eq!(stroke.previous_heightmap.as_ref().unwrap().data.len(), 1024);
    }
}
