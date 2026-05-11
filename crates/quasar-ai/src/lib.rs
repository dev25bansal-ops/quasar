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

pub mod behavior_tree;
pub mod blackboard;
pub mod debug;
pub mod editor;
pub mod goap;
pub mod navigation;
pub mod sensors;
pub mod steering;
pub mod utility;

pub use behavior_tree::{BehaviorTree, BtContext, BtNode, BtState, BtStatus};
pub use blackboard::{Blackboard, BlackboardKey, BlackboardValue};
pub use debug::{AiDebugger, DebugDraw};
pub use editor::{AiAgentConfig, AiAgentRegistry, AiEditorManager, EditorBehaviorTree};
pub use goap::{GoapAction, GoapGoal, GoapPlanner, GoapWorldState};
pub use navigation::{NavAgent, PathRequest, PathResult};
pub use sensors::{AwarenessLevel, Perception, SensorSystem};
pub use steering::{SteeringBehavior, SteeringOutput};
pub use utility::{Consideration, ResponseCurve, UtilityAction, UtilityBrain};

pub mod prelude {
    pub use crate::behavior_tree::{BehaviorTree, BtContext, BtNode, BtState, BtStatus};
    pub use crate::blackboard::{Blackboard, BlackboardKey, BlackboardValue};
    pub use crate::debug::{AiDebugger, DebugDraw};
    pub use crate::editor::{AiAgentConfig, AiAgentRegistry, AiEditorManager, EditorBehaviorTree};
    pub use crate::goap::{GoapAction, GoapGoal, GoapPlanner, GoapWorldState};
    pub use crate::navigation::{NavAgent, PathRequest, PathResult};
    pub use crate::sensors::{AwarenessLevel, Perception, SensorSystem};
    pub use crate::steering::{SteeringBehavior, SteeringOutput};
    pub use crate::utility::{Consideration, ResponseCurve, UtilityAction, UtilityBrain};
}
