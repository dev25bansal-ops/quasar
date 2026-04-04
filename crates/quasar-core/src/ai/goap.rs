//! GOAP (Goal-Oriented Action Planning) AI System.
//!
//! Provides:
//! - Goal-based action planning
//! - World state representation
//! - A* planner for action sequences
//! - Action costs and preconditions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// World state key-value storage.
pub type WorldState = HashMap<String, WorldValue>;

/// World state value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorldValue {
    Bool(bool),
    Int(i64),
    Float(f32),
    String(String),
    Vector([f32; 3]),
    Entity(u64),
}

impl WorldValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn matches(&self, other: &WorldValue) -> bool {
        self == other
    }
}

/// GOAP Action definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapAction {
    /// Action ID.
    pub id: String,
    /// Action name for debugging.
    pub name: String,
    /// Cost of executing this action.
    pub cost: f32,
    /// Preconditions (world state must match).
    pub preconditions: HashMap<String, WorldValue>,
    /// Effects (world state changes).
    pub effects: HashMap<String, WorldValue>,
    /// Required target type.
    pub target_type: Option<String>,
    /// Maximum distance to target.
    pub max_distance: Option<f32>,
    /// Duration in seconds.
    pub duration: f32,
    /// Priority boost when this action is relevant.
    pub priority: f32,
}

impl GoapAction {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            cost: 1.0,
            preconditions: HashMap::new(),
            effects: HashMap::new(),
            target_type: None,
            max_distance: None,
            duration: 0.0,
            priority: 0.0,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_cost(mut self, cost: f32) -> Self {
        self.cost = cost;
        self
    }

    pub fn precondition(mut self, key: impl Into<String>, value: WorldValue) -> Self {
        self.preconditions.insert(key.into(), value);
        self
    }

    pub fn effect(mut self, key: impl Into<String>, value: WorldValue) -> Self {
        self.effects.insert(key.into(), value);
        self
    }

    pub fn with_target(mut self, target_type: impl Into<String>, max_distance: f32) -> Self {
        self.target_type = Some(target_type.into());
        self.max_distance = Some(max_distance);
        self
    }

    /// Check if preconditions are met.
    pub fn can_execute(&self, world_state: &WorldState) -> bool {
        self.preconditions.iter().all(|(key, value)| {
            world_state
                .get(key)
                .map(|v| v.matches(value))
                .unwrap_or(false)
        })
    }

    /// Apply effects to world state.
    pub fn apply_effects(&self, world_state: &mut WorldState) {
        for (key, value) in &self.effects {
            world_state.insert(key.clone(), value.clone());
        }
    }

    /// Simulate effects and return new state.
    pub fn simulate_effects(&self, world_state: &WorldState) -> WorldState {
        let mut new_state = world_state.clone();
        self.apply_effects(&mut new_state);
        new_state
    }
}

/// GOAP Goal definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapGoal {
    /// Goal ID.
    pub id: String,
    /// Goal name for debugging.
    pub name: String,
    /// Desired world state.
    pub desired_state: HashMap<String, WorldValue>,
    /// Priority of this goal.
    pub priority: f32,
    /// Is this goal interruptible.
    pub interruptible: bool,
}

impl GoapGoal {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            desired_state: HashMap::new(),
            priority: 1.0,
            interruptible: true,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn desire(mut self, key: impl Into<String>, value: WorldValue) -> Self {
        self.desired_state.insert(key.into(), value);
        self
    }

    pub fn with_priority(mut self, priority: f32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if goal is satisfied.
    pub fn is_satisfied(&self, world_state: &WorldState) -> bool {
        self.desired_state.iter().all(|(key, value)| {
            world_state
                .get(key)
                .map(|v| v.matches(value))
                .unwrap_or(false)
        })
    }
}

/// Planner node for A* search.
#[derive(Debug, Clone)]
struct PlannerNode {
    world_state: WorldState,
    actions: Vec<String>,
    g_cost: f32,
    h_cost: f32,
}

impl PlannerNode {
    fn f_cost(&self) -> f32 {
        self.g_cost + self.h_cost
    }
}

/// GOAP Planner.
pub struct GoapPlanner {
    /// All available actions.
    pub actions: HashMap<String, GoapAction>,
    /// Maximum search depth.
    pub max_depth: usize,
    /// Maximum nodes to explore.
    pub max_nodes: usize,
}

impl GoapPlanner {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
            max_depth: 10,
            max_nodes: 1000,
        }
    }

    pub fn add_action(&mut self, action: GoapAction) {
        self.actions.insert(action.id.clone(), action);
    }

    /// Plan a sequence of actions to achieve a goal.
    pub fn plan(&self, current_state: &WorldState, goal: &GoapGoal) -> Option<Vec<GoapAction>> {
        if goal.is_satisfied(current_state) {
            return Some(Vec::new());
        }

        // A* search
        let mut open_list: Vec<PlannerNode> = vec![PlannerNode {
            world_state: current_state.clone(),
            actions: Vec::new(),
            g_cost: 0.0,
            h_cost: self.heuristic(&current_state, goal),
        }];

        let mut closed_states: Vec<WorldState> = Vec::new();
        let mut nodes_explored = 0;

        while !open_list.is_empty() && nodes_explored < self.max_nodes {
            // Get node with lowest f_cost
            open_list.sort_by(|a, b| {
                a.f_cost()
                    .partial_cmp(&b.f_cost())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let current = open_list.remove(0);
            nodes_explored += 1;

            // Check if goal is satisfied
            if goal.is_satisfied(&current.world_state) {
                return Some(
                    current
                        .actions
                        .iter()
                        .filter_map(|id| self.actions.get(id).cloned())
                        .collect(),
                );
            }

            // Limit depth
            if current.actions.len() >= self.max_depth {
                continue;
            }

            // Skip if already visited
            if closed_states
                .iter()
                .any(|s| self.states_equal(s, &current.world_state))
            {
                continue;
            }
            closed_states.push(current.world_state.clone());

            // Try all applicable actions
            for action in self.actions.values() {
                if action.can_execute(&current.world_state) {
                    let new_state = action.simulate_effects(&current.world_state);

                    if closed_states
                        .iter()
                        .any(|s| self.states_equal(s, &new_state))
                    {
                        continue;
                    }

                    let mut new_actions = current.actions.clone();
                    new_actions.push(action.id.clone());

                    open_list.push(PlannerNode {
                        world_state: new_state,
                        actions: new_actions,
                        g_cost: current.g_cost + action.cost,
                        h_cost: self.heuristic(&current.world_state, goal),
                    });
                }
            }
        }

        None
    }

    fn heuristic(&self, state: &WorldState, goal: &GoapGoal) -> f32 {
        // Count unsatisfied goals
        goal.desired_state
            .iter()
            .filter(|(key, value)| state.get(*key).map(|v| !v.matches(value)).unwrap_or(true))
            .count() as f32
    }

    fn states_equal(&self, a: &WorldState, b: &WorldState) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter()
            .all(|(key, value)| b.get(key).map(|v| v.matches(value)).unwrap_or(false))
    }
}

impl Default for GoapPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// GOAP Agent component.
#[derive(Debug, Clone)]
pub struct GoapAgent {
    /// Current goal.
    pub current_goal: Option<String>,
    /// Current plan.
    pub current_plan: Vec<GoapAction>,
    /// Current action index.
    pub action_index: usize,
    /// Current world state.
    pub world_state: WorldState,
    /// Available actions (by ID).
    pub available_actions: Vec<String>,
    /// Goals with priorities.
    pub goals: HashMap<String, GoapGoal>,
    /// Action timer.
    pub action_timer: f32,
    /// Is planning.
    pub is_planning: bool,
    /// Last plan time.
    pub last_plan_time: f32,
}

impl GoapAgent {
    pub fn new() -> Self {
        Self {
            current_goal: None,
            current_plan: Vec::new(),
            action_index: 0,
            world_state: HashMap::new(),
            available_actions: Vec::new(),
            goals: HashMap::new(),
            action_timer: 0.0,
            is_planning: false,
            last_plan_time: 0.0,
        }
    }

    pub fn add_goal(&mut self, goal: GoapGoal) {
        self.goals.insert(goal.id.clone(), goal);
    }

    pub fn add_action(&mut self, action_id: impl Into<String>) {
        self.available_actions.push(action_id.into());
    }

    pub fn set_world_state(&mut self, key: impl Into<String>, value: WorldValue) {
        self.world_state.insert(key.into(), value);
    }

    /// Find highest priority unsatisfied goal.
    pub fn find_goal(&self) -> Option<&GoapGoal> {
        self.goals
            .values()
            .filter(|g| !g.is_satisfied(&self.world_state))
            .max_by(|a, b| {
                a.priority
                    .partial_cmp(&b.priority)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Request a new plan.
    pub fn request_plan(&mut self, planner: &GoapPlanner) -> bool {
        if let Some(goal) = self.find_goal() {
            let available_actions: Vec<_> = self
                .available_actions
                .iter()
                .filter_map(|id| planner.actions.get(id).cloned())
                .collect();

            // Create temporary planner with agent's available actions
            let mut agent_planner = GoapPlanner::new();
            for action in available_actions {
                agent_planner.add_action(action);
            }

            if let Some(plan) = agent_planner.plan(&self.world_state, goal) {
                self.current_goal = Some(goal.id.clone());
                self.current_plan = plan;
                self.action_index = 0;
                return true;
            }
        }
        false
    }

    /// Get current action.
    pub fn current_action(&self) -> Option<&GoapAction> {
        self.current_plan.get(self.action_index)
    }

    /// Advance to next action.
    pub fn advance_action(&mut self) {
        self.action_index += 1;
        self.action_timer = 0.0;
    }

    /// Check if plan is complete.
    pub fn is_plan_complete(&self) -> bool {
        self.action_index >= self.current_plan.len()
    }

    /// Update action timer.
    pub fn update(&mut self, dt: f32) {
        if let Some(action) = self.current_action() {
            self.action_timer += dt;
            if self.action_timer >= action.duration {
                self.advance_action();
            }
        }
    }
}

impl Default for GoapAgent {
    fn default() -> Self {
        Self::new()
    }
}

/// GOAP System.
pub struct GoapSystem {
    /// Planner instance.
    pub planner: GoapPlanner,
    /// Plan update interval.
    pub plan_interval: f32,
    /// Debug mode.
    pub debug: bool,
}

impl GoapSystem {
    pub fn new() -> Self {
        Self {
            planner: GoapPlanner::new(),
            plan_interval: 0.5,
            debug: false,
        }
    }

    pub fn update(&self, agents: &mut HashMap<u64, GoapAgent>, dt: f32) {
        for agent in agents.values_mut() {
            agent.last_plan_time += dt;

            // Replan if needed
            if agent.last_plan_time >= self.plan_interval || agent.is_plan_complete() {
                agent.request_plan(&self.planner);
                agent.last_plan_time = 0.0;
            }

            agent.update(dt);
        }
    }
}

impl Default for GoapSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_effects() {
        let action = GoapAction::new("attack")
            .precondition("has_weapon", WorldValue::Bool(true))
            .effect("enemy_dead", WorldValue::Bool(true));

        let mut state = WorldState::new();
        state.insert("has_weapon".to_string(), WorldValue::Bool(true));

        assert!(action.can_execute(&state));

        action.apply_effects(&mut state);
        assert_eq!(state.get("enemy_dead"), Some(&WorldValue::Bool(true)));
    }

    #[test]
    fn goal_satisfaction() {
        let goal = GoapGoal::new("kill_enemy").desire("enemy_dead", WorldValue::Bool(true));

        let mut state = WorldState::new();
        assert!(!goal.is_satisfied(&state));

        state.insert("enemy_dead".to_string(), WorldValue::Bool(true));
        assert!(goal.is_satisfied(&state));
    }

    #[test]
    fn planner_simple() {
        let mut planner = GoapPlanner::new();

        planner.add_action(
            GoapAction::new("get_weapon")
                .effect("has_weapon", WorldValue::Bool(true))
                .with_cost(1.0),
        );

        planner.add_action(
            GoapAction::new("attack")
                .precondition("has_weapon", WorldValue::Bool(true))
                .effect("enemy_dead", WorldValue::Bool(true))
                .with_cost(1.0),
        );

        let goal = GoapGoal::new("kill").desire("enemy_dead", WorldValue::Bool(true));

        let state = WorldState::new();
        let plan = planner.plan(&state, &goal);

        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].id, "get_weapon");
        assert_eq!(plan[1].id, "attack");
    }
}
