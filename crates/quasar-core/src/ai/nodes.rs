//! Built-in behavior tree node types and builders.
//!
//! This module provides:
//! - Pre-built action nodes for common AI tasks
//! - Pre-built condition nodes for common AI checks
//! - Decorator nodes for modifying child behavior
//! - Builder patterns for constructing nodes

#![allow(dead_code)]

use super::behavior_tree::{Node, NodeResult};
use super::blackboard::{Blackboard, BlackboardValue};
use crate::ecs::{Entity, World};

/// Trait for custom action nodes.
pub trait ActionNode: Send + Sync {
    fn name(&self) -> &str;
    fn execute(&self, blackboard: &mut Blackboard, world: &World, entity: Entity) -> NodeResult;
}

/// Trait for custom condition nodes.
pub trait ConditionNode: Send + Sync {
    fn name(&self) -> &str;
    fn evaluate(&self, blackboard: &Blackboard, world: &World, entity: Entity) -> bool;
}

/// Trait for decorator nodes.
pub trait DecoratorNode: Send + Sync {
    fn decorate(&self, result: NodeResult) -> NodeResult;
}

// ---------------------------------------------------------------------------
// Common Action Nodes
// ---------------------------------------------------------------------------

/// Waits for a specified duration.
pub fn wait(duration_seconds: f32) -> impl Fn(&mut Blackboard, &World, Entity) -> NodeResult {
    move |blackboard: &mut Blackboard, _world: &World, _entity: Entity| {
        let elapsed = blackboard.get_float("__wait_elapsed");
        if elapsed >= duration_seconds {
            blackboard.set("__wait_elapsed", BlackboardValue::Float(0.0));
            NodeResult::Success
        } else {
            blackboard.set(
                "__wait_elapsed",
                BlackboardValue::Float(elapsed + 1.0 / 60.0),
            );
            NodeResult::Running
        }
    }
}

/// Checks if a cooldown has expired.
pub fn check_cooldown(
    key: &str,
    duration: f32,
) -> impl Fn(&mut Blackboard, &World, Entity) -> NodeResult + '_ {
    move |blackboard: &mut Blackboard, _world: &World, _entity: Entity| {
        let last_time = blackboard.get_float(key);
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs_f32())
            .unwrap_or(0.0);

        if current_time - last_time >= duration {
            blackboard.set(key, BlackboardValue::Float(current_time));
            NodeResult::Success
        } else {
            NodeResult::Failure
        }
    }
}

/// Sets a blackboard value.
pub fn set_value(
    key: &str,
    value: BlackboardValue,
) -> impl Fn(&mut Blackboard, &World, Entity) -> NodeResult + '_ {
    move |blackboard: &mut Blackboard, _world: &World, _entity: Entity| {
        blackboard.set(key, value.clone());
        NodeResult::Success
    }
}

/// Increments a blackboard integer.
pub fn increment(
    key: &str,
    amount: i64,
) -> impl Fn(&mut Blackboard, &World, Entity) -> NodeResult + '_ {
    move |blackboard: &mut Blackboard, _world: &World, _entity: Entity| {
        blackboard.increment(key, amount);
        NodeResult::Success
    }
}

/// Logs a message (for debugging).
pub fn log(message: &str) -> impl Fn(&mut Blackboard, &World, Entity) -> NodeResult + '_ {
    move |_blackboard: &mut Blackboard, _world: &World, _entity: Entity| {
        log::info!("[BehaviorTree] {}", message);
        NodeResult::Success
    }
}

// ---------------------------------------------------------------------------
// Common Condition Nodes
// ---------------------------------------------------------------------------

/// Checks if a blackboard bool is true.
pub fn is_true(key: &str) -> impl Fn(&Blackboard, &World, Entity) -> bool + '_ {
    move |blackboard: &Blackboard, _world: &World, _entity: Entity| blackboard.get_bool(key)
}

/// Checks if a blackboard bool is false.
pub fn is_false(key: &str) -> impl Fn(&Blackboard, &World, Entity) -> bool + '_ {
    move |blackboard: &Blackboard, _world: &World, _entity: Entity| !blackboard.get_bool(key)
}

/// Checks if a blackboard float is greater than a threshold.
pub fn is_greater_than(
    key: &str,
    threshold: f32,
) -> impl Fn(&Blackboard, &World, Entity) -> bool + '_ {
    move |blackboard: &Blackboard, _world: &World, _entity: Entity| {
        blackboard.get_float(key) > threshold
    }
}

/// Checks if a blackboard float is less than a threshold.
pub fn is_less_than(
    key: &str,
    threshold: f32,
) -> impl Fn(&Blackboard, &World, Entity) -> bool + '_ {
    move |blackboard: &Blackboard, _world: &World, _entity: Entity| {
        blackboard.get_float(key) < threshold
    }
}

/// Checks if a blackboard key equals a value.
pub fn equals_int(key: &str, expected: i64) -> impl Fn(&Blackboard, &World, Entity) -> bool + '_ {
    move |blackboard: &Blackboard, _world: &World, _entity: Entity| {
        blackboard.get_int(key) == expected
    }
}

/// Checks if a blackboard key exists.
pub fn has_key(key: &str) -> impl Fn(&Blackboard, &World, Entity) -> bool + '_ {
    move |blackboard: &Blackboard, _world: &World, _entity: Entity| blackboard.contains(key)
}

// ---------------------------------------------------------------------------
// Decorator Nodes
// ---------------------------------------------------------------------------

/// Inverter node - inverts the child result.
pub struct InverterNode;

impl InverterNode {
    pub fn new(child: Node) -> Node {
        Node::inverter(child)
    }
}

/// Repeater node - repeats child N times.
pub struct RepeaterNode;

impl RepeaterNode {
    pub fn new(child: Node, count: u32) -> Node {
        Node::repeater(child, Some(count))
    }

    pub fn forever(child: Node) -> Node {
        Node::repeat_until(child, NodeResult::Failure)
    }

    pub fn until_success(child: Node) -> Node {
        Node::repeat_until(child, NodeResult::Success)
    }
}

/// Timeout node - fails after duration.
pub struct TimeoutNode;

impl TimeoutNode {
    pub fn new(child: Node, duration_seconds: f32) -> Node {
        Node::timeout(child, duration_seconds)
    }
}

/// Wait for condition node.
pub struct WaitForNode;

impl WaitForNode {
    pub fn new(condition: &str) -> Node {
        Node::wait_for(condition)
    }
}

/// Parallel node - runs all children simultaneously.
pub struct ParallelNode;

impl ParallelNode {
    pub fn new(children: Vec<Node>) -> Node {
        let len = children.len();
        Node::Parallel {
            children,
            success_threshold: len,
            failure_threshold: 1,
        }
    }

    pub fn require_all(children: Vec<Node>) -> Node {
        let len = children.len();
        Node::Parallel {
            children,
            success_threshold: len,
            failure_threshold: 1,
        }
    }

    pub fn require_one(children: Vec<Node>) -> Node {
        let len = children.len();
        Node::Parallel {
            children,
            success_threshold: 1,
            failure_threshold: len,
        }
    }
}

/// Random selector - picks one child at random.
pub struct RandomSelectorNode;

impl RandomSelectorNode {
    pub fn new(children: Vec<Node>) -> Node {
        Node::RandomSelector(children)
    }
}

// ---------------------------------------------------------------------------
// Node Builders
// ---------------------------------------------------------------------------

/// Builder for creating behavior trees.
pub struct TreeBuilder {
    root: Option<Node>,
}

impl TreeBuilder {
    pub fn new() -> Self {
        Self { root: None }
    }

    pub fn selector(mut self, children: Vec<Node>) -> Self {
        self.root = Some(Node::selector(children));
        self
    }

    pub fn sequence(mut self, children: Vec<Node>) -> Self {
        self.root = Some(Node::sequence(children));
        self
    }

    pub fn action(mut self, name: &str) -> Self {
        self.root = Some(Node::action(name));
        self
    }

    pub fn condition(mut self, name: &str) -> Self {
        self.root = Some(Node::condition(name));
        self
    }

    pub fn build(self) -> Option<Node> {
        self.root
    }
}

impl Default for TreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for selector nodes.
pub struct SelectorBuilder {
    children: Vec<Node>,
}

impl SelectorBuilder {
    pub fn new() -> Self {
        Self { children: vec![] }
    }

    pub fn child(mut self, node: Node) -> Self {
        self.children.push(node);
        self
    }

    pub fn action(self, name: &str) -> Self {
        self.child(Node::action(name))
    }

    pub fn condition(self, name: &str) -> Self {
        self.child(Node::condition(name))
    }

    pub fn sequence(self, children: Vec<Node>) -> Self {
        self.child(Node::sequence(children))
    }

    pub fn build(self) -> Node {
        Node::selector(self.children)
    }
}

impl Default for SelectorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for sequence nodes.
pub struct SequenceBuilder {
    children: Vec<Node>,
}

impl SequenceBuilder {
    pub fn new() -> Self {
        Self { children: vec![] }
    }

    pub fn child(mut self, node: Node) -> Self {
        self.children.push(node);
        self
    }

    pub fn action(self, name: &str) -> Self {
        self.child(Node::action(name))
    }

    pub fn condition(self, name: &str) -> Self {
        self.child(Node::condition(name))
    }

    pub fn set(self, key: &str, value: BlackboardValue) -> Self {
        self.child(Node::set(key, value))
    }

    pub fn wait_for(self, condition: &str) -> Self {
        self.child(Node::wait_for(condition))
    }

    pub fn build(self) -> Node {
        Node::sequence(self.children)
    }
}

impl Default for SequenceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::behavior_tree::{BehaviorTree, NodeState};

    #[test]
    fn wait_action() {
        let action = wait(0.1);
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();

        let result = action(&mut bb, &world, entity);
        assert_eq!(result, NodeResult::Running);
    }

    #[test]
    fn is_true_condition() {
        let cond = is_true("flag");
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();

        assert!(!cond(&bb, &world, entity));

        bb.set("flag", BlackboardValue::Bool(true));
        assert!(cond(&bb, &world, entity));
    }

    #[test]
    fn is_greater_than_condition() {
        let cond = is_greater_than("health", 50.0);
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();

        bb.set("health", BlackboardValue::Float(75.0));
        assert!(cond(&bb, &world, entity));

        bb.set("health", BlackboardValue::Float(25.0));
        assert!(!cond(&bb, &world, entity));
    }

    #[test]
    fn selector_builder() {
        let node = SelectorBuilder::new()
            .condition("has_target")
            .action("find_target")
            .build();

        let tree = BehaviorTree::new(node);
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Failure);
    }

    #[test]
    fn sequence_builder() {
        let node = SequenceBuilder::new()
            .set("counter", BlackboardValue::Int(0))
            .condition("ready")
            .build();

        let tree = BehaviorTree::new(node);
        let mut bb = Blackboard::new();
        bb.set("ready", BlackboardValue::Bool(true));
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
        assert_eq!(bb.get_int("counter"), 0);
    }

    #[test]
    fn parallel_require_all() {
        let node = ParallelNode::require_all(vec![Node::success(), Node::success()]);
        let tree = BehaviorTree::new(node);
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn parallel_require_one() {
        let node = ParallelNode::require_one(vec![Node::success(), Node::failure()]);
        let tree = BehaviorTree::new(node);
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }
}
