//! Dialog system with localization support.
//!
//! Provides:
//! - Dialog trees with branching conversations
//! - Localization integration for all text
//! - Conditions and effects on dialog nodes
//! - Speaker portraits and voice-over support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dialog tree containing all nodes for a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogTree {
    /// Unique identifier for this dialog.
    pub id: String,
    /// Display name (localized key).
    pub name_key: String,
    /// All dialog nodes.
    pub nodes: HashMap<String, DialogNode>,
    /// Starting node ID.
    pub start_node: String,
    /// Speakers in this dialog.
    pub speakers: HashMap<String, DialogSpeaker>,
}

impl DialogTree {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: String::new(),
            nodes: HashMap::new(),
            start_node: String::new(),
            speakers: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: DialogNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_speaker(&mut self, speaker: DialogSpeaker) {
        self.speakers.insert(speaker.id.clone(), speaker);
    }

    pub fn get_node(&self, id: &str) -> Option<&DialogNode> {
        self.nodes.get(id)
    }

    pub fn get_start_node(&self) -> Option<&DialogNode> {
        self.nodes.get(&self.start_node)
    }
}

/// A single dialog node (line of dialogue).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogNode {
    /// Unique node ID.
    pub id: String,
    /// Speaker ID reference.
    pub speaker: String,
    /// Dialog text localization key.
    pub text_key: String,
    /// Optional portrait emotion.
    pub portrait: Option<String>,
    /// Optional voice-over clip path.
    pub voice_over: Option<String>,
    /// Response options (player choices).
    pub responses: Vec<DialogResponse>,
    /// Conditions to show this node.
    pub conditions: Vec<DialogCondition>,
    /// Effects to apply when entering this node.
    pub effects: Vec<DialogEffect>,
    /// Next node if no responses (auto-advance).
    pub next_node: Option<String>,
}

impl DialogNode {
    pub fn new(
        id: impl Into<String>,
        speaker: impl Into<String>,
        text_key: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            speaker: speaker.into(),
            text_key: text_key.into(),
            portrait: None,
            voice_over: None,
            responses: Vec::new(),
            conditions: Vec::new(),
            effects: Vec::new(),
            next_node: None,
        }
    }

    pub fn with_portrait(mut self, portrait: impl Into<String>) -> Self {
        self.portrait = Some(portrait.into());
        self
    }

    pub fn with_voice_over(mut self, path: impl Into<String>) -> Self {
        self.voice_over = Some(path.into());
        self
    }

    pub fn add_response(mut self, response: DialogResponse) -> Self {
        self.responses.push(response);
        self
    }

    pub fn add_condition(mut self, condition: DialogCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn add_effect(mut self, effect: DialogEffect) -> Self {
        self.effects.push(effect);
        self
    }
}

/// Player response option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogResponse {
    /// Response text localization key.
    pub text_key: String,
    /// Target node ID when selected.
    pub target_node: String,
    /// Conditions for this response to appear.
    pub conditions: Vec<DialogCondition>,
    /// Effects when this response is selected.
    pub effects: Vec<DialogEffect>,
}

impl DialogResponse {
    pub fn new(text_key: impl Into<String>, target_node: impl Into<String>) -> Self {
        Self {
            text_key: text_key.into(),
            target_node: target_node.into(),
            conditions: Vec::new(),
            effects: Vec::new(),
        }
    }

    pub fn with_condition(mut self, condition: DialogCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn with_effect(mut self, effect: DialogEffect) -> Self {
        self.effects.push(effect);
        self
    }
}

/// Speaker/character in a dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogSpeaker {
    /// Speaker ID.
    pub id: String,
    /// Display name localization key.
    pub name_key: String,
    /// Default portrait.
    pub default_portrait: Option<String>,
    /// Portrait variations (emotion -> path).
    pub portraits: HashMap<String, String>,
}

impl DialogSpeaker {
    pub fn new(id: impl Into<String>, name_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: name_key.into(),
            default_portrait: None,
            portraits: HashMap::new(),
        }
    }

    pub fn add_portrait(mut self, emotion: impl Into<String>, path: impl Into<String>) -> Self {
        self.portraits.insert(emotion.into(), path.into());
        self
    }
}

/// Dialog condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogCondition {
    /// Condition type.
    pub condition_type: DialogConditionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogConditionType {
    /// Check if a flag is set.
    FlagSet { flag: String },
    /// Check if a flag is not set.
    FlagNotSet { flag: String },
    /// Check if an item is in inventory.
    HasItem { item_id: String, count: u32 },
    /// Check if a quest is in a state.
    QuestState { quest_id: String, state: String },
    /// Check a numeric comparison.
    Compare {
        variable: String,
        operator: CompareOp,
        value: f32,
    },
    /// Check if a dialog has been seen.
    DialogSeen { dialog_id: String },
    /// Custom condition by ID.
    Custom {
        id: String,
        params: HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompareOp {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

/// Dialog effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogEffect {
    /// Effect type.
    pub effect_type: DialogEffectType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogEffectType {
    /// Set a flag.
    SetFlag { flag: String, value: bool },
    /// Give an item.
    GiveItem { item_id: String, count: u32 },
    /// Take an item.
    TakeItem { item_id: String, count: u32 },
    /// Set a variable.
    SetVariable { variable: String, value: f32 },
    /// Modify a variable.
    ModifyVariable {
        variable: String,
        operation: ModifyOp,
        value: f32,
    },
    /// Start a quest.
    StartQuest { quest_id: String },
    /// Complete a quest objective.
    CompleteObjective {
        quest_id: String,
        objective_id: String,
    },
    /// Mark dialog as seen.
    MarkDialogSeen { dialog_id: String },
    /// Trigger a custom event.
    TriggerEvent {
        event_id: String,
        params: HashMap<String, String>,
    },
    /// Play a sound.
    PlaySound { sound_id: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ModifyOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Set,
}

/// Runtime dialog state.
#[derive(Debug, Clone)]
pub struct DialogState {
    /// Current dialog tree.
    pub current_dialog: Option<String>,
    /// Current node ID.
    pub current_node: Option<String>,
    /// Dialogs that have been seen.
    pub seen_dialogs: HashMap<String, bool>,
    /// Dialog variables.
    pub variables: HashMap<String, f32>,
    /// Dialog flags.
    pub flags: HashMap<String, bool>,
    /// Current response index (for UI).
    pub selected_response: usize,
    /// Is waiting for player input.
    pub waiting_for_input: bool,
    /// Auto-advance timer.
    pub auto_advance_timer: f32,
}

impl Default for DialogState {
    fn default() -> Self {
        Self {
            current_dialog: None,
            current_node: None,
            seen_dialogs: HashMap::new(),
            variables: HashMap::new(),
            flags: HashMap::new(),
            selected_response: 0,
            waiting_for_input: false,
            auto_advance_timer: 0.0,
        }
    }
}

impl DialogState {
    pub fn start_dialog(&mut self, dialog_id: &str, start_node: &str) {
        self.current_dialog = Some(dialog_id.to_string());
        self.current_node = Some(start_node.to_string());
        self.selected_response = 0;
        self.waiting_for_input = false;
    }

    pub fn end_dialog(&mut self) {
        self.current_dialog = None;
        self.current_node = None;
        self.waiting_for_input = false;
    }

    pub fn is_in_dialog(&self) -> bool {
        self.current_dialog.is_some()
    }

    pub fn set_flag(&mut self, flag: &str, value: bool) {
        self.flags.insert(flag.to_string(), value);
    }

    pub fn get_flag(&self, flag: &str) -> bool {
        self.flags.get(flag).copied().unwrap_or(false)
    }

    pub fn set_variable(&mut self, name: &str, value: f32) {
        self.variables.insert(name.to_string(), value);
    }

    pub fn get_variable(&self, name: &str) -> f32 {
        self.variables.get(name).copied().unwrap_or(0.0)
    }

    pub fn mark_seen(&mut self, dialog_id: &str) {
        self.seen_dialogs.insert(dialog_id.to_string(), true);
    }

    pub fn has_seen(&self, dialog_id: &str) -> bool {
        self.seen_dialogs.get(dialog_id).copied().unwrap_or(false)
    }

    pub fn evaluate_condition(&self, condition: &DialogCondition) -> bool {
        match &condition.condition_type {
            DialogConditionType::FlagSet { flag } => self.get_flag(flag),
            DialogConditionType::FlagNotSet { flag } => !self.get_flag(flag),
            DialogConditionType::HasItem { item_id, count } => {
                // Would check inventory - placeholder
                self.get_variable(&format!("item_{}", item_id)) >= *count as f32
            }
            DialogConditionType::QuestState { quest_id, state } => {
                // Would check quest system - placeholder
                self.get_variable(&format!("quest_{}_state", quest_id))
                    == match state.as_str() {
                        "inactive" => 0.0,
                        "active" => 1.0,
                        "completed" => 2.0,
                        "failed" => 3.0,
                        _ => 0.0,
                    }
            }
            DialogConditionType::Compare {
                variable,
                operator,
                value,
            } => {
                let var_value = self.get_variable(variable);
                match operator {
                    CompareOp::Equal => (var_value - value).abs() < 0.001,
                    CompareOp::NotEqual => (var_value - value).abs() >= 0.001,
                    CompareOp::Greater => var_value > *value,
                    CompareOp::GreaterEqual => var_value >= *value,
                    CompareOp::Less => var_value < *value,
                    CompareOp::LessEqual => var_value <= *value,
                }
            }
            DialogConditionType::DialogSeen { dialog_id } => self.has_seen(dialog_id),
            DialogConditionType::Custom { .. } => true,
        }
    }

    pub fn apply_effect(&mut self, effect: &DialogEffect) {
        match &effect.effect_type {
            DialogEffectType::SetFlag { flag, value } => {
                self.set_flag(flag, *value);
            }
            DialogEffectType::GiveItem { item_id, count } => {
                let key = format!("item_{}", item_id);
                let current = self.get_variable(&key);
                self.set_variable(&key, current + *count as f32);
            }
            DialogEffectType::TakeItem { item_id, count } => {
                let key = format!("item_{}", item_id);
                let current = self.get_variable(&key);
                self.set_variable(&key, (current - *count as f32).max(0.0));
            }
            DialogEffectType::SetVariable { variable, value } => {
                self.set_variable(variable, *value);
            }
            DialogEffectType::ModifyVariable {
                variable,
                operation,
                value,
            } => {
                let current = self.get_variable(variable);
                let new_value = match operation {
                    ModifyOp::Add => current + value,
                    ModifyOp::Subtract => current - value,
                    ModifyOp::Multiply => current * value,
                    ModifyOp::Divide => current / value.max(0.001),
                    ModifyOp::Set => *value,
                };
                self.set_variable(variable, new_value);
            }
            DialogEffectType::MarkDialogSeen { dialog_id } => {
                self.mark_seen(dialog_id);
            }
            _ => {}
        }
    }
}

/// Dialog system resource.
pub struct DialogSystem {
    /// All loaded dialog trees.
    pub dialogs: HashMap<String, DialogTree>,
    /// Runtime state.
    pub state: DialogState,
    /// Localization prefix for dialog keys.
    pub localization_prefix: String,
}

impl DialogSystem {
    pub fn new() -> Self {
        Self {
            dialogs: HashMap::new(),
            state: DialogState::default(),
            localization_prefix: "dialog".to_string(),
        }
    }

    pub fn load_dialog(&mut self, dialog: DialogTree) {
        self.dialogs.insert(dialog.id.clone(), dialog);
    }

    pub fn start(&mut self, dialog_id: &str) {
        if let Some(dialog) = self.dialogs.get(dialog_id) {
            if let Some(start_node) = dialog.get_start_node() {
                self.state.start_dialog(dialog_id, &start_node.id);
                for effect in &start_node.effects {
                    self.state.apply_effect(effect);
                }
            }
        }
    }

    pub fn select_response(&mut self, index: usize) {
        let dialog_id = match &self.state.current_dialog {
            Some(id) => id.clone(),
            None => return,
        };

        let node = match &self.state.current_node {
            Some(id) => {
                let dialog = match self.dialogs.get(&dialog_id) {
                    Some(d) => d,
                    None => return,
                };
                match dialog.get_node(id) {
                    Some(n) => n.clone(),
                    None => return,
                }
            }
            None => return,
        };

        if index < node.responses.len() {
            let response = &node.responses[index];
            for effect in &response.effects {
                self.state.apply_effect(effect);
            }
            self.state.current_node = Some(response.target_node.clone());
            self.state.selected_response = 0;
        }
    }

    pub fn advance(&mut self) {
        let dialog_id = match &self.state.current_dialog {
            Some(id) => id.clone(),
            None => return,
        };

        let node = match &self.state.current_node {
            Some(id) => {
                let dialog = match self.dialogs.get(&dialog_id) {
                    Some(d) => d,
                    None => return,
                };
                match dialog.get_node(id) {
                    Some(n) => n.clone(),
                    None => return,
                }
            }
            None => return,
        };

        if node.responses.is_empty() {
            if let Some(next) = &node.next_node {
                self.state.current_node = Some(next.clone());
            } else {
                self.state.end_dialog();
            }
        }
    }

    pub fn get_localized_text(&self, key: &str) -> String {
        format!("{}.{}", self.localization_prefix, key)
    }

    pub fn get_available_responses(&self) -> Vec<(String, String)> {
        let dialog_id = match &self.state.current_dialog {
            Some(id) => id.clone(),
            None => return Vec::new(),
        };

        let node = match &self.state.current_node {
            Some(id) => {
                let dialog = match self.dialogs.get(&dialog_id) {
                    Some(d) => d,
                    None => return Vec::new(),
                };
                match dialog.get_node(id) {
                    Some(n) => n,
                    None => return Vec::new(),
                }
            }
            None => return Vec::new(),
        };

        node.responses
            .iter()
            .filter(|r| {
                r.conditions
                    .iter()
                    .all(|c| self.state.evaluate_condition(c))
            })
            .map(|r| (r.text_key.clone(), r.target_node.clone()))
            .collect()
    }
}

impl Default for DialogSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_tree_creation() {
        let mut tree = DialogTree::new("test_dialog");
        let node = DialogNode::new("node1", "npc", "dialog.test.greeting");
        tree.add_node(node);
        tree.start_node = "node1".to_string();

        assert!(tree.get_node("node1").is_some());
    }

    #[test]
    fn dialog_state_flags() {
        let mut state = DialogState::default();
        state.set_flag("talked_to_npc", true);
        assert!(state.get_flag("talked_to_npc"));
        assert!(!state.get_flag("not_set"));
    }

    #[test]
    fn dialog_state_variables() {
        let mut state = DialogState::default();
        state.set_variable("gold", 100.0);
        assert_eq!(state.get_variable("gold"), 100.0);

        state.apply_effect(&DialogEffect {
            effect_type: DialogEffectType::ModifyVariable {
                variable: "gold".to_string(),
                operation: ModifyOp::Add,
                value: 50.0,
            },
        });
        assert_eq!(state.get_variable("gold"), 150.0);
    }

    #[test]
    fn dialog_conditions() {
        let mut state = DialogState::default();
        state.set_flag("has_key", true);

        let condition = DialogCondition {
            condition_type: DialogConditionType::FlagSet {
                flag: "has_key".to_string(),
            },
        };
        assert!(state.evaluate_condition(&condition));

        let condition2 = DialogCondition {
            condition_type: DialogConditionType::FlagNotSet {
                flag: "has_key".to_string(),
            },
        };
        assert!(!state.evaluate_condition(&condition2));
    }
}
