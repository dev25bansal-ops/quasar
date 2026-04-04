//! Sensor System - Perception and Awareness for AI Agents

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum AwarenessLevel {
    #[default]
    Unaware,
    Suspicious,
    Alert,
    Threat,
}


#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SensoryMemory {
    pub last_seen_time: f32,
    pub last_seen_position: [f32; 3],
    pub total_visibility_time: f32,
    pub times_seen: u32,
}

impl Default for SensoryMemory {
    fn default() -> Self {
        Self {
            last_seen_time: -f32::MAX,
            last_seen_position: [0.0; 3],
            total_visibility_time: 0.0,
            times_seen: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perception {
    pub entity_id: EntityId,
    pub awareness: AwarenessLevel,
    pub last_known_position: Option<[f32; 3]>,
    pub visibility: f32,
    pub distance: f32,
    pub is_visible: bool,
    pub memory: SensoryMemory,
}

impl Perception {
    pub fn new(entity_id: EntityId) -> Self {
        Self {
            entity_id,
            awareness: AwarenessLevel::Unaware,
            last_known_position: None,
            visibility: 0.0,
            distance: f32::MAX,
            is_visible: false,
            memory: SensoryMemory::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SightSensor {
    pub range: f32,
    pub angle: f32,
    pub height: f32,
    pub update_interval: f32,
    pub last_update: f32,
}

impl Default for SightSensor {
    fn default() -> Self {
        Self {
            range: 50.0,
            angle: std::f32::consts::FRAC_PI_4,
            height: 1.7,
            update_interval: 0.1,
            last_update: 0.0,
        }
    }
}

impl SightSensor {
    pub fn new(range: f32, angle: f32) -> Self {
        Self {
            range,
            angle,
            ..Default::default()
        }
    }

    pub fn can_see(&self, origin: [f32; 3], direction: [f32; 3], target: [f32; 3]) -> (bool, f32) {
        let dx = target[0] - origin[0];
        let dy = target[1] - origin[1];
        let dz = target[2] - origin[2];

        let distance = (dx * dx + dy * dy + dz * dz).sqrt();

        if distance > self.range {
            return (false, 0.0);
        }

        let to_target = [dx / distance, dy / distance, dz / distance];
        let dot =
            direction[0] * to_target[0] + direction[1] * to_target[1] + direction[2] * to_target[2];
        let angle_to_target = dot.acos();

        if angle_to_target > self.angle {
            return (false, 0.0);
        }

        let visibility = 1.0 - (distance / self.range);
        (true, visibility)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HearingSensor {
    pub range: f32,
    pub sensitivity: f32,
    pub update_interval: f32,
    pub last_update: f32,
}

impl Default for HearingSensor {
    fn default() -> Self {
        Self {
            range: 30.0,
            sensitivity: 0.5,
            update_interval: 0.2,
            last_update: 0.0,
        }
    }
}

impl HearingSensor {
    pub fn new(range: f32, sensitivity: f32) -> Self {
        Self {
            range,
            sensitivity,
            ..Default::default()
        }
    }

    pub fn can_hear(&self, origin: [f32; 3], source: [f32; 3], volume: f32) -> (bool, f32) {
        let dx = source[0] - origin[0];
        let dy = source[1] - origin[1];
        let dz = source[2] - origin[2];

        let distance = (dx * dx + dy * dy + dz * dz).sqrt();

        if distance > self.range {
            return (false, 0.0);
        }

        let effective_volume = volume * self.sensitivity * (1.0 - distance / self.range);
        (effective_volume > 0.1, effective_volume)
    }
}

pub struct SensorSystem {
    sight: SightSensor,
    hearing: HearingSensor,
    perceptions: HashMap<EntityId, Perception>,
    position: [f32; 3],
    direction: [f32; 3],
    memory_duration: f32,
    alert_decay_rate: f32,
}

impl Default for SensorSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorSystem {
    pub fn new() -> Self {
        Self {
            sight: SightSensor::default(),
            hearing: HearingSensor::default(),
            perceptions: HashMap::new(),
            position: [0.0; 3],
            direction: [0.0, 0.0, 1.0],
            memory_duration: 10.0,
            alert_decay_rate: 0.1,
        }
    }

    pub fn with_sight(mut self, sight: SightSensor) -> Self {
        self.sight = sight;
        self
    }

    pub fn with_hearing(mut self, hearing: HearingSensor) -> Self {
        self.hearing = hearing;
        self
    }

    pub fn set_position(&mut self, pos: [f32; 3]) {
        self.position = pos;
    }

    pub fn set_direction(&mut self, dir: [f32; 3]) {
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len > 0.0 {
            self.direction = [dir[0] / len, dir[1] / len, dir[2] / len];
        }
    }

    pub fn update(&mut self, current_time: f32, targets: &[(EntityId, [f32; 3])]) {
        for (entity_id, target_pos) in targets {
            let (visible, visibility) =
                self.sight
                    .can_see(self.position, self.direction, *target_pos);
            let distance = ((target_pos[0] - self.position[0]).powi(2)
                + (target_pos[1] - self.position[1]).powi(2)
                + (target_pos[2] - self.position[2]).powi(2))
            .sqrt();

            let perception = self
                .perceptions
                .entry(*entity_id)
                .or_insert(Perception::new(*entity_id));

            if visible {
                perception.is_visible = true;
                perception.visibility = visibility;
                perception.distance = distance;
                perception.last_known_position = Some(*target_pos);
                perception.memory.last_seen_time = current_time;
                perception.memory.last_seen_position = *target_pos;
                perception.memory.total_visibility_time += self.sight.update_interval;
                perception.memory.times_seen += 1;

                if visibility > 0.8 {
                    perception.awareness = AwarenessLevel::Threat;
                } else if visibility > 0.5 {
                    perception.awareness = AwarenessLevel::Alert;
                } else if visibility > 0.2 {
                    perception.awareness = AwarenessLevel::Suspicious;
                }
            } else {
                perception.is_visible = false;
                perception.visibility = 0.0;
            }
        }

        self.perceptions
            .retain(|_, p| current_time - p.memory.last_seen_time < self.memory_duration);

        for perception in self.perceptions.values_mut() {
            if !perception.is_visible {
                match perception.awareness {
                    AwarenessLevel::Threat => {
                        perception.awareness = AwarenessLevel::Alert;
                    }
                    AwarenessLevel::Alert => {
                        perception.awareness = AwarenessLevel::Suspicious;
                    }
                    AwarenessLevel::Suspicious => {
                        perception.awareness = AwarenessLevel::Unaware;
                    }
                    AwarenessLevel::Unaware => {}
                }
            }
        }
    }

    pub fn get_perception(&self, entity_id: EntityId) -> Option<&Perception> {
        self.perceptions.get(&entity_id)
    }

    pub fn get_awareness(&self, entity_id: EntityId) -> AwarenessLevel {
        self.perceptions
            .get(&entity_id)
            .map(|p| p.awareness)
            .unwrap_or(AwarenessLevel::Unaware)
    }

    pub fn get_visible_entities(&self) -> Vec<EntityId> {
        self.perceptions
            .iter()
            .filter(|(_, p)| p.is_visible)
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn get_threats(&self) -> Vec<&Perception> {
        self.perceptions
            .values()
            .filter(|p| matches!(p.awareness, AwarenessLevel::Threat | AwarenessLevel::Alert))
            .collect()
    }

    pub fn get_nearest_visible(&self) -> Option<&Perception> {
        self.perceptions
            .values()
            .filter(|p| p.is_visible)
            .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap())
    }

    pub fn clear(&mut self) {
        self.perceptions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sight_sensor_in_range() {
        let sensor = SightSensor::new(10.0, std::f32::consts::FRAC_PI_4);
        let origin = [0.0, 0.0, 0.0];
        let direction = [0.0, 0.0, 1.0];
        let target = [0.0, 0.0, 5.0];

        let (visible, _) = sensor.can_see(origin, direction, target);
        assert!(visible);
    }

    #[test]
    fn sight_sensor_out_of_range() {
        let sensor = SightSensor::new(10.0, std::f32::consts::FRAC_PI_4);
        let origin = [0.0, 0.0, 0.0];
        let direction = [0.0, 0.0, 1.0];
        let target = [0.0, 0.0, 15.0];

        let (visible, _) = sensor.can_see(origin, direction, target);
        assert!(!visible);
    }

    #[test]
    fn sight_sensor_out_of_angle() {
        let sensor = SightSensor::new(10.0, std::f32::consts::FRAC_PI_4);
        let origin = [0.0, 0.0, 0.0];
        let direction = [0.0, 0.0, 1.0];
        let target = [10.0, 0.0, 0.0];

        let (visible, _) = sensor.can_see(origin, direction, target);
        assert!(!visible);
    }

    #[test]
    fn hearing_sensor_detects() {
        let sensor = HearingSensor::new(20.0, 1.0);
        let origin = [0.0, 0.0, 0.0];
        let source = [5.0, 0.0, 0.0];

        let (heard, volume) = sensor.can_hear(origin, source, 1.0);
        assert!(heard);
        assert!(volume > 0.0);
    }

    #[test]
    fn sensor_system_update() {
        let mut system = SensorSystem::new();
        system.set_position([0.0, 0.0, 0.0]);
        system.set_direction([0.0, 0.0, 1.0]);

        let targets = vec![(EntityId(1), [0.0, 0.0, 5.0])];
        system.update(0.0, &targets);

        assert!(system.get_perception(EntityId(1)).is_some());
        assert_eq!(system.get_awareness(EntityId(1)), AwarenessLevel::Threat);
    }

    #[test]
    fn sensor_system_visible_entities() {
        let mut system = SensorSystem::new();
        system.set_position([0.0, 0.0, 0.0]);
        system.set_direction([0.0, 0.0, 1.0]);

        let targets = vec![
            (EntityId(1), [0.0, 0.0, 5.0]),
            (EntityId(2), [0.0, 0.0, 60.0]),
        ];
        system.update(0.0, &targets);

        let visible = system.get_visible_entities();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0], EntityId(1));
    }
}
