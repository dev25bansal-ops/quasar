//! State machine system (complement to behavior trees).
//!
//! Provides:
//! - Hierarchical state machines (HFSM)
//! - State transitions with conditions
//! - State actions (enter, update, exit)
//! - Parallel states

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// State machine definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineDef {
    /// State machine ID.
    pub id: String,
    /// Initial state ID.
    pub initial_state: String,
    /// All states.
    pub states: HashMap<String, StateDef>,
    /// Global transitions (any state -> target).
    pub global_transitions: Vec<Transition>,
    /// State machine variables.
    pub variables: HashMap<String, StateVarValue>,
}

impl StateMachineDef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            initial_state: String::new(),
            states: HashMap::new(),
            global_transitions: Vec::new(),
            variables: HashMap::new(),
        }
    }

    pub fn add_state(&mut self, state: StateDef) {
        self.states.insert(state.id.clone(), state);
    }
}

/// State definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDef {
    /// State ID.
    pub id: String,
    /// Parent state (for hierarchical FSM).
    pub parent: Option<String>,
    /// Child states (sub-state machine).
    pub children: Vec<String>,
    /// Initial child state (if has children).
    pub initial_child: Option<String>,
    /// Enter actions.
    pub on_enter: Vec<StateAction>,
    /// Update actions (every tick).
    pub on_update: Vec<StateAction>,
    /// Exit actions.
    pub on_exit: Vec<StateAction>,
    /// Outgoing transitions.
    pub transitions: Vec<Transition>,
    /// Is this a parallel state (runs all children).
    pub parallel: bool,
}

impl StateDef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            parent: None,
            children: Vec::new(),
            initial_child: None,
            on_enter: Vec::new(),
            on_update: Vec::new(),
            on_exit: Vec::new(),
            transitions: Vec::new(),
            parallel: false,
        }
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn add_child(mut self, child_id: impl Into<String>) -> Self {
        self.children.push(child_id.into());
        self
    }

    pub fn on_enter(mut self, action: StateAction) -> Self {
        self.on_enter.push(action);
        self
    }

    pub fn on_update(mut self, action: StateAction) -> Self {
        self.on_update.push(action);
        self
    }

    pub fn on_exit(mut self, action: StateAction) -> Self {
        self.on_exit.push(action);
        self
    }

    pub fn add_transition(mut self, transition: Transition) -> Self {
        self.transitions.push(transition);
        self
    }

    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }
}

/// Transition between states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    /// Target state ID.
    pub target: String,
    /// Transition conditions (all must be true).
    pub conditions: Vec<TransitionCondition>,
    /// Actions on transition.
    pub actions: Vec<StateAction>,
    /// Priority (higher = checked first).
    pub priority: i32,
}

impl Transition {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            conditions: Vec::new(),
            actions: Vec::new(),
            priority: 0,
        }
    }

    pub fn with_condition(mut self, condition: TransitionCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Transition condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionCondition {
    pub condition_type: TransitionConditionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransitionConditionType {
    VariableEquals { name: String, value: StateVarValue },
    VariableGreaterThan { name: String, value: f32 },
    VariableLessThan { name: String, value: f32 },
    FlagSet { name: String },
    FlagNotSet { name: String },
    Timeout { duration: f32 },
    AnimationFinished,
    Custom { id: String },
}

/// State action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateAction {
    pub action_type: StateActionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateActionType {
    SetVariable {
        name: String,
        value: StateVarValue,
    },
    ModifyVariable {
        name: String,
        operation: VarOp,
        value: f32,
    },
    SetFlag {
        name: String,
        value: bool,
    },
    PlayAnimation {
        animation_id: String,
    },
    StopAnimation,
    PlaySound {
        sound_id: String,
    },
    TriggerEvent {
        event_id: String,
    },
    SetTimer {
        name: String,
        duration: f32,
    },
    Custom {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VarOp {
    Set,
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// State variable value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateVarValue {
    Bool(bool),
    Int(i64),
    Float(f32),
    String(String),
}

/// Runtime state machine instance.
#[derive(Debug, Clone)]
pub struct StateMachineInstance {
    /// State machine ID.
    pub state_machine_id: String,
    /// Current active states (stack for hierarchy).
    pub active_states: Vec<String>,
    /// State variables.
    pub variables: HashMap<String, StateVarValue>,
    /// State flags.
    pub flags: HashMap<String, bool>,
    /// State timers.
    pub timers: HashMap<String, f32>,
    /// Time in current state.
    pub state_time: f32,
    /// Total time.
    pub total_time: f32,
}

impl StateMachineInstance {
    pub fn new(state_machine_id: impl Into<String>, initial_state: impl Into<String>) -> Self {
        Self {
            state_machine_id: state_machine_id.into(),
            active_states: vec![initial_state.into()],
            variables: HashMap::new(),
            flags: HashMap::new(),
            timers: HashMap::new(),
            state_time: 0.0,
            total_time: 0.0,
        }
    }

    pub fn current_state(&self) -> Option<&String> {
        self.active_states.last()
    }

    pub fn change_state(&mut self, new_state: String) {
        self.active_states.push(new_state);
        self.state_time = 0.0;
    }

    pub fn pop_state(&mut self) {
        if self.active_states.len() > 1 {
            self.active_states.pop();
            self.state_time = 0.0;
        }
    }

    pub fn set_variable(&mut self, name: &str, value: StateVarValue) {
        self.variables.insert(name.to_string(), value);
    }

    pub fn get_variable(&self, name: &str) -> Option<&StateVarValue> {
        self.variables.get(name)
    }

    pub fn set_flag(&mut self, name: &str, value: bool) {
        self.flags.insert(name.to_string(), value);
    }

    pub fn get_flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    pub fn start_timer(&mut self, name: &str, duration: f32) {
        self.timers.insert(name.to_string(), duration);
    }

    pub fn timer_elapsed(&self, name: &str) -> bool {
        self.timers.get(name).map(|&t| t <= 0.0).unwrap_or(false)
    }

    pub fn update(&mut self, dt: f32) {
        self.state_time += dt;
        self.total_time += dt;

        // Update timers
        for timer in self.timers.values_mut() {
            *timer -= dt;
        }
    }

    pub fn check_transition(&self, transition: &Transition) -> bool {
        transition
            .conditions
            .iter()
            .all(|c| self.check_condition(c))
    }

    fn check_condition(&self, condition: &TransitionCondition) -> bool {
        match &condition.condition_type {
            TransitionConditionType::VariableEquals { name, value } => self
                .variables
                .get(name)
                .map(|v| v == value)
                .unwrap_or(false),
            TransitionConditionType::VariableGreaterThan { name, value } => self
                .variables
                .get(name)
                .and_then(|v| {
                    if let StateVarValue::Float(f) = v {
                        Some(*f > *value)
                    } else {
                        None
                    }
                })
                .unwrap_or(false),
            TransitionConditionType::VariableLessThan { name, value } => self
                .variables
                .get(name)
                .and_then(|v| {
                    if let StateVarValue::Float(f) = v {
                        Some(*f < *value)
                    } else {
                        None
                    }
                })
                .unwrap_or(false),
            TransitionConditionType::FlagSet { name } => self.get_flag(name),
            TransitionConditionType::FlagNotSet { name } => !self.get_flag(name),
            TransitionConditionType::Timeout { duration } => self.state_time >= *duration,
            TransitionConditionType::AnimationFinished => false,
            TransitionConditionType::Custom { .. } => true,
        }
    }

    pub fn apply_action(&mut self, action: &StateAction) {
        match &action.action_type {
            StateActionType::SetVariable { name, value } => {
                self.set_variable(name, value.clone());
            }
            StateActionType::ModifyVariable {
                name,
                operation,
                value,
            } => {
                if let Some(StateVarValue::Float(current)) = self.variables.get(name) {
                    let new_value = match operation {
                        VarOp::Set => *value,
                        VarOp::Add => current + value,
                        VarOp::Subtract => current - value,
                        VarOp::Multiply => current * value,
                        VarOp::Divide => current / value.max(0.001),
                    };
                    self.set_variable(name, StateVarValue::Float(new_value));
                }
            }
            StateActionType::SetFlag { name, value } => {
                self.set_flag(name, *value);
            }
            StateActionType::SetTimer { name, duration } => {
                self.start_timer(name, *duration);
            }
            _ => {}
        }
    }
}

/// State machine system.
pub struct StateMachineSystem {
    /// State machine definitions.
    pub definitions: HashMap<String, StateMachineDef>,
    /// Active instances.
    pub instances: HashMap<u64, StateMachineInstance>,
    /// Next instance ID.
    pub next_id: u64,
}

impl StateMachineSystem {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            instances: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn register(&mut self, def: StateMachineDef) {
        self.definitions.insert(def.id.clone(), def);
    }

    pub fn create_instance(&mut self, state_machine_id: &str) -> Option<u64> {
        let def = self.definitions.get(state_machine_id)?;
        let id = self.next_id;
        self.next_id += 1;

        let instance = StateMachineInstance::new(state_machine_id, &def.initial_state);
        self.instances.insert(id, instance);

        Some(id)
    }

    pub fn update(&mut self, dt: f32) {
        for (instance_id, instance) in &mut self.instances {
            let current_state_id = match instance.current_state() {
                Some(s) => s.clone(),
                None => continue,
            };

            let def = match self.definitions.get(&instance.state_machine_id) {
                Some(d) => d,
                None => continue,
            };

            let state_def = match def.states.get(&current_state_id) {
                Some(s) => s,
                None => continue,
            };

            // Update state time
            instance.update(dt);

            // Check transitions (sorted by priority)
            let mut transitions: Vec<_> = state_def
                .transitions
                .iter()
                .chain(def.global_transitions.iter())
                .collect();
            transitions.sort_by(|a, b| b.priority.cmp(&a.priority));

            for transition in transitions {
                if instance.check_transition(transition) {
                    // Exit actions
                    for action in &state_def.on_exit {
                        instance.apply_action(action);
                    }

                    // Transition actions
                    for action in &transition.actions {
                        instance.apply_action(action);
                    }

                    // Change state
                    instance.change_state(transition.target.clone());

                    // Enter actions
                    if let Some(new_state_def) = def.states.get(&transition.target) {
                        for action in &new_state_def.on_enter {
                            instance.apply_action(action);
                        }
                    }

                    break;
                }
            }

            // Execute update actions
            if let Some(state_def) = def.states.get(&current_state_id) {
                for action in &state_def.on_update {
                    instance.apply_action(action);
                }
            }
        }
    }

    pub fn get_instance(&self, id: u64) -> Option<&StateMachineInstance> {
        self.instances.get(&id)
    }

    pub fn get_instance_mut(&mut self, id: u64) -> Option<&mut StateMachineInstance> {
        self.instances.get_mut(&id)
    }

    pub fn destroy_instance(&mut self, id: u64) {
        self.instances.remove(&id);
    }
}

impl Default for StateMachineSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_creation() {
        let mut def = StateMachineDef::new("test_fsm");
        def.initial_state = "idle".to_string();
        def.add_state(StateDef::new("idle"));
        def.add_state(StateDef::new("walking"));

        let mut system = StateMachineSystem::new();
        system.register(def);

        let id = system.create_instance("test_fsm");
        assert!(id.is_some());
    }

    #[test]
    fn state_transitions() {
        let mut instance = StateMachineInstance::new("test", "idle");
        assert_eq!(instance.current_state(), Some(&"idle".to_string()));

        instance.change_state("walking".to_string());
        assert_eq!(instance.current_state(), Some(&"walking".to_string()));

        instance.pop_state();
        assert_eq!(instance.current_state(), Some(&"idle".to_string()));
    }

    #[test]
    fn state_variables() {
        let mut instance = StateMachineInstance::new("test", "idle");
        instance.set_variable("health", StateVarValue::Float(100.0));
        instance.set_flag("attacking", true);

        assert_eq!(
            instance.get_variable("health"),
            Some(&StateVarValue::Float(100.0))
        );
        assert!(instance.get_flag("attacking"));
    }

    #[test]
    fn timeout_condition() {
        let mut instance = StateMachineInstance::new("test", "idle");

        let condition = TransitionCondition {
            condition_type: TransitionConditionType::Timeout { duration: 1.0 },
        };

        assert!(!instance.check_condition(&condition));

        instance.update(1.5);
        assert!(instance.check_condition(&condition));
    }
}
