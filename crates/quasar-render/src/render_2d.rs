//! Complete 2D rendering pipeline for Quasar Engine.
//!
//! Provides:
//! - Sprite animation with sprite sheets
//! - Tilemap rendering with chunking
//! - 2D camera with parallax scrolling
//! - 2D particles
//! - Shape primitives (lines, rectangles, circles, polygons)
//! - 9-slice scaling for UI elements

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

// ────────────────────────────────────────────────────────────────────────────
// Sprite Animation
// ────────────────────────────────────────────────────────────────────────────

/// A single frame in a sprite animation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationFrame {
    pub texture_rect: SpriteRect,
    pub duration_ms: f32,
}

/// An animation sequence (e.g., "walk", "run", "idle").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationSequence {
    pub name: String,
    pub frames: Vec<AnimationFrame>,
    pub looped: bool,
}

/// Sprite animation controller.
#[derive(Debug, Clone)]
pub struct SpriteAnimator {
    sequences: HashMap<String, AnimationSequence>,
    current_sequence: Option<String>,
    current_frame: usize,
    elapsed_ms: f32,
    speed: f32,
    playing: bool,
}

impl SpriteAnimator {
    pub fn new() -> Self {
        Self {
            sequences: HashMap::new(),
            current_sequence: None,
            current_frame: 0,
            elapsed_ms: 0.0,
            speed: 1.0,
            playing: false,
        }
    }

    pub fn add_sequence(&mut self, sequence: AnimationSequence) {
        self.sequences.insert(sequence.name.clone(), sequence);
    }

    pub fn play(&mut self, name: &str) {
        if self.sequences.contains_key(name) {
            if self.current_sequence.as_deref() != Some(name) {
                self.current_sequence = Some(name.to_string());
                self.current_frame = 0;
                self.elapsed_ms = 0.0;
            }
            self.playing = true;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.max(0.0);
    }

    pub fn update(&mut self, delta_ms: f32) {
        if !self.playing {
            return;
        }

        let sequence_name = match &self.current_sequence {
            Some(name) => name.clone(),
            None => return,
        };

        let sequence = match self.sequences.get(&sequence_name) {
            Some(s) => s,
            None => return,
        };

        if sequence.frames.is_empty() {
            return;
        }

        self.elapsed_ms += delta_ms * self.speed;

        let frame = &sequence.frames[self.current_frame];
        if self.elapsed_ms >= frame.duration_ms {
            self.elapsed_ms -= frame.duration_ms;
            self.current_frame += 1;

            if self.current_frame >= sequence.frames.len() {
                if sequence.looped {
                    self.current_frame = 0;
                } else {
                    self.current_frame = sequence.frames.len() - 1;
                    self.playing = false;
                }
            }
        }
    }

    pub fn current_frame(&self) -> Option<&AnimationFrame> {
        let sequence_name = self.current_sequence.as_ref()?;
        let sequence = self.sequences.get(sequence_name)?;
        sequence.frames.get(self.current_frame)
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn current_sequence(&self) -> Option<&str> {
        self.current_sequence.as_deref()
    }
}

impl Default for SpriteAnimator {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tilemap
// ────────────────────────────────────────────────────────────────────────────

/// A single tile in a tilemap.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Tile {
    pub tile_id: u32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rotation: u8,
}

impl Default for Tile {
    fn default() -> Self {
        Self {
            tile_id: 0,
            flip_x: false,
            flip_y: false,
            rotation: 0,
        }
    }
}

/// A tileset containing tile textures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tileset {
    pub texture_path: String,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub spacing: u32,
    pub margin: u32,
}

impl Tileset {
    pub fn new(
        texture_path: &str,
        tile_width: u32,
        tile_height: u32,
        columns: u32,
        rows: u32,
    ) -> Self {
        Self {
            texture_path: texture_path.to_string(),
            tile_width,
            tile_height,
            columns,
            rows,
            spacing: 0,
            margin: 0,
        }
    }

    pub fn tile_count(&self) -> u32 {
        self.columns * self.rows
    }

    pub fn tile_uv(&self, tile_id: u32) -> SpriteRect {
        let col = tile_id % self.columns;
        let row = tile_id / self.columns;

        let texture_width =
            self.columns * (self.tile_width + self.spacing) + self.margin * 2 - self.spacing;
        let texture_height =
            self.rows * (self.tile_height + self.spacing) + self.margin * 2 - self.spacing;

        let x =
            (self.margin + col * (self.tile_width + self.spacing)) as f32 / texture_width as f32;
        let y =
            (self.margin + row * (self.tile_height + self.spacing)) as f32 / texture_height as f32;
        let w = self.tile_width as f32 / texture_width as f32;
        let h = self.tile_height as f32 / texture_height as f32;

        SpriteRect::new(x, y, w, h)
    }
}

/// A chunk of tiles for efficient rendering.
#[derive(Debug, Clone)]
pub struct TileChunk {
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub tiles: Vec<Tile>,
    pub dirty: bool,
}

impl TileChunk {
    pub fn new(chunk_x: i32, chunk_y: i32, size: usize) -> Self {
        Self {
            chunk_x,
            chunk_y,
            tiles: vec![Tile::default(); size * size],
            dirty: true,
        }
    }

    pub fn get_tile(&self, local_x: usize, local_y: usize, chunk_size: usize) -> &Tile {
        &self.tiles[local_y * chunk_size + local_x]
    }

    pub fn set_tile(&mut self, local_x: usize, local_y: usize, chunk_size: usize, tile: Tile) {
        self.tiles[local_y * chunk_size + local_x] = tile;
        self.dirty = true;
    }
}

/// A tilemap with chunked storage for large maps.
#[derive(Debug, Clone)]
pub struct Tilemap {
    pub tileset: Tileset,
    pub chunk_size: usize,
    pub tile_width: f32,
    pub tile_height: f32,
    chunks: HashMap<(i32, i32), TileChunk>,
}

impl Tilemap {
    pub fn new(tileset: Tileset, chunk_size: usize) -> Self {
        let tile_width = tileset.tile_width as f32;
        let tile_height = tileset.tile_height as f32;
        Self {
            tileset,
            chunk_size,
            tile_width,
            tile_height,
            chunks: HashMap::new(),
        }
    }

    pub fn world_to_tile(&self, x: f32, y: f32) -> (i32, i32) {
        (
            (x / self.tile_width).floor() as i32,
            (y / self.tile_height).floor() as i32,
        )
    }

    pub fn tile_to_chunk(&self, tile_x: i32, tile_y: i32) -> (i32, i32, usize, usize) {
        let chunk_x = tile_x.div_euclid(self.chunk_size as i32);
        let chunk_y = tile_y.div_euclid(self.chunk_size as i32);
        let local_x = tile_x.rem_euclid(self.chunk_size as i32) as usize;
        let local_y = tile_y.rem_euclid(self.chunk_size as i32) as usize;
        (chunk_x, chunk_y, local_x, local_y)
    }

    pub fn get_tile(&self, tile_x: i32, tile_y: i32) -> Option<&Tile> {
        let (chunk_x, chunk_y, local_x, local_y) = self.tile_to_chunk(tile_x, tile_y);
        let chunk = self.chunks.get(&(chunk_x, chunk_y))?;
        Some(chunk.get_tile(local_x, local_y, self.chunk_size))
    }

    pub fn set_tile(&mut self, tile_x: i32, tile_y: i32, tile: Tile) {
        let (chunk_x, chunk_y, local_x, local_y) = self.tile_to_chunk(tile_x, tile_y);
        let chunk = self
            .chunks
            .entry((chunk_x, chunk_y))
            .or_insert_with(|| TileChunk::new(chunk_x, chunk_y, self.chunk_size));
        chunk.set_tile(local_x, local_y, self.chunk_size, tile);
    }

    pub fn chunks(&self) -> impl Iterator<Item = &TileChunk> {
        self.chunks.values()
    }

    pub fn visible_chunks(&self, camera_bounds: SpriteRect) -> impl Iterator<Item = &TileChunk> {
        let start_tile = self.world_to_tile(camera_bounds.x, camera_bounds.y);
        let end_tile = self.world_to_tile(
            camera_bounds.x + camera_bounds.width,
            camera_bounds.y + camera_bounds.height,
        );

        let start_chunk_x = start_tile.0.div_euclid(self.chunk_size as i32);
        let start_chunk_y = start_tile.1.div_euclid(self.chunk_size as i32);
        let end_chunk_x = end_tile.0.div_euclid(self.chunk_size as i32);
        let end_chunk_y = end_tile.1.div_euclid(self.chunk_size as i32);

        self.chunks.values().filter(move |chunk| {
            chunk.chunk_x >= start_chunk_x - 1
                && chunk.chunk_x <= end_chunk_x + 1
                && chunk.chunk_y >= start_chunk_y - 1
                && chunk.chunk_y <= end_chunk_y + 1
        })
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 2D Camera with Parallax
// ────────────────────────────────────────────────────────────────────────────

/// A parallax layer for the 2D camera.
#[derive(Debug, Clone)]
pub struct ParallaxLayer {
    pub name: String,
    pub scroll_speed: glam::Vec2,
    pub depth: f32,
    pub offset: glam::Vec2,
}

impl ParallaxLayer {
    pub fn new(name: &str, scroll_speed: glam::Vec2, depth: f32) -> Self {
        Self {
            name: name.to_string(),
            scroll_speed,
            depth,
            offset: glam::Vec2::ZERO,
        }
    }
}

/// Extended 2D camera with parallax scrolling and bounds.
#[derive(Debug, Clone)]
pub struct Camera2D {
    pub position: glam::Vec3,
    pub zoom: f32,
    pub rotation: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub bounds: Option<SpriteRect>,
    pub parallax_layers: Vec<ParallaxLayer>,
    pub pixel_perfect: bool,
    pub smooth_follow: Option<SmoothFollow>,
}

#[derive(Debug, Clone)]
pub struct SmoothFollow {
    pub target: glam::Vec2,
    pub speed: f32,
    pub deadzone: f32,
}

impl Camera2D {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            position: glam::Vec3::new(width / 2.0, height / 2.0, 0.0),
            zoom: 1.0,
            rotation: 0.0,
            viewport_width: width,
            viewport_height: height,
            bounds: None,
            parallax_layers: Vec::new(),
            pixel_perfect: false,
            smooth_follow: None,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    pub fn set_bounds(&mut self, bounds: SpriteRect) {
        self.bounds = Some(bounds);
    }

    pub fn add_parallax_layer(&mut self, layer: ParallaxLayer) {
        self.parallax_layers.push(layer);
    }

    pub fn follow(&mut self, target: glam::Vec2) {
        if let Some(ref mut follow) = self.smooth_follow {
            follow.target = target;
        } else {
            self.position.x = target.x;
            self.position.y = target.y;
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        if let Some(ref mut follow) = self.smooth_follow {
            let diff = follow.target - glam::Vec2::new(self.position.x, self.position.y);
            let distance = diff.length();

            if distance > follow.deadzone {
                let t = (follow.speed * delta_time).min(1.0);
                self.position.x += diff.x * t;
                self.position.y += diff.y * t;
            }
        }

        if let Some(bounds) = self.bounds {
            let half_width = (self.viewport_width / 2.0) / self.zoom;
            let half_height = (self.viewport_height / 2.0) / self.zoom;

            self.position.x = self
                .position
                .x
                .clamp(bounds.x + half_width, bounds.x + bounds.width - half_width);
            self.position.y = self.position.y.clamp(
                bounds.y + half_height,
                bounds.y + bounds.height - half_height,
            );
        }

        if self.pixel_perfect {
            self.position.x = self.position.x.round();
            self.position.y = self.position.y.round();
        }
    }

    pub fn view_matrix(&self) -> glam::Mat4 {
        let scale = 1.0 / self.zoom;
        let rotation = glam::Mat4::from_rotation_z(self.rotation);
        let translation = glam::Mat4::from_translation(-self.position);

        let proj = glam::Mat4::orthographic_rh(
            -self.viewport_width / 2.0 * scale,
            self.viewport_width / 2.0 * scale,
            -self.viewport_height / 2.0 * scale,
            self.viewport_height / 2.0 * scale,
            -1000.0,
            1000.0,
        );

        proj * rotation * translation
    }

    pub fn view_matrix_for_layer(&self, layer: &ParallaxLayer) -> glam::Mat4 {
        let offset = layer.offset
            + glam::Vec2::new(self.position.x, self.position.y) * (1.0 - layer.scroll_speed);
        let scale = 1.0 / self.zoom;

        glam::Mat4::orthographic_rh(
            offset.x - self.viewport_width / 2.0 * scale,
            offset.x + self.viewport_width / 2.0 * scale,
            offset.y - self.viewport_height / 2.0 * scale,
            offset.y + self.viewport_height / 2.0 * scale,
            -1000.0,
            1000.0,
        )
    }

    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> glam::Vec2 {
        let scale = 1.0 / self.zoom;
        glam::Vec2::new(
            (screen_x - self.viewport_width / 2.0) * scale + self.position.x,
            (screen_y - self.viewport_height / 2.0) * scale + self.position.y,
        )
    }

    pub fn world_to_screen(&self, world_x: f32, world_y: f32) -> glam::Vec2 {
        let scale = self.zoom;
        glam::Vec2::new(
            (world_x - self.position.x) * scale + self.viewport_width / 2.0,
            (world_y - self.position.y) * scale + self.viewport_height / 2.0,
        )
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 2D Particles
// ────────────────────────────────────────────────────────────────────────────

/// A single particle.
#[derive(Debug, Clone)]
pub struct Particle2D {
    pub position: glam::Vec2,
    pub velocity: glam::Vec2,
    pub acceleration: glam::Vec2,
    pub color: [f32; 4],
    pub size: f32,
    pub size_delta: f32,
    pub rotation: f32,
    pub rotation_speed: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

impl Particle2D {
    pub fn is_alive(&self) -> bool {
        self.lifetime > 0.0
    }

    pub fn update(&mut self, dt: f32) {
        self.velocity += self.acceleration * dt;
        self.position += self.velocity * dt;
        self.rotation += self.rotation_speed * dt;
        self.size += self.size_delta * dt;
        self.lifetime -= dt;
    }

    pub fn alpha(&self) -> f32 {
        (self.lifetime / self.max_lifetime).clamp(0.0, 1.0)
    }
}

/// Particle emitter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleEmitterConfig {
    pub emission_rate: f32,
    pub max_particles: usize,
    pub lifetime_min: f32,
    pub lifetime_max: f32,
    pub speed_min: f32,
    pub speed_max: f32,
    pub direction_min: f32,
    pub direction_max: f32,
    pub size_min: f32,
    pub size_max: f32,
    pub size_delta_min: f32,
    pub size_delta_max: f32,
    pub color_start: [f32; 4],
    pub color_end: [f32; 4],
    pub gravity: glam::Vec2,
    pub rotation_min: f32,
    pub rotation_max: f32,
    pub rotation_speed_min: f32,
    pub rotation_speed_max: f32,
}

impl Default for ParticleEmitterConfig {
    fn default() -> Self {
        Self {
            emission_rate: 10.0,
            max_particles: 1000,
            lifetime_min: 0.5,
            lifetime_max: 2.0,
            speed_min: 50.0,
            speed_max: 100.0,
            direction_min: 0.0,
            direction_max: std::f32::consts::TAU,
            size_min: 4.0,
            size_max: 8.0,
            size_delta_min: -1.0,
            size_delta_max: 0.0,
            color_start: [1.0, 1.0, 1.0, 1.0],
            color_end: [1.0, 1.0, 1.0, 0.0],
            gravity: glam::Vec2::new(0.0, -100.0),
            rotation_min: 0.0,
            rotation_max: std::f32::consts::TAU,
            rotation_speed_min: -1.0,
            rotation_speed_max: 1.0,
        }
    }
}

/// Particle emitter.
#[derive(Debug, Clone)]
pub struct ParticleEmitter2D {
    pub position: glam::Vec2,
    pub config: ParticleEmitterConfig,
    pub active: bool,
    particles: Vec<Particle2D>,
    emission_accumulator: f32,
}

impl ParticleEmitter2D {
    pub fn new(position: glam::Vec2, config: ParticleEmitterConfig) -> Self {
        let max_particles = config.max_particles;
        Self {
            position,
            config,
            active: true,
            particles: Vec::with_capacity(max_particles),
            emission_accumulator: 0.0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        if self.active {
            self.emission_accumulator += dt * self.config.emission_rate;

            while self.emission_accumulator >= 1.0
                && self.particles.len() < self.config.max_particles
            {
                self.emit_particle();
                self.emission_accumulator -= 1.0;
            }
        }

        for particle in &mut self.particles {
            particle.update(dt);
        }

        self.particles.retain(|p| p.is_alive());
    }

    fn emit_particle(&mut self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let lifetime = rng.gen_range(self.config.lifetime_min..=self.config.lifetime_max);
        let speed = rng.gen_range(self.config.speed_min..=self.config.speed_max);
        let direction = rng.gen_range(self.config.direction_min..=self.config.direction_max);
        let size = rng.gen_range(self.config.size_min..=self.config.size_max);
        let size_delta = rng.gen_range(self.config.size_delta_min..=self.config.size_delta_max);
        let rotation = rng.gen_range(self.config.rotation_min..=self.config.rotation_max);
        let rotation_speed =
            rng.gen_range(self.config.rotation_speed_min..=self.config.rotation_speed_max);

        let velocity = glam::Vec2::new(direction.cos(), direction.sin()) * speed;

        self.particles.push(Particle2D {
            position: self.position,
            velocity,
            acceleration: self.config.gravity,
            color: self.config.color_start,
            size,
            size_delta,
            rotation,
            rotation_speed,
            lifetime,
            max_lifetime: lifetime,
        });
    }

    pub fn particles(&self) -> &[Particle2D] {
        &self.particles
    }

    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }

    pub fn burst(&mut self, count: usize) {
        for _ in 0..count.min(self.config.max_particles - self.particles.len()) {
            self.emit_particle();
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 2D Shapes
// ────────────────────────────────────────────────────────────────────────────

/// 2D shape for rendering primitives.
#[derive(Debug, Clone)]
pub enum Shape2D {
    Line {
        start: glam::Vec2,
        end: glam::Vec2,
        color: [f32; 4],
        thickness: f32,
    },
    Rectangle {
        rect: SpriteRect,
        color: [f32; 4],
        filled: bool,
        corner_radius: f32,
    },
    Circle {
        center: glam::Vec2,
        radius: f32,
        color: [f32; 4],
        filled: bool,
        segments: u32,
    },
    Polygon {
        points: Vec<glam::Vec2>,
        color: [f32; 4],
        filled: bool,
    },
    Capsule {
        center: glam::Vec2,
        half_height: f32,
        radius: f32,
        color: [f32; 4],
        filled: bool,
    },
}

/// Shape batch renderer.
#[derive(Debug, Clone)]
pub struct ShapeBatch2D {
    shapes: Vec<Shape2D>,
    default_segments: u32,
}

impl ShapeBatch2D {
    pub fn new() -> Self {
        Self {
            shapes: Vec::new(),
            default_segments: 32,
        }
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
    }

    pub fn draw_line(
        &mut self,
        start: glam::Vec2,
        end: glam::Vec2,
        color: [f32; 4],
        thickness: f32,
    ) {
        self.shapes.push(Shape2D::Line {
            start,
            end,
            color,
            thickness,
        });
    }

    pub fn draw_rect(&mut self, rect: SpriteRect, color: [f32; 4], filled: bool) {
        self.shapes.push(Shape2D::Rectangle {
            rect,
            color,
            filled,
            corner_radius: 0.0,
        });
    }

    pub fn draw_rounded_rect(&mut self, rect: SpriteRect, color: [f32; 4], corner_radius: f32) {
        self.shapes.push(Shape2D::Rectangle {
            rect,
            color,
            filled: true,
            corner_radius,
        });
    }

    pub fn draw_circle(&mut self, center: glam::Vec2, radius: f32, color: [f32; 4], filled: bool) {
        self.shapes.push(Shape2D::Circle {
            center,
            radius,
            color,
            filled,
            segments: self.default_segments,
        });
    }

    pub fn draw_polygon(&mut self, points: Vec<glam::Vec2>, color: [f32; 4], filled: bool) {
        self.shapes.push(Shape2D::Polygon {
            points,
            color,
            filled,
        });
    }

    pub fn draw_capsule(
        &mut self,
        center: glam::Vec2,
        half_height: f32,
        radius: f32,
        color: [f32; 4],
        filled: bool,
    ) {
        self.shapes.push(Shape2D::Capsule {
            center,
            half_height,
            radius,
            color,
            filled,
        });
    }

    pub fn shapes(&self) -> &[Shape2D] {
        &self.shapes
    }
}

impl Default for ShapeBatch2D {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// 9-Slice Scaling
// ────────────────────────────────────────────────────────────────────────────

/// 9-slice configuration for scalable UI elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NineSlice {
    pub texture_path: String,
    pub borders: [f32; 4], // left, top, right, bottom
    pub size: [f32; 2],
    pub color: [f32; 4],
}

impl NineSlice {
    pub fn new(texture_path: &str, borders: [f32; 4]) -> Self {
        Self {
            texture_path: texture_path.to_string(),
            borders,
            size: [100.0, 100.0],
            color: [1.0; 4],
        }
    }

    /// Generate the 9 quad positions and UVs for rendering.
    pub fn generate_quads(&self, rect: &SpriteRect) -> [(SpriteRect, SpriteRect); 9] {
        let [bl, bt, br, bb] = self.borders;
        let w = rect.width;
        let h = rect.height;

        let center_w = (w - bl - br).max(0.0);
        let center_h = (h - bt - bb).max(0.0);

        let u_left = bl / w;
        let u_right = (w - br) / w;
        let v_top = bt / h;
        let v_bottom = (h - bb) / h;

        let positions = [
            // Top row
            (0.0, 0.0, bl, bt),
            (bl, 0.0, center_w, bt),
            (w - br, 0.0, br, bt),
            // Middle row
            (0.0, bt, bl, center_h),
            (bl, bt, center_w, center_h),
            (w - br, bt, br, center_h),
            // Bottom row
            (0.0, h - bb, bl, bb),
            (bl, h - bb, center_w, bb),
            (w - br, h - bb, br, bb),
        ];

        let uvs = [
            // Top row
            (0.0, 0.0, u_left, v_top),
            (u_left, 0.0, u_right - u_left, v_top),
            (u_right, 0.0, 1.0 - u_right, v_top),
            // Middle row
            (0.0, v_top, u_left, v_bottom - v_top),
            (u_left, v_top, u_right - u_left, v_bottom - v_top),
            (u_right, v_top, 1.0 - u_right, v_bottom - v_top),
            // Bottom row
            (0.0, v_bottom, u_left, 1.0 - v_bottom),
            (u_left, v_bottom, u_right - u_left, 1.0 - v_bottom),
            (u_right, v_bottom, 1.0 - u_right, 1.0 - v_bottom),
        ];

        let mut result = [(
            SpriteRect::new(0.0, 0.0, 0.0, 0.0),
            SpriteRect::new(0.0, 0.0, 0.0, 0.0),
        ); 9];
        for i in 0..9 {
            let (px, py, pw, ph) = positions[i];
            let (ux, uy, uw, uh) = uvs[i];
            result[i] = (
                SpriteRect::new(rect.x + px, rect.y + py, pw, ph),
                SpriteRect::new(ux, uy, uw, uh),
            );
        }
        result
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_animator_play_sequence() {
        let mut animator = SpriteAnimator::new();
        animator.add_sequence(AnimationSequence {
            name: "walk".to_string(),
            frames: vec![
                AnimationFrame {
                    texture_rect: SpriteRect::new(0.0, 0.0, 0.5, 0.5),
                    duration_ms: 100.0,
                },
                AnimationFrame {
                    texture_rect: SpriteRect::new(0.5, 0.0, 0.5, 0.5),
                    duration_ms: 100.0,
                },
            ],
            looped: true,
        });

        animator.play("walk");
        assert!(animator.is_playing());
        assert_eq!(animator.current_sequence(), Some("walk"));
    }

    #[test]
    fn sprite_animator_frame_advance() {
        let mut animator = SpriteAnimator::new();
        animator.add_sequence(AnimationSequence {
            name: "test".to_string(),
            frames: vec![
                AnimationFrame {
                    texture_rect: SpriteRect::new(0.0, 0.0, 0.5, 0.5),
                    duration_ms: 100.0,
                },
                AnimationFrame {
                    texture_rect: SpriteRect::new(0.5, 0.0, 0.5, 0.5),
                    duration_ms: 100.0,
                },
            ],
            looped: false,
        });

        animator.play("test");
        animator.update(50.0);
        assert_eq!(animator.current_frame().unwrap().texture_rect.x, 0.0);

        animator.update(60.0); // Total 110ms > 100ms
        assert_eq!(animator.current_frame().unwrap().texture_rect.x, 0.5);
    }

    #[test]
    fn tilemap_set_get_tile() {
        let tileset = Tileset::new("tiles.png", 32, 32, 4, 4);
        let mut tilemap = Tilemap::new(tileset, 16);

        tilemap.set_tile(
            5,
            7,
            Tile {
                tile_id: 3,
                ..Default::default()
            },
        );

        let tile = tilemap.get_tile(5, 7);
        assert!(tile.is_some());
        assert_eq!(tile.unwrap().tile_id, 3);
    }

    #[test]
    fn camera_2d_screen_world_conversion() {
        let camera = Camera2D::new(800.0, 600.0);

        let world = camera.screen_to_world(400.0, 300.0);
        assert!((world.x - camera.position.x).abs() < 0.1);
        assert!((world.y - camera.position.y).abs() < 0.1);

        let screen = camera.world_to_screen(camera.position.x, camera.position.y);
        assert!((screen.x - 400.0).abs() < 0.1);
        assert!((screen.y - 300.0).abs() < 0.1);
    }

    #[test]
    fn camera_2d_bounds_clamping() {
        let mut camera = Camera2D::new(800.0, 600.0);
        camera.set_bounds(SpriteRect::new(0.0, 0.0, 2000.0, 2000.0));

        camera.position.x = -100.0;
        camera.position.y = 3000.0;
        camera.update(0.0);

        assert!(camera.position.x >= 400.0);
        assert!(camera.position.y <= 1700.0);
    }

    #[test]
    fn particle_emitter_emit() {
        let config = ParticleEmitterConfig {
            emission_rate: 0.0,
            max_particles: 100,
            ..Default::default()
        };
        let mut emitter = ParticleEmitter2D::new(glam::Vec2::ZERO, config);

        emitter.burst(10);
        assert_eq!(emitter.particle_count(), 10);
    }

    #[test]
    fn particle_emitter_lifetime() {
        let config = ParticleEmitterConfig {
            emission_rate: 0.0,
            lifetime_min: 0.1,
            lifetime_max: 0.1,
            ..Default::default()
        };
        let mut emitter = ParticleEmitter2D::new(glam::Vec2::ZERO, config);

        emitter.burst(1);
        emitter.update(0.15); // Past lifetime

        assert_eq!(emitter.particle_count(), 0);
    }

    #[test]
    fn shape_batch_add_shapes() {
        let mut batch = ShapeBatch2D::new();

        batch.draw_line(glam::Vec2::ZERO, glam::Vec2::new(10.0, 10.0), [1.0; 4], 1.0);
        batch.draw_rect(SpriteRect::new(0.0, 0.0, 100.0, 100.0), [1.0; 4], true);
        batch.draw_circle(glam::Vec2::new(50.0, 50.0), 25.0, [1.0; 4], true);

        assert_eq!(batch.shapes().len(), 3);
    }

    #[test]
    fn nine_slice_quads() {
        let nine_slice = NineSlice::new("panel.png", [10.0, 10.0, 10.0, 10.0]);
        let rect = SpriteRect::new(0.0, 0.0, 100.0, 100.0);
        let quads = nine_slice.generate_quads(&rect);

        assert_eq!(quads.len(), 9);
    }
}
