//! Core Behavior Tree Implementation.
//!
//! Behavior trees are a hierarchical state machine pattern for AI.
//! Each node returns a status (Success, Failure, Running) and the
//! tree is traversed based on these results.

use crate::ecs::{Entity, System, World};

use super::blackboard::{Blackboard, BlackboardValue};

/// Result of executing a behavior tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeResult {
    /// The node completed successfully.
    Success,
    /// The node failed to complete.
    Failure,
    /// The node is still running (will be ticked again).
    Running,
}

impl NodeResult {
    pub fn is_success(&self) -> bool {
        matches!(self, NodeResult::Success)
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, NodeResult::Failure)
    }

    pub fn is_running(&self) -> bool {
        matches!(self, NodeResult::Running)
    }

    pub fn invert(self) -> Self {
        match self {
            NodeResult::Success => NodeResult::Failure,
            NodeResult::Failure => NodeResult::Success,
            NodeResult::Running => NodeResult::Running,
        }
    }
}

/// Action function type for custom behavior.
#[allow(dead_code)]
pub type ActionFn = fn(&mut Blackboard, &World, Entity) -> NodeResult;

/// Condition function type for custom checks.
#[allow(dead_code)]
pub type ConditionFn = fn(&Blackboard, &World, Entity) -> bool;

/// A node in the behavior tree.
#[derive(Clone)]
pub enum Node {
    /// Executes children in order until one succeeds (OR logic).
    Selector(Vec<Node>),
    /// Executes children in order until one fails (AND logic).
    Sequence(Vec<Node>),
    /// Executes all children in parallel.
    Parallel {
        children: Vec<Node>,
        success_threshold: usize,
        failure_threshold: usize,
    },
    /// Inverts the result of its child.
    Inverter(Box<Node>),
    /// Repeats its child N times or until failure.
    Repeater {
        child: Box<Node>,
        count: Option<u32>,
        until: Option<NodeResult>,
    },
    /// Always succeeds, regardless of child result.
    Succeeder(Box<Node>),
    /// Runs child with a timeout.
    Timeout {
        child: Box<Node>,
        duration_seconds: f32,
    },
    /// Waits for a condition to become true.
    WaitFor { condition: String },
    /// A custom action node.
    Action {
        name: String,
        #[allow(clippy::type_complexity)]
        function: Option<fn(&mut Blackboard, &World, Entity) -> NodeResult>,
    },
    /// A condition check.
    Condition {
        name: String,
        function: Option<fn(&Blackboard, &World, Entity) -> bool>,
        expected: bool,
    },
    /// Set a blackboard value.
    Set { key: String, value: BlackboardValue },
    /// Increment a blackboard integer.
    Increment { key: String, amount: i64 },
    /// Random selector - picks one child at random.
    RandomSelector(Vec<Node>),
    /// Random sequence - shuffles children before executing.
    RandomSequence(Vec<Node>),
    /// Decorator that limits how often the child can run.
    Cooldown {
        child: Box<Node>,
        duration_seconds: f32,
    },
    /// Executes child only if condition is true.
    Guard { condition: String, child: Box<Node> },
    /// Always returns the specified result.
    Constant(NodeResult),
    /// No-op node (always succeeds).
    Noop,
}

impl Node {
    pub fn selector(children: Vec<Node>) -> Self {
        Node::Selector(children)
    }

    pub fn sequence(children: Vec<Node>) -> Self {
        Node::Sequence(children)
    }

    pub fn action(name: &str) -> Self {
        Node::Action {
            name: name.to_string(),
            function: None,
        }
    }

    pub fn action_with_fn(
        name: &str,
        f: fn(&mut Blackboard, &World, Entity) -> NodeResult,
    ) -> Self {
        Node::Action {
            name: name.to_string(),
            function: Some(f),
        }
    }

    pub fn condition(name: &str) -> Self {
        Node::Condition {
            name: name.to_string(),
            function: None,
            expected: true,
        }
    }

    pub fn condition_not(name: &str) -> Self {
        Node::Condition {
            name: name.to_string(),
            function: None,
            expected: false,
        }
    }

    pub fn condition_with_fn(
        name: &str,
        expected: bool,
        f: fn(&Blackboard, &World, Entity) -> bool,
    ) -> Self {
        Node::Condition {
            name: name.to_string(),
            function: Some(f),
            expected,
        }
    }

    pub fn set(key: &str, value: BlackboardValue) -> Self {
        Node::Set {
            key: key.to_string(),
            value,
        }
    }

    pub fn wait_for(condition: &str) -> Self {
        Node::WaitFor {
            condition: condition.to_string(),
        }
    }

    pub fn inverter(child: Node) -> Self {
        Node::Inverter(Box::new(child))
    }

    pub fn repeater(child: Node, count: Option<u32>) -> Self {
        Node::Repeater {
            child: Box::new(child),
            count,
            until: None,
        }
    }

    pub fn repeat_until(child: Node, result: NodeResult) -> Self {
        Node::Repeater {
            child: Box::new(child),
            count: None,
            until: Some(result),
        }
    }

    pub fn succeeder(child: Node) -> Self {
        Node::Succeeder(Box::new(child))
    }

    pub fn timeout(child: Node, seconds: f32) -> Self {
        Node::Timeout {
            child: Box::new(child),
            duration_seconds: seconds,
        }
    }

    pub fn cooldown(child: Node, seconds: f32) -> Self {
        Node::Cooldown {
            child: Box::new(child),
            duration_seconds: seconds,
        }
    }

    pub fn guard(condition: &str, child: Node) -> Self {
        Node::Guard {
            condition: condition.to_string(),
            child: Box::new(child),
        }
    }

    pub fn constant(result: NodeResult) -> Self {
        Node::Constant(result)
    }

    pub fn success() -> Self {
        Node::Constant(NodeResult::Success)
    }

    pub fn failure() -> Self {
        Node::Constant(NodeResult::Failure)
    }

    pub fn running() -> Self {
        Node::Constant(NodeResult::Running)
    }
}

/// State tracked for running nodes.
#[derive(Debug, Clone, Default)]
pub struct NodeState {
    pub running_child: Option<usize>,
    pub elapsed_time: f32,
    pub repeat_count: u32,
    pub cooldown_time: f32,
    pub parallel_results: Vec<Option<NodeResult>>,
}

/// A complete behavior tree.
#[derive(Clone)]
pub struct BehaviorTree {
    pub root: Node,
    pub name: String,
}

impl BehaviorTree {
    pub fn new(root: Node) -> Self {
        Self {
            root,
            name: "tree".to_string(),
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Tick the tree once. Returns the root result.
    pub fn tick(
        &self,
        blackboard: &mut Blackboard,
        world: &World,
        entity: Entity,
        state: &mut NodeState,
    ) -> NodeResult {
        self.tick_node(&self.root, blackboard, world, entity, state)
    }

    fn tick_node(
        &self,
        node: &Node,
        blackboard: &mut Blackboard,
        world: &World,
        entity: Entity,
        state: &mut NodeState,
    ) -> NodeResult {
        match node {
            Node::Selector(children) => {
                if let Some(running) = state.running_child {
                    if running < children.len() {
                        let result =
                            self.tick_node(&children[running], blackboard, world, entity, state);
                        if result == NodeResult::Running {
                            return NodeResult::Running;
                        }
                        if result == NodeResult::Success {
                            state.running_child = None;
                            return NodeResult::Success;
                        }
                    }
                }

                let start = state.running_child.unwrap_or(0);
                for (i, child) in children.iter().enumerate().skip(start) {
                    state.running_child = Some(i);
                    let result = self.tick_node(child, blackboard, world, entity, state);
                    if result != NodeResult::Failure {
                        if result == NodeResult::Running {
                            return NodeResult::Running;
                        }
                        state.running_child = None;
                        return NodeResult::Success;
                    }
                }
                state.running_child = None;
                NodeResult::Failure
            }

            Node::Sequence(children) => {
                if let Some(running) = state.running_child {
                    if running < children.len() {
                        let result =
                            self.tick_node(&children[running], blackboard, world, entity, state);
                        if result == NodeResult::Running {
                            return NodeResult::Running;
                        }
                        if result == NodeResult::Failure {
                            state.running_child = None;
                            return NodeResult::Failure;
                        }
                    }
                }

                let start = state.running_child.map_or(0, |r| r + 1);
                for (i, child) in children.iter().enumerate().skip(start) {
                    state.running_child = Some(i);
                    let result = self.tick_node(child, blackboard, world, entity, state);
                    if result != NodeResult::Success {
                        if result == NodeResult::Running {
                            return NodeResult::Running;
                        }
                        state.running_child = None;
                        return NodeResult::Failure;
                    }
                }
                state.running_child = None;
                NodeResult::Success
            }

            Node::Parallel {
                children,
                success_threshold,
                failure_threshold,
            } => {
                if state.parallel_results.len() != children.len() {
                    state.parallel_results = vec![None; children.len()];
                }

                let mut successes = 0;
                let mut failures = 0;
                let mut running = false;

                for (i, child) in children.iter().enumerate() {
                    if state.parallel_results[i].is_none() {
                        let result = self.tick_node(child, blackboard, world, entity, state);
                        if result == NodeResult::Running {
                            state.parallel_results[i] = None;
                            running = true;
                        } else {
                            state.parallel_results[i] = Some(result);
                        }
                    }

                    match state.parallel_results[i] {
                        Some(NodeResult::Success) => successes += 1,
                        Some(NodeResult::Failure) => failures += 1,
                        None => running = true,
                        _ => {}
                    }

                    if successes >= *success_threshold {
                        state.parallel_results.clear();
                        return NodeResult::Success;
                    }
                    if failures >= *failure_threshold {
                        state.parallel_results.clear();
                        return NodeResult::Failure;
                    }
                }

                if running {
                    NodeResult::Running
                } else {
                    state.parallel_results.clear();
                    if successes >= *success_threshold {
                        NodeResult::Success
                    } else {
                        NodeResult::Failure
                    }
                }
            }

            Node::Inverter(child) => self
                .tick_node(child, blackboard, world, entity, state)
                .invert(),

            Node::Repeater {
                child,
                count,
                until,
            } => {
                let result = self.tick_node(child, blackboard, world, entity, state);

                if let Some(target) = until {
                    if result == *target {
                        return NodeResult::Success;
                    }
                }

                if result == NodeResult::Running {
                    return NodeResult::Running;
                }

                if result == NodeResult::Failure && count.is_none() && until.is_none() {
                    return NodeResult::Failure;
                }

                state.repeat_count += 1;
                if let Some(max) = count {
                    if state.repeat_count >= *max {
                        state.repeat_count = 0;
                        return NodeResult::Success;
                    }
                }

                NodeResult::Running
            }

            Node::Succeeder(child) => {
                let result = self.tick_node(child, blackboard, world, entity, state);
                if result == NodeResult::Running {
                    NodeResult::Running
                } else {
                    NodeResult::Success
                }
            }

            Node::Timeout {
                child,
                duration_seconds,
            } => {
                state.elapsed_time += 1.0 / 60.0;
                if state.elapsed_time >= *duration_seconds {
                    state.elapsed_time = 0.0;
                    return NodeResult::Failure;
                }

                let result = self.tick_node(child, blackboard, world, entity, state);
                if result != NodeResult::Running {
                    state.elapsed_time = 0.0;
                }
                result
            }

            Node::WaitFor { condition } => {
                if blackboard.get_bool(condition) {
                    NodeResult::Success
                } else {
                    NodeResult::Running
                }
            }

            Node::Action { name, function } => {
                if let Some(f) = function {
                    f(blackboard, world, entity)
                } else {
                    if blackboard
                        .get_bool(name) { NodeResult::Success } else { NodeResult::Failure }
                }
            }

            Node::Condition {
                name,
                function,
                expected,
            } => {
                let result = if let Some(f) = function {
                    f(blackboard, world, entity)
                } else {
                    blackboard.get_bool(name)
                };

                if result == *expected {
                    NodeResult::Success
                } else {
                    NodeResult::Failure
                }
            }

            Node::Set { key, value } => {
                blackboard.set(key, value.clone());
                NodeResult::Success
            }

            Node::Increment { key, amount } => {
                blackboard.increment(key, *amount);
                NodeResult::Success
            }

            Node::RandomSelector(children) => {
                if children.is_empty() {
                    return NodeResult::Failure;
                }

                if let Some(running) = state.running_child {
                    if running < children.len() {
                        let result =
                            self.tick_node(&children[running], blackboard, world, entity, state);
                        if result == NodeResult::Running {
                            return NodeResult::Running;
                        }
                        state.running_child = None;
                        return result;
                    }
                }

                use std::time::SystemTime;
                let idx = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as usize % children.len())
                    .unwrap_or(0);

                state.running_child = Some(idx);
                let result = self.tick_node(&children[idx], blackboard, world, entity, state);
                if result != NodeResult::Running {
                    state.running_child = None;
                }
                result
            }

            Node::RandomSequence(children) => {
                if children.is_empty() {
                    return NodeResult::Success;
                }

                if state.parallel_results.is_empty() {
                    state.parallel_results = vec![Some(NodeResult::Success); children.len()];
                    for i in 0..children.len() {
                        state.parallel_results[i] = None;
                    }
                }

                for (i, child) in children.iter().enumerate() {
                    if state.parallel_results[i].is_none() {
                        let result = self.tick_node(child, blackboard, world, entity, state);
                        if result == NodeResult::Running {
                            return NodeResult::Running;
                        }
                        if result == NodeResult::Failure {
                            state.parallel_results.clear();
                            return NodeResult::Failure;
                        }
                        state.parallel_results[i] = Some(result);
                    }
                }

                state.parallel_results.clear();
                NodeResult::Success
            }

            Node::Cooldown {
                child,
                duration_seconds,
            } => {
                state.cooldown_time += 1.0 / 60.0;
                if state.cooldown_time < *duration_seconds {
                    return NodeResult::Failure;
                }

                let result = self.tick_node(child, blackboard, world, entity, state);
                if result != NodeResult::Running {
                    state.cooldown_time = 0.0;
                }
                result
            }

            Node::Guard { condition, child } => {
                if !blackboard.get_bool(condition) {
                    return NodeResult::Failure;
                }
                self.tick_node(child, blackboard, world, entity, state)
            }

            Node::Constant(result) => *result,

            Node::Noop => NodeResult::Success,
        }
    }
}

/// ECS component that runs a behavior tree.
#[derive(Clone)]
pub struct BehaviorTreeRunner {
    pub tree: BehaviorTree,
    pub blackboard: Blackboard,
    pub state: NodeState,
    pub enabled: bool,
    pub tick_interval: f32,
    pub time_since_tick: f32,
}

impl BehaviorTreeRunner {
    pub fn new(tree: BehaviorTree) -> Self {
        Self {
            tree,
            blackboard: Blackboard::new(),
            state: NodeState::default(),
            enabled: true,
            tick_interval: 0.0,
            time_since_tick: 0.0,
        }
    }

    pub fn with_blackboard(mut self, blackboard: Blackboard) -> Self {
        self.blackboard = blackboard;
        self
    }

    pub fn with_tick_interval(mut self, interval: f32) -> Self {
        self.tick_interval = interval;
        self
    }

    pub fn tick(&mut self, world: &World, entity: Entity) -> NodeResult {
        if !self.enabled {
            return NodeResult::Success;
        }
        self.tree
            .tick(&mut self.blackboard, world, entity, &mut self.state)
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.blackboard.set(key, BlackboardValue::Bool(value));
    }

    pub fn set_float(&mut self, key: &str, value: f32) {
        self.blackboard.set(key, BlackboardValue::Float(value));
    }

    pub fn set_int(&mut self, key: &str, value: i64) {
        self.blackboard.set(key, BlackboardValue::Int(value));
    }

    pub fn reset(&mut self) {
        self.state = NodeState::default();
    }
}

/// System that runs behavior trees.
pub struct BehaviorTreeSystem;

impl System for BehaviorTreeSystem {
    fn name(&self) -> &str {
        "behavior_tree"
    }

    fn run(&mut self, world: &mut World) {
        let delta = 1.0 / 60.0;

        let entities: Vec<(Entity, BehaviorTreeRunner)> = world
            .query::<BehaviorTreeRunner>()
            .into_iter()
            .map(|(e, r)| (e, r.clone()))
            .collect();

        for (entity, mut runner) in entities {
            runner.time_since_tick += delta;

            if runner.tick_interval > 0.0 && runner.time_since_tick < runner.tick_interval {
                if let Some(r) = world.get_mut::<BehaviorTreeRunner>(entity) {
                    *r = runner;
                }
                continue;
            }

            runner.time_since_tick = 0.0;
            runner.tick(world, entity);

            if let Some(r) = world.get_mut::<BehaviorTreeRunner>(entity) {
                *r = runner;
            }
        }
    }
}

/// Plugin for behavior tree system.
pub struct BehaviorTreePlugin;

impl crate::Plugin for BehaviorTreePlugin {
    fn name(&self) -> &str {
        "BehaviorTreePlugin"
    }

    fn build(&self, app: &mut crate::App) {
        app.schedule.add_system(
            crate::ecs::SystemStage::Update,
            Box::new(BehaviorTreeSystem),
        );
        log::info!("BehaviorTreePlugin loaded — AI behavior system active");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_result_invert() {
        assert_eq!(NodeResult::Success.invert(), NodeResult::Failure);
        assert_eq!(NodeResult::Failure.invert(), NodeResult::Success);
        assert_eq!(NodeResult::Running.invert(), NodeResult::Running);
    }

    #[test]
    fn behavior_tree_constant() {
        let tree = BehaviorTree::new(Node::success());
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn behavior_tree_selector() {
        let tree = BehaviorTree::new(Node::selector(vec![
            Node::failure(),
            Node::success(),
            Node::failure(),
        ]));
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn behavior_tree_sequence() {
        let tree = BehaviorTree::new(Node::sequence(vec![
            Node::success(),
            Node::success(),
            Node::failure(),
        ]));
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Failure);
    }

    #[test]
    fn behavior_tree_inverter() {
        let tree = BehaviorTree::new(Node::inverter(Node::failure()));
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn behavior_tree_set_blackboard() {
        let tree = BehaviorTree::new(Node::set("test", BlackboardValue::Int(42)));
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(bb.get_int("test"), 42);
    }

    #[test]
    fn behavior_tree_condition_from_blackboard() {
        let tree = BehaviorTree::new(Node::condition("is_valid"));
        let mut bb = Blackboard::new();
        bb.set("is_valid", BlackboardValue::Bool(true));
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn behavior_tree_runner() {
        let tree = BehaviorTree::new(Node::success());
        let mut runner = BehaviorTreeRunner::new(tree);
        let mut world = World::new();
        let entity = world.spawn();

        let result = runner.tick(&world, entity);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn behavior_tree_succeeder() {
        let tree = BehaviorTree::new(Node::succeeder(Node::failure()));
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);
    }

    #[test]
    fn behavior_tree_guard() {
        let tree = BehaviorTree::new(Node::guard("can_act", Node::success()));
        let mut bb = Blackboard::new();
        bb.set("can_act", BlackboardValue::Bool(true));
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Success);

        bb.set("can_act", BlackboardValue::Bool(false));
        let result = tree.tick(&mut bb, &world, entity, &mut state);
        assert_eq!(result, NodeResult::Failure);
    }

    #[test]
    fn behavior_tree_repeater_count() {
        let tree = BehaviorTree::new(Node::repeater(Node::success(), Some(3)));
        let mut bb = Blackboard::new();
        let mut world = World::new();
        let entity = world.spawn();
        let mut state = NodeState::default();

        let mut result = NodeResult::Running;
        for _ in 0..3 {
            result = tree.tick(&mut bb, &world, entity, &mut state);
        }
        assert_eq!(result, NodeResult::Success);
    }
}
