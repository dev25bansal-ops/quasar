//! Signed Distance Field (SDF) rendering system.
//!
//! Provides high-quality, resolution-independent rendering using SDFs:
//! - **SDF Text** - Crisp text at any scale with outlines and glow
//! - **SDF Shapes** - Vector graphics that scale perfectly
//! - **SDF Effects** - Soft shadows, outlines, glows
//!
//! Based on techniques from:
//! - Valve's "Improved Alpha-Tested Magnification" (2007)
//! - NVIDIA's SDF rendering best practices
//! - Lazybrush antialiasing technique

mod font;
mod shape;
mod effects;

pub use font::*;
pub use shape::*;
pub use effects::*;

use bytemuck::{Pod, Zeroable};

/// SDF distance threshold for edge detection.
pub const SDF_EDGE_THRESHOLD: f32 = 0.5;

/// SDF sampling scale for derivatives.
pub const SDF_DERIVATIVE_SCALE: f32 = 1.0 / 255.0;

/// SDF rendering settings.
#[derive(Debug, Clone, Copy)]
pub struct SdfSettings {
    /// Edge sharpness (higher = sharper, lower = softer).
    pub sharpness: f32,
    /// Enable anti-aliasing.
    pub anti_alias: bool,
    /// AA filter radius in pixels.
    pub aa_radius: f32,
    /// Enable smooth derivatives.
    pub smooth_derivatives: bool,
}

impl Default for SdfSettings {
    fn default() -> Self {
        Self {
            sharpness: 1.0,
            anti_alias: true,
            aa_radius: 1.5,
            smooth_derivatives: true,
        }
    }
}

/// Sample an SDF texture and return the signed distance.
/// Positive = inside, negative = outside, zero = on edge.
#[inline]
pub fn sample_sdf(sdf_value: f32) -> f32 {
    (sdf_value - SDF_EDGE_THRESHOLD) * 2.0
}

/// Convert SDF distance to alpha using smooth step.
#[inline]
pub fn sdf_to_alpha(distance: f32, sharpness: f32) -> f32 {
    let d = distance * sharpness;
    1.0 / (1.0 + (-d).exp())
}

/// SDF anti-aliased edge using derivatives (Lazybrush technique).
#[inline]
pub fn sdf_aa(distance: f32, dx: f32, dy: f32) -> f32 {
    let width = (dx * dx + dy * dy).sqrt();
    let d = distance / width.max(1e-6);
    (d + 0.5).clamp(0.0, 1.0)
}

/// 8x8 super-sampled SDF sampling for high quality.
pub fn sdf_sample_8x8(sdf_texture: &[f32], width: u32, height: u32, u: f32, v: f32) -> f32 {
    let x = u * (width - 1) as f32;
    let y = v * (height - 1) as f32;

    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(width as usize - 1);
    let y1 = (y0 + 1).min(height as usize - 1);

    let fx = x - x0 as f32;
    let fy = y - y0 as f32;

    let v00 = sdf_texture[y0 * width as usize + x0];
    let v10 = sdf_texture[y0 * width as usize + x1];
    let v01 = sdf_texture[y1 * width as usize + x0];
    let v11 = sdf_texture[y1 * width as usize + x1];

    let v0 = v00 * (1.0 - fx) + v10 * fx;
    let v1 = v01 * (1.0 - fx) + v11 * fx;

    v0 * (1.0 - fy) + v1 * fy
}

/// SDF uniform buffer for shaders.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SdfUniform {
    /// Transform matrix.
    pub transform: [[f32; 4]; 4],
    /// Color (RGBA).
    pub color: [f32; 4],
    /// Edge parameters (sharpness, outline_width, outline_offset, _pad).
    pub edge_params: [f32; 4],
    /// Glow parameters (glow_color_rgb, glow_intensity).
    pub glow_color: [f32; 4],
    /// Shadow parameters (offset_xy, blur, intensity).
    pub shadow_params: [f32; 4],
    /// Resolution (xy), padding.
    pub resolution: [f32; 4],
}

/// SDF outline configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfOutline {
    /// Outline width in pixels.
    pub width: f32,
    /// Outline offset from edge (0 = on edge, negative = inside, positive = outside).
    pub offset: f32,
    /// Outline color.
    pub color: [f32; 4],
}

impl Default for SdfOutline {
    fn default() -> Self {
        Self {
            width: 0.0,
            offset: 0.0,
            color: [0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// SDF glow configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfGlow {
    /// Glow color.
    pub color: [f32; 4],
    /// Glow intensity.
    pub intensity: f32,
    /// Glow spread in pixels.
    pub spread: f32,
}

impl Default for SdfGlow {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0],
            intensity: 0.0,
            spread: 4.0,
        }
    }
}

/// SDF shadow configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfShadow {
    /// Shadow offset in pixels.
    pub offset: [f32; 2],
    /// Shadow blur radius.
    pub blur: f32,
    /// Shadow intensity.
    pub intensity: f32,
    /// Shadow color.
    pub color: [f32; 4],
}

impl Default for SdfShadow {
    fn default() -> Self {
        Self {
            offset: [2.0, 2.0],
            blur: 4.0,
            intensity: 0.5,
            color: [0.0, 0.0, 0.0, 0.5],
        }
    }
}
