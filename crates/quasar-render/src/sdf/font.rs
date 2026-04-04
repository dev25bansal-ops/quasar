//! SDF font rendering for crisp, scalable text.
//!
//! Generates and renders SDF-based font atlases for:
//! - Resolution-independent text
//! - Outlines and glows
//! - Soft shadows

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

use super::{SdfGlow, SdfOutline, SdfShadow, SdfUniform};

/// SDF font atlas containing pre-rendered glyphs.
pub struct SdfFontAtlas {
    /// GPU texture containing all glyphs.
    pub texture: Option<wgpu::Texture>,
    pub texture_view: Option<wgpu::TextureView>,
    /// Glyph metadata.
    pub glyphs: HashMap<char, SdfGlyph>,
    /// Atlas width.
    pub width: u32,
    /// Atlas height.
    pub height: u32,
    /// Font size used to generate the atlas.
    pub font_size: f32,
    /// SDF spread in pixels.
    pub sdf_spread: f32,
}

/// Single glyph in the SDF atlas.
#[derive(Debug, Clone, Copy)]
pub struct SdfGlyph {
    /// Unicode character.
    pub char: char,
    /// Position in atlas (u0, v0, u1, v1).
    pub atlas_rect: [f32; 4],
    /// Glyph size in pixels.
    pub size: [f32; 2],
    /// Bearing offset from origin.
    pub bearing: [f32; 2],
    /// Advance width.
    pub advance: f32,
}

impl SdfFontAtlas {
    /// Create an empty font atlas.
    pub fn new(width: u32, height: u32, font_size: f32, sdf_spread: f32) -> Self {
        Self {
            texture: None,
            texture_view: None,
            glyphs: HashMap::new(),
            width,
            height,
            font_size,
            sdf_spread,
        }
    }

    /// Create font atlas and GPU texture from fontdue font.
    pub fn from_font(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font: &fontdue::Font,
        font_size: f32,
        chars: &[char],
        sdf_spread: f32,
    ) -> Self {
        let mut glyphs = HashMap::new();
        let mut atlas_data = vec![0.0f32; (512 * 512 * 4) as usize];

        let padding = sdf_spread.ceil() as u32 + 1;
        let mut x_pos: u32 = padding;
        let mut y_pos: u32 = padding;
        let mut max_height: u32 = 0;

        for &c in chars {
            let (metrics, bitmap) = font.rasterize(c, font_size);

            let glyph_width = metrics.width as u32 + 2 * padding;
            let glyph_height = metrics.height as u32 + 2 * padding;
            let sdf_data =
                generate_sdf_from_bitmap(&bitmap, metrics.width, metrics.height, sdf_spread);

            if x_pos + glyph_width > 512 {
                x_pos = padding;
                y_pos += max_height + padding;
                max_height = 0;
            }

            for py in 0..glyph_height {
                for px in 0..glyph_width {
                    let sx = px as i32 - padding as i32;
                    let sy = py as i32 - padding as i32;

                    let value = if sx >= 0
                        && sx < metrics.width as i32
                        && sy >= 0
                        && sy < metrics.height as i32
                    {
                        sdf_data[(sy as u32 * metrics.width as u32 + sx as u32) as usize]
                    } else {
                        0.0
                    };

                    let dst_idx = ((y_pos + py) * 512 + (x_pos + px)) as usize * 4;
                    if dst_idx + 3 < atlas_data.len() {
                        atlas_data[dst_idx] = value;
                        atlas_data[dst_idx + 1] = value;
                        atlas_data[dst_idx + 2] = value;
                        atlas_data[dst_idx + 3] = 1.0;
                    }
                }
            }

            glyphs.insert(
                c,
                SdfGlyph {
                    char: c,
                    atlas_rect: [
                        x_pos as f32 / 512.0,
                        y_pos as f32 / 512.0,
                        (x_pos + glyph_width) as f32 / 512.0,
                        (y_pos + glyph_height) as f32 / 512.0,
                    ],
                    size: [glyph_width as f32, glyph_height as f32],
                    bearing: [metrics.bounds.xmin, metrics.bounds.ymin - padding as f32],
                    advance: metrics.advance_width,
                },
            );

            x_pos += glyph_width + padding;
            max_height = max_height.max(glyph_height);
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sdf_font_atlas"),
            size: wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&atlas_data),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(512 * 16),
                rows_per_image: Some(512),
            },
            wgpu::Extent3d {
                width: 512,
                height: 512,
                depth_or_array_layers: 1,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            texture: Some(texture),
            texture_view: Some(texture_view),
            glyphs,
            width: 512,
            height: 512,
            font_size,
            sdf_spread,
        }
    }

    /// Get glyph metadata for a character.
    pub fn get_glyph(&self, c: char) -> Option<&SdfGlyph> {
        self.glyphs.get(&c)
    }
}

/// Generate SDF from a binary bitmap using 8SSED algorithm.
pub fn generate_sdf_from_bitmap(
    bitmap: &[u8],
    width: usize,
    height: usize,
    spread: f32,
) -> Vec<f32> {
    let size = width * height;
    let mut sdf = vec![0.0f32; size];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let inside = bitmap[idx] > 128;

            let mut min_dist = f32::MAX;

            for dy in -spread as i32..=spread as i32 {
                for dx in -spread as i32..=spread as i32 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;

                    if nx < 0 || nx >= width as i32 || ny < 0 || ny >= height as i32 {
                        continue;
                    }

                    let nidx = ny as usize * width + nx as usize;
                    let neighbor_inside = bitmap[nidx] > 128;

                    if neighbor_inside != inside {
                        let dist = ((dx * dx + dy * dy) as f32).sqrt();
                        min_dist = min_dist.min(dist);
                    }
                }
            }

            sdf[idx] = if inside {
                min_dist / spread
            } else {
                -min_dist / spread
            };
        }
    }

    sdf
}

/// SDF text renderer.
pub struct SdfTextRenderer {
    /// Pipeline for rendering SDF text.
    pub pipeline: Option<wgpu::RenderPipeline>,
    /// Bind group layout.
    pub bind_group_layout: wgpu::BindGroupLayout,
    /// Vertex buffer for text quads.
    pub vertex_buffer: Option<wgpu::Buffer>,
    /// Instance buffer for text instances.
    pub instance_buffer: Option<wgpu::Buffer>,
    /// Uniform buffer.
    pub uniform_buffer: wgpu::Buffer,
    /// Sampler.
    pub sampler: wgpu::Sampler,
}

impl SdfTextRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sdf_text_uniform"),
            size: std::mem::size_of::<SdfUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sdf_text_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sdf_text_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        Self {
            pipeline: None,
            bind_group_layout,
            vertex_buffer: None,
            instance_buffer: None,
            uniform_buffer,
            sampler,
        }
    }

    /// Prepare text for rendering.
    pub fn prepare_text(
        &self,
        _queue: &wgpu::Queue,
        atlas: &SdfFontAtlas,
        text: &str,
        position: [f32; 2],
        scale: f32,
        color: [f32; 4],
        outline: Option<SdfOutline>,
        glow: Option<SdfGlow>,
        shadow: Option<SdfShadow>,
    ) -> Vec<TextInstance> {
        let mut instances = Vec::new();
        let mut x = position[0];
        let y = position[1];

        for c in text.chars() {
            if let Some(glyph) = atlas.get_glyph(c) {
                let instance = TextInstance {
                    position: [x + glyph.bearing[0] * scale, y + glyph.bearing[1] * scale],
                    size: [glyph.size[0] * scale, glyph.size[1] * scale],
                    atlas_rect: glyph.atlas_rect,
                    color,
                    outline: outline.map(|o| o.into()).unwrap_or_default(),
                    glow: glow.map(|g| g.into()).unwrap_or_default(),
                    shadow: shadow.map(|s| s.into()).unwrap_or_default(),
                };
                instances.push(instance);
                x += glyph.advance * scale;
            }
        }

        instances
    }

    /// Render text instances.
    pub fn render(
        &self,
        _encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        atlas: &SdfFontAtlas,
        instances: &[TextInstance],
        _target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) {
        if instances.is_empty() {
            return;
        }

        let Some(_pipeline) = &self.pipeline else {
            return;
        };
        let Some(_atlas_view) = &atlas.texture_view else {
            return;
        };

        let uniform = SdfUniform {
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            color: [1.0, 1.0, 1.0, 1.0],
            edge_params: [1.0, 0.0, 0.0, 0.0],
            glow_color: [0.0; 4],
            shadow_params: [0.0; 4],
            resolution: [width as f32, height as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }
}

/// Text instance for rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct TextInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub atlas_rect: [f32; 4],
    pub color: [f32; 4],
    pub outline: SdfOutlineData,
    pub glow: SdfGlowData,
    pub shadow: SdfShadowData,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct SdfOutlineData {
    pub width: f32,
    pub offset: f32,
    pub color: [f32; 4],
}

impl From<SdfOutline> for SdfOutlineData {
    fn from(o: SdfOutline) -> Self {
        Self {
            width: o.width,
            offset: o.offset,
            color: o.color,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct SdfGlowData {
    pub color: [f32; 4],
    pub intensity: f32,
    pub spread: f32,
    pub _pad: [f32; 2],
}

impl From<SdfGlow> for SdfGlowData {
    fn from(g: SdfGlow) -> Self {
        Self {
            color: g.color,
            intensity: g.intensity,
            spread: g.spread,
            _pad: [0.0; 2],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
pub struct SdfShadowData {
    pub offset: [f32; 2],
    pub blur: f32,
    pub intensity: f32,
    pub color: [f32; 4],
}

impl From<SdfShadow> for SdfShadowData {
    fn from(s: SdfShadow) -> Self {
        Self {
            offset: s.offset,
            blur: s.blur,
            intensity: s.intensity,
            color: s.color,
        }
    }
}

/// Vertex for SDF text quad.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SdfVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

/// Calculate text width in pixels.
pub fn measure_text(atlas: &SdfFontAtlas, text: &str, scale: f32) -> f32 {
    let mut width = 0.0;
    for c in text.chars() {
        if let Some(glyph) = atlas.get_glyph(c) {
            width += glyph.advance * scale;
        }
    }
    width
}
