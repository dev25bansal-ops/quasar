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
            .into_iter()
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
        app.schedule.add_system(
            crate::ecs::SystemStage::Update,
            Box::new(AnimationStateMachineSystem),
        );
        log::info!("AnimationPlugin loaded — keyframe animation + state machine active");
    }
}

// ---------------------------------------------------------------------------
// Animation State Machine
// ---------------------------------------------------------------------------

/// A condition that gates a transition between animation states.
#[derive(Debug, Clone)]
pub enum TransitionCondition {
    /// Transition when the named float parameter crosses a threshold.
    FloatGreaterThan(String, f32),
    /// Transition when the named float parameter is below a threshold.
    FloatLessThan(String, f32),
    /// Transition when a bool parameter is true.
    BoolTrue(String),
    /// Transition when the current clip finishes.
    ClipFinished,
}

/// A transition between two states.
#[derive(Debug, Clone)]
pub struct AnimationTransition {
    /// Source state name.
    pub from: String,
    /// Destination state name.
    pub to: String,
    /// Conditions that must all be true for the transition to fire.
    pub conditions: Vec<TransitionCondition>,
    /// Cross-fade duration in seconds.
    pub blend_duration: f32,
}

/// A single state in the animation state machine.
#[derive(Debug, Clone)]
pub struct AnimationStateNode {
    /// State name.
    pub name: String,
    /// Clip to play in this state.
    pub clip_name: String,
    /// Playback speed multiplier.
    pub speed: f32,
}

impl AnimationStateNode {
    pub fn new(name: impl Into<String>, clip_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            clip_name: clip_name.into(),
            speed: 1.0,
        }
    }
}

/// Animation state machine — ECS component.
///
/// Attach to an entity alongside `AnimationPlayer`. The state machine
/// system will drive the player's clip selection and handle cross-fade
/// transitions.
#[derive(Debug, Clone)]
pub struct AnimationStateMachine {
    pub states: Vec<AnimationStateNode>,
    pub transitions: Vec<AnimationTransition>,
    pub current_state: String,
    pub float_params: HashMap<String, f32>,
    pub bool_params: HashMap<String, bool>,
    /// Active cross-fade (from_clip, from_time, to_clip, elapsed, duration).
    pub crossfade: Option<(String, f32, String, f32, f32)>,
}

impl AnimationStateMachine {
    pub fn new(initial_state: impl Into<String>) -> Self {
        Self {
            states: Vec::new(),
            transitions: Vec::new(),
            current_state: initial_state.into(),
            float_params: HashMap::new(),
            bool_params: HashMap::new(),
            crossfade: None,
        }
    }

    pub fn add_state(mut self, state: AnimationStateNode) -> Self {
        self.states.push(state);
        self
    }

    pub fn add_transition(mut self, transition: AnimationTransition) -> Self {
        self.transitions.push(transition);
        self
    }

    pub fn set_float(&mut self, name: &str, value: f32) {
        self.float_params.insert(name.to_string(), value);
    }

    pub fn set_bool(&mut self, name: &str, value: bool) {
        self.bool_params.insert(name.to_string(), value);
    }

    /// Evaluate transitions and return the next state name if a transition fires.
    fn evaluate_transitions(
        &self,
        clip_finished: bool,
    ) -> Option<(&AnimationTransition,)> {
        for transition in &self.transitions {
            if transition.from != self.current_state {
                continue;
            }
            let all_met = transition.conditions.iter().all(|cond| match cond {
                TransitionCondition::FloatGreaterThan(name, threshold) => {
                    self.float_params.get(name).copied().unwrap_or(0.0) > *threshold
                }
                TransitionCondition::FloatLessThan(name, threshold) => {
                    self.float_params.get(name).copied().unwrap_or(0.0) < *threshold
                }
                TransitionCondition::BoolTrue(name) => {
                    self.bool_params.get(name).copied().unwrap_or(false)
                }
                TransitionCondition::ClipFinished => clip_finished,
            });
            if all_met {
                return Some((transition,));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Blend Tree
// ---------------------------------------------------------------------------

/// A node in a blend tree.
#[derive(Debug, Clone)]
pub enum BlendTreeNode {
    /// Plays a single clip.
    Clip {
        clip_name: String,
    },
    /// Blends between two children based on a float parameter (0.0–1.0).
    Lerp {
        parameter: String,
        children: [Box<BlendTreeNode>; 2],
    },
}

impl BlendTreeNode {
    /// Sample the blend tree and produce a (possibly blended) transform.
    pub fn sample(
        &self,
        clips: &HashMap<String, AnimationClip>,
        params: &HashMap<String, f32>,
        time: f32,
    ) -> Option<Transform> {
        match self {
            BlendTreeNode::Clip { clip_name } => {
                clips.get(clip_name)?.sample(time)
            }
            BlendTreeNode::Lerp {
                parameter,
                children,
            } => {
                let t = params.get(parameter).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                let a = children[0].sample(clips, params, time)?;
                let b = children[1].sample(clips, params, time)?;
                Some(Transform {
                    position: a.position.lerp(b.position, t),
                    rotation: a.rotation.slerp(b.rotation, t),
                    scale: a.scale.lerp(b.scale, t),
                })
            }
        }
    }
}

/// ECS component that uses a blend tree instead of a single clip.
#[derive(Debug, Clone)]
pub struct AnimationBlendTree {
    pub root: BlendTreeNode,
    pub time: f32,
    pub speed: f32,
    pub params: HashMap<String, f32>,
}

impl AnimationBlendTree {
    pub fn new(root: BlendTreeNode) -> Self {
        Self {
            root,
            time: 0.0,
            speed: 1.0,
            params: HashMap::new(),
        }
    }

    pub fn set_param(&mut self, name: &str, value: f32) {
        self.params.insert(name.to_string(), value);
    }
}

/// System that drives the animation state machine component.
pub struct AnimationStateMachineSystem;

impl System for AnimationStateMachineSystem {
    fn name(&self) -> &str {
        "animation_state_machine"
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

        // Collect state machines.
        let machines: Vec<(Entity, AnimationStateMachine)> = world
            .query::<AnimationStateMachine>()
            .into_iter()
            .map(|(e, sm)| (e, sm.clone()))
            .collect();

        for (entity, mut sm) in machines {
            // Check if the current clip has finished.
            let clip_finished = clips.get(
                sm.states
                    .iter()
                    .find(|s| s.name == sm.current_state)
                    .map(|s| s.clip_name.as_str())
                    .unwrap_or(""),
            )
            .map(|clip| {
                let player_time = world
                    .get::<AnimationPlayer>(entity)
                    .map(|p| p.time)
                    .unwrap_or(0.0);
                !clip.looped && player_time >= clip.duration
            })
            .unwrap_or(false);

            // Evaluate transitions.
            if let Some((transition,)) = sm.evaluate_transitions(clip_finished) {
                let to_state = transition.to.clone();
                let blend_dur = transition.blend_duration;
                let from_clip = sm
                    .states
                    .iter()
                    .find(|s| s.name == sm.current_state)
                    .map(|s| s.clip_name.clone())
                    .unwrap_or_default();
                let to_clip = sm
                    .states
                    .iter()
                    .find(|s| s.name == to_state)
                    .map(|s| s.clip_name.clone())
                    .unwrap_or_default();

                sm.current_state = to_state;

                // Capture the outgoing clip's time BEFORE resetting the player.
                let from_time = world
                    .get::<AnimationPlayer>(entity)
                    .map(|p| p.time)
                    .unwrap_or(0.0);

                if blend_dur > 0.0 {
                    sm.crossfade = Some((from_clip, from_time, to_clip.clone(), 0.0, blend_dur));
                }

                // Update the player's clip.
                if let Some(player) = world.get_mut::<AnimationPlayer>(entity) {
                    player.clip_name = to_clip;
                    player.time = 0.0;
                    player.state = AnimationState::Playing;
                }
            }

            // Advance cross-fade.
            if let Some((ref from_clip, from_time, ref to_clip, ref mut elapsed, duration)) =
                sm.crossfade
            {
                *elapsed += delta;
                let t = (*elapsed / duration).clamp(0.0, 1.0);

                // Blend the two clips.
                // Sample the "from" clip at its captured time (where it was
                // when the transition fired), and the "to" clip at the current
                // player time.
                if let (Some(from), Some(to)) = (clips.get(from_clip), clips.get(to_clip)) {
                    let player_time = world
                        .get::<AnimationPlayer>(entity)
                        .map(|p| p.time)
                        .unwrap_or(0.0);
                    if let (Some(tf_a), Some(tf_b)) = (from.sample(from_time), to.sample(player_time)) {
                        let blended = Transform {
                            position: tf_a.position.lerp(tf_b.position, t),
                            rotation: tf_a.rotation.slerp(tf_b.rotation, t),
                            scale: tf_a.scale.lerp(tf_b.scale, t),
                        };
                        if let Some(tf) = world.get_mut::<Transform>(entity) {
                            *tf = blended;
                        }
                    }
                }

                if *elapsed >= duration {
                    sm.crossfade = None;
                }
            }

            // Write back state machine.
            if let Some(existing) = world.get_mut::<AnimationStateMachine>(entity) {
                *existing = sm;
            }
        }

        // Handle blend trees.
        let blend_trees: Vec<(Entity, AnimationBlendTree)> = world
            .query::<AnimationBlendTree>()
            .into_iter()
            .map(|(e, bt)| (e, bt.clone()))
            .collect();

        for (entity, mut bt) in blend_trees {
            bt.time += delta * bt.speed;
            if let Some(tf) = bt.root.sample(&clips, &bt.params, bt.time) {
                if let Some(t) = world.get_mut::<Transform>(entity) {
                    *t = tf;
                }
            }
            if let Some(existing) = world.get_mut::<AnimationBlendTree>(entity) {
                *existing = bt;
            }
        }
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
