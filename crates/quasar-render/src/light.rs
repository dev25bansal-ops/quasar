//! Light components for various light types.
//!
//! Supports directional, point, and spot lights that can be collected
//! and passed to shaders for multi-light rendering.

use quasar_math::Vec3;

/// Maximum number of lights supported in the shader.
pub const MAX_LIGHTS: usize = 16;

/// A directional light (sun-like, infinite distance).
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLight {
    /// Direction the light is pointing (should be normalized).
    pub direction: Vec3,
    /// RGB color intensity.
    pub color: Vec3,
    /// Intensity multiplier.
    pub intensity: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: Vec3::new(0.0, -1.0, 0.0),
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: 1.0,
        }
    }
}

/// A point light (omnidirectional, position-based).
#[derive(Debug, Clone, Copy)]
pub struct PointLight {
    /// World position of the light.
    pub position: Vec3,
    /// RGB color intensity.
    pub color: Vec3,
    /// Intensity multiplier.
    pub intensity: f32,
    /// Distance at which light begins to fall off.
    pub range: f32,
    /// Controls how quickly light falls off with distance.
    pub falloff: f32,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: 1.0,
            range: 10.0,
            falloff: 1.0,
        }
    }
}

impl PointLight {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }

    pub fn with_color(mut self, color: Vec3) -> Self {
        self.color = color;
        self
    }

    pub fn with_intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }
}

/// A spot light (cone-shaped, direction and angle based).
#[derive(Debug, Clone, Copy)]
pub struct SpotLight {
    /// World position of the light.
    pub position: Vec3,
    /// Direction the spotlight is pointing (should be normalized).
    pub direction: Vec3,
    /// RGB color intensity.
    pub color: Vec3,
    /// Intensity multiplier.
    pub intensity: f32,
    /// Inner cone angle in radians (fully lit).
    pub inner_angle: f32,
    /// Outer cone angle in radians (edge of falloff).
    pub outer_angle: f32,
    /// Maximum distance the light reaches.
    pub range: f32,
}

impl Default for SpotLight {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            direction: Vec3::new(0.0, -1.0, 0.0),
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: 1.0,
            inner_angle: 0.5,
            outer_angle: 1.0,
            range: 10.0,
        }
    }
}

impl SpotLight {
    pub fn new(position: Vec3, direction: Vec3) -> Self {
        Self {
            position,
            direction,
            ..Default::default()
        }
    }

    pub fn with_angles(mut self, inner: f32, outer: f32) -> Self {
        self.inner_angle = inner;
        self.outer_angle = outer;
        self
    }
}

/// Ambient light settings for global illumination.
#[derive(Debug, Clone, Copy)]
pub struct AmbientLight {
    /// RGB color.
    pub color: Vec3,
    /// Intensity multiplier.
    pub intensity: f32,
}

impl Default for AmbientLight {
    fn default() -> Self {
        Self {
            color: Vec3::new(0.1, 0.1, 0.1),
            intensity: 1.0,
        }
    }
}

/// Uniform structure for passing light data to shaders.
/// Matches the layout expected by the WGSL shader.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LightData {
    /// Position (for point/spot) or direction (for directional).
    pub position_or_direction: [f32; 4],
    /// Color + intensity packed as RGB + unused.
    pub color_intensity: [f32; 4],
    /// Type: 0 = directional, 1 = point, 2 = spot.
    pub light_type: u32,
    /// Inner angle (for spot).
    pub inner_angle: u32,
    /// Outer angle (for spot).
    pub outer_angle: u32,
    /// Range (for point/spot).
    pub range: u32,
    /// Falloff parameter.
    pub falloff: f32,
    /// Padding for alignment.
    pub _padding: [f32; 3],
}

impl LightData {
    pub fn from_directional(light: &DirectionalLight) -> Self {
        Self {
            position_or_direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
            color_intensity: [
                light.color.x * light.intensity,
                light.color.y * light.intensity,
                light.color.z * light.intensity,
                0.0,
            ],
            light_type: 0,
            inner_angle: 0,
            outer_angle: 0,
            range: 0,
            falloff: 0.0,
            _padding: [0.0; 3],
        }
    }

    pub fn from_point(light: &PointLight) -> Self {
        Self {
            position_or_direction: [light.position.x, light.position.y, light.position.z, 1.0],
            color_intensity: [
                light.color.x * light.intensity,
                light.color.y * light.intensity,
                light.color.z * light.intensity,
                0.0,
            ],
            light_type: 1,
            inner_angle: 0,
            outer_angle: 0,
            range: light.range.to_bits(),
            falloff: light.falloff,
            _padding: [0.0; 3],
        }
    }

    pub fn from_spot(light: &SpotLight) -> Self {
        Self {
            position_or_direction: [light.position.x, light.position.y, light.position.z, 1.0],
            color_intensity: [
                light.color.x * light.intensity,
                light.color.y * light.intensity,
                light.color.z * light.intensity,
                0.0,
            ],
            light_type: 2,
            inner_angle: light.inner_angle.to_bits(),
            outer_angle: light.outer_angle.to_bits(),
            range: light.range.to_bits(),
            falloff: 0.0,
            _padding: [0.0; 3],
        }
    }
}

/// Uniform buffer containing all active lights.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LightsUniform {
    /// Array of lights (MAX_LIGHTS entries).
    pub lights: [LightData; MAX_LIGHTS],
    /// Number of active lights.
    pub count: u32,
    /// Padding for alignment.
    pub _padding: [u32; 3],
}

impl Default for LightsUniform {
    fn default() -> Self {
        Self {
            lights: [LightData {
                position_or_direction: [0.0; 4],
                color_intensity: [0.0; 4],
                light_type: 0,
                inner_angle: 0,
                outer_angle: 0,
                range: 0,
                falloff: 0.0,
                _padding: [0.0; 3],
            }; MAX_LIGHTS],
            count: 0,
            _padding: [0; 3],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directional_light_data() {
        let light = DirectionalLight {
            direction: Vec3::new(0.0, -1.0, 0.0),
            color: Vec3::new(1.0, 1.0, 1.0),
            intensity: 2.0,
        };
        let data = LightData::from_directional(&light);
        assert_eq!(data.light_type, 0);
        assert_eq!(data.color_intensity[0], 2.0);
    }

    #[test]
    fn point_light_data() {
        let light = PointLight::new(Vec3::new(1.0, 2.0, 3.0)).with_intensity(0.5);
        let data = LightData::from_point(&light);
        assert_eq!(data.light_type, 1);
        assert_eq!(data.position_or_direction[0], 1.0);
    }

    #[test]
    fn spot_light_data() {
        let light = SpotLight::new(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.0)).with_angles(0.5, 1.0);
        let data = LightData::from_spot(&light);
        assert_eq!(data.light_type, 2);
    }
}
