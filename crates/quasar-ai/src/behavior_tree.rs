//! Behavior Tree Implementation
//!
//! Hierarchical task execution for game AI.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BtStatus {
    Running,
    Success,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BtNode {
    Sequence {
        children: Vec<BtNode>,
    },
    Selector {
        children: Vec<BtNode>,
    },
    Parallel {
        children: Vec<BtNode>,
        policy: ParallelPolicy,
    },
    Inverter {
        child: Box<BtNode>,
    },
    Repeater {
        child: Box<BtNode>,
        count: Option<u32>,
    },
    Retry {
        child: Box<BtNode>,
        max_tries: u32,
    },
    Timeout {
        child: Box<BtNode>,
        duration_secs: f32,
    },
    Condition {
        key: String,
        expected: crate::blackboard::BlackboardValue,
    },
    Action {
        name: String,
    },
    Wait {
        duration_secs: f32,
    },
    Succeed,
    Fail,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParallelPolicy {
    RequireAll,
    RequireOne,
}

impl BtNode {
    pub fn sequence(children: Vec<Self>) -> Self {
        Self::Sequence { children }
    }

    pub fn selector(children: Vec<Self>) -> Self {
        Self::Selector { children }
    }

    pub fn parallel(children: Vec<Self>, policy: ParallelPolicy) -> Self {
        Self::Parallel { children, policy }
    }

    pub fn inverter(child: Self) -> Self {
        Self::Inverter {
            child: Box::new(child),
        }
    }

    pub fn repeater(child: Self, count: Option<u32>) -> Self {
        Self::Repeater {
            child: Box::new(child),
            count,
        }
    }

    pub fn retry(child: Self, max_tries: u32) -> Self {
        Self::Retry {
            child: Box::new(child),
            max_tries,
        }
    }

    pub fn timeout(child: Self, duration_secs: f32) -> Self {
        Self::Timeout {
            child: Box::new(child),
            duration_secs,
        }
    }

    pub fn condition(key: &str, expected: crate::blackboard::BlackboardValue) -> Self {
        Self::Condition {
            key: key.to_string(),
            expected,
        }
    }

    pub fn action(name: &str) -> Self {
        Self::Action {
            name: name.to_string(),
        }
    }

    pub fn wait(duration_secs: f32) -> Self {
        Self::Wait { duration_secs }
    }
}

pub struct BtContext<'a> {
    pub blackboard: &'a crate::blackboard::Blackboard,
    pub delta_time: f32,
    pub elapsed_time: f32,
}

pub struct BtState {
    running_node: Option<usize>,
    running_time: f32,
    retry_count: u32,
    repeater_count: u32,
    parallel_states: SmallVec<[BtStatus; 4]>,
}

impl Default for BtState {
    fn default() -> Self {
        Self::new()
    }
}

impl BtState {
    pub fn new() -> Self {
        Self {
            running_node: None,
            running_time: 0.0,
            retry_count: 0,
            repeater_count: 0,
            parallel_states: SmallVec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.running_node = None;
        self.running_time = 0.0;
        self.retry_count = 0;
        self.repeater_count = 0;
        self.parallel_states.clear();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorTree {
    root: BtNode,
    name: String,
}

impl BehaviorTree {
    pub fn new(name: &str, root: BtNode) -> Self {
        Self {
            root,
            name: name.to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn root(&self) -> &BtNode {
        &self.root
    }

    pub fn tick(&self, ctx: &BtContext, state: &mut BtState) -> BtStatus {
        self.tick_node(&self.root, ctx, state)
    }

    fn tick_node(&self, node: &BtNode, ctx: &BtContext, state: &mut BtState) -> BtStatus {
        match node {
            BtNode::Sequence { children } => self.tick_sequence(children, ctx, state),
            BtNode::Selector { children } => self.tick_selector(children, ctx, state),
            BtNode::Parallel { children, policy } => {
                self.tick_parallel(children, *policy, ctx, state)
            }
            BtNode::Inverter { child } => match self.tick_node(child, ctx, state) {
                BtStatus::Success => BtStatus::Failure,
                BtStatus::Failure => BtStatus::Success,
                BtStatus::Running => BtStatus::Running,
            },
            BtNode::Repeater { child, count } => {
                let max = count.unwrap_or(u32::MAX);
                if state.repeater_count >= max {
                    state.repeater_count = 0;
                    return BtStatus::Success;
                }
                match self.tick_node(child, ctx, state) {
                    BtStatus::Success => {
                        state.repeater_count += 1;
                        if state.repeater_count >= max {
                            state.repeater_count = 0;
                            BtStatus::Success
                        } else {
                            BtStatus::Running
                        }
                    }
                    BtStatus::Failure => {
                        state.repeater_count = 0;
                        BtStatus::Failure
                    }
                    BtStatus::Running => BtStatus::Running,
                }
            }
            BtNode::Retry { child, max_tries } => match self.tick_node(child, ctx, state) {
                BtStatus::Success => {
                    state.retry_count = 0;
                    BtStatus::Success
                }
                BtStatus::Failure => {
                    state.retry_count += 1;
                    if state.retry_count >= *max_tries {
                        state.retry_count = 0;
                        BtStatus::Failure
                    } else {
                        BtStatus::Running
                    }
                }
                BtStatus::Running => BtStatus::Running,
            },
            BtNode::Timeout {
                child,
                duration_secs,
            } => {
                state.running_time += ctx.delta_time;
                if state.running_time >= *duration_secs {
                    state.running_time = 0.0;
                    return BtStatus::Failure;
                }
                let status = self.tick_node(child, ctx, state);
                if status != BtStatus::Running {
                    state.running_time = 0.0;
                }
                status
            }
            BtNode::Condition { key, expected } => {
                if let Some(value) = ctx.blackboard.get(key) {
                    if value == expected {
                        BtStatus::Success
                    } else {
                        BtStatus::Failure
                    }
                } else {
                    BtStatus::Failure
                }
            }
            BtNode::Action { .. } => BtStatus::Running,
            BtNode::Wait { duration_secs } => {
                state.running_time += ctx.delta_time;
                if state.running_time >= *duration_secs {
                    state.running_time = 0.0;
                    BtStatus::Success
                } else {
                    BtStatus::Running
                }
            }
            BtNode::Succeed => BtStatus::Success,
            BtNode::Fail => BtStatus::Failure,
            BtNode::Running => BtStatus::Running,
        }
    }

    fn tick_sequence(&self, children: &[BtNode], ctx: &BtContext, state: &mut BtState) -> BtStatus {
        let start = state.running_node.unwrap_or(0);
        for (i, child) in children.iter().enumerate().skip(start) {
            state.running_node = Some(i);
            match self.tick_node(child, ctx, state) {
                BtStatus::Running => return BtStatus::Running,
                BtStatus::Failure => {
                    state.running_node = None;
                    return BtStatus::Failure;
                }
                BtStatus::Success => continue,
            }
        }
        state.running_node = None;
        BtStatus::Success
    }

    fn tick_selector(&self, children: &[BtNode], ctx: &BtContext, state: &mut BtState) -> BtStatus {
        let start = state.running_node.unwrap_or(0);
        for (i, child) in children.iter().enumerate().skip(start) {
            state.running_node = Some(i);
            match self.tick_node(child, ctx, state) {
                BtStatus::Running => return BtStatus::Running,
                BtStatus::Success => {
                    state.running_node = None;
                    return BtStatus::Success;
                }
                BtStatus::Failure => continue,
            }
        }
        state.running_node = None;
        BtStatus::Failure
    }

    fn tick_parallel(
        &self,
        children: &[BtNode],
        policy: ParallelPolicy,
        ctx: &BtContext,
        state: &mut BtState,
    ) -> BtStatus {
        if state.parallel_states.len() != children.len() {
            state.parallel_states = children.iter().map(|_| BtStatus::Running).collect();
        }

        let mut success_count = 0;
        let mut failure_count = 0;
        let mut running_count = 0;

        for (i, child) in children.iter().enumerate() {
            if state.parallel_states[i] == BtStatus::Running {
                let mut child_state = BtState::new();
                state.parallel_states[i] = self.tick_node(child, ctx, &mut child_state);
            }
            match state.parallel_states[i] {
                BtStatus::Success => success_count += 1,
                BtStatus::Failure => failure_count += 1,
                BtStatus::Running => running_count += 1,
            }
        }

        match policy {
            ParallelPolicy::RequireAll => {
                if success_count == children.len() {
                    state.parallel_states.clear();
                    BtStatus::Success
                } else if failure_count > 0 {
                    state.parallel_states.clear();
                    BtStatus::Failure
                } else {
                    BtStatus::Running
                }
            }
            ParallelPolicy::RequireOne => {
                if success_count > 0 {
                    state.parallel_states.clear();
                    BtStatus::Success
                } else if failure_count == children.len() {
                    state.parallel_states.clear();
                    BtStatus::Failure
                } else {
                    BtStatus::Running
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blackboard::{Blackboard, BlackboardValue};

    #[test]
    fn bt_succeed_node() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new("test", BtNode::Succeed);
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
    }

    #[test]
    fn bt_fail_node() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new("test", BtNode::Fail);
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Failure);
    }

    #[test]
    fn bt_sequence_all_success() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new(
            "test",
            BtNode::sequence(vec![BtNode::Succeed, BtNode::Succeed, BtNode::Succeed]),
        );
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
    }

    #[test]
    fn bt_sequence_one_failure() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new(
            "test",
            BtNode::sequence(vec![BtNode::Succeed, BtNode::Fail, BtNode::Succeed]),
        );
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Failure);
    }

    #[test]
    fn bt_selector_first_success() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new(
            "test",
            BtNode::selector(vec![BtNode::Succeed, BtNode::Fail]),
        );
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
    }

    #[test]
    fn bt_selector_try_all() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new(
            "test",
            BtNode::selector(vec![BtNode::Fail, BtNode::Fail, BtNode::Succeed]),
        );
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
    }

    #[test]
    fn bt_inverter() {
        let bb = Blackboard::new();
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new("test", BtNode::inverter(BtNode::Succeed));
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Failure);
    }

    #[test]
    fn bt_condition() {
        let mut bb = Blackboard::new();
        bb.set_bool("flag", true);
        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.016,
            elapsed_time: 0.0,
        };
        let tree = BehaviorTree::new(
            "test",
            BtNode::condition("flag", BlackboardValue::Bool(true)),
        );
        let mut state = BtState::new();
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
    }

    #[test]
    fn bt_wait() {
        let bb = Blackboard::new();
        let tree = BehaviorTree::new("test", BtNode::wait(0.1));
        let mut state = BtState::new();

        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.05,
            elapsed_time: 0.05,
        };
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Running);

        let ctx = BtContext {
            blackboard: &bb,
            delta_time: 0.06,
            elapsed_time: 0.11,
        };
        assert_eq!(tree.tick(&ctx, &mut state), BtStatus::Success);
    }
}
