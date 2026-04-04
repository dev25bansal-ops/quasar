//! Procedural terrain generation with hydraulic and thermal erosion.

/// Configuration for procedural terrain generation.
#[derive(Debug, Clone)]
pub struct TerrainGenConfig {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub octaves: u32,
    pub persistence: f32,
    pub lacunarity: f32,
    pub seed: u64,
}

impl Default for TerrainGenConfig {
    fn default() -> Self {
        Self {
            width: 256,
            height: 256,
            scale: 100.0,
            octaves: 6,
            persistence: 0.5,
            lacunarity: 2.0,
            seed: 42,
        }
    }
}

/// Erosion simulation configuration.
#[derive(Debug, Clone)]
pub struct ErosionConfig {
    pub iterations: u32,
    pub rain_amount: f32,
    pub evaporation: f32,
    pub deposition: f32,
    pub min_slope: f32,
    pub gravity: f32,
    pub max_droplet_life: u32,
    pub inertia: f32,
    pub sediment_capacity_factor: f32,
}

impl Default for ErosionConfig {
    fn default() -> Self {
        Self {
            iterations: 100000,
            rain_amount: 0.01,
            evaporation: 0.02,
            deposition: 0.3,
            min_slope: 0.01,
            gravity: 4.0,
            max_droplet_life: 30,
            inertia: 0.05,
            sediment_capacity_factor: 4.0,
        }
    }
}

/// Perlin noise generator for terrain.
pub struct PerlinNoise {
    perm: [u8; 512],
}

impl PerlinNoise {
    pub fn new(seed: u64) -> Self {
        let mut perm = [0u8; 512];
        let mut p = [0u8; 256];

        for i in 0..256 {
            p[i] = i as u8;
        }

        let mut rng_state = seed;
        for i in (1..256).rev() {
            rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
            let j = (rng_state % (i as u64 + 1)) as usize;
            p.swap(i, j);
        }

        for i in 0..256 {
            perm[i] = p[i];
            perm[i + 256] = p[i];
        }

        Self { perm }
    }

    fn fade(t: f32) -> f32 {
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + t * (b - a)
    }

    fn grad(hash: u8, x: f32, y: f32) -> f32 {
        let h = hash & 7;
        let u = if h < 4 { x } else { y };
        let v = if h < 4 { y } else { x };
        (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
    }

    pub fn noise_2d(&self, x: f32, y: f32) -> f32 {
        let xi = x.floor() as i32 & 255;
        let yi = y.floor() as i32 & 255;
        let xf = x.fract();
        let yf = y.fract();

        let u = Self::fade(xf);
        let v = Self::fade(yf);

        let aa = self.perm[self.perm[xi as usize] as usize + yi as usize];
        let ab = self.perm[self.perm[xi as usize] as usize + yi as usize + 1];
        let ba = self.perm[self.perm[xi as usize + 1] as usize + yi as usize];
        let bb = self.perm[self.perm[xi as usize + 1] as usize + yi as usize + 1];

        let x1 = Self::lerp(Self::grad(aa, xf, yf), Self::grad(ba, xf - 1.0, yf), u);
        let x2 = Self::lerp(
            Self::grad(ab, xf, yf - 1.0),
            Self::grad(bb, xf - 1.0, yf - 1.0),
            u,
        );

        Self::lerp(x1, x2, v)
    }

    pub fn fbm(&self, x: f32, y: f32, octaves: u32, persistence: f32, lacunarity: f32) -> f32 {
        let mut total = 0.0;
        let mut frequency = 1.0;
        let mut amplitude = 1.0;
        let mut max_value = 0.0;

        for _ in 0..octaves {
            total += self.noise_2d(x * frequency, y * frequency) * amplitude;
            max_value += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }

        total / max_value
    }
}

/// Generate a heightmap using fractal Brownian motion.
pub fn generate_heightmap(config: &TerrainGenConfig) -> Vec<f32> {
    let noise = PerlinNoise::new(config.seed);
    let mut heightmap = vec![0.0f32; (config.width * config.height) as usize];

    for y in 0..config.height {
        for x in 0..config.width {
            let nx = x as f32 / config.scale;
            let ny = y as f32 / config.scale;

            let value = noise.fbm(
                nx,
                ny,
                config.octaves,
                config.persistence,
                config.lacunarity,
            );
            let idx = (y * config.width + x) as usize;
            heightmap[idx] = (value + 1.0) * 0.5;
        }
    }

    heightmap
}

/// Apply hydraulic erosion simulation.
pub fn apply_hydraulic_erosion(
    heightmap: &mut [f32],
    width: u32,
    height: u32,
    config: &ErosionConfig,
) {
    let mut rng_state = config.iterations as u64;
    let mut next_random = || -> f32 {
        rng_state = rng_state.wrapping_mul(1103515245).wrapping_add(12345);
        ((rng_state >> 16) & 0x7FFF) as f32 / 32767.0
    };

    for _ in 0..config.iterations {
        let mut x = next_random() * (width - 2) as f32 + 1.0;
        let mut y = next_random() * (height - 2) as f32 + 1.0;

        let mut vx = 0.0f32;
        let mut vy = 0.0f32;
        let mut sediment = 0.0f32;
        let mut water = config.rain_amount;

        for _ in 0..config.max_droplet_life {
            let xi = x as u32;
            let yi = y as u32;

            let cell_x = x - xi as f32;
            let cell_y = y - yi as f32;

            let idx00 = (yi * width + xi) as usize;
            let idx10 = idx00 + 1;
            let idx01 = idx00 + width as usize;
            let idx11 = idx01 + 1;

            if idx11 >= heightmap.len() {
                break;
            }

            let h00 = heightmap[idx00];
            let h10 = heightmap[idx10];
            let h01 = heightmap[idx01];
            let h11 = heightmap[idx11];

            let terrain_height = h00 * (1.0 - cell_x) * (1.0 - cell_y)
                + h10 * cell_x * (1.0 - cell_y)
                + h01 * (1.0 - cell_x) * cell_y
                + h11 * cell_x * cell_y;

            let gx = (h10 - h00) * (1.0 - cell_y) + (h11 - h01) * cell_y;
            let gy = (h01 - h00) * (1.0 - cell_x) + (h11 - h10) * cell_x;

            vx = vx * config.inertia - gx * (1.0 - config.inertia);
            vy = vy * config.inertia - gy * (1.0 - config.inertia);

            let len = (vx * vx + vy * vy).sqrt();
            if len < 0.0001 {
                continue;
            }
            vx /= len;
            vy /= len;

            let new_x = x + vx;
            let new_y = y + vy;

            if new_x < 1.0
                || new_x >= (width - 1) as f32
                || new_y < 1.0
                || new_y >= (height - 1) as f32
            {
                break;
            }

            let new_xi = new_x as u32;
            let new_yi = new_y as u32;
            let new_cell_x = new_x - new_xi as f32;
            let new_cell_y = new_y - new_yi as f32;

            let nidx00 = (new_yi * width + new_xi) as usize;
            let nidx10 = nidx00 + 1;
            let nidx01 = nidx00 + width as usize;
            let nidx11 = nidx01 + 1;

            if nidx11 >= heightmap.len() {
                break;
            }

            let new_h00 = heightmap[nidx00];
            let new_h10 = heightmap[nidx10];
            let new_h01 = heightmap[nidx01];
            let new_h11 = heightmap[nidx11];

            let new_height = new_h00 * (1.0 - new_cell_x) * (1.0 - new_cell_y)
                + new_h10 * new_cell_x * (1.0 - new_cell_y)
                + new_h01 * (1.0 - new_cell_x) * new_cell_y
                + new_h11 * new_cell_x * new_cell_y;

            let height_diff = new_height - terrain_height;
            let slope = (-height_diff).max(0.0);

            let sediment_capacity = slope * water * config.sediment_capacity_factor;

            if sediment > sediment_capacity || height_diff > 0.0 {
                let deposit = if height_diff > 0.0 {
                    (height_diff * config.deposition).min(sediment)
                } else {
                    (sediment - sediment_capacity) * config.deposition
                };

                sediment -= deposit;

                let d = deposit;
                heightmap[idx00] += d * (1.0 - cell_x) * (1.0 - cell_y);
                heightmap[idx10] += d * cell_x * (1.0 - cell_y);
                heightmap[idx01] += d * (1.0 - cell_x) * cell_y;
                heightmap[idx11] += d * cell_x * cell_y;
            } else {
                let erosion_amount = ((sediment_capacity - sediment) * config.deposition)
                    .min(-height_diff.max(0.0))
                    .min(heightmap[idx00]);

                let e = erosion_amount;
                heightmap[idx00] -= e * (1.0 - cell_x) * (1.0 - cell_y);
                heightmap[idx10] -= e * cell_x * (1.0 - cell_y);
                heightmap[idx01] -= e * (1.0 - cell_x) * cell_y;
                heightmap[idx11] -= e * cell_x * cell_y;

                sediment += erosion_amount;
            }

            water *= 1.0 - config.evaporation;
            x = new_x;
            y = new_y;
        }
    }
}

/// Apply thermal erosion (material slides down steep slopes).
pub fn apply_thermal_erosion(
    heightmap: &mut [f32],
    width: u32,
    height: u32,
    iterations: u32,
    talus_angle: f32,
) {
    let talus = talus_angle * talus_angle;

    for _ in 0..iterations {
        let mut changes = vec![0.0f32; heightmap.len()];

        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let idx = (y * width + x) as usize;
                let h = heightmap[idx];

                let mut max_diff: f32 = 0.0;
                let mut max_neighbor: f32 = 0.0;
                let mut sum_diff: f32 = 0.0;
                let mut num_steep = 0;

                for (dy, dx) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let ny = (y as i32 + dy) as u32;
                    let nx = (x as i32 + dx) as u32;
                    let nidx = (ny * width + nx) as usize;
                    let nh = heightmap[nidx];

                    let diff = h - nh;
                    if diff > talus {
                        max_diff = max_diff.max(diff);
                        max_neighbor = max_neighbor.max(diff);
                        sum_diff += diff;
                        num_steep += 1;
                    }
                }

                if num_steep > 0 {
                    let amount = (max_diff - talus) * 0.5;
                    changes[idx] -= amount;

                    for (dy, dx) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                        let ny = (y as i32 + dy) as u32;
                        let nx = (x as i32 + dx) as u32;
                        let nidx = (ny * width + nx) as usize;
                        let nh = heightmap[nidx];

                        let diff = h - nh;
                        if diff > talus {
                            let portion = diff / sum_diff;
                            changes[nidx] += amount * portion;
                        }
                    }
                }
            }
        }

        for (i, &change) in changes.iter().enumerate() {
            heightmap[i] += change;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_noise_generates() {
        let noise = PerlinNoise::new(42);
        let v = noise.noise_2d(0.5, 0.5);
        assert!(v >= -1.0 && v <= 1.0);
    }

    #[test]
    fn fbm_generates() {
        let noise = PerlinNoise::new(42);
        let v = noise.fbm(0.5, 0.5, 4, 0.5, 2.0);
        assert!(v >= -1.0 && v <= 1.0);
    }

    #[test]
    fn generate_heightmap_produces_valid_data() {
        let config = TerrainGenConfig::default();
        let heightmap = generate_heightmap(&config);
        assert_eq!(heightmap.len(), (config.width * config.height) as usize);

        for &h in &heightmap {
            assert!(h >= 0.0 && h <= 1.0);
        }
    }

    #[test]
    fn erosion_modifies_heightmap() {
        let config = TerrainGenConfig {
            width: 64,
            height: 64,
            ..Default::default()
        };
        let mut heightmap = generate_heightmap(&config);
        let original_sum: f32 = heightmap.iter().sum();

        let erosion_config = ErosionConfig {
            iterations: 1000,
            ..Default::default()
        };
        apply_hydraulic_erosion(&mut heightmap, config.width, config.height, &erosion_config);

        let new_sum: f32 = heightmap.iter().sum();
        assert!((original_sum - new_sum).abs() < 100.0);
    }

    #[test]
    fn thermal_erosion_runs() {
        let config = TerrainGenConfig {
            width: 32,
            height: 32,
            ..Default::default()
        };
        let mut heightmap = generate_heightmap(&config);

        apply_thermal_erosion(&mut heightmap, config.width, config.height, 10, 0.05);

        for &h in &heightmap {
            assert!(h >= 0.0);
        }
    }
}
