//! Vehicle physics simulation.
//!
//! Provides realistic vehicle dynamics with:
//! - Wheel and suspension simulation
//! - Tire friction models
//! - Steering and throttle input
//! - Aerodynamic drag

use glam::{Quat, Vec3};

/// A single wheel on a vehicle.
#[derive(Debug, Clone, Copy)]
pub struct Wheel {
    pub position: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub suspension_length: f32,
    pub suspension_compression: f32,
    pub steering_angle: f32,
    pub rotation_speed: f32,
    pub is_grounded: bool,
    pub ground_normal: Vec3,
}

impl Wheel {
    pub fn new(position: Vec3, radius: f32, suspension_length: f32) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
            radius,
            suspension_length,
            suspension_compression: 0.0,
            steering_angle: 0.0,
            rotation_speed: 0.0,
            is_grounded: false,
            ground_normal: Vec3::Y,
        }
    }

    pub fn world_position(&self, chassis_pos: Vec3, chassis_rot: Quat) -> Vec3 {
        chassis_pos + chassis_rot * self.position
    }

    pub fn contact_point(&self) -> Vec3 {
        self.position - Vec3::Y * (self.suspension_length - self.suspension_compression + self.radius)
    }
}

/// Suspension configuration.
#[derive(Debug, Clone, Copy)]
pub struct SuspensionConfig {
    pub stiffness: f32,
    pub damping: f32,
    pub max_compression: f32,
    pub min_compression: f32,
}

impl Default for SuspensionConfig {
    fn default() -> Self {
        Self {
            stiffness: 50000.0,
            damping: 5000.0,
            max_compression: 0.3,
            min_compression: 0.0,
        }
    }
}

/// Tire configuration.
#[derive(Debug, Clone, Copy)]
pub struct TireConfig {
    pub friction_coefficient: f32,
    pub slip_ratio: f32,
    pub cornering_stiffness: f32,
}

impl Default for TireConfig {
    fn default() -> Self {
        Self {
            friction_coefficient: 1.0,
            slip_ratio: 0.1,
            cornering_stiffness: 10000.0,
        }
    }
}

/// Vehicle configuration.
#[derive(Debug, Clone)]
pub struct VehicleConfig {
    pub mass: f32,
    pub inertia: f32,
    pub wheel_base: f32,
    pub track_width: f32,
    pub center_of_mass_height: f32,
    pub max_steering_angle: f32,
    pub max_brake_torque: f32,
    pub max_engine_torque: f32,
    pub aerodynamic_drag: f32,
    pub rolling_resistance: f32,
    pub suspension: SuspensionConfig,
    pub tire: TireConfig,
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            mass: 1500.0,
            inertia: 3000.0,
            wheel_base: 2.7,
            track_width: 1.8,
            center_of_mass_height: 0.5,
            max_steering_angle: 0.6,
            max_brake_torque: 3000.0,
            max_engine_torque: 500.0,
            aerodynamic_drag: 0.3,
            rolling_resistance: 0.01,
            suspension: SuspensionConfig::default(),
            tire: TireConfig::default(),
        }
    }
}

/// Vehicle input state.
#[derive(Debug, Clone, Copy, Default)]
pub struct VehicleInput {
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
    pub handbrake: bool,
}

/// A simulated vehicle.
#[derive(Debug, Clone)]
pub struct Vehicle {
    pub position: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub wheels: Vec<Wheel>,
    pub config: VehicleConfig,
    pub input: VehicleInput,
    pub speed: f32,
    pub engine_rpm: f32,
    pub gear: i32,
}

impl Vehicle {
    pub fn new(config: VehicleConfig) -> Self {
        let half_wheel_base = config.wheel_base * 0.5;
        let half_track = config.track_width * 0.5;

        let wheels = vec![
            Wheel::new(Vec3::new(-half_track, 0.0, half_wheel_base), 0.35, 0.4),
            Wheel::new(Vec3::new(half_track, 0.0, half_wheel_base), 0.35, 0.4),
            Wheel::new(Vec3::new(-half_track, 0.0, -half_wheel_base), 0.35, 0.4),
            Wheel::new(Vec3::new(half_track, 0.0, -half_wheel_base), 0.35, 0.4),
        ];

        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            wheels,
            config,
            input: VehicleInput::default(),
            speed: 0.0,
            engine_rpm: 1000.0,
            gear: 1,
        }
    }

    pub fn set_position(&mut self, pos: Vec3) {
        self.position = pos;
    }

    pub fn set_rotation(&mut self, rot: Quat) {
        self.rotation = rot;
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation * -Vec3::Z
    }

    pub fn right(&self) -> Vec3 {
        self.rotation * Vec3::X
    }

    pub fn up(&self) -> Vec3 {
        self.rotation * Vec3::Y
    }

    pub fn local_velocity(&self) -> Vec3 {
        let forward = self.forward();
        let right = self.right();
        Vec3::new(
            self.velocity.dot(right),
            0.0,
            self.velocity.dot(forward),
        )
    }

    pub fn step(&mut self, dt: f32, ground_height: impl Fn(Vec3) -> f32) {
        self.update_steering(dt);
        self.update_suspension(&ground_height);
        self.update_wheels(dt);
        self.update_dynamics(dt);
        self.update_position(dt);
    }

    fn update_steering(&mut self, dt: f32) {
        let target_angle = self.input.steering * self.config.max_steering_angle;
        let steering_rate = 3.0;
        
        for wheel in self.wheels.iter_mut().take(2) {
            wheel.steering_angle += (target_angle - wheel.steering_angle) * steering_rate * dt;
        }
    }

    fn update_suspension(&mut self, ground_height: &impl Fn(Vec3) -> f32) {
        for wheel in &mut self.wheels {
            let world_pos = wheel.world_position(self.position, self.rotation);
            let ground_y = ground_height(world_pos);
            
            let suspension_bottom = world_pos.y - wheel.suspension_length;
            let penetration = suspension_bottom - ground_y;
            
            wheel.suspension_compression = penetration.clamp(
                self.config.suspension.min_compression,
                self.config.suspension.max_compression,
            );
            
            wheel.is_grounded = wheel.suspension_compression > 0.0;
            wheel.ground_normal = Vec3::Y;
        }
    }

    fn update_wheels(&mut self, dt: f32) {
        let forward_speed = self.local_velocity().z;
        
        for (i, wheel) in self.wheels.iter_mut().enumerate() {
            if wheel.is_grounded {
                let wheel_speed = if i < 2 {
                    forward_speed / (wheel.steering_angle.cos() + 0.001)
                } else {
                    forward_speed
                };
                
                wheel.rotation_speed = wheel_speed / (2.0 * std::f32::consts::PI * wheel.radius);
            } else {
                wheel.rotation_speed *= 0.99;
            }
            
            let rotation_delta = wheel.rotation_speed * dt * 2.0 * std::f32::consts::PI;
            wheel.rotation = Quat::from_rotation_x(rotation_delta) * wheel.rotation;
        }
    }

    fn update_dynamics(&mut self, dt: f32) {
        let forward = self.forward();
        let right = self.right();
        let local_vel = self.local_velocity();
        
        self.speed = self.velocity.length();

        let mut total_force = Vec3::ZERO;
        let mut total_torque = Vec3::ZERO;

        let gravity = Vec3::new(0.0, -9.81, 0.0) * self.config.mass;
        total_force += gravity;

        let drag = -self.velocity * self.velocity.length() * self.config.aerodynamic_drag;
        total_force += drag;

        let rolling = -self.velocity.normalize() * self.config.rolling_resistance * self.config.mass * 9.81;
        if rolling.is_finite() {
            total_force += rolling;
        }

        for (i, wheel) in self.wheels.iter().enumerate() {
            if !wheel.is_grounded {
                continue;
            }

            let spring_force = wheel.suspension_compression * self.config.suspension.stiffness;
            let spring_dir = wheel.ground_normal;
            total_force += spring_dir * spring_force;

            let suspension_velocity = wheel.suspension_compression - wheel.suspension_length;
            let damping_force = suspension_velocity * self.config.suspension.damping;
            total_force += spring_dir * damping_force;

            let wheel_forward = if i < 2 {
                Quat::from_rotation_y(wheel.steering_angle) * forward
            } else {
                forward
            };

            let wheel_speed = local_vel.z;
            let slip_angle = if wheel_speed.abs() > 0.1 {
                (local_vel.x / wheel_speed.abs()).atan()
            } else {
                0.0
            };

            let lateral_force = -slip_angle * self.config.tire.cornering_stiffness * right;
            total_force += lateral_force;

            if self.input.throttle > 0.0 {
                let engine_force = self.input.throttle * self.config.max_engine_torque / wheel.radius;
                total_force += wheel_forward * engine_force;
            }

            if self.input.brake > 0.0 {
                let brake_force = self.input.brake * self.config.max_brake_torque / wheel.radius;
                let brake_dir = -self.velocity.normalize();
                if brake_dir.is_finite() {
                    total_force += brake_dir * brake_force;
                }
            }

            if self.input.handbrake && i >= 2 {
                let brake_force = self.config.max_brake_torque / wheel.radius;
                let brake_dir = -self.velocity.normalize();
                if brake_dir.is_finite() {
                    total_force += brake_dir * brake_force;
                }
            }
        }

        if self.input.steering.abs() > 0.01 && self.speed > 0.1 {
            let turning_radius = self.config.wheel_base / self.input.steering.abs().tan();
            let angular_vel = self.speed / turning_radius * self.input.steering.signum();
            total_torque = Vec3::new(0.0, angular_vel * self.config.inertia, 0.0);
        }

        let acceleration = total_force / self.config.mass;
        self.velocity += acceleration * dt;

        let angular_acceleration = total_torque / self.config.inertia;
        self.angular_velocity += angular_acceleration * dt;
        self.angular_velocity *= 0.98;
    }

    fn update_position(&mut self, dt: f32) {
        self.position += self.velocity * dt;

        let angular_vel = self.angular_velocity.length();
        if angular_vel > 0.001 {
            let axis = self.angular_velocity.normalize();
            let angle = angular_vel * dt;
            let delta_rot = Quat::from_axis_angle(axis, angle);
            self.rotation = delta_rot * self.rotation;
        }
    }

    pub fn get_wheel_world_transforms(&self) -> Vec<(Vec3, Quat)> {
        self.wheels.iter().map(|wheel| {
            let world_pos = self.position + self.rotation * wheel.position
                - Vec3::Y * wheel.suspension_compression;
            let steering_rot = Quat::from_rotation_y(wheel.steering_angle);
            let world_rot = self.rotation * steering_rot * wheel.rotation;
            (world_pos, world_rot)
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_new() {
        let w = Wheel::new(Vec3::X, 0.35, 0.4);
        assert!((w.position - Vec3::X).length() < 0.001);
        assert!((w.radius - 0.35).abs() < 0.001);
        assert!(!w.is_grounded);
    }

    #[test]
    fn suspension_config_default() {
        let s = SuspensionConfig::default();
        assert!(s.stiffness > 0.0);
        assert!(s.damping > 0.0);
    }

    #[test]
    fn tire_config_default() {
        let t = TireConfig::default();
        assert!((t.friction_coefficient - 1.0).abs() < 0.001);
    }

    #[test]
    fn vehicle_config_default() {
        let c = VehicleConfig::default();
        assert!((c.mass - 1500.0).abs() < 0.001);
    }

    #[test]
    fn vehicle_gear_default() {
        let v = Vehicle::new(VehicleConfig::default());
        assert_eq!(v.gear, 1);
    }

    #[test]
    fn vehicle_new() {
        let v = Vehicle::new(VehicleConfig::default());
        assert_eq!(v.wheels.len(), 4);
        assert!(v.position.length() < 0.001);
    }

    #[test]
    fn vehicle_forward() {
        let v = Vehicle::new(VehicleConfig::default());
        let fwd = v.forward();
        assert!((fwd + Vec3::Z).length() < 0.001);
    }

    #[test]
    fn vehicle_set_position() {
        let mut v = Vehicle::new(VehicleConfig::default());
        v.set_position(Vec3::new(10.0, 5.0, 3.0));
        assert!((v.position - Vec3::new(10.0, 5.0, 3.0)).length() < 0.001);
    }

    #[test]
    fn vehicle_step() {
        let mut v = Vehicle::new(VehicleConfig::default());
        v.step(0.016, |p| 0.0);
        assert!(v.wheels.len() == 4);
    }

    #[test]
    fn vehicle_throttle() {
        let mut v = Vehicle::new(VehicleConfig::default());
        v.input.throttle = 1.0;
        v.step(0.016, |p| 0.0);
        v.step(0.016, |p| 0.0);
        assert!(v.speed > 0.0);
    }

    #[test]
    fn vehicle_steering() {
        let mut v = Vehicle::new(VehicleConfig::default());
        v.input.steering = 1.0;
        v.input.throttle = 0.5;
        for _ in 0..100 {
            v.step(0.016, |p| 0.0);
        }
        assert!(v.wheels[0].steering_angle > 0.0);
    }

    #[test]
    fn vehicle_local_velocity() {
        let mut v = Vehicle::new(VehicleConfig::default());
        v.velocity = Vec3::new(0.0, 0.0, -10.0);
        let local = v.local_velocity();
        assert!((local.z - 10.0).abs() < 0.1);
    }

    #[test]
    fn vehicle_wheel_transforms() {
        let v = Vehicle::new(VehicleConfig::default());
        let transforms = v.get_wheel_world_transforms();
        assert_eq!(transforms.len(), 4);
    }
}
