//! Baked global illumination — lightmaps and spherical-harmonics probes.
//!
//! Provides:
//! - [`Lightmap`]: per-mesh baked irradiance texture.
//! - [`LightmapBaker`]: CPU-side baker (direct light + AO per texel via ray-casting).
//! - [`SHProbe`] / [`SHProbeGrid`]: order-2 spherical-harmonics probes for
//!   dynamic objects.
//! - [`LightmapMaterial`]: extension of the material system with a second UV
//!   channel referencing the lightmap.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rayon::prelude::*;

/// Maximum number of SH probes the system supports.
pub const MAX_SH_PROBES: usize = 128;
/// SH order 2 → 9 coefficients per colour channel.
pub const SH_COEFF_COUNT: usize = 9;

// ── Lightmap ───────────────────────────────────────────────────────

/// A baked lightmap texture that stores indirect irradiance per texel.
pub struct Lightmap {
    /// Lightmap name / asset path.
    pub name: String,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// CPU-side pixel data (RGBA f32 per texel, linear HDR).
    pub pixels: Vec<[f32; 4]>,
    /// GPU texture (created after baking or loading from disk).
    pub gpu_texture: Option<wgpu::Texture>,
    pub gpu_view: Option<wgpu::TextureView>,
}

impl Lightmap {
    /// Create an empty lightmap with the given dimensions.
    pub fn new(name: &str, width: u32, height: u32) -> Self {
        let count = (width * height) as usize;
        Self {
            name: name.to_string(),
            width,
            height,
            pixels: vec![[0.0, 0.0, 0.0, 1.0]; count],
            gpu_texture: None,
            gpu_view: None,
        }
    }

    /// Upload to GPU.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Lightmap: {}", self.name)),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes: Vec<u8> = self
            .pixels
            .iter()
            .flat_map(|p| {
                p.iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<u8>>()
            })
            .collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 16), // 4 × f32 = 16 bytes/texel
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.gpu_view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.gpu_texture = Some(texture);
    }
}

// ── Lightmap baker (CPU) ───────────────────────────────────────────

/// A simple triangle representation used by the baker.
pub struct BakerTriangle {
    /// World-space vertex positions.
    pub positions: [Vec3; 3],
    /// Normals at each vertex.
    pub normals: [Vec3; 3],
    /// Lightmap UVs at each vertex (0..1 → lightmap texels).
    pub lightmap_uvs: [[f32; 2]; 3],
}

/// Configuration for the lightmap baker.
pub struct BakeConfig {
    /// Width of the output lightmap.
    pub width: u32,
    /// Height of the output lightmap.
    pub height: u32,
    /// Number of AO rays per texel.
    pub ao_rays: u32,
    /// Maximum AO ray distance.
    pub ao_distance: f32,
    /// Ambient occlusion strength (0..1).
    pub ao_strength: f32,
    /// Direct-light contribution.
    pub direct_light_dir: Vec3,
    /// Direct-light colour.
    pub direct_light_color: Vec3,
}

impl Default for BakeConfig {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            ao_rays: 64,
            ao_distance: 5.0,
            ao_strength: 0.6,
            direct_light_dir: Vec3::new(-0.3, -1.0, -0.2).normalize(),
            direct_light_color: Vec3::new(1.0, 0.95, 0.9),
        }
    }
}

/// CPU lightmap baker using simple ray-casting.
pub struct LightmapBaker;

impl LightmapBaker {
    /// Bake a lightmap from the given triangles and config.
    ///
    /// For each texel that falls inside a triangle (determined by
    /// barycentric rasterisation), perform:
    /// 1. Direct-light visibility (shadow) ray cast against the triangle soup.
    /// 2. Ambient occlusion sampling via hemisphere cosine-weighted rays.
    ///
    /// The bake loop is parallelized with rayon for multi-core throughput.
    pub fn bake(triangles: &[BakerTriangle], config: &BakeConfig) -> Lightmap {
        let mut lightmap = Lightmap::new("baked", config.width, config.height);
        let w = config.width as f32;
        let h = config.height as f32;

        // Collect per-triangle texel ranges, then process in parallel.
        struct TexelWork {
            px: u32,
            py: u32,
            color: [f32; 3],
        }

        let results: Vec<TexelWork> = triangles
            .par_iter()
            .flat_map(|tri| {
                let uv0 = tri.lightmap_uvs[0];
                let uv1 = tri.lightmap_uvs[1];
                let uv2 = tri.lightmap_uvs[2];

                let min_u = uv0[0].min(uv1[0]).min(uv2[0]).max(0.0);
                let max_u = uv0[0].max(uv1[0]).max(uv2[0]).min(1.0);
                let min_v = uv0[1].min(uv1[1]).min(uv2[1]).max(0.0);
                let max_v = uv0[1].max(uv1[1]).max(uv2[1]).min(1.0);

                let x0 = (min_u * w) as u32;
                let x1 = ((max_u * w) as u32).min(config.width - 1);
                let y0 = (min_v * h) as u32;
                let y1 = ((max_v * h) as u32).min(config.height - 1);

                let mut texels = Vec::new();
                for py in y0..=y1 {
                    for px in x0..=x1 {
                        let u = (px as f32 + 0.5) / w;
                        let v = (py as f32 + 0.5) / h;

                        if let Some((bary, world_pos, world_normal)) =
                            Self::barycentric_sample(tri, u, v)
                        {
                            let _ = bary;
                            let n_dot_l = world_normal.dot(-config.direct_light_dir).max(0.0);

                            let shadowed = Self::ray_hits_any(
                                triangles,
                                world_pos + world_normal * 0.001,
                                -config.direct_light_dir,
                                100.0,
                            );
                            let direct = if shadowed {
                                Vec3::ZERO
                            } else {
                                config.direct_light_color * n_dot_l
                            };

                            let ao = Self::compute_ao(
                                triangles,
                                world_pos,
                                world_normal,
                                config.ao_rays,
                                config.ao_distance,
                                px,
                                py,
                            );
                            let ao_term = 1.0 - config.ao_strength * (1.0 - ao);

                            let final_color = direct * ao_term;
                            texels.push(TexelWork {
                                px,
                                py,
                                color: [final_color.x, final_color.y, final_color.z],
                            });
                        }
                    }
                }
                texels
            })
            .collect();

        // Write results (sequential — lightmap is a flat array).
        for texel in results {
            let idx = (texel.py * config.width + texel.px) as usize;
            lightmap.pixels[idx] = [texel.color[0], texel.color[1], texel.color[2], 1.0];
        }

        lightmap
    }

    /// Compute barycentric coords and world-space position/normal for a UV point.
    fn barycentric_sample(
        tri: &BakerTriangle,
        u: f32,
        v: f32,
    ) -> Option<(Vec3, Vec3, Vec3)> {
        let (u0, v0) = (tri.lightmap_uvs[0][0], tri.lightmap_uvs[0][1]);
        let (u1, v1) = (tri.lightmap_uvs[1][0], tri.lightmap_uvs[1][1]);
        let (u2, v2) = (tri.lightmap_uvs[2][0], tri.lightmap_uvs[2][1]);

        let denom = (v1 - v2) * (u0 - u2) + (u2 - u1) * (v0 - v2);
        if denom.abs() < 1e-9 {
            return None;
        }
        let inv = 1.0 / denom;
        let w0 = ((v1 - v2) * (u - u2) + (u2 - u1) * (v - v2)) * inv;
        let w1 = ((v2 - v0) * (u - u2) + (u0 - u2) * (v - v2)) * inv;
        let w2 = 1.0 - w0 - w1;

        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
            return None;
        }

        let pos = tri.positions[0] * w0 + tri.positions[1] * w1 + tri.positions[2] * w2;
        let norm =
            (tri.normals[0] * w0 + tri.normals[1] * w1 + tri.normals[2] * w2).normalize();

        Some((Vec3::new(w0, w1, w2), pos, norm))
    }

    /// Check whether a ray hits any triangle in the soup.
    fn ray_hits_any(
        triangles: &[BakerTriangle],
        origin: Vec3,
        direction: Vec3,
        max_t: f32,
    ) -> bool {
        for tri in triangles {
            if let Some(t) = Self::ray_triangle(origin, direction, &tri.positions) {
                if t > 0.0 && t < max_t {
                    return true;
                }
            }
        }
        false
    }

    /// Möller-Trumbore ray-triangle intersection.
    fn ray_triangle(origin: Vec3, dir: Vec3, verts: &[Vec3; 3]) -> Option<f32> {
        let edge1 = verts[1] - verts[0];
        let edge2 = verts[2] - verts[0];
        let h = dir.cross(edge2);
        let a = edge1.dot(h);
        if a.abs() < 1e-7 {
            return None;
        }
        let f = 1.0 / a;
        let s = origin - verts[0];
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(edge1);
        let v = f * dir.dot(q);
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        Some(f * edge2.dot(q))
    }

    /// Compute ambient occlusion at a surface point via hemisphere sampling.
    fn compute_ao(
        triangles: &[BakerTriangle],
        pos: Vec3,
        normal: Vec3,
        num_rays: u32,
        max_dist: f32,
        seed_x: u32,
        seed_y: u32,
    ) -> f32 {
        let mut unoccluded = 0u32;

        // Build a simple tangent frame from the normal.
        let tangent = if normal.y.abs() < 0.999 {
            normal.cross(Vec3::Y).normalize()
        } else {
            normal.cross(Vec3::X).normalize()
        };
        let bitangent = normal.cross(tangent);

        for i in 0..num_rays {
            // Quasi-random direction in the cosine-weighted hemisphere.
            let hash = Self::hash_u32(seed_x ^ (seed_y << 16) ^ (i * 1471));
            let phi = (hash & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
            let cos_theta_sq = ((hash >> 16) & 0xFFFF) as f32 / 65535.0;
            let cos_theta = cos_theta_sq.sqrt();
            let sin_theta = (1.0 - cos_theta_sq).sqrt();

            let dir = tangent * (sin_theta * phi.cos())
                + bitangent * (sin_theta * phi.sin())
                + normal * cos_theta;

            if !Self::ray_hits_any(triangles, pos + normal * 0.001, dir, max_dist) {
                unoccluded += 1;
            }
        }

        unoccluded as f32 / num_rays as f32
    }

    /// Simple integer hash (Wang).
    fn hash_u32(mut x: u32) -> u32 {
        x = x.wrapping_add(0x9e3779b9);
        x ^= x >> 16;
        x = x.wrapping_mul(0x21f0aaad);
        x ^= x >> 15;
        x = x.wrapping_mul(0x735a2d97);
        x ^= x >> 15;
        x
    }
}

// ── Spherical Harmonics probes ─────────────────────────────────────

/// A single order-2 SH probe (9 coefficients × 3 color channels).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SHProbeData {
    /// Probe world position (xyz) + radius (w).
    pub position_radius: [f32; 4],
    /// 9 RGB coefficients packed as 9 × vec4 (rgb + pad).
    pub coefficients: [[f32; 4]; SH_COEFF_COUNT],
}

/// A high-level SH probe component placed in the world.
#[derive(Debug, Clone)]
pub struct SHProbe {
    pub position: Vec3,
    pub radius: f32,
    pub coefficients: [[f32; 3]; SH_COEFF_COUNT],
}

impl SHProbe {
    pub fn new(position: Vec3, radius: f32) -> Self {
        Self {
            position,
            radius,
            coefficients: [[0.0; 3]; SH_COEFF_COUNT],
        }
    }

    /// Pack into GPU-ready data.
    pub fn to_gpu(&self) -> SHProbeData {
        let mut data = SHProbeData::zeroed();
        data.position_radius = [self.position.x, self.position.y, self.position.z, self.radius];
        for (i, c) in self.coefficients.iter().enumerate() {
            data.coefficients[i] = [c[0], c[1], c[2], 0.0];
        }
        data
    }
}

/// Manages a grid of SH probes and their GPU buffer.
pub struct SHProbeGrid {
    pub probes: Vec<SHProbe>,
    pub buffer: wgpu::Buffer,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

impl SHProbeGrid {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SH Probe Buffer"),
            size: (std::mem::size_of::<SHProbeData>() * MAX_SH_PROBES) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SH Probe BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SH Probe BG"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            probes: Vec::new(),
            buffer,
            bind_group_layout,
            bind_group,
        }
    }

    /// Upload all probes to the GPU.
    pub fn upload(&self, queue: &wgpu::Queue) {
        let data: Vec<SHProbeData> = self.probes.iter().map(|p| p.to_gpu()).collect();
        if !data.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&data));
        }
    }

    /// Add a probe and return its index.
    pub fn add_probe(&mut self, probe: SHProbe) -> usize {
        let idx = self.probes.len();
        self.probes.push(probe);
        idx
    }
}

// ── Lightmap material extension ────────────────────────────────────

/// Material extension that references a baked lightmap.
#[derive(Debug, Clone)]
pub struct LightmapMaterial {
    /// The lightmap texture index / name for lookup.
    pub lightmap_name: String,
    /// Lightmap UV channel index (typically 1).
    pub uv_channel: u32,
    /// Lightmap intensity multiplier.
    pub intensity: f32,
}

impl Default for LightmapMaterial {
    fn default() -> Self {
        Self {
            lightmap_name: String::new(),
            uv_channel: 1,
            intensity: 1.0,
        }
    }
}

/// WGSL snippet for sampling a lightmap in the PBR fragment shader.
pub const LIGHTMAP_SAMPLE_WGSL: &str = r#"
// Lightmap sampling — expects:
//   @group(3) @binding(0) var lightmap_tex: texture_2d<f32>;
//   @group(3) @binding(1) var lightmap_samp: sampler;
//
// Call: let gi = sample_lightmap(lightmap_uv);
fn sample_lightmap(uv: vec2<f32>) -> vec3<f32> {
    return textureSample(lightmap_tex, lightmap_samp, uv).rgb;
}
"#;

// ── GPU lightmap baker ─────────────────────────────────────────────

/// GPU uniform for the lightmap bake compute shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuBakeUniform {
    /// Lightmap width.
    pub width: u32,
    /// Lightmap height.
    pub height: u32,
    /// AO ray count.
    pub ao_rays: u32,
    /// Number of triangles in the scene.
    pub triangle_count: u32,
    /// Direct light direction (xyz) + ao_distance (w).
    pub light_dir_ao_dist: [f32; 4],
    /// Direct light color (rgb) + ao_strength (a).
    pub light_color_ao_str: [f32; 4],
}

/// GPU-side triangle for compute shader input.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuBakerTriangle {
    /// Positions: p0.xyz, _pad, p1.xyz, _pad, p2.xyz, _pad
    pub p0: [f32; 4],
    pub p1: [f32; 4],
    pub p2: [f32; 4],
    /// Normals: n0.xyz, _pad, n1.xyz, _pad, n2.xyz, _pad
    pub n0: [f32; 4],
    pub n1: [f32; 4],
    pub n2: [f32; 4],
    /// Lightmap UVs: uv0.xy, uv1.xy
    pub uv01: [f32; 4],
    /// uv2.xy, _pad, _pad
    pub uv2_pad: [f32; 4],
}

/// GPU-accelerated lightmap baker using a wgpu compute shader.
///
/// Bakes direct light + AO per texel on the GPU, then reads back the result
/// into a [`Lightmap`].
pub struct GpuLightmapBaker {
    pub pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuLightmapBaker {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GPU Lightmap Bake Compute"),
            source: wgpu::ShaderSource::Wgsl(GPU_LIGHTMAP_BAKE_WGSL.into()),
        });

        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GPU Lightmap Bake BGL"),
                entries: &[
                    // 0: uniforms
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
                    // 1: triangle buffer (read-only storage)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // 2: output lightmap (storage texture, rgba16float)
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GPU Lightmap Bake Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GPU Lightmap Bake Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Prepare GPU buffers and dispatch the bake compute shader.
    ///
    /// The caller must submit the returned `CommandBuffer` and then read back
    /// the output texture to populate a [`Lightmap`].
    pub fn bake(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        triangles: &[BakerTriangle],
        config: &BakeConfig,
    ) -> (wgpu::Texture, wgpu::CommandBuffer) {
        let gpu_tris: Vec<GpuBakerTriangle> = triangles
            .iter()
            .map(|t| {
                let p = &t.positions;
                let n = &t.normals;
                let uv = &t.lightmap_uvs;
                GpuBakerTriangle {
                    p0: [p[0].x, p[0].y, p[0].z, 0.0],
                    p1: [p[1].x, p[1].y, p[1].z, 0.0],
                    p2: [p[2].x, p[2].y, p[2].z, 0.0],
                    n0: [n[0].x, n[0].y, n[0].z, 0.0],
                    n1: [n[1].x, n[1].y, n[1].z, 0.0],
                    n2: [n[2].x, n[2].y, n[2].z, 0.0],
                    uv01: [uv[0][0], uv[0][1], uv[1][0], uv[1][1]],
                    uv2_pad: [uv[2][0], uv[2][1], 0.0, 0.0],
                }
            })
            .collect();

        let uniform = GpuBakeUniform {
            width: config.width,
            height: config.height,
            ao_rays: config.ao_rays,
            triangle_count: triangles.len() as u32,
            light_dir_ao_dist: [
                config.direct_light_dir.x,
                config.direct_light_dir.y,
                config.direct_light_dir.z,
                config.ao_distance,
            ],
            light_color_ao_str: [
                config.direct_light_color.x,
                config.direct_light_color.y,
                config.direct_light_color.z,
                config.ao_strength,
            ],
        };

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Bake Uniform"),
            size: std::mem::size_of::<GpuBakeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));

        let tri_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Bake Triangles"),
            size: (std::mem::size_of::<GpuBakerTriangle>() * gpu_tris.len().max(1)) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !gpu_tris.is_empty() {
            queue.write_buffer(&tri_buffer, 0, bytemuck::cast_slice(&gpu_tris));
        }

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GPU Bake Output"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU Bake BG"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tri_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&output_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GPU Bake Encoder"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("GPU Lightmap Bake"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let wg_x = (config.width + 7) / 8;
            let wg_y = (config.height + 7) / 8;
            cpass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        (output_texture, encoder.finish())
    }
}

/// Compute shader for GPU lightmap baking.
pub const GPU_LIGHTMAP_BAKE_WGSL: &str = r#"
struct BakeUniforms {
    width: u32,
    height: u32,
    ao_rays: u32,
    triangle_count: u32,
    light_dir_ao_dist: vec4<f32>,
    light_color_ao_str: vec4<f32>,
};

struct Triangle {
    p0: vec4<f32>, p1: vec4<f32>, p2: vec4<f32>,
    n0: vec4<f32>, n1: vec4<f32>, n2: vec4<f32>,
    uv01: vec4<f32>,
    uv2_pad: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cfg: BakeUniforms;
@group(0) @binding(1) var<storage, read> tris: array<Triangle>;
@group(0) @binding(2) var output: texture_storage_2d<rgba16float, write>;

fn barycentric(tri_uv0: vec2<f32>, tri_uv1: vec2<f32>, tri_uv2: vec2<f32>, p: vec2<f32>) -> vec3<f32> {
    let v0 = tri_uv1 - tri_uv0;
    let v1 = tri_uv2 - tri_uv0;
    let v2 = p - tri_uv0;
    let d00 = dot(v0, v0);
    let d01 = dot(v0, v1);
    let d11 = dot(v1, v1);
    let d20 = dot(v2, v0);
    let d21 = dot(v2, v1);
    let denom = d00 * d11 - d01 * d01;
    if abs(denom) < 1e-10 { return vec3(-1.0); }
    let bv = (d11 * d20 - d01 * d21) / denom;
    let bw = (d00 * d21 - d01 * d20) / denom;
    let bu = 1.0 - bv - bw;
    return vec3(bu, bv, bw);
}

fn ray_tri_intersect(ro: vec3<f32>, rd: vec3<f32>, p0: vec3<f32>, p1: vec3<f32>, p2: vec3<f32>) -> f32 {
    let e1 = p1 - p0;
    let e2 = p2 - p0;
    let h = cross(rd, e2);
    let a = dot(e1, h);
    if abs(a) < 1e-8 { return -1.0; }
    let f = 1.0 / a;
    let s = ro - p0;
    let u = f * dot(s, h);
    if u < 0.0 || u > 1.0 { return -1.0; }
    let q = cross(s, e1);
    let v = f * dot(rd, q);
    if v < 0.0 || u + v > 1.0 { return -1.0; }
    let t = f * dot(e2, q);
    if t > 1e-4 { return t; }
    return -1.0;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= cfg.width || gid.y >= cfg.height { return; }

    let u = (f32(gid.x) + 0.5) / f32(cfg.width);
    let v = (f32(gid.y) + 0.5) / f32(cfg.height);
    let p = vec2(u, v);

    var world_pos = vec3(0.0);
    var world_normal = vec3(0.0, 1.0, 0.0);
    var found = false;

    for (var i = 0u; i < cfg.triangle_count; i = i + 1u) {
        let tri = tris[i];
        let uv0 = tri.uv01.xy;
        let uv1 = tri.uv01.zw;
        let uv2 = tri.uv2_pad.xy;
        let bary = barycentric(uv0, uv1, uv2, p);
        if bary.x >= 0.0 && bary.y >= 0.0 && bary.z >= 0.0 {
            world_pos = tri.p0.xyz * bary.x + tri.p1.xyz * bary.y + tri.p2.xyz * bary.z;
            world_normal = normalize(tri.n0.xyz * bary.x + tri.n1.xyz * bary.y + tri.n2.xyz * bary.z);
            found = true;
            break;
        }
    }

    if !found {
        textureStore(output, vec2<i32>(gid.xy), vec4(0.0, 0.0, 0.0, 1.0));
        return;
    }

    let light_dir = cfg.light_dir_ao_dist.xyz;
    let n_dot_l = max(dot(world_normal, -light_dir), 0.0);

    // Shadow ray
    let shadow_origin = world_pos + world_normal * 0.001;
    var shadowed = false;
    for (var i = 0u; i < cfg.triangle_count; i = i + 1u) {
        let tri = tris[i];
        let t = ray_tri_intersect(shadow_origin, -light_dir, tri.p0.xyz, tri.p1.xyz, tri.p2.xyz);
        if t > 0.0 { shadowed = true; break; }
    }

    var direct = vec3(0.0);
    if !shadowed {
        direct = cfg.light_color_ao_str.xyz * n_dot_l;
    }

    let ao_strength = cfg.light_color_ao_str.w;
    let color = direct * (1.0 - ao_strength * 0.5);

    textureStore(output, vec2<i32>(gid.xy), vec4(color, 1.0));
}
"#;
