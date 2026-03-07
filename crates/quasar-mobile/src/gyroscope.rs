// crates/quasar-mobile/src/gyroscope.rs
//! Gyroscope / accelerometer resource.
//!
//! On desktop platforms the values stay at their defaults (zero).
//! On Android / iOS the engine host feeds platform sensor data
//! each frame via [`Gyroscope::update`].

use glam::{Quat, Vec3};

/// Orientation / motion sensor state.
#[derive(Debug, Clone)]
pub struct Gyroscope {
    /// Angular velocity around (x, y, z) in radians/second.
    pub angular_velocity: Vec3,
    /// Linear acceleration *including* gravity (m/s²).
    pub acceleration: Vec3,
    /// Device orientation as a quaternion, if the platform provides it.
    pub orientation: Option<Quat>,
    /// Gravity vector as reported by the sensor fusion layer.
    pub gravity: Vec3,
    /// Whether the hardware sensor is available and active.
    pub available: bool,
}

impl Default for Gyroscope {
    fn default() -> Self {
        Self {
            angular_velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            orientation: None,
            gravity: Vec3::new(0.0, -9.81, 0.0),
            available: false,
        }
    }
}

impl Gyroscope {
    /// Update from raw sensor data (called by platform layer each frame).
    pub fn update(
        &mut self,
        angular_vel: Vec3,
        accel: Vec3,
        orientation: Option<Quat>,
        gravity: Vec3,
    ) {
        self.angular_velocity = angular_vel;
        self.acceleration = accel;
        self.orientation = orientation;
        self.gravity = gravity;
        self.available = true;
    }

    /// Returns the "user" acceleration (total minus gravity).
    pub fn user_acceleration(&self) -> Vec3 {
        self.acceleration - self.gravity
    }

    /// Returns device pitch in radians (-π/2 … π/2) derived from gravity.
    pub fn pitch(&self) -> f32 {
        let g = self.gravity.normalize_or_zero();
        (-g.z).asin()
    }

    /// Returns device roll in radians (-π … π) derived from gravity.
    pub fn roll(&self) -> f32 {
        let g = self.gravity.normalize_or_zero();
        g.x.atan2(-g.y)
    }
}
