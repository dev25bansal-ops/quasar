//! Behavior Tree AI System for Quasar Engine.
//!
//! **DEPRECATED**: This module is deprecated. Use the `quasar-ai` crate instead,
//! which provides more comprehensive AI systems including GOAP, Utility AI,
//! Behavior Trees, Navigation, Steering, and Sensors.
//!
//! ``toml
//! # Add to Cargo.toml
//! quasar-ai = { workspace = true }
//! ``
//!
//! # Migration Guide
//!
//! | quasar_core::ai | quasar_ai |
//! |-----------------|-----------|
//! | BehaviorTree | BehaviorTree |
//! | Node | BtNode |
//! | Blackboard | Blackboard |
//! | BlackboardValue | BlackboardValue |
//! | NodeResult | BtStatus |
//!
//! See `quasar-ai` documentation for full API.

#![deprecated(since = "0.1.0", note = "Use quasar-ai crate instead for comprehensive AI systems")]

mod behavior_tree;
mod blackboard;
mod nodes;

#[allow(deprecated)]
pub use behavior_tree::{
    BehaviorTree, BehaviorTreePlugin, BehaviorTreeRunner, BehaviorTreeSystem, Node, NodeResult,
};
#[allow(deprecated)]
pub use blackboard::{Blackboard, BlackboardValue};
#[allow(deprecated)]
pub use nodes::{
    ActionNode, ConditionNode, DecoratorNode, InverterNode, ParallelNode, RandomSelectorNode,
    RepeaterNode, TimeoutNode, WaitForNode,
};
