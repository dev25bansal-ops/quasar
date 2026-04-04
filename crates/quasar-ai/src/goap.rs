//! GOAP (Goal-Oriented Action Planning) Implementation
//!
//! A* planning for intelligent agent behavior.

use crate::blackboard::{Blackboard, BlackboardValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapWorldState {
    pub values: HashMap<String, BlackboardValue>,
}

impl GoapWorldState {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn from_blackboard(bb: &Blackboard) -> Self {
        Self {
            values: bb.snapshot(),
        }
    }

    pub fn set(&mut self, key: &str, value: BlackboardValue) -> &mut Self {
        self.values.insert(key.to_string(), value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&BlackboardValue> {
        self.values.get(key)
    }

    pub fn matches(&self, other: &GoapWorldState) -> bool {
        for (key, value) in &other.values {
            if self.values.get(key) != Some(value) {
                return false;
            }
        }
        true
    }

    pub fn distance(&self, other: &GoapWorldState) -> u32 {
        let mut count = 0;
        for (key, value) in &other.values {
            if self.values.get(key) != Some(value) {
                count += 1;
            }
        }
        count
    }

    pub fn apply(&mut self, effects: &HashMap<String, BlackboardValue>) {
        for (key, value) in effects {
            self.values.insert(key.clone(), value.clone());
        }
    }
}

impl Default for GoapWorldState {
    fn default() -> Self {
        Self::new()
    }
}

impl Hash for GoapWorldState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut keys: Vec<_> = self.values.keys().collect();
        keys.sort();
        for key in keys {
            key.hash(state);
            format!("{:?}", self.values[key]).hash(state);
        }
    }
}

impl PartialEq for GoapWorldState {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl Eq for GoapWorldState {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapAction {
    pub name: String,
    pub cost: f32,
    pub preconditions: HashMap<String, BlackboardValue>,
    pub effects: HashMap<String, BlackboardValue>,
}

impl GoapAction {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cost: 1.0,
            preconditions: HashMap::new(),
            effects: HashMap::new(),
        }
    }

    pub fn cost(mut self, cost: f32) -> Self {
        self.cost = cost;
        self
    }

    pub fn require(mut self, key: &str, value: BlackboardValue) -> Self {
        self.preconditions.insert(key.to_string(), value);
        self
    }

    pub fn effect(mut self, key: &str, value: BlackboardValue) -> Self {
        self.effects.insert(key.to_string(), value);
        self
    }

    pub fn can_execute(&self, world: &GoapWorldState) -> bool {
        for (key, value) in &self.preconditions {
            if world.values.get(key) != Some(value) {
                return false;
            }
        }
        true
    }

    pub fn execute(&self, world: &mut GoapWorldState) {
        world.apply(&self.effects);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoapGoal {
    pub name: String,
    pub priority: f32,
    pub world_state: GoapWorldState,
}

impl GoapGoal {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            priority: 1.0,
            world_state: GoapWorldState::new(),
        }
    }

    pub fn priority(mut self, priority: f32) -> Self {
        self.priority = priority;
        self
    }

    pub fn require(mut self, key: &str, value: BlackboardValue) -> Self {
        self.world_state.set(key, value);
        self
    }

    pub fn is_satisfied(&self, world: &GoapWorldState) -> bool {
        world.matches(&self.world_state)
    }
}

#[derive(Debug, Clone)]
pub struct GoapPlan {
    pub actions: Vec<GoapAction>,
    pub total_cost: f32,
    pub goal: String,
}

impl GoapPlan {
    pub fn new(goal: &str) -> Self {
        Self {
            actions: Vec::new(),
            total_cost: 0.0,
            goal: goal.to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn current_action(&self) -> Option<&GoapAction> {
        self.actions.first()
    }

    pub fn advance(&mut self) {
        if !self.actions.is_empty() {
            self.actions.remove(0);
        }
    }
}

pub struct GoapPlanner {
    max_depth: usize,
    max_iterations: usize,
}

impl Default for GoapPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl GoapPlanner {
    pub fn new() -> Self {
        Self {
            max_depth: 20,
            max_iterations: 1000,
        }
    }

    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn max_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }

    pub fn plan(
        &self,
        start: &GoapWorldState,
        goal: &GoapGoal,
        actions: &[GoapAction],
    ) -> Option<GoapPlan> {
        if goal.is_satisfied(start) {
            return Some(GoapPlan::new(&goal.name));
        }

        let mut open_set: Vec<(GoapWorldState, Vec<GoapAction>, f32)> =
            vec![(start.clone(), Vec::new(), 0.0)];
        let mut closed_set: Vec<u64> = Vec::new();
        let mut iterations = 0;

        while !open_set.is_empty() && iterations < self.max_iterations {
            iterations += 1;

            open_set.sort_by(|a, b| {
                let a_h = a.0.distance(&goal.world_state) as f32;
                let b_h = b.0.distance(&goal.world_state) as f32;
                (a.2 + a_h).partial_cmp(&(b.2 + b_h)).unwrap()
            });

            let (current, path, g) = open_set.remove(0);

            let current_hash = {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                current.hash(&mut hasher);
                hasher.finish()
            };

            if closed_set.contains(&current_hash) {
                continue;
            }
            closed_set.push(current_hash);

            if goal.is_satisfied(&current) {
                let total_cost: f32 = path.iter().map(|a| a.cost).sum();
                return Some(GoapPlan {
                    actions: path,
                    total_cost,
                    goal: goal.name.clone(),
                });
            }

            if path.len() >= self.max_depth {
                continue;
            }

            for action in actions {
                if action.can_execute(&current) {
                    let mut new_state = current.clone();
                    action.execute(&mut new_state);
                    let new_g = g + action.cost;

                    let new_hash = {
                        let mut hasher = std::collections::hash_map::DefaultHasher::new();
                        new_state.hash(&mut hasher);
                        hasher.finish()
                    };

                    if !closed_set.contains(&new_hash) {
                        let mut new_path = path.clone();
                        new_path.push(action.clone());
                        open_set.push((new_state, new_path, new_g));
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_state_basic() {
        let mut ws = GoapWorldState::new();
        ws.set("health", BlackboardValue::Int(100));
        assert_eq!(ws.get("health"), Some(&BlackboardValue::Int(100)));
    }

    #[test]
    fn world_state_matches() {
        let mut ws1 = GoapWorldState::new();
        ws1.set("a", BlackboardValue::Bool(true));

        let mut ws2 = GoapWorldState::new();
        ws2.set("a", BlackboardValue::Bool(true));
        ws2.set("b", BlackboardValue::Bool(false));

        assert!(ws1.matches(&ws2) == false);
        assert!(ws2.matches(&ws1));
    }

    #[test]
    fn action_preconditions() {
        let action = GoapAction::new("attack")
            .require("has_weapon", BlackboardValue::Bool(true))
            .effect("attacked", BlackboardValue::Bool(true));

        let mut world = GoapWorldState::new();
        assert!(!action.can_execute(&world));

        world.set("has_weapon", BlackboardValue::Bool(true));
        assert!(action.can_execute(&world));
    }

    #[test]
    fn action_effects() {
        let action = GoapAction::new("pickup").effect("has_item", BlackboardValue::Bool(true));

        let mut world = GoapWorldState::new();
        action.execute(&mut world);

        assert_eq!(world.get("has_item"), Some(&BlackboardValue::Bool(true)));
    }

    #[test]
    fn goal_satisfied() {
        let goal = GoapGoal::new("survive").require("alive", BlackboardValue::Bool(true));

        let mut world = GoapWorldState::new();
        assert!(!goal.is_satisfied(&world));

        world.set("alive", BlackboardValue::Bool(true));
        assert!(goal.is_satisfied(&world));
    }

    #[test]
    fn planner_simple() {
        let planner = GoapPlanner::new();

        let start = GoapWorldState::new();

        let goal = GoapGoal::new("have_item").require("has_item", BlackboardValue::Bool(true));

        let actions =
            vec![GoapAction::new("pickup_item").effect("has_item", BlackboardValue::Bool(true))];

        let plan = planner.plan(&start, &goal, &actions);
        assert!(plan.is_some());
        assert_eq!(plan.unwrap().actions.len(), 1);
    }

    #[test]
    fn planner_chain() {
        let planner = GoapPlanner::new();

        let mut start = GoapWorldState::new();
        start.set("enemy_alive", BlackboardValue::Bool(true));

        let goal = GoapGoal::new("enemy_dead").require("enemy_alive", BlackboardValue::Bool(false));

        let actions = vec![
            GoapAction::new("attack")
                .require("has_weapon", BlackboardValue::Bool(true))
                .require("enemy_alive", BlackboardValue::Bool(true))
                .effect("enemy_alive", BlackboardValue::Bool(false)),
            GoapAction::new("pickup_weapon").effect("has_weapon", BlackboardValue::Bool(true)),
        ];

        let plan = planner.plan(&start, &goal, &actions);
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.actions[0].name, "pickup_weapon");
        assert_eq!(plan.actions[1].name, "attack");
    }

    #[test]
    fn planner_no_path() {
        let planner = GoapPlanner::new();

        let start = GoapWorldState::new();

        let goal = GoapGoal::new("impossible").require("unreachable", BlackboardValue::Bool(true));

        let actions: Vec<GoapAction> = vec![];

        let plan = planner.plan(&start, &goal, &actions);
        assert!(plan.is_none());
    }
}
