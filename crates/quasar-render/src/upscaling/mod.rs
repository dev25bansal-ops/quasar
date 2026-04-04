//! Upscaling systems for super-resolution rendering.
//!
//! Provides:
//! - **FSR 2** (FidelityFX Super Resolution) — AMD's temporal upscaler
//! - **FSR 3** with frame generation
//! - **NIS** (NVIDIA Image Scaling) — spatial upscaler
//! - **DLSS** placeholder — requires NVIDIA SDK integration
//!
//! All upscalers follow the same interface for easy switching.

mod fsr;
mod nis;

pub use fsr::*;
pub use nis::*;

use bytemuck::{Pod, Zeroable};

/// Upscaling quality modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum UpscaleQuality {
    /// Native resolution (no upscaling).
    Native,
    /// Quality mode (1.5x upscale).
    #[default]
    Quality,
    /// Balanced mode (1.7x upscale).
    Balanced,
    /// Performance mode (2x upscale).
    Performance,
    /// Ultra performance mode (3x upscale).
    UltraPerformance,
}

impl UpscaleQuality {
    /// Get the render resolution scale factor.
    pub fn scale_factor(&self) -> f32 {
        match self {
            Self::Native => 1.0,
            Self::Quality => 0.67,
            Self::Balanced => 0.59,
            Self::Performance => 0.5,
            Self::UltraPerformance => 0.33,
        }
    }

    /// Calculate render resolution from display resolution.
    pub fn render_resolution(&self, display_width: u32, display_height: u32) -> (u32, u32) {
        let scale = self.scale_factor();
        (
            (display_width as f32 * scale).max(1.0) as u32,
            (display_height as f32 * scale).max(1.0) as u32,
        )
    }
}


/// Available upscaling methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum UpscaleMethod {
    /// No upscaling (native resolution).
    None,
    /// Bilinear upscaling (simple, fast).
    Bilinear,
    /// FSR 2 temporal upscaling.
    #[default]
    Fsr2,
    /// FSR 3 with frame generation.
    Fsr3,
    /// NVIDIA Image Scaling (spatial).
    Nis,
    /// DLSS (requires NVIDIA SDK).
    Dlss,
}


/// Upscaling settings resource.
#[derive(Debug, Clone)]
pub struct UpscalingSettings {
    /// Selected upscaling method.
    pub method: UpscaleMethod,
    /// Quality level.
    pub quality: UpscaleQuality,
    /// Enable sharpening pass.
    pub sharpening: bool,
    /// Sharpening intensity (0.0 - 1.0).
    pub sharpness: f32,
    /// Enable frame generation (FSR 3 / DLSS 3).
    pub frame_generation: bool,
    /// Enable motion vector jittering.
    pub jitter_enabled: bool,
}

impl Default for UpscalingSettings {
    fn default() -> Self {
        Self {
            method: UpscaleMethod::Fsr2,
            quality: UpscaleQuality::Quality,
            sharpening: true,
            sharpness: 0.25,
            frame_generation: false,
            jitter_enabled: true,
        }
    }
}

/// Jitter pattern for temporal upscaling.
#[derive(Debug, Clone, Copy)]
pub struct JitterPattern {
    /// Current jitter offset (x, y) in pixels.
    pub offset: [f32; 2],
    /// Frame index for pattern.
    pub frame_index: u32,
    /// Pattern length (number of unique jitter positions).
    pub pattern_length: u32,
}

impl JitterPattern {
    /// Halton sequence for FSR-style jittering.
    pub fn halton(index: u32, base: u32) -> f32 {
        let mut index = index as f32;
        let mut result = 0.0;
        let mut f = 1.0 / base as f32;
        while index > 0.0 {
            result += f * (index % base as f32);
            index = (index / base as f32).floor();
            f /= base as f32;
        }
        result
    }

    /// Create jitter pattern for FSR 2.
    pub fn fsr2(frame_index: u32, render_width: u32, render_height: u32) -> Self {
        let pattern_length = 16;
        let idx = frame_index % pattern_length;

        let x = Self::halton(idx, 2) - 0.5;
        let y = Self::halton(idx, 3) - 0.5;

        let pixel_offset_x = x * 2.0 / render_width as f32;
        let pixel_offset_y = y * 2.0 / render_height as f32;

        Self {
            offset: [pixel_offset_x, pixel_offset_y],
            frame_index: idx,
            pattern_length,
        }
    }

    /// Create jitter pattern for DLSS.
    pub fn dlss(frame_index: u32, render_width: u32, render_height: u32) -> Self {
        let pattern_length = 8;
        let idx = frame_index % pattern_length;

        let x = Self::halton(idx, 2) - 0.5;
        let y = Self::halton(idx, 3) - 0.5;

        let pixel_offset_x = x * 2.0 / render_width as f32;
        let pixel_offset_y = y * 2.0 / render_height as f32;

        Self {
            offset: [pixel_offset_x, pixel_offset_y],
            frame_index: idx,
            pattern_length,
        }
    }
}

/// Uniform buffer for upscaling shaders.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct UpscaleUniform {
    /// Render resolution (xy), display resolution (zw).
    pub resolution: [f32; 4],
    /// Jitter offset (xy), previous jitter (zw).
    pub jitter: [f32; 4],
    /// Sharpness (x), frame_index (y), motion_scale (zw).
    pub params: [f32; 4],
    /// Inverse view-projection (current frame).
    pub inv_view_proj: [[f32; 4]; 4],
    /// Previous frame view-projection.
    pub prev_view_proj: [[f32; 4]; 4],
}

/// Upscaling pass interface.
pub trait UpscalePass: Send + Sync {
    /// Get the current render resolution.
    fn render_resolution(&self) -> (u32, u32);

    /// Get the display resolution.
    fn display_resolution(&self) -> (u32, u32);

    /// Get current jitter offset.
    fn jitter(&self) -> [f32; 2];

    /// Resize the upscaler.
    fn resize(&mut self, device: &wgpu::Device, display_width: u32, display_height: u32);

    /// Dispatch the upscaling pass.
    fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        motion_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
    );

    /// Update settings.
    fn set_settings(&mut self, settings: &UpscalingSettings);
}

/// Upscaler availability detection.
pub fn detect_upscaler_availability(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
) -> UpscalerAvailability {
    let features = adapter.features();
    let limits = device.limits();

    UpscalerAvailability {
        fsr2: true,
        fsr3: false,
        nis: true,
        dlss: false,
        vrs: features.contains(wgpu::Features::PIPELINE_STATISTICS_QUERY),
        mesh_shaders: limits.max_compute_workgroup_storage_size > 0,
    }
}

/// What upscalers are available on this hardware.
#[derive(Debug, Clone, Copy)]
pub struct UpscalerAvailability {
    /// FSR 2 available.
    pub fsr2: bool,
    /// FSR 3 available.
    pub fsr3: bool,
    /// NIS available.
    pub nis: bool,
    /// DLSS available (requires NVIDIA SDK).
    pub dlss: bool,
    /// Variable rate shading available.
    pub vrs: bool,
    /// Mesh shaders available.
    pub mesh_shaders: bool,
}
