//! Percentage-Closer Soft Shadows (PCSS) configuration and utilities.
//!
//! PCSS provides contact-hardening soft shadows by:
//! 1. Searching for blockers in the shadow map
//! 2. Estimating penumbra width based on blocker distance
//! 3. Applying variable-size PCF filtering
//!
//! This module provides:
//! - Configuration for PCSS quality levels
//! - PCSS uniform data structures
//! - Utility functions for shadow filtering

use serde::{Deserialize, Serialize};

/// PCSS quality preset determining sample counts and filter settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PcssQuality {
    /// Low quality: 8 samples, fast but noisy
    Low,
    /// Medium quality: 16 samples, balanced (default)
    #[default]
    Medium,
    /// High quality: 32 samples, smooth shadows
    High,
    /// Ultra quality: 64 samples, best quality
    Ultra,
}

impl PcssQuality {
    /// Returns the number of samples for blocker search.
    pub fn blocker_search_samples(&self) -> u32 {
        match self {
            PcssQuality::Low => 8,
            PcssQuality::Medium => 16,
            PcssQuality::High => 32,
            PcssQuality::Ultra => 64,
        }
    }

    /// Returns the number of samples for PCF filtering.
    pub fn pcf_samples(&self) -> u32 {
        self.blocker_search_samples()
    }

    /// Returns the search radius multiplier.
    pub fn search_radius_multiplier(&self) -> f32 {
        match self {
            PcssQuality::Low => 4.0,
            PcssQuality::Medium => 8.0,
            PcssQuality::High => 12.0,
            PcssQuality::Ultra => 16.0,
        }
    }
}

/// PCSS configuration parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcssConfig {
    /// Light source size in world units (larger = softer shadows).
    pub light_size: f32,
    /// Quality preset for sample counts.
    pub quality: PcssQuality,
    /// Minimum filter radius in texels (prevents aliasing).
    pub min_filter_radius_texels: f32,
    /// Maximum filter radius in texels (limits performance cost).
    pub max_filter_radius_texels: f32,
    /// Enable contact hardening (sharper shadows near contact).
    pub contact_hardening: bool,
    /// Bias to prevent shadow acne.
    pub depth_bias: f32,
    /// Normal bias to reduce acne on sloped surfaces.
    pub normal_bias: f32,
}

impl Default for PcssConfig {
    fn default() -> Self {
        Self {
            light_size: 0.1,
            quality: PcssQuality::default(),
            min_filter_radius_texels: 1.0,
            max_filter_radius_texels: 32.0,
            contact_hardening: true,
            depth_bias: 0.001,
            normal_bias: 0.01,
        }
    }
}

impl PcssConfig {
    /// Creates a new PCSS configuration with the specified light size.
    pub fn new(light_size: f32) -> Self {
        Self {
            light_size,
            ..Default::default()
        }
    }

    /// Sets the quality preset.
    pub fn with_quality(mut self, quality: PcssQuality) -> Self {
        self.quality = quality;
        self
    }

    /// Enables or disables contact hardening.
    pub fn with_contact_hardening(mut self, enabled: bool) -> Self {
        self.contact_hardening = enabled;
        self
    }

    /// Sets the minimum filter radius.
    pub fn with_min_filter_radius(mut self, radius: f32) -> Self {
        self.min_filter_radius_texels = radius;
        self
    }

    /// Sets the maximum filter radius.
    pub fn with_max_filter_radius(mut self, radius: f32) -> Self {
        self.max_filter_radius_texels = radius;
        self
    }
}

/// GPU-side PCSS uniform matching the WGSL `PcssUniform` struct.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PcssUniform {
    /// Light size in world units.
    pub light_size: f32,
    /// Shadow map size in texels.
    pub shadow_map_size: f32,
    /// Minimum filter radius in texels.
    pub min_filter_radius: f32,
    /// Maximum filter radius in texels.
    pub max_filter_radius: f32,
    /// Sample count for blocker search.
    pub blocker_samples: u32,
    /// Sample count for PCF.
    pub pcf_samples: u32,
    /// Enable contact hardening (0.0 or 1.0).
    pub contact_hardening: f32,
    /// Padding for alignment.
    pub _pad: f32,
}

impl PcssUniform {
    /// Creates a new PCSS uniform from configuration.
    pub fn new(config: &PcssConfig, shadow_map_size: u32) -> Self {
        Self {
            light_size: config.light_size,
            shadow_map_size: shadow_map_size as f32,
            min_filter_radius: config.min_filter_radius_texels,
            max_filter_radius: config.max_filter_radius_texels,
            blocker_samples: config.quality.blocker_search_samples(),
            pcf_samples: config.quality.pcf_samples(),
            contact_hardening: if config.contact_hardening { 1.0 } else { 0.0 },
            _pad: 0.0,
        }
    }
}

/// 16-sample Poisson disk for shadow filtering.
pub const POISSON_DISK_16: [[f32; 2]; 16] = [
    [-0.94201624, -0.39906216],
    [0.945_586_1, -0.76890725],
    [-0.094_184_1, -0.929_388_7],
    [0.34495938, 0.293_877_6],
    [-0.915_885_8, 0.45771432],
    [-0.815_442_3, -0.87912464],
    [-0.38277543, 0.27676845],
    [0.974_844, 0.756_483_8],
    [0.44323325, -0.97511554],
    [0.537_429_8, -0.473_734_2],
    [-0.264_969_1, -0.41893023],
    [0.79197514, 0.19090188],
    [-0.241_888_4, 0.99706507],
    [-0.81409955, 0.914_375_9],
    [0.19984126, 0.78641367],
    [0.14383161, -0.141_007_9],
];

/// 32-sample Poisson disk for higher quality filtering.
pub const POISSON_DISK_32: [[f32; 2]; 32] = [
    [-0.94201624, -0.39906216],
    [0.945_586_1, -0.76890725],
    [-0.094_184_1, -0.929_388_7],
    [0.34495938, 0.293_877_6],
    [-0.915_885_8, 0.45771432],
    [-0.815_442_3, -0.87912464],
    [-0.38277543, 0.27676845],
    [0.974_844, 0.756_483_8],
    [0.44323325, -0.97511554],
    [0.537_429_8, -0.473_734_2],
    [-0.264_969_1, -0.41893023],
    [0.79197514, 0.19090188],
    [-0.241_888_4, 0.99706507],
    [-0.81409955, 0.914_375_9],
    [0.19984126, 0.78641367],
    [0.14383161, -0.141_007_9],
    [0.52160436, 0.163_129_1],
    [-0.705_839_6, 0.545_815_8],
    [0.122, -0.603],
    [-0.467, -0.225],
    [0.637, -0.267],
    [-0.306, 0.742],
    [0.844, 0.457],
    [-0.588, -0.625],
    [0.094, 0.337],
    [-0.159, -0.958],
    [0.334, 0.618],
    [-0.942, 0.137],
    [0.582, -0.859],
    [-0.631, 0.473],
    [0.789, -0.295],
    [-0.285, -0.542],
];

/// Estimates the penumbra width given receiver and blocker depths.
///
/// Uses the formula: `w_penumbra = (d_receiver - d_blocker) * w_light / d_blocker`
#[inline]
pub fn estimate_penumbra(receiver_depth: f32, blocker_depth: f32, light_size: f32) -> f32 {
    if blocker_depth <= 0.0 {
        return 0.0;
    }
    let penumbra_ratio = (receiver_depth - blocker_depth) / blocker_depth;
    light_size * penumbra_ratio
}

/// Calculates the search radius for blocker search in texel space.
#[inline]
pub fn calculate_search_radius(light_size: f32, shadow_map_size: u32, multiplier: f32) -> f32 {
    let texel_size = 1.0 / shadow_map_size as f32;
    light_size * texel_size * multiplier
}

/// Interleaved gradient noise for temporal variance reduction.
#[inline]
pub fn interleaved_gradient_noise(pixel_coord: glam::Vec2) -> f32 {
    let magic = glam::vec3(0.06711056, 0.00583715, 52.982_918);
    (magic.z * (pixel_coord.x * magic.x + pixel_coord.y * magic.y).fract()).fract()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcss_quality_samples() {
        assert_eq!(PcssQuality::Low.blocker_search_samples(), 8);
        assert_eq!(PcssQuality::Medium.blocker_search_samples(), 16);
        assert_eq!(PcssQuality::High.blocker_search_samples(), 32);
        assert_eq!(PcssQuality::Ultra.blocker_search_samples(), 64);
    }

    #[test]
    fn pcss_config_default() {
        let config = PcssConfig::default();
        assert!(config.light_size > 0.0);
        assert!(config.contact_hardening);
    }

    #[test]
    fn pcss_config_builder() {
        let config = PcssConfig::new(0.5)
            .with_quality(PcssQuality::High)
            .with_contact_hardening(false);

        assert_eq!(config.light_size, 0.5);
        assert_eq!(config.quality, PcssQuality::High);
        assert!(!config.contact_hardening);
    }

    #[test]
    fn pcss_uniform_creation() {
        let config = PcssConfig::new(0.1);
        let uniform = PcssUniform::new(&config, 1024);

        assert_eq!(uniform.light_size, 0.1);
        assert_eq!(uniform.shadow_map_size, 1024.0);
        assert_eq!(uniform.blocker_samples, 16);
    }

    #[test]
    fn estimate_penumbra_calculation() {
        let penumbra = estimate_penumbra(10.0, 5.0, 1.0);
        assert!((penumbra - 1.0).abs() < 0.001);
    }

    #[test]
    fn poisson_disk_size() {
        assert_eq!(POISSON_DISK_16.len(), 16);
        assert_eq!(POISSON_DISK_32.len(), 32);
    }

    #[test]
    fn interleaved_gradient_noise_range() {
        for x in 0..10 {
            for y in 0..10 {
                let noise = interleaved_gradient_noise(glam::vec2(x as f32, y as f32));
                assert!(noise >= 0.0 && noise < 1.0);
            }
        }
    }

    #[test]
    fn search_radius_calculation() {
        let radius = calculate_search_radius(0.1, 1024, 8.0);
        let expected = 0.1 * (1.0 / 1024.0) * 8.0;
        assert!((radius - expected).abs() < 0.0001);
    }
}
