//! Tests for quasar-ai crate

use quasar_ai::prelude::*;

#[test]
fn test_blackboard_creation() {
    let mut blackboard = Blackboard::new();
    
    blackboard.set("health", 100);
    blackboard.set("position", (10.0, 20.0, 30.0));
    
    assert_eq!(blackboard.get::<i32>("health"), Some(100));
    assert_eq!(blackboard.get::<(f32, f32, f32)>("position"), Some((10.0, 20.0, 30.0)));
}

#[test]
fn test_blackboard_key() {
    let key = BlackboardKey::new("test_key");
    assert_eq!(key.as_str(), "test_key");
}

#[test]
fn test_utility_action_creation() {
    let action = UtilityAction::new("test_action")
        .with_score(0.5);
    
    assert_eq!(action.name(), "test_action");
    assert_eq!(action.score(), 0.5);
}

#[test]
fn test_consideration_curve() {
    let curve = ResponseCurve::linear();
    let score = curve.evaluate(0.5);
    
    assert!((score - 0.5).abs() < 0.001);
}

#[test]
fn test_behavior_tree_status() {
    assert_eq!(BtStatus::Success.is_success(), true);
    assert_eq!(BtStatus::Failure.is_failure(), true);
    assert_eq!(BtStatus::Running.is_running(), true);
}

#[test]
fn test_goap_goal_creation() {
    let goal = GoapGoal::new("test_goal")
        .require("condition1", true)
        .require("condition2", false);
    
    assert_eq!(goal.name(), "test_goal");
    assert!(goal.requires("condition1"));
    assert!(goal.requires("condition2"));
}

#[test]
fn test_steering_output() {
    let output = SteeringOutput::new();
    
    assert_eq!(output.linear.length(), 0.0);
    assert_eq!(output.angular, 0.0);
}

#[test]
fn test_perception_levels() {
    assert_eq!(AwarenessLevel::None as i32, 0);
    assert_eq!(AwarenessLevel::Low as i32, 1);
    assert_eq!(AwarenessLevel::Medium as i32, 2);
    assert_eq!(AwarenessLevel::High as i32, 3);
}

#[test]
fn test_path_request() {
    let request = PathRequest::new(
        [0.0, 0.0, 0.0],
        [10.0, 10.0, 10.0],
    );
    
    assert_eq!(request.start(), [0.0, 0.0, 0.0]);
    assert_eq!(request.end(), [10.0, 10.0, 10.0]);
}

#[test]
fn test_blackboard_value_types() {
    let mut blackboard = Blackboard::new();
    
    // Test different value types
    blackboard.set("int_value", 42i32);
    blackboard.set("float_value", 3.14f32);
    blackboard.set("bool_value", true);
    blackboard.set("string_value", "test");
    
    assert_eq!(blackboard.get::<i32>("int_value"), Some(42));
    assert_eq!(blackboard.get::<f32>("float_value"), Some(3.14));
    assert_eq!(blackboard.get::<bool>("bool_value"), Some(true));
    assert_eq!(blackboard.get::<String>("string_value"), Some("test".to_string()));
}

#[test]
fn test_blackboard_remove() {
    let mut blackboard = Blackboard::new();
    
    blackboard.set("temp_value", 100);
    assert_eq!(blackboard.get::<i32>("temp_value"), Some(100));
    
    blackboard.remove("temp_value");
    assert_eq!(blackboard.get::<i32>("temp_value"), None);
}

#[test]
fn test_utility_action_considerations() {
    let action = UtilityAction::new("test_action")
        .add_consideration(|_context| 0.5)
        .add_consideration(|_context| 0.3);
    
    // The action should have 2 considerations
    assert_eq!(action.consideration_count(), 2);
}

#[test]
fn test_response_curve_types() {
    let linear = ResponseCurve::linear();
    let quadratic = ResponseCurve::quadratic();
    let logistic = ResponseCurve::logistic();
    
    // Test that different curves produce different results
    let input = 0.5;
    let linear_score = linear.evaluate(input);
    let quadratic_score = quadratic.evaluate(input);
    let logistic_score = logistic.evaluate(input);
    
    // All should be valid scores between 0 and 1
    assert!((0.0..=1.0).contains(&linear_score));
    assert!((0.0..=1.0).contains(&quadratic_score));
    assert!((0.0..=1.0).contains(&logistic_score));
}

#[test]
fn test_steering_behavior_types() {
    let seek = SteeringBehavior::seek([1.0, 0.0, 0.0]);
    let flee = SteeringBehavior::flee([1.0, 0.0, 0.0]);
    let arrive = SteeringBehavior::arrive([1.0, 0.0, 0.0], 1.0);
    
    // All should create valid behaviors
    let output_seek = seek.compute([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    let output_flee = flee.compute([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    let output_arrive = arrive.compute([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    
    // Outputs should be valid
    assert!((0.0..=1.0).contains(&output_seek.linear.length()));
    assert!((0.0..=1.0).contains(&output_flee.linear.length()));
    assert!((0.0..=1.0).contains(&output_arrive.linear.length()));
}