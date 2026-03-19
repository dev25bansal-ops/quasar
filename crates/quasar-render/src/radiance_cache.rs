//! Lumen-style screen-space radiance cache.
//!
//! Implements a sparse 3D radiance cache (world-space voxel grid) that is
//! populated each frame from screen-space probes and accumulated over time.
//! The cache provides low-cost diffuse GI by sampling cached radiance during
//! the lighting pass, avoiding full ray-tracing.
//!
//! # Architecture
//!
//! 1. **Probe placement:** Probes are placed on a 3D uniform grid aligned to
//!    the camera frustum. Each probe stores a low-res spherical harmonic (SH)
//!    representation of incoming radiance.
//!
//! 2. **Radiance injection:** Each frame, screen-space trace results (from the
//!    SSGI pass) are splatted into nearby probes to update their SH coefficients.
//!
//! 3. **Temporal accumulation:** Probes blend new samples with previous values
//!    using an exponential moving average to reduce noise.
//!
//! 4. **Cache lookup:** During the main lighting pass, each pixel reconstructs
//!    diffuse GI by trilinearly interpolating between the 8 surrounding probes.

/// SH coefficient order. L1 (4 coefficients per channel) gives a smooth
/// diffuse approximation; L2 (9 per channel) adds gentle directionality.
pub const SH_COEFF_COUNT: usize = 9; // L2

/// A single radiance probe storing second-order SH coefficients (RGB).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RadianceProbe {
    /// SH coefficients for red channel.
    pub sh_r: [f32; SH_COEFF_COUNT],
    /// SH coefficients for green channel.
    pub sh_g: [f32; SH_COEFF_COUNT],
    /// SH coefficients for blue channel.
    pub sh_b: [f32; SH_COEFF_COUNT],
    /// Number of samples accumulated (for blending).
    pub sample_count: u32,
}

impl Default for RadianceProbe {
    fn default() -> Self {
        Self {
            sh_r: [0.0; SH_COEFF_COUNT],
            sh_g: [0.0; SH_COEFF_COUNT],
            sh_b: [0.0; SH_COEFF_COUNT],
            sample_count: 0,
        }
    }
}

/// Settings for the radiance cache.
#[derive(Debug, Clone)]
pub struct RadianceCacheSettings {
    /// Number of probes along each axis of the 3D grid.
    pub grid_resolution: [u32; 3],
    /// World-space extent of the cache volume (half-extents).
    pub half_extents: [f32; 3],
    /// Temporal blend factor (0.0 = keep old, 1.0 = fully replace).
    pub temporal_blend: f32,
    /// Maximum world-space distance a probe can contribute.
    pub max_probe_distance: f32,
    /// Enable/disable the cache.
    pub enabled: bool,
}

impl Default for RadianceCacheSettings {
    fn default() -> Self {
        Self {
            grid_resolution: [32, 16, 32],
            half_extents: [64.0, 32.0, 64.0],
            temporal_blend: 0.05,
            max_probe_distance: 16.0,
            enabled: true,
        }
    }
}

/// The CPU-side radiance cache that owns the probe grid.
pub struct RadianceCache {
    pub settings: RadianceCacheSettings,
    /// Flat array of probes: index = z * (res_x * res_y) + y * res_x + x.
    pub probes: Vec<RadianceProbe>,
    /// Center of the cache volume in world space.
    pub center: [f32; 3],
    /// GPU buffer holding the probe SH data (uploaded each frame).
    pub gpu_buffer: Option<wgpu::Buffer>,
    /// Bind group for the cache.
    pub bind_group: Option<wgpu::BindGroup>,
    pub bind_group_layout: Option<wgpu::BindGroupLayout>,
}

impl RadianceCache {
    pub fn new(settings: RadianceCacheSettings) -> Self {
        let count = (settings.grid_resolution[0]
            * settings.grid_resolution[1]
            * settings.grid_resolution[2]) as usize;
        Self {
            probes: vec![RadianceProbe::default(); count],
            center: [0.0; 3],
            gpu_buffer: None,
            bind_group: None,
            bind_group_layout: None,
            settings,
        }
    }

    /// Total number of probes in the grid.
    pub fn probe_count(&self) -> usize {
        self.probes.len()
    }

    /// Convert a world-space position to a probe grid index (clamped).
    pub fn world_to_grid(&self, pos: [f32; 3]) -> [u32; 3] {
        let res = &self.settings.grid_resolution;
        let he = &self.settings.half_extents;
        let mut idx = [0u32; 3];
        for i in 0..3 {
            let local = pos[i] - (self.center[i] - he[i]);
            let cell_size = (2.0 * he[i]) / res[i] as f32;
            let g = (local / cell_size)
                .floor()
                .max(0.0)
                .min((res[i] - 1) as f32) as u32;
            idx[i] = g;
        }
        idx
    }

    /// Convert a grid coordinate to a flat index.
    pub fn grid_to_flat(&self, gx: u32, gy: u32, gz: u32) -> usize {
        let res = &self.settings.grid_resolution;
        (gz * res[0] * res[1] + gy * res[0] + gx) as usize
    }

    /// Get the world-space center of a probe cell.
    pub fn probe_world_pos(&self, gx: u32, gy: u32, gz: u32) -> [f32; 3] {
        let res = &self.settings.grid_resolution;
        let he = &self.settings.half_extents;
        [
            self.center[0] - he[0] + (gx as f32 + 0.5) * (2.0 * he[0]) / res[0] as f32,
            self.center[1] - he[1] + (gy as f32 + 0.5) * (2.0 * he[1]) / res[1] as f32,
            self.center[2] - he[2] + (gz as f32 + 0.5) * (2.0 * he[2]) / res[2] as f32,
        ]
    }

    /// Inject a radiance sample into the nearest probe.
    ///
    /// `direction` should be a unit vector pointing from the surface towards
    /// the light source. `radiance` is the RGB energy.
    pub fn inject(&mut self, world_pos: [f32; 3], direction: [f32; 3], radiance: [f32; 3]) {
        let grid = self.world_to_grid(world_pos);
        let idx = self.grid_to_flat(grid[0], grid[1], grid[2]);
        if idx >= self.probes.len() {
            return;
        }
        let probe = &mut self.probes[idx];

        // Compute L2 SH basis for the direction.
        let sh = sh_l2_basis(direction);

        let blend = self.settings.temporal_blend;
        #[allow(clippy::needless_range_loop)]
        for c in 0..SH_COEFF_COUNT {
            probe.sh_r[c] = probe.sh_r[c] * (1.0 - blend) + radiance[0] * sh[c] * blend;
            probe.sh_g[c] = probe.sh_g[c] * (1.0 - blend) + radiance[1] * sh[c] * blend;
            probe.sh_b[c] = probe.sh_b[c] * (1.0 - blend) + radiance[2] * sh[c] * blend;
        }
        probe.sample_count += 1;
    }

    /// Sample the cache at a world position along a normal direction.
    ///
    /// Returns the trilinearly interpolated irradiance (RGB).
    pub fn sample(&self, world_pos: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
        let res = &self.settings.grid_resolution;
        let he = &self.settings.half_extents;

        // Fractional grid coords.
        let mut fx = [0.0f32; 3];
        for i in 0..3 {
            let local = world_pos[i] - (self.center[i] - he[i]);
            let cell_size = (2.0 * he[i]) / res[i] as f32;
            fx[i] = (local / cell_size - 0.5).max(0.0).min((res[i] - 2) as f32);
        }

        let ix = fx[0] as u32;
        let iy = fx[1] as u32;
        let iz = fx[2] as u32;
        let tx = fx[0].fract();
        let ty = fx[1].fract();
        let tz = fx[2].fract();

        // Trilinear interpolation over 8 probes.
        let sh_basis = sh_l2_basis(normal);
        let mut result = [0.0f32; 3];

        for dz in 0..2u32 {
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let gx = (ix + dx).min(res[0] - 1);
                    let gy = (iy + dy).min(res[1] - 1);
                    let gz = (iz + dz).min(res[2] - 1);
                    let idx = self.grid_to_flat(gx, gy, gz);
                    let probe = &self.probes[idx];

                    let wx = if dx == 0 { 1.0 - tx } else { tx };
                    let wy = if dy == 0 { 1.0 - ty } else { ty };
                    let wz = if dz == 0 { 1.0 - tz } else { tz };
                    let w = wx * wy * wz;

                    #[allow(clippy::needless_range_loop)]
                    for c in 0..SH_COEFF_COUNT {
                        result[0] += probe.sh_r[c] * sh_basis[c] * w;
                        result[1] += probe.sh_g[c] * sh_basis[c] * w;
                        result[2] += probe.sh_b[c] * sh_basis[c] * w;
                    }
                }
            }
        }

        // Clamp to non-negative.
        [result[0].max(0.0), result[1].max(0.0), result[2].max(0.0)]
    }

    /// Initialize GPU resources.
    pub fn init_gpu(&mut self, device: &wgpu::Device) {
        let byte_size = self.probes.len() * std::mem::size_of::<RadianceProbe>();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Radiance Cache Probes"),
            size: byte_size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Radiance Cache BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Radiance Cache BG"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        self.gpu_buffer = Some(buffer);
        self.bind_group_layout = Some(layout);
        self.bind_group = Some(bind_group);
    }

    /// Upload probe data to the GPU.
    pub fn upload(&self, queue: &wgpu::Queue) {
        if let Some(buf) = &self.gpu_buffer {
            let data: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    self.probes.as_ptr() as *const u8,
                    self.probes.len() * std::mem::size_of::<RadianceProbe>(),
                )
            };
            queue.write_buffer(buf, 0, data);
        }
    }

    /// Recenter the cache around a new world position (e.g., the camera).
    ///
    /// Probes that fall outside the new volume are cleared.
    pub fn recenter(&mut self, new_center: [f32; 3]) {
        // Simple approach: if center moved by more than one cell, clear all.
        let res = &self.settings.grid_resolution;
        let he = &self.settings.half_extents;
        let cell_size_x = (2.0 * he[0]) / res[0] as f32;
        let dx = (new_center[0] - self.center[0]).abs();
        let dy = (new_center[1] - self.center[1]).abs();
        let dz = (new_center[2] - self.center[2]).abs();

        if dx > cell_size_x || dy > cell_size_x || dz > cell_size_x {
            // Clear all probes — a scrolling approach could be more efficient.
            for probe in &mut self.probes {
                *probe = RadianceProbe::default();
            }
        }

        self.center = new_center;
    }
}

/// Compute L2 spherical harmonic basis functions for a unit direction.
fn sh_l2_basis(dir: [f32; 3]) -> [f32; SH_COEFF_COUNT] {
    let (x, y, z) = (dir[0], dir[1], dir[2]);
    let c0 = 0.282_095; // 1 / (2 sqrt(pi))
    let c1 = 0.488_603; // sqrt(3) / (2 sqrt(pi))
    let c2 = 1.092_548; // sqrt(15) / (2 sqrt(pi))
    let c3 = 0.315_392; // sqrt(5) / (4 sqrt(pi))
    let c4 = 0.546_274; // sqrt(15) / (4 sqrt(pi))

    [
        c0,
        c1 * y,
        c1 * z,
        c1 * x,
        c2 * x * y,
        c2 * y * z,
        c3 * (3.0 * z * z - 1.0),
        c2 * x * z,
        c4 * (x * x - y * y),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_to_grid_center() {
        let cache = RadianceCache::new(RadianceCacheSettings::default());
        // Center of the default cache is [0,0,0], grid should map to middle.
        let g = cache.world_to_grid([0.0, 0.0, 0.0]);
        assert_eq!(g[0], 16); // half of 32
        assert_eq!(g[2], 16);
    }

    #[test]
    fn inject_and_sample_roundtrip() {
        let mut cache = RadianceCache::new(RadianceCacheSettings {
            grid_resolution: [4, 4, 4],
            half_extents: [4.0, 4.0, 4.0],
            temporal_blend: 1.0, // fully replace for test
            ..Default::default()
        });

        // Inject strong white light from above at center.
        for _ in 0..10 {
            cache.inject([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 1.0]);
        }

        let result = cache.sample([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        // Should be non-zero in all channels.
        assert!(result[0] > 0.0);
        assert!(result[1] > 0.0);
        assert!(result[2] > 0.0);
    }

    #[test]
    fn sh_l2_basis_unit() {
        let basis = sh_l2_basis([0.0, 1.0, 0.0]);
        // L0 coefficient should be ~0.282.
        assert!((basis[0] - 0.282_095).abs() < 0.001);
    }
}
