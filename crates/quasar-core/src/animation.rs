//! Animation system — keyframe-based animation for entity transforms.
//!
//! Provides a simple but powerful animation system that can interpolate
//! between keyframes over time. Supports position, rotation, and scale
//! animation for any entity with a Transform component.

use crate::ecs::{Entity, System, World};
use crate::TimeSnapshot;
use quasar_math::{Quat, Transform, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransformKeyframe {
    pub time: f32,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl TransformKeyframe {
    pub fn at_identity(time: f32) -> Self {
        Self {
            time,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub fn at_position(time: f32, position: Vec3) -> Self {
        Self {
            time,
            position,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub fn at_rotation(time: f32, rotation: Quat) -> Self {
        Self {
            time,
            position: Vec3::ZERO,
            rotation,
            scale: Vec3::ONE,
        }
    }

    pub fn lerp(&self, other: &Self, t: f32) -> Transform {
        let position = self.position.lerp(other.position, t);
        let rotation = self.rotation.slerp(other.rotation, t);
        let scale = self.scale.lerp(other.scale, t);
        Transform {
            position,
            rotation,
            scale,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub keyframes: Vec<TransformKeyframe>,
    pub looped: bool,
}

impl AnimationClip {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            duration: 0.0,
            keyframes: Vec::new(),
            looped: true,
        }
    }

    pub fn with_duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self
    }

    pub fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }

    pub fn add_keyframe(mut self, keyframe: TransformKeyframe) -> Self {
        self.keyframes.push(keyframe);
        self.keyframes
            .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        if let Some(last) = self.keyframes.last() {
            self.duration = self.duration.max(last.time);
        }
        self
    }

    pub fn sample(&self, time: f32) -> Option<Transform> {
        if self.keyframes.is_empty() {
            return None;
        }

        let time = if self.looped {
            time % self.duration
        } else {
            time.clamp(0.0, self.duration)
        };

        if self.keyframes.len() == 1 {
            let kf = &self.keyframes[0];
            return Some(Transform {
                position: kf.position,
                rotation: kf.rotation,
                scale: kf.scale,
            });
        }

        let (idx, t) = self.find_keyframe_interval(time);

        if idx >= self.keyframes.len() - 1 {
            let kf = self.keyframes.last()?;
            return Some(Transform {
                position: kf.position,
                rotation: kf.rotation,
                scale: kf.scale,
            });
        }

        let kf1 = &self.keyframes[idx];
        let kf2 = &self.keyframes[idx + 1];
        Some(kf1.lerp(kf2, t))
    }

    fn find_keyframe_interval(&self, time: f32) -> (usize, f32) {
        for i in 0..self.keyframes.len() - 1 {
            let kf1 = &self.keyframes[i];
            let kf2 = &self.keyframes[i + 1];
            if time >= kf1.time && time <= kf2.time {
                let duration = kf2.time - kf1.time;
                let t = if duration > 0.0 {
                    (time - kf1.time) / duration
                } else {
                    0.0
                };
                return (i, t);
            }
        }
        (self.keyframes.len().saturating_sub(2), 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct AnimationPlayer {
    pub clip_name: String,
    pub time: f32,
    pub speed: f32,
    pub state: AnimationState,
}

impl AnimationPlayer {
    pub fn new(clip_name: impl Into<String>) -> Self {
        Self {
            clip_name: clip_name.into(),
            time: 0.0,
            speed: 1.0,
            state: AnimationState::Playing,
        }
    }

    pub fn play(&mut self) {
        self.state = AnimationState::Playing;
    }

    pub fn pause(&mut self) {
        self.state = AnimationState::Paused;
    }

    pub fn stop(&mut self) {
        self.state = AnimationState::Stopped;
        self.time = 0.0;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    pub fn reset(&mut self) {
        self.time = 0.0;
        self.state = AnimationState::Playing;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletalAnimationClip {
    pub name: String,
    pub duration: f32,
    pub bone_keyframes: HashMap<String, Vec<TransformKeyframe>>,
    pub looped: bool,
}

impl SkeletalAnimationClip {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            duration: 0.0,
            bone_keyframes: HashMap::new(),
            looped: true,
        }
    }

    pub fn add_bone_keyframe(mut self, bone_name: &str, keyframe: TransformKeyframe) -> Self {
        let keyframes = self
            .bone_keyframes
            .entry(bone_name.to_string())
            .or_default();
        keyframes.push(keyframe);
        keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        if let Some(last) = keyframes.last() {
            self.duration = self.duration.max(last.time);
        }
        self
    }

    pub fn sample_bone(&self, bone_name: &str, time: f32) -> Option<Transform> {
        let keyframes = self.bone_keyframes.get(bone_name)?;
        if keyframes.is_empty() {
            return None;
        }

        let time = if self.looped {
            time % self.duration
        } else {
            time.clamp(0.0, self.duration)
        };

        if keyframes.len() == 1 {
            let kf = &keyframes[0];
            return Some(Transform {
                position: kf.position,
                rotation: kf.rotation,
                scale: kf.scale,
            });
        }

        let (idx, t) = self.find_bone_keyframe_interval(keyframes, time);

        if idx >= keyframes.len() - 1 {
            let kf = keyframes.last()?;
            return Some(Transform {
                position: kf.position,
                rotation: kf.rotation,
                scale: kf.scale,
            });
        }

        let kf1 = &keyframes[idx];
        let kf2 = &keyframes[idx + 1];
        Some(kf1.lerp(kf2, t))
    }

    fn find_bone_keyframe_interval(
        &self,
        keyframes: &[TransformKeyframe],
        time: f32,
    ) -> (usize, f32) {
        for i in 0..keyframes.len() - 1 {
            let kf1 = &keyframes[i];
            let kf2 = &keyframes[i + 1];
            if time >= kf1.time && time <= kf2.time {
                let duration = kf2.time - kf1.time;
                let t = if duration > 0.0 {
                    (time - kf1.time) / duration
                } else {
                    0.0
                };
                return (i, t);
            }
        }
        (keyframes.len().saturating_sub(2), 1.0)
    }
}

pub struct AnimationResource {
    clips: HashMap<String, AnimationClip>,
}

impl AnimationResource {
    pub fn new() -> Self {
        Self {
            clips: HashMap::new(),
        }
    }

    pub fn add_clip(&mut self, clip: AnimationClip) {
        self.clips.insert(clip.name.clone(), clip);
    }

    pub fn get_clip(&self, name: &str) -> Option<&AnimationClip> {
        self.clips.get(name)
    }

    pub fn remove_clip(&mut self, name: &str) -> Option<AnimationClip> {
        self.clips.remove(name)
    }

    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }
}

impl Default for AnimationResource {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AnimationSystem;

impl System for AnimationSystem {
    fn name(&self) -> &str {
        "animation"
    }

    fn run(&mut self, world: &mut World) {
        let delta = world
            .resource::<TimeSnapshot>()
            .map(|t| t.delta_seconds)
            .unwrap_or(1.0 / 60.0);

        let clips = if let Some(res) = world.resource::<AnimationResource>() {
            res.clips.clone()
        } else {
            return;
        };

        let players: Vec<(Entity, AnimationPlayer)> = world
            .query::<AnimationPlayer>()
            .map(|(e, p)| (e, p.clone()))
            .collect();

        for (entity, mut player) in players {
            if player.state != AnimationState::Playing {
                continue;
            }

            player.time += delta * player.speed;

            if let Some(clip) = clips.get(&player.clip_name) {
                if let Some(transform) = clip.sample(player.time) {
                    if let Some(t) = world.get_mut::<Transform>(entity) {
                        *t = transform;
                    }
                }
            }

            if let Some(p) = world.get_mut::<AnimationPlayer>(entity) {
                *p = player;
            }
        }
    }
}

pub struct AnimationPlugin;

impl crate::Plugin for AnimationPlugin {
    fn name(&self) -> &str {
        "AnimationPlugin"
    }

    fn build(&self, app: &mut crate::App) {
        app.world.insert_resource(AnimationResource::new());
        app.schedule
            .add_system(crate::ecs::SystemStage::Update, Box::new(AnimationSystem));
        log::info!("AnimationPlugin loaded — keyframe animation active");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframe_lerp_position() {
        let kf1 = TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0));
        let kf2 = TransformKeyframe::at_position(1.0, Vec3::new(10.0, 0.0, 0.0));

        let t = kf1.lerp(&kf2, 0.5);
        assert!((t.position.x - 5.0).abs() < 0.01);
    }

    #[test]
    fn clip_sample_interpolates() {
        let clip = AnimationClip::new("test")
            .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                1.0,
                Vec3::new(10.0, 0.0, 0.0),
            ));

        let t = clip.sample(0.5).unwrap();
        assert!((t.position.x - 5.0).abs() < 0.01);
    }

    #[test]
    fn clip_loops() {
        let clip = AnimationClip::new("test")
            .looped(true)
            .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::ZERO))
            .add_keyframe(TransformKeyframe::at_position(
                1.0,
                Vec3::new(10.0, 0.0, 0.0),
            ));

        let t = clip.sample(1.5).unwrap();
        assert!((t.position.x - 5.0).abs() < 0.01);
    }

    #[test]
    fn animation_player_plays() {
        let mut player = AnimationPlayer::new("test");
        assert_eq!(player.state, AnimationState::Playing);

        player.pause();
        assert_eq!(player.state, AnimationState::Paused);

        player.stop();
        assert_eq!(player.state, AnimationState::Stopped);
        assert_eq!(player.time, 0.0);
    }
}
