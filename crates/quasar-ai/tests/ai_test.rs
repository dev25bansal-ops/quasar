//! Public API tests for quasar-ai.

use quasar_ai::prelude::*;
use quasar_ai::steering::Kinematic;
use std::collections::HashMap;

#[test]
fn blackboard_stores_typed_values() {
    let mut blackboard = Blackboard::new();

    let health_key = blackboard.insert("health", BlackboardValue::Int(100));
    blackboard.insert("position", BlackboardValue::Vec3([10.0, 20.0, 30.0]));
    blackboard.insert("alive", BlackboardValue::Bool(true));

    assert_eq!(blackboard.get_int("health"), Some(100));
    assert_eq!(
        blackboard
            .get_key(health_key)
            .and_then(BlackboardValue::as_int),
        Some(100)
    );
    assert_eq!(
        blackboard
            .get("position")
            .and_then(BlackboardValue::as_vec3),
        Some([10.0, 20.0, 30.0])
    );
    assert_eq!(blackboard.get_bool("alive"), Some(true));
    assert!(blackboard.contains("health"));
}

#[test]
fn blackboard_key_is_stable_for_name() {
    let key_a = BlackboardKey::new("target");
    let key_b = BlackboardKey::new("target");

    assert_eq!(key_a, key_b);
    assert_eq!(BlackboardKey::from_u64(key_a.as_u64()), key_a);
}

#[test]
fn utility_action_scores_considerations() {
    let action = UtilityAction::new("attack")
        .consideration(Consideration::new("distance").curve(ResponseCurve::linear()))
        .weight(0.5);
    let mut inputs = HashMap::new();
    inputs.insert("distance".to_string(), 0.8);

    assert_eq!(action.name, "attack");
    assert_eq!(action.considerations.len(), 1);
    assert!((action.calculate_score(&inputs, 10.0) - 0.4).abs() < 0.001);
}

#[test]
fn utility_brain_picks_highest_scoring_action() {
    let mut brain = UtilityBrain::new();
    brain.add_action(UtilityAction::new("idle").weight(0.2));
    brain.add_action(UtilityAction::new("attack").weight(0.8));

    let choice = brain.decide(&HashMap::new(), 1.0);

    assert_eq!(choice, Some("attack"));
    assert_eq!(brain.actions().len(), 2);
}

#[test]
fn behavior_tree_condition_reads_blackboard() {
    let mut blackboard = Blackboard::new();
    blackboard.insert("has_target", BlackboardValue::Bool(true));
    let tree = BehaviorTree::new(
        "combat",
        BtNode::condition("has_target", BlackboardValue::Bool(true)),
    );
    let ctx = BtContext {
        blackboard: &blackboard,
        delta_time: 0.016,
        elapsed_time: 1.0,
    };
    let mut state = BtState::new();

    assert_eq!(tree.name(), "combat");
    assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
}

#[test]
fn goap_planner_finds_single_action_plan() {
    let mut start = GoapWorldState::new();
    start.set("has_weapon", BlackboardValue::Bool(true));
    start.set("enemy_defeated", BlackboardValue::Bool(false));

    let goal = GoapGoal::new("defeat_enemy").require("enemy_defeated", BlackboardValue::Bool(true));
    let action = GoapAction::new("attack")
        .require("has_weapon", BlackboardValue::Bool(true))
        .effect("enemy_defeated", BlackboardValue::Bool(true));

    let plan = GoapPlanner::new()
        .plan(&start, &goal, &[action])
        .expect("plan should exist");

    assert_eq!(plan.goal, "defeat_enemy");
    assert_eq!(plan.len(), 1);
    assert_eq!(
        plan.current_action().map(|a| a.name.as_str()),
        Some("attack")
    );
}

#[test]
fn steering_behaviors_produce_expected_outputs() {
    let behavior = SteeringBehavior::new();
    let character = Kinematic::new([0.0, 0.0, 0.0]);

    let seek = behavior.seek(&character, [1.0, 0.0, 0.0]);
    let flee = behavior.flee(&character, [1.0, 0.0, 0.0]);
    let arrive_at_current_position = behavior.arrive(&character, [0.0, 0.0, 0.0]);

    assert!(seek.linear[0] > 0.0);
    assert!(flee.linear[0] < 0.0);
    assert!(arrive_at_current_position.is_zero());
}

#[test]
fn perception_and_navigation_types_are_constructible() {
    let perception = Perception::new(quasar_ai::sensors::EntityId(7));
    assert_eq!(perception.awareness, AwarenessLevel::Unaware);

    let request = PathRequest {
        id: 1,
        start: [0.0, 0.0, 0.0],
        end: [10.0, 0.0, 0.0],
    };
    let mut result = PathResult::new(request.id);
    result.waypoints.push(request.end);

    assert_eq!(request.start, [0.0, 0.0, 0.0]);
    assert_eq!(result.current_waypoint(), Some([10.0, 0.0, 0.0]));
}
