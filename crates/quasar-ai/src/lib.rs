//! # Quasar AI Systems
//!
//! Comprehensive AI framework for game development:
//! - **GOAP** (Goal-Oriented Action Planning) - A* planning for intelligent agents
//! - **Utility AI** - Score-based decision making for emergent behavior
//! - **Behavior Trees** - Hierarchical task execution
//! - **Navigation** - Pathfinding and steering behaviors
//! - **Blackboard** - Shared knowledge system
//! - **Sensors** - Perception and awareness
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use quasar_ai::prelude::*;
//!
//! // Create a GOAP agent
//! let mut planner = GoapPlanner::new();
//! let goal = GoapGoal::new("attack_player")
//!     .require("player_visible", true)
//!     .require("in_attack_range", true);
//!
//! let plan = planner.plan(&world_state, &goal, &actions)?;
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod blackboard;
pub mod behavior_tree;
pub mod goap;
pub mod utility;
pub mod sensors;
pub mod navigation;
pub mod steering;
pub mod debug;

pub use blackboard::{Blackboard, BlackboardKey, BlackboardValue};
pub use behavior_tree::{BehaviorTree, BtNode, BtStatus, BtContext, BtState};
pub use goap::{GoapPlanner, GoapAction, GoapGoal, GoapWorldState};
pub use utility::{UtilityBrain, UtilityAction, Consideration, ResponseCurve};
pub use sensors::{SensorSystem, Perception, AwarenessLevel};
pub use navigation::{NavAgent, PathRequest, PathResult};
pub use steering::{SteeringBehavior, SteeringOutput};
pub use debug::{AiDebugger, DebugDraw};

pub mod prelude {
    pub use crate::blackboard::{Blackboard, BlackboardKey, BlackboardValue};
    pub use crate::behavior_tree::{BehaviorTree, BtNode, BtStatus, BtContext, BtState};
    pub use crate::goap::{GoapPlanner, GoapAction, GoapGoal, GoapWorldState};
    pub use crate::utility::{UtilityBrain, UtilityAction, Consideration, ResponseCurve};
    pub use crate::sensors::{SensorSystem, Perception, AwarenessLevel};
    pub use crate::navigation::{NavAgent, PathRequest, PathResult};
    pub use crate::steering::{SteeringBehavior, SteeringOutput};
    pub use crate::debug::{AiDebugger, DebugDraw};
}
