//! AI Systems Demo - Showcases GOAP, Behavior Trees, and Utility AI
//!
//! This example demonstrates:
//! - GOAP (Goal-Oriented Action Planning)
//! - Behavior Trees for AI decision making
//! - Utility AI for action selection
//! - Blackboard for shared AI knowledge

use glam::Vec3;
use log::info;
use quasar_ai::prelude::*;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("=== Quasar AI Systems Demo ===\n");

    demo_behavior_tree();
    demo_goap();
    demo_utility_ai();
    demo_blackboard();

    info!("\n=== Demo Complete ===");
}

fn demo_behavior_tree() {
    info!("\n--- Behavior Tree Demo ---");

    let tree = BehaviorTree::new(
        "Guard AI",
        BtNode::selector(vec![
            BtNode::sequence(vec![
                BtNode::condition("enemy_visible", BlackboardValue::Bool(true)),
                BtNode::action("attack_enemy"),
            ]),
            BtNode::sequence(vec![
                BtNode::condition("has_target", BlackboardValue::Bool(true)),
                BtNode::action("move_to_target"),
            ]),
            BtNode::action("patrol"),
        ]),
    );

    let mut blackboard = Blackboard::new();
    blackboard.set_bool("enemy_visible", false);
    blackboard.set_bool("has_target", true);

    let ctx = BtContext {
        blackboard: &blackboard,
        delta_time: 0.016,
        elapsed_time: 0.0,
    };

    let mut state = BtState::new();

    info!("Created behavior tree: {}", tree.name());
    info!("Blackboard: enemy_visible=false, has_target=true");
    info!("Expected: Agent will move to target (first condition fails, second succeeds)");

    let status = tree.tick(&ctx, &mut state);
    info!("Tree execution status: {:?}", status);
}

fn demo_goap() {
    info!("\n--- GOAP Demo ---");

    let mut world_state = GoapWorldState::new();
    world_state.set("has_weapon", BlackboardValue::Bool(true));
    world_state.set("has_ammo", BlackboardValue::Bool(true));
    world_state.set("enemy_alive", BlackboardValue::Bool(true));
    world_state.set("enemy_in_range", BlackboardValue::Bool(false));

    let goal = GoapGoal::new("kill_enemy").require("enemy_alive", BlackboardValue::Bool(false));

    let actions = vec![
        GoapAction::new("approach_enemy")
            .require("enemy_alive", BlackboardValue::Bool(true))
            .effect("enemy_in_range", BlackboardValue::Bool(true))
            .cost(1.0),
        GoapAction::new("shoot_enemy")
            .require("has_weapon", BlackboardValue::Bool(true))
            .require("has_ammo", BlackboardValue::Bool(true))
            .require("enemy_in_range", BlackboardValue::Bool(true))
            .effect("enemy_alive", BlackboardValue::Bool(false))
            .cost(1.0),
    ];

    info!("World state: has_weapon=true, has_ammo=true, enemy_alive=true, enemy_in_range=false");
    info!("Goal: enemy_alive=false");

    let planner = GoapPlanner::new();
    match planner.plan(&world_state, &goal, &actions) {
        Some(plan) => {
            info!("GOAP plan found (cost: {:.1}):", plan.total_cost);
            for (i, action) in plan.actions.iter().enumerate() {
                info!("  {}. {}", i + 1, action.name);
            }
        }
        None => {
            info!("No plan found");
        }
    }
}

fn demo_utility_ai() {
    info!("\n--- Utility AI Demo ---");

    let mut brain = UtilityBrain::new();

    brain.add_action(
        UtilityAction::new("attack")
            .consideration(Consideration::new("health").curve(ResponseCurve::linear()))
            .consideration(Consideration::new("ammo").curve(ResponseCurve::linear()))
            .weight(1.5),
    );

    brain.add_action(
        UtilityAction::new("heal")
            .consideration(
                Consideration::new("health").curve(ResponseCurve::quadratic().slope(-1.0)),
            )
            .weight(1.0),
    );

    brain.add_action(
        UtilityAction::new("flee")
            .consideration(
                Consideration::new("health").curve(ResponseCurve::quadratic().slope(-2.0)),
            )
            .weight(0.8),
    );

    let mut inputs = std::collections::HashMap::new();
    inputs.insert("health".to_string(), 0.3);
    inputs.insert("ammo".to_string(), 0.8);

    info!("Context: health=0.3, ammo=0.8");

    let scores = brain.get_all_scores(&inputs, 0.0);
    for (name, score) in &scores {
        info!("  {}: {:.3}", name, score);
    }

    let best = brain.decide(&inputs, 0.0);
    info!("Best action: {}", best.unwrap_or("none"));

    inputs.insert("health".to_string(), 0.1);
    info!("\nContext: health=0.1, ammo=0.8");

    let scores = brain.get_all_scores(&inputs, 0.0);
    for (name, score) in &scores {
        info!("  {}: {:.3}", name, score);
    }

    let best = brain.decide(&inputs, 1.0);
    info!("Best action: {}", best.unwrap_or("none"));
}

fn demo_blackboard() {
    info!("\n--- Blackboard Demo ---");

    let mut bb = Blackboard::new();

    bb.set_vec3("target_position", Vec3::new(10.0, 0.0, 5.0).into());
    bb.set_float("alert_level", 0.75);
    bb.set_bool("enemy_detected", true);
    bb.set_vec3("last_known_position", Vec3::new(5.0, 0.0, 3.0).into());

    info!("Created blackboard with AI knowledge:");

    for key in bb.keys() {
        if let Some(value) = bb.get(key) {
            info!("  {}: {:?}", key, value);
        }
    }

    bb.set_float("alert_level", 1.0);
    info!("\nUpdated alert_level to 1.0 (maximum alert!)");

    if let Some(level) = bb.get_float("alert_level") {
        if level >= 1.0 {
            info!("AI is at maximum alert - triggering combat mode!");
        }
    }
}
