//! 2D rendering and UI — orthographic camera, sprite batch, font atlas.
//!
//! Provides:
//! - Orthographic camera for 2D rendering
//! - Sprite batch renderer with SpriteBundle (Texture + Rect + Color)
//! - Font atlas renderer using fontdue for rasterization
//! - Needed for HUDs, menus, and 2D games

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

pub const SPRITE_BATCH_SIZE: usize = 2048;

#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct SpriteVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl SpriteVertex {
    pub fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 20,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpriteRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl SpriteRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprite {
    pub texture_path: String,
    pub rect: SpriteRect,
    pub z_index: f32,
    pub color: [f32; 4],
    pub rotation: f32,
    pub scale: [f32; 2],
    pub pivot: [f32; 2],
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture_path: String::new(),
            rect: SpriteRect::new(0.0, 0.0, 1.0, 1.0),
            z_index: 0.0,
            color: [1.0; 4],
            rotation: 0.0,
            scale: [1.0, 1.0],
            pivot: [0.5, 0.5],
            flip_x: false,
            flip_y: false,
        }
    }
}

pub struct SpriteBatch {
    pub sprites: Vec<Sprite>,
    pub texture_cache: HashMap<String, wgpu::Texture>,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub instance_buffer: wgpu::Buffer,
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}

impl SpriteBatch {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sprite Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/sprite.wgsl").into(),
            ),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sprite Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sprite Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sprite Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sprite Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[SpriteVertex::buffer_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Vertex Buffer"),
            size: (SPRITE_BATCH_SIZE * 4 * std::mem::size_of::<SpriteVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Index Buffer"),
            size: (SPRITE_BATCH_SIZE * 6 * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sprite Instance Buffer"),
            size: (SPRITE_BATCH_SIZE * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            sprites: Vec::new(),
            texture_cache: HashMap::new(),
            vertex_buffer,
            index_buffer,
            instance_buffer,
            pipeline,
            bind_group_layout,
            sampler,
        }
    }

    pub fn add_sprite(&mut self, sprite: Sprite) {
        self.sprites.push(sprite);
    }

    pub fn clear(&mut self) {
        self.sprites.clear();
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
    }
}

use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SpriteInstance {
    pub position: [f32; 3],
    pub scale: [f32; 2],
    pub rotation: f32,
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

pub struct OrthographicCamera {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
    pub near: f32,
    pub far: f32,
    pub position: glam::Vec3,
    pub zoom: f32,
}

impl OrthographicCamera {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            left: 0.0,
            right: width,
            bottom: 0.0,
            top: height,
            near: -1.0,
            far: 1.0,
            position: glam::Vec3::ZERO,
            zoom: 1.0,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.right = width;
        self.top = height;
    }

    pub fn view_projection(&self) -> glam::Mat4 {
        let scale = 1.0 / self.zoom;
        let left = self.left * scale + self.position.x;
        let right = self.right * scale + self.position.x;
        let bottom = self.bottom * scale + self.position.y;
        let top = self.top * scale + self.position.y;

        glam::Mat4::orthographic_rh(left, right, bottom, top, self.near, self.far)
    }
}

pub struct FontAtlas {
    font: fontdue::Font,
    texture: Option<wgpu::Texture>,
    texture_view: Option<wgpu::TextureView>,
    glyphs: HashMap<char, GlyphInfo>,
    size: u32,
}

#[derive(Debug, Clone)]
pub struct GlyphInfo {
    pub texture_rect: SpriteRect,
    pub offset: [f32; 2],
    pub advance: f32,
}

impl FontAtlas {
    pub fn new(font_bytes: &[u8], size: u32) -> Result<Self, String> {
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
            .map_err(|e| format!("Failed to load font: {:?}", e))?;

        Ok(Self {
            font,
            texture: None,
            texture_view: None,
            glyphs: HashMap::new(),
            size,
        })
    }

    pub fn rasterize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, text: &str) {
        let mut max_width = 0u32;
        let mut max_height = 0u32;
        let mut x_offset = 0u32;
        let mut y_offset = 0u32;

        for ch in text.chars() {
            if self.glyphs.contains_key(&ch) {
                continue;
            }

            let (metrics, bitmap) = self.font.rasterize(ch, self.size as f32);

            let glyph_width = metrics.width as u32;
            let glyph_height = metrics.height as u32;

            if x_offset + glyph_width > 512 {
                x_offset = 0;
                y_offset += max_height;
                max_height = 0;
            }

            max_width = max_width.max(x_offset + glyph_width);
            max_height = max_height.max(y_offset + glyph_height);

            x_offset += glyph_width;
        }

        let texture_width = max_width.next_power_of_two().max(512);
        let texture_height = max_height.next_power_of_two().max(512);

        if self.texture.is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Font Atlas"),
                size: wgpu::Extent3d {
                    width: texture_width,
                    height: texture_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            self.texture = Some(texture);
            self.texture_view = Some(
                self.texture
                    .as_ref()
                    .unwrap()
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            );
        }

        x_offset = 0;
        y_offset = 0;

        for ch in text.chars() {
            if self.glyphs.contains_key(&ch) {
                continue;
            }

            let (metrics, bitmap) = self.font.rasterize(ch, self.size as f32);

            let glyph_width = metrics.width as u32;
            let glyph_height = metrics.height as u32;

            if x_offset + glyph_width > texture_width {
                x_offset = 0;
                y_offset += self.size;
            }

            if let Some(texture) = &self.texture {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: x_offset,
                            y: y_offset,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &bitmap,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(metrics.width as u32),
                        rows_per_image: Some(metrics.height as u32),
                    },
                    wgpu::Extent3d {
                        width: glyph_width,
                        height: glyph_height,
                        depth_or_array_layers: 1,
                    },
                );
            }

            self.glyphs.insert(
                ch,
                GlyphInfo {
                    texture_rect: SpriteRect::new(
                        x_offset as f32 / texture_width as f32,
                        y_offset as f32 / texture_height as f32,
                        glyph_width as f32 / texture_width as f32,
                        glyph_height as f32 / texture_height as f32,
                    ),
                    offset: [metrics.xmin as f32, -metrics.ymin as f32],
                    advance: metrics.advance_width,
                },
            );

            x_offset += glyph_width;
        }
    }

    pub fn get_glyph(&self, ch: char) -> Option<&GlyphInfo> {
        self.glyphs.get(&ch)
    }
}

pub struct TextRenderer {
    pub font_atlas: FontAtlas,
    pub sprite_batch: SpriteBatch,
}

impl TextRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        font_bytes: &[u8],
        size: u32,
    ) -> Result<Self, String> {
        let font_atlas = FontAtlas::new(font_bytes, size)?;
        let sprite_batch = SpriteBatch::new(device, format);

        Ok(Self {
            font_atlas,
            sprite_batch,
        })
    }

    pub fn render_text(
        &mut self,
        text: &str,
        position: [f32; 2],
        color: [f32; 4],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.font_atlas.rasterize(device, queue, text);

        let mut x = position[0];
        let y = position[1];

        for ch in text.chars() {
            if let Some(glyph) = self.font_atlas.get_glyph(ch) {
                let sprite = Sprite {
                    texture_path: String::from("__font_atlas__"),
                    rect: glyph.texture_rect.clone(),
                    z_index: 0.0,
                    color,
                    rotation: 0.0,
                    scale: [1.0, 1.0],
                    pivot: [0.0, 0.0],
                    flip_x: false,
                    flip_y: false,
                };

                self.sprite_batch.add_sprite(sprite);
                x += glyph.advance;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_rect_creation() {
        let rect = SpriteRect::new(0.0, 0.0, 100.0, 100.0);
        assert_eq!(rect.width, 100.0);
    }

    #[test]
    fn orthographic_camera_view_proj() {
        let camera = OrthographicCamera::new(800.0, 600.0);
        let vp = camera.view_projection();
        assert!(vp.is_finite());
    }

    #[test]
    fn sprite_default() {
        let sprite = Sprite::default();
        assert_eq!(sprite.color, [1.0; 4]);
        assert_eq!(sprite.rotation, 0.0);
    }
}
