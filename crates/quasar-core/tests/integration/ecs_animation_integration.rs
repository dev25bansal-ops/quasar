//! Integration tests for ECS + Animation interaction.
//!
//! Verifies that entities with animation components correctly interact
//! with the animation system, including:
//! - Creating entities with AnimationPlayer + AnimationClip
//! - Testing animation state machine transitions
//! - Verifying animation clips apply transforms correctly
//! - Testing cross-fade between animations

use quasar_core::animation::{
    AnimationClip, AnimationPlayer, AnimationResource, AnimationState, AnimationStateNode,
    AnimationStateMachine, AnimationTransition, TransitionCondition, TransformKeyframe,
};
use quasar_core::ecs::World;
use quasar_math::{Quat, Transform, Vec3};

// ---------------------------------------------------------------------------
// 1. Create entities with AnimationPlayer + AnimationClip
// ---------------------------------------------------------------------------

#[test]
fn test_spawn_entity_with_animation_player() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));
    world.insert(entity, AnimationPlayer::new("idle"));

    assert!(world.get::<AnimationPlayer>(entity).is_some());
    let player = world.get::<AnimationPlayer>(entity).unwrap();
    assert_eq!(player.clip_name, "idle");
    assert_eq!(player.state, AnimationState::Playing);
}

#[test]
fn test_spawn_entity_with_animation_resource() {
    let mut world = World::new();

    // Create animation clips
    let idle_clip = AnimationClip::new("idle")
        .with_duration(2.0)
        .looped(true)
        .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0)))
        .add_keyframe(TransformKeyframe::at_position(2.0, Vec3::new(0.0, 0.0, 0.0)));

    let walk_clip = AnimationClip::new("walk")
        .with_duration(1.0)
        .looped(true)
        .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0)))
        .add_keyframe(TransformKeyframe::at_position(1.0, Vec3::new(2.0, 0.0, 0.0)));

    let mut anim_resource = AnimationResource::new();
    anim_resource.add_clip(idle_clip);
    anim_resource.add_clip(walk_clip);
    world.insert_resource(anim_resource);

    // Spawn entity with animation
    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, AnimationPlayer::new("idle"));

    assert!(world.get::<AnimationPlayer>(entity).is_some());
    assert!(world.resource::<AnimationResource>().is_some());
    assert_eq!(world.resource::<AnimationResource>().unwrap().clip_count(), 2);
}

#[test]
fn test_spawn_multiple_animated_entities() {
    let mut world = World::new();

    for i in 0..5 {
        let entity = world.spawn();
        world.insert(
            entity,
            Transform::from_position(Vec3::new(i as f32 * 2.0, 0.0, 0.0)),
        );
        world.insert(entity, AnimationPlayer::new("idle"));
    }

    let player_count = world.query::<AnimationPlayer>().into_iter().count();
    assert_eq!(player_count, 5);
}

// ---------------------------------------------------------------------------
// 2. Test animation state machine transitions
// ---------------------------------------------------------------------------

#[test]
fn test_animation_state_machine_creation() {
    let sm = AnimationStateMachine::new("idle")
        .add_state(AnimationStateNode::new("idle", "idle_clip"))
        .add_state(AnimationStateNode::new("walking", "walk_clip"));

    assert_eq!(sm.current_state, "idle");
    assert_eq!(sm.states.len(), 2);
}

#[test]
fn test_animation_state_machine_with_transitions() {
    let sm = AnimationStateMachine::new("idle")
        .add_state(AnimationStateNode::new("idle", "idle_clip"))
        .add_state(AnimationStateNode::new("walking", "walk_clip"))
        .add_transition(AnimationTransition {
            from: "idle".to_string(),
            to: "walking".to_string(),
            conditions: vec![TransitionCondition::BoolTrue("is_moving".to_string())],
            blend_duration: 0.2,
        })
        .add_transition(AnimationTransition {
            from: "walking".to_string(),
            to: "idle".to_string(),
            conditions: vec![TransitionCondition::BoolTrue("is_stopped".to_string())],
            blend_duration: 0.2,
        });

    assert_eq!(sm.transitions.len(), 2);
    assert_eq!(sm.states.len(), 2);
}

#[test]
fn test_animation_state_machine_with_entity() {
    let mut world = World::new();

    let sm = AnimationStateMachine::new("idle")
        .add_state(AnimationStateNode::new("idle", "idle_clip"))
        .add_state(AnimationStateNode::new("walking", "walk_clip"))
        .add_transition(AnimationTransition {
            from: "idle".to_string(),
            to: "walking".to_string(),
            conditions: vec![TransitionCondition::BoolTrue("is_moving".to_string())],
            blend_duration: 0.25,
        });

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, AnimationPlayer::new("idle"));
    world.insert(entity, sm);

    assert!(world.get::<AnimationStateMachine>(entity).is_some());
    assert!(world.get::<AnimationPlayer>(entity).is_some());
}

#[test]
fn test_animation_state_machine_set_params() {
    let mut sm = AnimationStateMachine::new("idle")
        .add_state(AnimationStateNode::new("idle", "idle_clip"))
        .add_state(AnimationStateNode::new("walking", "walk_clip"))
        .add_transition(AnimationTransition {
            from: "idle".to_string(),
            to: "walking".to_string(),
            conditions: vec![TransitionCondition::FloatGreaterThan("speed".to_string(), 0.1)],
            blend_duration: 0.3,
        });

    sm.set_float("speed", 1.5);
    sm.set_bool("is_moving", true);

    assert!((sm.float_params.get("speed").unwrap() - 1.5).abs() < 0.001);
    assert!(*sm.bool_params.get("is_moving").unwrap());
}

// ---------------------------------------------------------------------------
// 3. Verify animation clips apply transforms correctly
// ---------------------------------------------------------------------------

#[test]
fn test_animation_clip_sampling() {
    let clip = AnimationClip::new("test_clip")
        .with_duration(1.0)
        .looped(false)
        .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0)))
        .add_keyframe(TransformKeyframe::at_position(1.0, Vec3::new(10.0, 0.0, 0.0)));

    // Sample at start
    let start = clip.sample(0.0).unwrap();
    assert!((start.position.x - 0.0).abs() < 0.001);

    // Sample at end
    let end = clip.sample(1.0).unwrap();
    assert!((end.position.x - 10.0).abs() < 0.001);

    // Sample at midpoint (should interpolate)
    let mid = clip.sample(0.5).unwrap();
    assert!((mid.position.x - 5.0).abs() < 0.1);
}

#[test]
fn test_animation_clip_looping() {
    let clip = AnimationClip::new("looped_clip")
        .with_duration(2.0)
        .looped(true)
        .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0)))
        .add_keyframe(TransformKeyframe::at_position(2.0, Vec3::new(5.0, 0.0, 0.0)));

    // Sample past duration should wrap around
    let sampled = clip.sample(3.0).unwrap(); // 3.0 % 2.0 = 1.0
    assert!(sampled.position.x > 0.0 && sampled.position.x < 5.0);
}

#[test]
fn test_animation_player_update_time() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, AnimationPlayer::new("walk"));

    // Manually update player time (simulating system update)
    let mut player = world.get_mut::<AnimationPlayer>(entity).unwrap();
    player.time = 0.5;

    let player = world.get::<AnimationPlayer>(entity).unwrap();
    assert!((player.time - 0.5).abs() < 0.001);
}

#[test]
fn test_animation_keyframe_transform_lerp() {
    let kf1 = TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0));
    let kf2 = TransformKeyframe::at_position(1.0, Vec3::new(10.0, 0.0, 0.0));

    let transform = kf1.lerp(&kf2, 0.5);

    assert!((transform.position.x - 5.0).abs() < 0.001);
    assert!((transform.position.y - 0.0).abs() < 0.001);
    assert!((transform.position.z - 0.0).abs() < 0.001);
}

#[test]
fn test_animation_keyframe_rotation_lerp() {
    let kf1 = TransformKeyframe::at_rotation(0.0, Quat::IDENTITY);
    let kf2 = TransformKeyframe::at_rotation(
        1.0,
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
    );

    let transform = kf1.lerp(&kf2, 0.5);

    // Rotation should be halfway between identity and 90° Y rotation
    assert!(transform.rotation.w > 0.7 && transform.rotation.w < 0.8);
}

#[test]
fn test_animation_keyframe_scale_lerp() {
    let mut kf1 = TransformKeyframe::at_position(0.0, Vec3::ZERO);
    kf1.scale = Vec3::ONE;

    let mut kf2 = TransformKeyframe::at_position(1.0, Vec3::ZERO);
    kf2.scale = Vec3::splat(2.0);

    let transform = kf1.lerp(&kf2, 0.5);

    assert!((transform.scale.x - 1.5).abs() < 0.001);
    assert!((transform.scale.y - 1.5).abs() < 0.001);
    assert!((transform.scale.z - 1.5).abs() < 0.001);
}

#[test]
fn test_animation_clip_rotation_keyframes() {
    let clip = AnimationClip::new("rotate")
        .with_duration(1.0)
        .looped(false)
        .add_keyframe(TransformKeyframe::at_rotation(0.0, Quat::IDENTITY))
        .add_keyframe(TransformKeyframe::at_rotation(
            1.0,
            Quat::from_rotation_y(std::f32::consts::PI),
        ));

    let start = clip.sample(0.0).unwrap();
    assert!((start.rotation.w - 1.0).abs() < 0.001);

    let mid = clip.sample(0.5).unwrap();
    // Should be halfway rotation
    assert!(mid.rotation.w > 0.0 && mid.rotation.w < 1.0);
}

// ---------------------------------------------------------------------------
// 4. Test cross-fade between animations
// ---------------------------------------------------------------------------

#[test]
fn test_animation_state_machine_crossfade_setup() {
    let mut sm = AnimationStateMachine::new("idle")
        .add_state(AnimationStateNode::new("idle", "idle_clip"))
        .add_state(AnimationStateNode::new("walking", "walk_clip"))
        .add_transition(AnimationTransition {
            from: "idle".to_string(),
            to: "walking".to_string(),
            conditions: vec![TransitionCondition::BoolTrue("start_walk".to_string())],
            blend_duration: 0.5, // 500ms crossfade
        });

    // Trigger transition
    sm.set_bool("start_walk", true);

    // Verify transition is configured with blend
    assert_eq!(sm.transitions[0].blend_duration, 0.5);
}

#[test]
fn test_animation_player_playback_control() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);

    let mut player = AnimationPlayer::new("walk");
    player.time = 1.5;
    world.insert(entity, player);

    // Pause
    {
        let mut player = world.get_mut::<AnimationPlayer>(entity).unwrap();
        player.pause();
    }
    assert_eq!(
        world.get::<AnimationPlayer>(entity).unwrap().state,
        AnimationState::Paused
    );

    // Play
    {
        let mut player = world.get_mut::<AnimationPlayer>(entity).unwrap();
        player.play();
    }
    assert_eq!(
        world.get::<AnimationPlayer>(entity).unwrap().state,
        AnimationState::Playing
    );

    // Stop
    {
        let mut player = world.get_mut::<AnimationPlayer>(entity).unwrap();
        player.stop();
    }
    assert_eq!(
        world.get::<AnimationPlayer>(entity).unwrap().state,
        AnimationState::Stopped
    );
    assert!((world.get::<AnimationPlayer>(entity).unwrap().time - 0.0).abs() < 0.001);
}

#[test]
fn test_animation_player_speed_control() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);

    let mut player = AnimationPlayer::new("walk");
    player.set_speed(2.0);
    world.insert(entity, player);

    assert!((world.get::<AnimationPlayer>(entity).unwrap().speed - 2.0).abs() < 0.001);
}

#[test]
fn test_animation_resource_management() {
    let mut world = World::new();

    let clip = AnimationClip::new("jump")
        .with_duration(1.5)
        .looped(false);

    let mut resource = AnimationResource::new();
    resource.add_clip(clip.clone());

    assert_eq!(resource.clip_count(), 1);
    assert!(resource.get_clip("jump").is_some());
    assert!(resource.get_clip("nonexistent").is_none());

    // Remove clip
    let removed = resource.remove_clip("jump");
    assert!(removed.is_some());
    assert_eq!(resource.clip_count(), 0);

    world.insert_resource(resource);
}

#[test]
fn test_multiple_animation_clips_in_world() {
    let mut world = World::new();

    // Create multiple clips
    let idle_clip = AnimationClip::new("idle")
        .with_duration(2.0)
        .looped(true);

    let walk_clip = AnimationClip::new("walk")
        .with_duration(1.0)
        .looped(true);

    let mut anim_resource = AnimationResource::new();
    anim_resource.add_clip(idle_clip);
    anim_resource.add_clip(walk_clip);
    world.insert_resource(anim_resource);

    // Spawn entities using different animations
    let entity_a = world.spawn();
    world.insert(entity_a, Transform::IDENTITY);
    world.insert(entity_a, AnimationPlayer::new("idle"));

    let entity_b = world.spawn();
    world.insert(entity_b, Transform::IDENTITY);
    world.insert(entity_b, AnimationPlayer::new("walk"));

    assert_eq!(
        world.get::<AnimationPlayer>(entity_a).unwrap().clip_name,
        "idle"
    );
    assert_eq!(
        world.get::<AnimationPlayer>(entity_b).unwrap().clip_name,
        "walk"
    );
}

#[test]
fn test_animation_query_with_transform() {
    let mut world = World::new();

    for i in 0..3 {
        let entity = world.spawn();
        world.insert(
            entity,
            Transform::from_position(Vec3::new(i as f32, 0.0, 0.0)),
        );
        world.insert(entity, AnimationPlayer::new("idle"));
    }

    let animated_count = world
        .query2::<Transform, AnimationPlayer>()
        .into_iter()
        .count();

    assert_eq!(animated_count, 3);
}

#[test]
fn test_animation_with_entity_lifecycle() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, AnimationPlayer::new("test_anim"));

    // Verify animation exists
    assert!(world.get::<AnimationPlayer>(entity).is_some());

    // Despawn entity
    assert!(world.despawn(entity));
    assert!(!world.is_alive(entity));
}

#[test]
fn test_animation_clip_empty_returns_none() {
    let clip = AnimationClip::new("empty").with_duration(0.0);
    assert!(clip.sample(0.0).is_none());
}

#[test]
fn test_animation_clip_single_keyframe() {
    let clip = AnimationClip::new("single")
        .with_duration(1.0)
        .add_keyframe(TransformKeyframe::at_position(0.5, Vec3::new(5.0, 0.0, 0.0)));

    // Single keyframe should return that transform at any time
    let sampled = clip.sample(0.5).unwrap();
    assert!((sampled.position.x - 5.0).abs() < 0.001);
}

#[test]
fn test_animation_transform_application() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);

    // Create animation clip
    let clip = AnimationClip::new("move")
        .with_duration(1.0)
        .add_keyframe(TransformKeyframe::at_position(0.0, Vec3::new(0.0, 0.0, 0.0)))
        .add_keyframe(TransformKeyframe::at_position(1.0, Vec3::new(10.0, 5.0, 0.0)));

    // Sample and apply to transform
    let sampled = clip.sample(0.5).unwrap();
    {
        let mut transform = world.get_mut::<Transform>(entity).unwrap();
        transform.position = sampled.position;
        transform.rotation = sampled.rotation;
        transform.scale = sampled.scale;
    }

    let final_transform = world.get::<Transform>(entity).unwrap();
    assert!((final_transform.position.x - 5.0).abs() < 0.1);
}

#[test]
fn test_animation_state_machine_crossfade_with_entity() {
    let mut world = World::new();

    // Create clips
    let mut anim_resource = AnimationResource::new();
    anim_resource.add_clip(
        AnimationClip::new("idle_clip")
            .with_duration(2.0)
            .looped(true),
    );
    anim_resource.add_clip(
        AnimationClip::new("walk_clip")
            .with_duration(1.0)
            .looped(true),
    );
    world.insert_resource(anim_resource);

    // Create state machine with crossfade transition
    let sm = AnimationStateMachine::new("idle")
        .add_state(AnimationStateNode::new("idle", "idle_clip"))
        .add_state(AnimationStateNode::new("walking", "walk_clip"))
        .add_transition(AnimationTransition {
            from: "idle".to_string(),
            to: "walking".to_string(),
            conditions: vec![TransitionCondition::BoolTrue("start_walk".to_string())],
            blend_duration: 0.5,
        });

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, AnimationPlayer::new("idle_clip"));
    world.insert(entity, sm);

    // Verify setup
    let sm = world.get::<AnimationStateMachine>(entity).unwrap();
    assert_eq!(sm.current_state, "idle");
    assert!(sm.crossfade.is_none()); // No active crossfade yet
}
