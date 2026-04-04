//! SDF effects - outlines, glows, shadows, and soft effects.
//!
//! Provides high-quality post-process style effects for SDF shapes:
//! - Soft outlines with configurable width and offset
//! - Glows with color and spread
//! - Soft shadows with blur
//! - Inner shadows

use bytemuck::{Pod, Zeroable};

use super::sdf_to_alpha;

/// SDF effect configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfEffectConfig {
    /// Base color.
    pub fill_color: [f32; 4],
    /// Outline effect.
    pub outline: Option<SdfOutlineEffect>,
    /// Glow effect.
    pub glow: Option<SdfGlowEffect>,
    /// Shadow effect.
    pub shadow: Option<SdfShadowEffect>,
    /// Inner shadow effect.
    pub inner_shadow: Option<SdfInnerShadowEffect>,
}

impl Default for SdfEffectConfig {
    fn default() -> Self {
        Self {
            fill_color: [1.0, 1.0, 1.0, 1.0],
            outline: None,
            glow: None,
            shadow: None,
            inner_shadow: None,
        }
    }
}

/// Outline effect configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfOutlineEffect {
    /// Outline width in pixels (SDF units).
    pub width: f32,
    /// Offset from edge (0 = on edge, negative = inside, positive = outside).
    pub offset: f32,
    /// Outline color.
    pub color: [f32; 4],
    /// Softness (0 = sharp, 1 = soft).
    pub softness: f32,
}

impl Default for SdfOutlineEffect {
    fn default() -> Self {
        Self {
            width: 2.0,
            offset: 0.0,
            color: [0.0, 0.0, 0.0, 1.0],
            softness: 0.0,
        }
    }
}

/// Glow effect configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfGlowEffect {
    /// Glow color.
    pub color: [f32; 4],
    /// Glow intensity (0-1).
    pub intensity: f32,
    /// Glow spread in pixels.
    pub spread: f32,
    /// Use inner glow instead of outer.
    pub inner: bool,
}

impl Default for SdfGlowEffect {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0],
            intensity: 0.5,
            spread: 8.0,
            inner: false,
        }
    }
}

/// Shadow effect configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfShadowEffect {
    /// Shadow offset in pixels.
    pub offset: [f32; 2],
    /// Shadow blur radius.
    pub blur: f32,
    /// Shadow intensity (0-1).
    pub intensity: f32,
    /// Shadow color.
    pub color: [f32; 4],
}

impl Default for SdfShadowEffect {
    fn default() -> Self {
        Self {
            offset: [4.0, 4.0],
            blur: 8.0,
            intensity: 0.5,
            color: [0.0, 0.0, 0.0, 0.5],
        }
    }
}

/// Inner shadow effect configuration.
#[derive(Debug, Clone, Copy)]
pub struct SdfInnerShadowEffect {
    /// Shadow offset in pixels (inside the shape).
    pub offset: [f32; 2],
    /// Shadow blur radius.
    pub blur: f32,
    /// Shadow intensity (0-1).
    pub intensity: f32,
    /// Shadow color.
    pub color: [f32; 4],
}

impl Default for SdfInnerShadowEffect {
    fn default() -> Self {
        Self {
            offset: [2.0, 2.0],
            blur: 4.0,
            intensity: 0.3,
            color: [0.0, 0.0, 0.0, 0.5],
        }
    }
}

/// Apply SDF effects and return the final color.
pub fn apply_sdf_effects(distance: f32, config: &SdfEffectConfig) -> [f32; 4] {
    let mut result = [0.0f32; 4];

    // Fill
    let fill_alpha = sdf_to_alpha(distance, 1.0);
    result[0] = config.fill_color[0];
    result[1] = config.fill_color[1];
    result[2] = config.fill_color[2];
    result[3] = config.fill_color[3] * fill_alpha;

    // Outline
    if let Some(ref outline) = config.outline {
        let outline_distance = distance - outline.offset;
        let outline_alpha = if outline.softness > 0.0 {
            smooth_step(
                -outline.width - outline.softness,
                outline.softness,
                outline_distance,
            )
        } else {
            (1.0 - smooth_step(-outline.width, 0.0, outline_distance)).min(1.0)
        };

        result[0] = result[0] * (1.0 - outline_alpha) + outline.color[0] * outline_alpha;
        result[1] = result[1] * (1.0 - outline_alpha) + outline.color[1] * outline_alpha;
        result[2] = result[2] * (1.0 - outline_alpha) + outline.color[2] * outline_alpha;
        result[3] = result[3].max(outline.color[3] * outline_alpha);
    }

    // Glow
    if let Some(ref glow) = config.glow {
        let glow_distance = if glow.inner {
            -distance - glow.spread
        } else {
            distance - glow.spread
        };

        let glow_alpha = smooth_step(glow.spread, 0.0, glow_distance) * glow.intensity;

        result[0] = result[0] + glow.color[0] * glow_alpha * (1.0 - result[0]);
        result[1] = result[1] + glow.color[1] * glow_alpha * (1.0 - result[1]);
        result[2] = result[2] + glow.color[2] * glow_alpha * (1.0 - result[2]);
        result[3] = result[3].max(glow.color[3] * glow_alpha);
    }

    result
}

/// Apply shadow to a color.
pub fn apply_shadow(base_color: [f32; 4], shadow_color: [f32; 4], shadow_alpha: f32) -> [f32; 4] {
    let alpha = shadow_color[3] * shadow_alpha;
    [
        base_color[0] * (1.0 - alpha) + shadow_color[0] * alpha,
        base_color[1] * (1.0 - alpha) + shadow_color[1] * alpha,
        base_color[2] * (1.0 - alpha) + shadow_color[2] * alpha,
        base_color[3].max(alpha),
    ]
}

/// Smooth step function.
#[inline]
pub fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear step function.
#[inline]
pub fn linear_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0)
}

/// Exponential step for soft edges.
#[inline]
pub fn exp_step(edge: f32, decay: f32, x: f32) -> f32 {
    ((x - edge) * decay).exp()
}

/// Gaussian blur approximation for SDF shadows.
pub fn gaussian_shadow(distance: f32, blur: f32) -> f32 {
    if blur <= 0.0 {
        return if distance < 0.0 { 1.0 } else { 0.0 };
    }

    let sigma = blur / 3.0;
    let x = distance / sigma;
    (-x * x / 2.0).exp()
}

/// SDF effect uniforms for GPU.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SdfEffectUniform {
    /// Fill color (RGBA).
    pub fill_color: [f32; 4],
    /// Outline (width, offset, softness, _pad).
    pub outline_params: [f32; 4],
    /// Outline color (RGBA).
    pub outline_color: [f32; 4],
    /// Glow (intensity, spread, inner, _pad).
    pub glow_params: [f32; 4],
    /// Glow color (RGBA).
    pub glow_color: [f32; 4],
    /// Shadow (offset_xy, blur, intensity).
    pub shadow_params: [f32; 4],
    /// Shadow color (RGBA).
    pub shadow_color: [f32; 4],
    /// Inner shadow (offset_xy, blur, intensity).
    pub inner_shadow_params: [f32; 4],
    /// Inner shadow color (RGBA).
    pub inner_shadow_color: [f32; 4],
}

impl From<SdfEffectConfig> for SdfEffectUniform {
    fn from(config: SdfEffectConfig) -> Self {
        let outline = config.outline.unwrap_or_default();
        let glow = config.glow.unwrap_or_default();
        let shadow = config.shadow.unwrap_or_default();
        let inner_shadow = config.inner_shadow.unwrap_or_default();

        Self {
            fill_color: config.fill_color,
            outline_params: [outline.width, outline.offset, outline.softness, 0.0],
            outline_color: outline.color,
            glow_params: [
                glow.intensity,
                glow.spread,
                if glow.inner { 1.0 } else { 0.0 },
                0.0,
            ],
            glow_color: glow.color,
            shadow_params: [
                shadow.offset[0],
                shadow.offset[1],
                shadow.blur,
                shadow.intensity,
            ],
            shadow_color: shadow.color,
            inner_shadow_params: [
                inner_shadow.offset[0],
                inner_shadow.offset[1],
                inner_shadow.blur,
                inner_shadow.intensity,
            ],
            inner_shadow_color: inner_shadow.color,
        }
    }
}

/// SDF effect pass for post-processing.
pub struct SdfEffectPass {
    /// Compute pipeline for effect application.
    pub pipeline: Option<wgpu::ComputePipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Output texture.
    pub output_texture: Option<wgpu::Texture>,
    pub output_view: Option<wgpu::TextureView>,
}

impl SdfEffectPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sdf_effect_uniform"),
            size: std::mem::size_of::<SdfEffectUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sdf_effect_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        Self {
            pipeline: None,
            bind_group_layout,
            uniform_buffer,
            output_texture: None,
            output_view: None,
        }
    }

    /// Apply effects to an SDF texture.
    pub fn apply(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        config: &SdfEffectConfig,
        sdf_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        let Some(output_view) = &self.output_view else {
            return;
        };

        let uniform: SdfEffectUniform = (*config).into();
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sdf_effect_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(sdf_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
            ],
        });

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("sdf_effect_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);

        let groups_x = width.div_ceil(16);
        let groups_y = height.div_ceil(16);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }
}
