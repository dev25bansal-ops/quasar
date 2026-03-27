//! Behavior Tree AI System for Quasar Engine.
//!
//! Behavior trees provide a modular, hierarchical way to define AI behavior.
//!
//! # Architecture
//!
//! - `BehaviorTree` - The tree structure that defines AI logic
//! - `Node` - Individual behavior nodes (Selector, Sequence, Action, etc.)
//! - `Blackboard` - Shared memory/context for AI decision making
//! - `BehaviorTreeRunner` - ECS component that executes trees
//!
//! # Example
//!
//! ```ignore
//! use quasar_core::ai::*;
//!
//! let tree = BehaviorTree::new(
//!     Node::Selector(vec![
//!         Node::Sequence(vec![
//!             Node::Condition("is_hungry"),
//!             Node::Action("find_food"),
//!             Node::Action("eat"),
//!         ]),
//!         Node::Sequence(vec![
//!             Node::Condition("is_tired"),
//!             Node::Action("find_bed"),
//!             Node::Action("sleep"),
//!         ]),
//!         Node::Action("wander"),
//!     ]),
//! );
//! ```

mod behavior_tree;
mod blackboard;
mod nodes;

pub use behavior_tree::{
    BehaviorTree, BehaviorTreePlugin, BehaviorTreeRunner, BehaviorTreeSystem, Node, NodeResult,
};
pub use blackboard::{Blackboard, BlackboardValue};
pub use nodes::{
    ActionNode, ConditionNode, DecoratorNode, InverterNode, ParallelNode, RandomSelectorNode,
    RepeaterNode, TimeoutNode, WaitForNode,
};
