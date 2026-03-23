//! Light components for various light types.
//!
//! Supports directional, point, and spot lights that can be collected
//! and passed to shaders for multi-light rendering.

use quasar_math::Vec3;

/// Maximum number of lights supported in the shader.
pub const MAX_LIGHTS: usize = 256;

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
/// Matches the layout expected by the WGSL shader (4 × `vec4<f32>`).
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LightData {
    /// Position (xyz, w=1) for point/spot, or unused (0,0,0,0) for directional.
    pub position: [f32; 4],
    /// Color (rgb) and intensity (a).
    pub color: [f32; 4],
    /// Direction (xyz) for directional/spot lights (w unused).
    pub direction: [f32; 4],
    /// x = light_type (0.0=dir, 1.0=point, 2.0=spot).
    /// Point: y=range. Spot: y=inner_cutoff, z=outer_cutoff, w=range.
    pub params: [f32; 4],
}
impl LightData {
    pub fn from_directional(light: &DirectionalLight) -> Self {
        Self {
            position: [0.0, 0.0, 0.0, 0.0],
            color: [
                light.color.x * light.intensity,
                light.color.y * light.intensity,
                light.color.z * light.intensity,
                light.intensity,
            ],
            direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
            params: [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn from_point(light: &PointLight) -> Self {
        Self {
            position: [light.position.x, light.position.y, light.position.z, 1.0],
            color: [
                light.color.x * light.intensity,
                light.color.y * light.intensity,
                light.color.z * light.intensity,
                light.intensity,
            ],
            direction: [0.0, 0.0, 0.0, 0.0],
            params: [1.0, light.range, 0.0, 0.0],
        }
    }

    pub fn from_spot(light: &SpotLight) -> Self {
        Self {
            position: [light.position.x, light.position.y, light.position.z, 1.0],
            color: [
                light.color.x * light.intensity,
                light.color.y * light.intensity,
                light.color.z * light.intensity,
                light.intensity,
            ],
            direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
            params: [2.0, light.inner_angle, light.outer_angle, light.range],
        }
    }
}

/// Storage buffer containing all active lights.
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct LightsUniform {
    /// Array of lights (MAX_LIGHTS entries).
    pub lights: [LightData; MAX_LIGHTS],
    /// Number of active lights.
    pub count: u32,
    /// Padding for alignment.
    pub _padding: [u32; 3],
    /// Ambient color (rgb) and intensity (a).
    pub ambient: [f32; 4],
    /// Additional padding for WGSL alignment.
    pub _padding2: [f32; 4],
    /// Additional padding for WGSL struct alignment.
    pub _padding3: [f32; 4],
}

impl Default for LightsUniform {
    fn default() -> Self {
        Self {
            lights: [LightData {
                position: [0.0; 4],
                color: [0.0; 4],
                direction: [0.0; 4],
                params: [0.0; 4],
            }; MAX_LIGHTS],
            count: 0,
            _padding: [0; 3],
            ambient: [0.1, 0.1, 0.1, 1.0],
            _padding2: [0.0; 4],
            _padding3: [0.0; 4],
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
        assert_eq!(data.params[0], 0.0); // light_type = directional
        assert_eq!(data.color[0], 2.0);
        assert_eq!(data.direction[1], -1.0);
    }

    #[test]
    fn point_light_data() {
        let light = PointLight::new(Vec3::new(1.0, 2.0, 3.0)).with_intensity(0.5);
        let data = LightData::from_point(&light);
        assert_eq!(data.params[0], 1.0); // light_type = point
        assert_eq!(data.position[0], 1.0);
        assert_eq!(data.params[1], 10.0); // default range
    }

    #[test]
    fn spot_light_data() {
        let light = SpotLight::new(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.0)).with_angles(0.5, 1.0);
        let data = LightData::from_spot(&light);
        assert_eq!(data.params[0], 2.0); // light_type = spot
        assert_eq!(data.direction[1], -1.0);
        assert_eq!(data.params[1], 0.5); // inner_angle
        assert_eq!(data.params[2], 1.0); // outer_angle
    }
}
