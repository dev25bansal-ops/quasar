//! Steering Behaviors - Autonomous Movement for AI Agents

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SteeringOutput {
    pub linear: [f32; 3],
    pub angular: f32,
}

impl SteeringOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_linear(mut self, linear: [f32; 3]) -> Self {
        self.linear = linear;
        self
    }

    pub fn with_angular(mut self, angular: f32) -> Self {
        self.angular = angular;
        self
    }

    pub fn is_zero(&self) -> bool {
        self.linear == [0.0, 0.0, 0.0] && self.angular == 0.0
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Kinematic {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub orientation: f32,
    pub rotation: f32,
}

impl Kinematic {
    pub fn new(position: [f32; 3]) -> Self {
        Self {
            position,
            velocity: [0.0, 0.0, 0.0],
            orientation: 0.0,
            rotation: 0.0,
        }
    }

    pub fn update(&mut self, steering: &SteeringOutput, max_speed: f32, dt: f32) {
        self.velocity[0] += steering.linear[0] * dt;
        self.velocity[1] += steering.linear[1] * dt;
        self.velocity[2] += steering.linear[2] * dt;

        let speed =
            (self.velocity[0].powi(2) + self.velocity[1].powi(2) + self.velocity[2].powi(2)).sqrt();
        if speed > max_speed {
            let scale = max_speed / speed;
            self.velocity[0] *= scale;
            self.velocity[1] *= scale;
            self.velocity[2] *= scale;
        }

        self.position[0] += self.velocity[0] * dt;
        self.position[1] += self.velocity[1] * dt;
        self.position[2] += self.velocity[2] * dt;

        self.orientation += self.rotation * dt;
        self.rotation += steering.angular * dt;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteeringBehavior {
    max_acceleration: f32,
    max_angular_acceleration: f32,
    max_speed: f32,
    arrival_radius: f32,
    slow_radius: f32,
    time_to_target: f32,
}

impl Default for SteeringBehavior {
    fn default() -> Self {
        Self {
            max_acceleration: 10.0,
            max_angular_acceleration: 5.0,
            max_speed: 5.0,
            arrival_radius: 1.0,
            slow_radius: 5.0,
            time_to_target: 0.1,
        }
    }
}

impl SteeringBehavior {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seek(&self, character: &Kinematic, target: [f32; 3]) -> SteeringOutput {
        let direction = [
            target[0] - character.position[0],
            target[1] - character.position[1],
            target[2] - character.position[2],
        ];

        let distance = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();

        if distance < 0.001 {
            return SteeringOutput::new();
        }

        let linear = [
            direction[0] / distance * self.max_acceleration,
            direction[1] / distance * self.max_acceleration,
            direction[2] / distance * self.max_acceleration,
        ];

        SteeringOutput::new().with_linear(linear)
    }

    pub fn flee(&self, character: &Kinematic, target: [f32; 3]) -> SteeringOutput {
        let direction = [
            character.position[0] - target[0],
            character.position[1] - target[1],
            character.position[2] - target[2],
        ];

        let distance = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();

        if distance < 0.001 {
            return SteeringOutput::new();
        }

        let linear = [
            direction[0] / distance * self.max_acceleration,
            direction[1] / distance * self.max_acceleration,
            direction[2] / distance * self.max_acceleration,
        ];

        SteeringOutput::new().with_linear(linear)
    }

    pub fn arrive(&self, character: &Kinematic, target: [f32; 3]) -> SteeringOutput {
        let direction = [
            target[0] - character.position[0],
            target[1] - character.position[1],
            target[2] - character.position[2],
        ];

        let distance = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();

        if distance < self.arrival_radius {
            return SteeringOutput::new();
        }

        let target_speed = if distance > self.slow_radius {
            self.max_speed
        } else {
            self.max_speed * distance / self.slow_radius
        };

        let target_velocity = [
            direction[0] / distance * target_speed,
            direction[1] / distance * target_speed,
            direction[2] / distance * target_speed,
        ];

        let linear = [
            (target_velocity[0] - character.velocity[0]) / self.time_to_target,
            (target_velocity[1] - character.velocity[1]) / self.time_to_target,
            (target_velocity[2] - character.velocity[2]) / self.time_to_target,
        ];

        let accel = (linear[0].powi(2) + linear[1].powi(2) + linear[2].powi(2)).sqrt();
        let linear = if accel > self.max_acceleration {
            [
                linear[0] / accel * self.max_acceleration,
                linear[1] / accel * self.max_acceleration,
                linear[2] / accel * self.max_acceleration,
            ]
        } else {
            linear
        };

        SteeringOutput::new().with_linear(linear)
    }

    pub fn pursue(&self, character: &Kinematic, target: &Kinematic) -> SteeringOutput {
        let direction = [
            target.position[0] - character.position[0],
            target.position[1] - character.position[1],
            target.position[2] - character.position[2],
        ];

        let distance = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();
        let speed = (character.velocity[0].powi(2)
            + character.velocity[1].powi(2)
            + character.velocity[2].powi(2))
        .sqrt();

        let prediction = if speed <= 0.001 {
            0.0
        } else {
            distance / speed
        };

        let predicted_target = [
            target.position[0] + target.velocity[0] * prediction,
            target.position[1] + target.velocity[1] * prediction,
            target.position[2] + target.velocity[2] * prediction,
        ];

        self.seek(character, predicted_target)
    }

    pub fn evade(&self, character: &Kinematic, target: &Kinematic) -> SteeringOutput {
        let direction = [
            target.position[0] - character.position[0],
            target.position[1] - character.position[1],
            target.position[2] - character.position[2],
        ];

        let distance = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();
        let speed = (character.velocity[0].powi(2)
            + character.velocity[1].powi(2)
            + character.velocity[2].powi(2))
        .sqrt();

        let prediction = if speed <= 0.001 {
            0.0
        } else {
            distance / speed
        };

        let predicted_target = [
            target.position[0] + target.velocity[0] * prediction,
            target.position[1] + target.velocity[1] * prediction,
            target.position[2] + target.velocity[2] * prediction,
        ];

        self.flee(character, predicted_target)
    }

    pub fn wander(
        &self,
        character: &Kinematic,
        wander_offset: f32,
        wander_radius: f32,
        wander_rate: f32,
    ) -> SteeringOutput {
        let target = [
            character.position[0] + character.orientation.cos() * wander_offset,
            character.position[1],
            character.position[2] - character.orientation.sin() * wander_offset,
        ];

        let target = [
            target[0] + wander_radius * wander_offset.cos(),
            target[1],
            target[2] + wander_radius * wander_offset.sin(),
        ];

        let steering = self.seek(character, target);
        SteeringOutput::new()
            .with_linear(steering.linear)
            .with_angular(wander_rate * (rand_float() - 0.5))
    }

    pub fn separation(
        &self,
        character: &Kinematic,
        others: &[Kinematic],
        threshold: f32,
    ) -> SteeringOutput {
        let mut steering = SteeringOutput::new();
        let mut count = 0;

        for other in others {
            let direction = [
                character.position[0] - other.position[0],
                character.position[1] - other.position[1],
                character.position[2] - other.position[2],
            ];

            let distance =
                (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();

            if distance > 0.0 && distance < threshold {
                let strength = self.max_acceleration * (threshold - distance) / threshold;
                steering.linear[0] += direction[0] / distance * strength;
                steering.linear[1] += direction[1] / distance * strength;
                steering.linear[2] += direction[2] / distance * strength;
                count += 1;
            }
        }

        if count > 0 {
            steering.linear[0] /= count as f32;
            steering.linear[1] /= count as f32;
            steering.linear[2] /= count as f32;
        }

        steering
    }

    pub fn alignment(
        &self,
        character: &Kinematic,
        others: &[Kinematic],
        threshold: f32,
    ) -> SteeringOutput {
        let mut avg_velocity = [0.0f32; 3];
        let mut count = 0;

        for other in others {
            let dx = other.position[0] - character.position[0];
            let dy = other.position[1] - character.position[1];
            let dz = other.position[2] - character.position[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();

            if distance > 0.0 && distance < threshold {
                avg_velocity[0] += other.velocity[0];
                avg_velocity[1] += other.velocity[1];
                avg_velocity[2] += other.velocity[2];
                count += 1;
            }
        }

        if count > 0 {
            avg_velocity[0] /= count as f32;
            avg_velocity[1] /= count as f32;
            avg_velocity[2] /= count as f32;

            let steering = [
                (avg_velocity[0] - character.velocity[0]) / self.time_to_target,
                (avg_velocity[1] - character.velocity[1]) / self.time_to_target,
                (avg_velocity[2] - character.velocity[2]) / self.time_to_target,
            ];

            return SteeringOutput::new().with_linear(steering);
        }

        SteeringOutput::new()
    }

    pub fn cohesion(
        &self,
        character: &Kinematic,
        others: &[Kinematic],
        threshold: f32,
    ) -> SteeringOutput {
        let mut center = [0.0f32; 3];
        let mut count = 0;

        for other in others {
            let dx = other.position[0] - character.position[0];
            let dy = other.position[1] - character.position[1];
            let dz = other.position[2] - character.position[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();

            if distance > 0.0 && distance < threshold {
                center[0] += other.position[0];
                center[1] += other.position[1];
                center[2] += other.position[2];
                count += 1;
            }
        }

        if count > 0 {
            center[0] /= count as f32;
            center[1] /= count as f32;
            center[2] /= count as f32;

            return self.seek(character, center);
        }

        SteeringOutput::new()
    }
}

fn rand_float() -> f32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    nanos as f32 / u32::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_seek() {
        let behavior = SteeringBehavior::new();
        let character = Kinematic::new([0.0, 0.0, 0.0]);
        let target = [10.0, 0.0, 0.0];

        let steering = behavior.seek(&character, target);
        assert!(steering.linear[0] > 0.0);
    }

    #[test]
    fn steering_flee() {
        let behavior = SteeringBehavior::new();
        let character = Kinematic::new([5.0, 0.0, 0.0]);
        let target = [10.0, 0.0, 0.0];

        let steering = behavior.flee(&character, target);
        assert!(steering.linear[0] < 0.0);
    }

    #[test]
    fn steering_arrive_stop() {
        let behavior = SteeringBehavior::new();
        let character = Kinematic::new([0.0, 0.0, 0.0]);
        let target = [0.5, 0.0, 0.0];

        let steering = behavior.arrive(&character, target);
        assert!(steering.is_zero());
    }

    #[test]
    fn steering_arrive_slow() {
        let behavior = SteeringBehavior::new();
        let character = Kinematic::new([0.0, 0.0, 0.0]);
        let target = [3.0, 0.0, 0.0];

        let steering = behavior.arrive(&character, target);
        assert!(!steering.is_zero());
    }

    #[test]
    fn kinematic_update() {
        let mut kinematic = Kinematic::new([0.0, 0.0, 0.0]);
        let steering = SteeringOutput::new().with_linear([10.0, 0.0, 0.0]);

        kinematic.update(&steering, 100.0, 1.0);

        assert!(kinematic.position[0] > 0.0);
    }

    #[test]
    fn steering_separation() {
        let behavior = SteeringBehavior::new();
        let character = Kinematic::new([0.0, 0.0, 0.0]);
        let others = vec![Kinematic::new([1.0, 0.0, 0.0])];

        let steering = behavior.separation(&character, &others, 5.0);
        assert!(steering.linear[0] < 0.0);
    }

    #[test]
    fn steering_alignment() {
        let behavior = SteeringBehavior::new();
        let mut character = Kinematic::new([0.0, 0.0, 0.0]);
        character.velocity = [0.0, 0.0, 0.0];
        let mut other = Kinematic::new([1.0, 0.0, 0.0]);
        other.velocity = [5.0, 0.0, 0.0];
        let others = vec![other];

        let steering = behavior.alignment(&character, &others, 5.0);
        assert!(steering.linear[0] > 0.0);
    }
}
