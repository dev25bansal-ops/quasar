//! Enhanced dialogue tree system with Lua scripting integration.
//!
//! Provides:
//! - Dialogue tree definition with branching conversations
//! - Node-based dialogue with conditions and effects
//! - Lua scripting for dynamic dialogue conditions
//! - Dialogue state management for runtime conversations
//! - Integration with quest system
//!
//! # Example
//!
//! ```
//! use quasar_core::dialogue::*;
//!
//! let mut tree = DialogueTree::new("elder_greeting");
//!
//! // Add nodes
//! tree.add_node(DialogueNode::new("start", "Elder", "dialog.elder.greeting"));
//! tree.add_node(DialogueNode::new("ask_quest", "Elder", "dialog.elder.quest_available"));
//! tree.add_node(DialogueNode::new("end", "Elder", "dialog.elder.farewell"));
//!
//! // Add choices
//! tree.add_choice("start", "ask_quest", "dialog.elder.choice_quest", None);
//! tree.add_choice("start", "end", "dialog.elder.choice_bye", None);
//!
//! tree.start_node = "start".to_string();
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Dialogue Tree Definition
// ---------------------------------------------------------------------------

/// A complete dialogue tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueTree {
    /// Unique identifier for this dialogue tree.
    pub id: String,
    /// Display name (localization key).
    pub name_key: String,
    /// Description (localization key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    /// All dialogue nodes.
    pub nodes: HashMap<String, DialogueNode>,
    /// Starting node ID.
    pub start_node: String,
    /// Speakers in this dialogue.
    pub speakers: HashMap<String, DialogueSpeaker>,
    /// Tags for filtering.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Associated quest ID (if this dialogue is quest-specific).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quest_id: Option<String>,
    /// Cooldown between conversations (seconds).
    #[serde(default)]
    pub cooldown: Option<f32>,
    /// Minimum player level to start this dialogue.
    #[serde(default)]
    pub min_level: Option<u32>,
    /// One-time dialogue (can only be completed once).
    #[serde(default)]
    pub one_time: bool,
    /// Priority for dialogue selection (higher = preferred).
    #[serde(default)]
    pub priority: i32,
}

impl DialogueTree {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: String::new(),
            description_key: None,
            nodes: HashMap::new(),
            start_node: String::new(),
            speakers: HashMap::new(),
            tags: Vec::new(),
            quest_id: None,
            cooldown: None,
            min_level: None,
            one_time: false,
            priority: 0,
        }
    }

    pub fn with_name(mut self, key: impl Into<String>) -> Self {
        self.name_key = key.into();
        self
    }

    pub fn with_description(mut self, key: impl Into<String>) -> Self {
        self.description_key = Some(key.into());
        self
    }

    pub fn with_quest_id(mut self, quest_id: impl Into<String>) -> Self {
        self.quest_id = Some(quest_id.into());
        self
    }

    pub fn with_cooldown(mut self, seconds: f32) -> Self {
        self.cooldown = Some(seconds);
        self
    }

    pub fn min_level(mut self, level: u32) -> Self {
        self.min_level = Some(level);
        self
    }

    pub fn one_time(mut self) -> Self {
        self.one_time = true;
        self
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn add_node(&mut self, node: DialogueNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_speaker(&mut self, speaker: DialogueSpeaker) {
        self.speakers.insert(speaker.id.clone(), speaker);
    }

    /// Add a choice from one node to another.
    pub fn add_choice(
        &mut self,
        from_node: &str,
        to_node: &str,
        text_key: impl Into<String>,
        condition: Option<DialogueCondition>,
    ) -> bool {
        if let Some(node) = self.nodes.get_mut(from_node) {
            node.choices.push(DialogueChoice {
                text_key: text_key.into(),
                next_node: to_node.to_string(),
                condition,
                on_select_effects: Vec::new(),
                on_select_script: None,
                time_limit: None,
                required_item: None,
                required_skill: None,
            });
            true
        } else {
            false
        }
    }

    pub fn get_node(&self, id: &str) -> Option<&DialogueNode> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut DialogueNode> {
        self.nodes.get_mut(id)
    }

    pub fn get_start_node(&self) -> Option<&DialogueNode> {
        self.nodes.get(&self.start_node)
    }

    /// Validate the dialogue tree for common issues.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.start_node.is_empty() {
            errors.push("No start node defined".to_string());
        } else if !self.nodes.contains_key(&self.start_node) {
            errors.push(format!(
                "Start node '{}' not found",
                self.start_node
            ));
        }

        // Check for unreachable nodes
        let reachable = self.find_reachable_nodes();
        for node_id in self.nodes.keys() {
            if !reachable.contains(node_id) {
                errors.push(format!("Node '{}' is unreachable", node_id));
            }
        }

        // Check for dead-end nodes (no choices and no auto-advance)
        // End nodes and choice targets don't need outgoing connections
        let choice_targets: std::collections::HashSet<&str> = self.nodes
            .values()
            .flat_map(|n| n.choices.iter().map(|c| c.next_node.as_str()))
            .collect();

        for (id, node) in &self.nodes {
            if node.choices.is_empty()
                && node.next_node.is_none()
                && node.node_type != DialogueNodeType::End
                && !choice_targets.contains(id.as_str())
            {
                // Only flag nodes that are neither End nodes nor choice targets
                errors.push(format!("Node '{}' has no outgoing connections", id));
            }
        }

        errors
    }

    /// Find all reachable nodes from the start node using BFS.
    fn find_reachable_nodes(&self) -> Vec<String> {
        if self.start_node.is_empty() {
            return Vec::new();
        }

        let mut reachable = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = vec![self.start_node.clone()];

        while let Some(current) = queue.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            reachable.push(current.clone());

            if let Some(node) = self.nodes.get(&current) {
                // Add choice targets
                for choice in &node.choices {
                    if !visited.contains(&choice.next_node) {
                        queue.push(choice.next_node.clone());
                    }
                }
                // Add auto-advance target
                if let Some(ref next) = node.next_node {
                    if !visited.contains(next) {
                        queue.push(next.clone());
                    }
                }
            }
        }

        reachable
    }
}

// ---------------------------------------------------------------------------
// Dialogue Node
// ---------------------------------------------------------------------------

/// Type of dialogue node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DialogueNodeType {
    /// Standard dialogue line.
    #[default]
    Dialogue,
    /// Player choice node (branching point).
    Choice,
    /// Conditional branch.
    Condition,
    /// Script execution.
    Script,
    /// Wait/delay node.
    Wait,
    /// End of dialogue.
    End,
}

/// A single dialogue node (line of dialogue or branching point).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueNode {
    /// Unique node ID.
    pub id: String,
    /// Type of node.
    #[serde(default)]
    pub node_type: DialogueNodeType,
    /// Speaker ID reference.
    pub speaker: String,
    /// Dialogue text localization key.
    pub text_key: String,
    /// Optional portrait emotion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portrait_emotion: Option<String>,
    /// Optional voice-over clip path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_clip: Option<String>,
    /// Player response options (choices).
    pub choices: Vec<DialogueChoice>,
    /// Conditions for this node to be shown.
    #[serde(default)]
    pub conditions: Vec<DialogueCondition>,
    /// Effects to apply when entering this node.
    #[serde(default)]
    pub on_enter_effects: Vec<DialogueEffect>,
    /// Effects to apply when exiting this node.
    #[serde(default)]
    pub on_exit_effects: Vec<DialogueEffect>,
    /// Next node if no choices (auto-advance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_node: Option<String>,
    /// Auto-advance delay (seconds).
    #[serde(default)]
    pub auto_advance_delay: Option<f32>,
    /// Lua script to run when entering this node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_enter_script: Option<String>,
    /// Lua script to run when exiting this node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_exit_script: Option<String>,
    /// Whether this node can be skipped.
    #[serde(default)]
    pub skippable: bool,
}

impl DialogueNode {
    pub fn new(
        id: impl Into<String>,
        speaker: impl Into<String>,
        text_key: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            node_type: DialogueNodeType::Dialogue,
            speaker: speaker.into(),
            text_key: text_key.into(),
            portrait_emotion: None,
            voice_clip: None,
            choices: Vec::new(),
            conditions: Vec::new(),
            on_enter_effects: Vec::new(),
            on_exit_effects: Vec::new(),
            next_node: None,
            auto_advance_delay: None,
            on_enter_script: None,
            on_exit_script: None,
            skippable: false,
        }
    }

    pub fn with_type(mut self, node_type: DialogueNodeType) -> Self {
        self.node_type = node_type;
        self
    }

    pub fn with_portrait(mut self, emotion: impl Into<String>) -> Self {
        self.portrait_emotion = Some(emotion.into());
        self
    }

    pub fn with_voice_clip(mut self, clip: impl Into<String>) -> Self {
        self.voice_clip = Some(clip.into());
        self
    }

    pub fn add_choice(mut self, choice: DialogueChoice) -> Self {
        self.choices.push(choice);
        self
    }

    pub fn add_condition(mut self, condition: DialogueCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    pub fn add_enter_effect(mut self, effect: DialogueEffect) -> Self {
        self.on_enter_effects.push(effect);
        self
    }

    pub fn add_exit_effect(mut self, effect: DialogueEffect) -> Self {
        self.on_exit_effects.push(effect);
        self
    }

    pub fn auto_advance(mut self, next_node: impl Into<String>, delay: Option<f32>) -> Self {
        self.next_node = Some(next_node.into());
        self.auto_advance_delay = delay;
        self
    }

    pub fn with_enter_script(mut self, script: impl Into<String>) -> Self {
        self.on_enter_script = Some(script.into());
        self
    }

    pub fn with_exit_script(mut self, script: impl Into<String>) -> Self {
        self.on_exit_script = Some(script.into());
        self
    }

    pub fn skippable(mut self) -> Self {
        self.skippable = true;
        self
    }
}

// ---------------------------------------------------------------------------
// Dialogue Choice
// ---------------------------------------------------------------------------

/// A player choice/response option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueChoice {
    /// Choice text localization key.
    pub text_key: String,
    /// Target node ID when selected.
    pub next_node: String,
    /// Conditions for this choice to appear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<DialogueCondition>,
    /// Effects when this choice is selected.
    #[serde(default)]
    pub on_select_effects: Vec<DialogueEffect>,
    /// Lua script to run when selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_select_script: Option<String>,
    /// Quick-time event (timed choice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_limit: Option<f32>,
    /// Required item to show/select this choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_item: Option<String>,
    /// Required skill/stat level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_skill: Option<(String, u32)>,
}

impl DialogueChoice {
    pub fn new(text_key: impl Into<String>, next_node: impl Into<String>) -> Self {
        Self {
            text_key: text_key.into(),
            next_node: next_node.into(),
            condition: None,
            on_select_effects: Vec::new(),
            on_select_script: None,
            time_limit: None,
            required_item: None,
            required_skill: None,
        }
    }

    pub fn with_condition(mut self, condition: DialogueCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    pub fn with_effect(mut self, effect: DialogueEffect) -> Self {
        self.on_select_effects.push(effect);
        self
    }

    pub fn with_select_script(mut self, script: impl Into<String>) -> Self {
        self.on_select_script = Some(script.into());
        self
    }

    pub fn time_limit(mut self, seconds: f32) -> Self {
        self.time_limit = Some(seconds);
        self
    }

    pub fn required_item(mut self, item: impl Into<String>) -> Self {
        self.required_item = Some(item.into());
        self
    }

    pub fn required_skill(mut self, skill: impl Into<String>, level: u32) -> Self {
        self.required_skill = Some((skill.into(), level));
        self
    }
}

// ---------------------------------------------------------------------------
// Dialogue Speaker
// ---------------------------------------------------------------------------

/// A speaker/character in a dialogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueSpeaker {
    /// Speaker ID.
    pub id: String,
    /// Display name localization key.
    pub name_key: String,
    /// Default portrait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_portrait: Option<String>,
    /// Portrait variations (emotion -> path).
    #[serde(default)]
    pub portraits: HashMap<String, String>,
    /// Voice pitch/tone settings.
    #[serde(default)]
    pub voice_pitch: f32,
}

impl DialogueSpeaker {
    pub fn new(id: impl Into<String>, name_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: name_key.into(),
            default_portrait: None,
            portraits: HashMap::new(),
            voice_pitch: 1.0,
        }
    }

    pub fn add_portrait(mut self, emotion: impl Into<String>, path: impl Into<String>) -> Self {
        self.portraits.insert(emotion.into(), path.into());
        self
    }

    pub fn with_default_portrait(mut self, path: impl Into<String>) -> Self {
        self.default_portrait = Some(path.into());
        self
    }

    pub fn voice_pitch(mut self, pitch: f32) -> Self {
        self.voice_pitch = pitch;
        self
    }
}

// ---------------------------------------------------------------------------
// Dialogue Conditions & Effects
// ---------------------------------------------------------------------------

/// Condition for showing dialogue/choices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueCondition {
    pub condition_type: DialogueConditionType,
}

impl DialogueCondition {
    pub fn flag_set(flag: impl Into<String>) -> Self {
        Self {
            condition_type: DialogueConditionType::FlagSet { flag: flag.into() },
        }
    }

    pub fn flag_not_set(flag: impl Into<String>) -> Self {
        Self {
            condition_type: DialogueConditionType::FlagNotSet { flag: flag.into() },
        }
    }

    pub fn quest_complete(quest_id: impl Into<String>) -> Self {
        Self {
            condition_type: DialogueConditionType::QuestComplete {
                quest_id: quest_id.into(),
            },
        }
    }

    pub fn quest_active(quest_id: impl Into<String>) -> Self {
        Self {
            condition_type: DialogueConditionType::QuestActive {
                quest_id: quest_id.into(),
            },
        }
    }

    pub fn has_item(item_id: impl Into<String>, count: u32) -> Self {
        Self {
            condition_type: DialogueConditionType::HasItem {
                item_id: item_id.into(),
                count,
            },
        }
    }

    pub fn level(min_level: u32) -> Self {
        Self {
            condition_type: DialogueConditionType::Level { min_level },
        }
    }

    pub fn compare(
        variable: impl Into<String>,
        operator: CompareOp,
        value: f32,
    ) -> Self {
        Self {
            condition_type: DialogueConditionType::Compare {
                variable: variable.into(),
                operator,
                value,
            },
        }
    }

    pub fn lua_script(script: impl Into<String>) -> Self {
        Self {
            condition_type: DialogueConditionType::LuaScript {
                script: script.into(),
            },
        }
    }

    pub fn random_chance(chance: f32) -> Self {
        Self {
            condition_type: DialogueConditionType::RandomChance { chance },
        }
    }

    pub fn and(self, other: DialogueCondition) -> AndCondition {
        AndCondition {
            conditions: vec![self, other],
        }
    }

    pub fn or(self, other: DialogueCondition) -> OrCondition {
        OrCondition {
            conditions: vec![self, other],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogueConditionType {
    /// Check if a flag is set.
    FlagSet { flag: String },
    /// Check if a flag is not set.
    FlagNotSet { flag: String },
    /// Check if player has an item.
    HasItem { item_id: String, count: u32 },
    /// Check if a quest is complete.
    QuestComplete { quest_id: String },
    /// Check if a quest is active.
    QuestActive { quest_id: String },
    /// Check player level.
    Level { min_level: u32 },
    /// Check player reputation with faction.
    Reputation { faction: String, min_rep: i32 },
    /// Compare a variable.
    Compare {
        variable: String,
        operator: CompareOp,
        value: f32,
    },
    /// Check time of day.
    TimeOfDay { min_hour: f32, max_hour: f32 },
    /// Check if weather matches.
    Weather { weather_type: String },
    /// Check if dialogue has been seen.
    DialogueSeen { dialogue_id: String },
    /// Custom Lua script that returns boolean.
    LuaScript { script: String },
    /// Random chance (0.0-1.0).
    RandomChance { chance: f32 },
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompareOp {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

/// Compound AND condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndCondition {
    pub conditions: Vec<DialogueCondition>,
}

/// Compound OR condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrCondition {
    pub conditions: Vec<DialogueCondition>,
}

// ---------------------------------------------------------------------------
// Dialogue Effects
// ---------------------------------------------------------------------------

/// Effect applied during dialogue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueEffect {
    pub effect_type: DialogueEffectType,
}

impl DialogueEffect {
    pub fn set_flag(flag: impl Into<String>, value: bool) -> Self {
        Self {
            effect_type: DialogueEffectType::SetFlag {
                flag: flag.into(),
                value,
            },
        }
    }

    pub fn give_item(item_id: impl Into<String>, count: u32) -> Self {
        Self {
            effect_type: DialogueEffectType::GiveItem {
                item_id: item_id.into(),
                count,
            },
        }
    }

    pub fn take_item(item_id: impl Into<String>, count: u32) -> Self {
        Self {
            effect_type: DialogueEffectType::TakeItem {
                item_id: item_id.into(),
                count,
            },
        }
    }

    pub fn set_variable(variable: impl Into<String>, value: f32) -> Self {
        Self {
            effect_type: DialogueEffectType::SetVariable {
                variable: variable.into(),
                value,
            },
        }
    }

    pub fn modify_variable(
        variable: impl Into<String>,
        operation: ModifyOp,
        value: f32,
    ) -> Self {
        Self {
            effect_type: DialogueEffectType::ModifyVariable {
                variable: variable.into(),
                operation,
                value,
            },
        }
    }

    pub fn start_quest(quest_id: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::StartQuest {
                quest_id: quest_id.into(),
            },
        }
    }

    pub fn complete_quest_objective(
        quest_id: impl Into<String>,
        objective_id: impl Into<String>,
    ) -> Self {
        Self {
            effect_type: DialogueEffectType::CompleteQuestObjective {
                quest_id: quest_id.into(),
                objective_id: objective_id.into(),
            },
        }
    }

    pub fn add_reputation(faction: impl Into<String>, amount: i32) -> Self {
        Self {
            effect_type: DialogueEffectType::AddReputation {
                faction: faction.into(),
                amount,
            },
        }
    }

    pub fn add_gold(amount: u32) -> Self {
        Self {
            effect_type: DialogueEffectType::AddGold { amount },
        }
    }

    pub fn add_experience(amount: u32) -> Self {
        Self {
            effect_type: DialogueEffectType::AddExperience { amount },
        }
    }

    pub fn play_sound(sound_id: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::PlaySound {
                sound_id: sound_id.into(),
            },
        }
    }

    pub fn play_animation(anim_id: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::PlayAnimation {
                animation_id: anim_id.into(),
            },
        }
    }

    pub fn spawn_entity(entity_id: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::SpawnEntity {
                entity_id: entity_id.into(),
            },
        }
    }

    pub fn set_dialogue_seen(dialogue_id: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::SetDialogueSeen {
                dialogue_id: dialogue_id.into(),
            },
        }
    }

    pub fn lua_script(script: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::LuaScript {
                script: script.into(),
            },
        }
    }

    pub fn trigger_event(event_id: impl Into<String>) -> Self {
        Self {
            effect_type: DialogueEffectType::TriggerEvent {
                event_id: event_id.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogueEffectType {
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
    CompleteQuestObjective {
        quest_id: String,
        objective_id: String,
    },
    /// Add reputation with faction.
    AddReputation { faction: String, amount: i32 },
    /// Add gold.
    AddGold { amount: u32 },
    /// Add experience.
    AddExperience { amount: u32 },
    /// Play a sound.
    PlaySound { sound_id: String },
    /// Play an animation.
    PlayAnimation { animation_id: String },
    /// Spawn an entity.
    SpawnEntity { entity_id: String },
    /// Mark dialogue as seen.
    SetDialogueSeen { dialogue_id: String },
    /// Run a Lua script.
    LuaScript { script: String },
    /// Trigger a game event.
    TriggerEvent { event_id: String },
    /// End dialogue immediately.
    EndDialogue,
    /// Teleport player/entity.
    Teleport { x: f32, y: f32, z: f32 },
    /// Fade screen.
    FadeScreen { color: [f32; 4], duration: f32 },
    /// Camera shake.
    CameraShake { intensity: f32, duration: f32 },
}

/// Modification operator for variables.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ModifyOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Set,
}

// ---------------------------------------------------------------------------
// Dialogue Runtime State
// ---------------------------------------------------------------------------

/// Runtime dialogue state.
#[derive(Debug, Clone)]
pub struct DialogueState {
    /// Current dialogue tree ID.
    pub current_dialog: Option<String>,
    /// Current node ID.
    pub current_node: Option<String>,
    /// Previous node ID (for backtracking).
    pub previous_node: Option<String>,
    /// Dialogue tree history (for backtracking).
    pub history: Vec<String>,
    /// Dialogues that have been seen.
    pub seen_dialogues: HashMap<String, bool>,
    /// Last time each dialogue was seen (for cooldown).
    pub dialogue_timestamps: HashMap<String, f32>,
    /// Dialogue variables.
    pub variables: HashMap<String, f32>,
    /// Dialogue flags.
    pub flags: HashMap<String, bool>,
    /// Current selected response index (for UI).
    pub selected_response: usize,
    /// Is waiting for player input.
    pub waiting_for_input: bool,
    /// Auto-advance timer.
    pub auto_advance_timer: Option<f32>,
    /// Is dialogue active.
    pub active: bool,
    /// Speaker entity reference.
    pub speaker_entity: Option<u64>,
    /// Listener entity reference.
    pub listener_entity: Option<u64>,
    /// Effects pending execution.
    pub pending_effects: Vec<DialogueEffect>,
}

impl Default for DialogueState {
    fn default() -> Self {
        Self {
            current_dialog: None,
            current_node: None,
            previous_node: None,
            history: Vec::new(),
            seen_dialogues: HashMap::new(),
            dialogue_timestamps: HashMap::new(),
            variables: HashMap::new(),
            flags: HashMap::new(),
            selected_response: 0,
            waiting_for_input: false,
            auto_advance_timer: None,
            active: false,
            speaker_entity: None,
            listener_entity: None,
            pending_effects: Vec::new(),
        }
    }
}

impl DialogueState {
    /// Start a dialogue.
    pub fn start_dialog(&mut self, dialogue_id: &str, start_node: &str) {
        self.current_dialog = Some(dialogue_id.to_string());
        self.current_node = Some(start_node.to_string());
        self.previous_node = None;
        self.history.clear();
        self.selected_response = 0;
        self.waiting_for_input = false;
        self.auto_advance_timer = None;
        self.active = true;
    }

    /// End the current dialogue.
    pub fn end_dialog(&mut self) {
        let dialogue_id = self.current_dialog.clone();
        if let Some(dialogue_id) = dialogue_id {
            self.mark_seen(&dialogue_id);
        }
        self.current_dialog = None;
        self.current_node = None;
        self.previous_node = None;
        self.history.clear();
        self.waiting_for_input = false;
        self.auto_advance_timer = None;
        self.active = false;
        self.pending_effects.clear();
    }

    /// Navigate to the next node.
    pub fn advance_to(&mut self, node_id: &str) {
        if let Some(current) = self.current_node.take() {
            self.previous_node = Some(current.clone());
            self.history.push(current);
        }
        self.current_node = Some(node_id.to_string());
        self.selected_response = 0;
        self.auto_advance_timer = None;
    }

    /// Select a response/choice.
    pub fn select_response(&mut self, index: usize) {
        self.selected_response = index;
    }

    /// Check if in dialogue.
    pub fn is_in_dialog(&self) -> bool {
        self.active && self.current_dialog.is_some()
    }

    /// Get current node ID.
    pub fn current_node_id(&self) -> Option<&str> {
        self.current_node.as_deref()
    }

    /// Get current dialogue ID.
    pub fn current_dialog_id(&self) -> Option<&str> {
        self.current_dialog.as_deref()
    }

    /// Set a flag.
    pub fn set_flag(&mut self, flag: &str, value: bool) {
        self.flags.insert(flag.to_string(), value);
    }

    /// Get a flag.
    pub fn get_flag(&self, flag: &str) -> bool {
        self.flags.get(flag).copied().unwrap_or(false)
    }

    /// Set a variable.
    pub fn set_variable(&mut self, name: &str, value: f32) {
        self.variables.insert(name.to_string(), value);
    }

    /// Get a variable.
    pub fn get_variable(&self, name: &str) -> f32 {
        self.variables.get(name).copied().unwrap_or(0.0)
    }

    /// Modify a variable.
    pub fn modify_variable(&mut self, name: &str, op: ModifyOp, value: f32) {
        let current = self.get_variable(name);
        let new_value = match op {
            ModifyOp::Add => current + value,
            ModifyOp::Subtract => current - value,
            ModifyOp::Multiply => current * value,
            ModifyOp::Divide => {
                if value.abs() < 0.001 {
                    current
                } else {
                    current / value
                }
            }
            ModifyOp::Set => value,
        };
        self.set_variable(name, new_value);
    }

    /// Mark dialogue as seen.
    pub fn mark_seen(&mut self, dialogue_id: &str) {
        self.seen_dialogues.insert(dialogue_id.to_string(), true);
        self.dialogue_timestamps
            .insert(dialogue_id.to_string(), 0.0); // Will be set by system
    }

    /// Check if dialogue has been seen.
    pub fn has_seen(&self, dialogue_id: &str) -> bool {
        self.seen_dialogues
            .get(dialogue_id)
            .copied()
            .unwrap_or(false)
    }

    /// Record dialogue timestamp.
    pub fn record_timestamp(&mut self, dialogue_id: &str, time: f32) {
        self.dialogue_timestamps
            .insert(dialogue_id.to_string(), time);
    }

    /// Get time since last dialogue was seen.
    pub fn time_since_seen(&self, dialogue_id: &str, current_time: f32) -> f32 {
        if let Some(&last_time) = self.dialogue_timestamps.get(dialogue_id) {
            current_time - last_time
        } else {
            f32::MAX // Never seen
        }
    }

    /// Queue an effect for execution.
    pub fn queue_effect(&mut self, effect: DialogueEffect) {
        self.pending_effects.push(effect);
    }

    /// Take pending effects.
    pub fn take_pending_effects(&mut self) -> Vec<DialogueEffect> {
        std::mem::take(&mut self.pending_effects)
    }

    /// Can go back in history?
    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    /// Go back to previous node.
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.history.pop() {
            self.current_node = Some(prev);
            self.selected_response = 0;
            self.auto_advance_timer = None;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Dialogue System
// ---------------------------------------------------------------------------

/// Dialogue system manager.
pub struct DialogueSystem {
    /// All loaded dialogue trees.
    pub trees: HashMap<String, DialogueTree>,
    /// Runtime state.
    pub state: DialogueState,
    /// Localization prefix for dialogue keys.
    pub localization_prefix: String,
    /// Current game time (for cooldowns).
    pub current_time: f32,
    /// Player level (for condition checks).
    pub player_level: u32,
    /// Whether dialogue is paused (for cutscenes, etc.).
    pub paused: bool,
}

impl DialogueSystem {
    pub fn new() -> Self {
        Self {
            trees: HashMap::new(),
            state: DialogueState::default(),
            localization_prefix: "dialog".to_string(),
            current_time: 0.0,
            player_level: 1,
            paused: false,
        }
    }

    /// Load a dialogue tree.
    pub fn load_tree(&mut self, tree: DialogueTree) {
        self.trees.insert(tree.id.clone(), tree);
    }

    /// Start a dialogue by ID.
    pub fn start(&mut self, dialogue_id: &str) -> bool {
        let tree = match self.trees.get(dialogue_id) {
            Some(t) => t,
            None => {
                log::warn!("Dialogue tree '{}' not found", dialogue_id);
                return false;
            }
        };

        // Check cooldown
        if let Some(cooldown) = tree.cooldown {
            let elapsed = self.state.time_since_seen(dialogue_id, self.current_time);
            if elapsed < cooldown {
                log::debug!(
                    "Dialogue '{}' on cooldown: {:.1}s remaining",
                    dialogue_id,
                    cooldown - elapsed
                );
                return false;
            }
        }

        // Check min level
        if let Some(min_level) = tree.min_level {
            if self.player_level < min_level {
                log::debug!(
                    "Dialogue '{}' requires level {}, player is {}",
                    dialogue_id,
                    min_level,
                    self.player_level
                );
                return false;
            }
        }

        // Check one-time
        if tree.one_time && self.state.has_seen(dialogue_id) {
            log::debug!("Dialogue '{}' is one-time and already seen", dialogue_id);
            return false;
        }

        if tree.start_node.is_empty() {
            log::warn!("Dialogue '{}' has no start node", dialogue_id);
            return false;
        }

        self.state.start_dialog(dialogue_id, &tree.start_node);
        self.state.record_timestamp(dialogue_id, self.current_time);

        // Execute on-enter effects for start node
        if let Some(node) = tree.get_start_node() {
            for effect in &node.on_enter_effects {
                self.state.queue_effect(effect.clone());
            }

            // Set auto-advance timer if applicable
            if node.choices.is_empty() && node.next_node.is_some() {
                self.state.auto_advance_timer = node.auto_advance_delay;
            }
        }

        true
    }

    /// Select a response/choice.
    pub fn select_response(&mut self, index: usize) -> bool {
        let dialogue_id = match &self.state.current_dialog {
            Some(id) => id.clone(),
            None => return false,
        };

        let tree = match self.trees.get(&dialogue_id) {
            Some(t) => t,
            None => return false,
        };

        let node = match &self.state.current_node {
            Some(id) => match tree.get_node(id) {
                Some(n) => n,
                None => return false,
            },
            None => return false,
        };

        // Get available responses
        let available = self.get_available_responses();
        if index >= available.len() {
            return false;
        }

        let choice = &node.choices[available[index].1];

        // Execute on-select effects
        for effect in &choice.on_select_effects {
            self.state.queue_effect(effect.clone());
        }

        self.state.advance_to(&choice.next_node);

        // Execute on-enter effects for new node
        if let Some(next_node) = tree.get_node(&choice.next_node) {
            for effect in &next_node.on_enter_effects {
                self.state.queue_effect(effect.clone());
            }
        }

        // Check for auto-advance
        self.check_auto_advance(tree);

        true
    }

    /// Advance to next node (for nodes without choices).
    pub fn advance(&mut self) {
        let dialogue_id = match &self.state.current_dialog {
            Some(id) => id.clone(),
            None => return,
        };

        let tree = match self.trees.get(&dialogue_id) {
            Some(t) => t,
            None => return,
        };

        let node = match &self.state.current_node {
            Some(id) => match tree.get_node(id) {
                Some(n) => n.clone(),
                None => return,
            },
            None => return,
        };

        // Execute on-exit effects
        for effect in &node.on_exit_effects {
            self.state.queue_effect(effect.clone());
        }

        if let Some(next) = &node.next_node {
            self.state.advance_to(next);

            // Execute on-enter effects for new node
            if let Some(next_node) = tree.get_node(next) {
                for effect in &next_node.on_enter_effects {
                    self.state.queue_effect(effect.clone());
                }
            }

            self.check_auto_advance(tree);
        } else {
            self.state.end_dialog();
        }
    }

    /// Check if current node should auto-advance.
    fn check_auto_advance(&self, tree: &DialogueTree) {
        if let Some(ref node_id) = self.state.current_node {
            if let Some(node) = tree.get_node(node_id) {
                if node.choices.is_empty() {
                    if let Some(_delay) = node.auto_advance_delay {
                        // Timer will be set by update
                    } else if node.node_type == DialogueNodeType::End {
                        // Immediate end
                    }
                }
            }
        }
    }

    /// Get available responses for current node (filtered by conditions).
    pub fn get_available_responses(&self) -> Vec<(String, usize)> {
        let dialogue_id = match &self.state.current_dialog {
            Some(id) => id.clone(),
            None => return Vec::new(),
        };

        let tree = match self.trees.get(&dialogue_id) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let node = match &self.state.current_node {
            Some(id) => match tree.get_node(id) {
                Some(n) => n,
                None => return Vec::new(),
            },
            None => return Vec::new(),
        };

        node.choices
            .iter()
            .enumerate()
            .filter(|(_, c)| self.evaluate_condition(&c.condition))
            .map(|(i, c)| (c.text_key.clone(), i))
            .collect()
    }

    /// Evaluate a dialogue condition.
    pub fn evaluate_condition(&self, condition: &Option<DialogueCondition>) -> bool {
        let condition = match condition {
            Some(c) => c,
            None => return true, // No condition = always show
        };

        match &condition.condition_type {
            DialogueConditionType::FlagSet { flag } => self.state.get_flag(flag),
            DialogueConditionType::FlagNotSet { flag } => !self.state.get_flag(flag),
            DialogueConditionType::HasItem { item_id, count } => {
                let key = format!("item_{}", item_id);
                self.state.get_variable(&key) >= *count as f32
            }
            DialogueConditionType::QuestComplete { quest_id } => {
                self.state.get_variable(&format!("quest_{}_complete", quest_id)) > 0.5
            }
            DialogueConditionType::QuestActive { quest_id } => {
                let state = self.state.get_variable(&format!("quest_{}_state", quest_id));
                (state - 1.0).abs() < 0.5 // Active = 1.0
            }
            DialogueConditionType::Level { min_level } => self.player_level >= *min_level,
            DialogueConditionType::Reputation { faction, min_rep } => {
                let rep_key = format!("rep_{}", faction);
                let current = self.state.get_variable(&rep_key) as i32;
                current >= *min_rep
            }
            DialogueConditionType::Compare {
                variable,
                operator,
                value,
            } => {
                let var_value = self.state.get_variable(variable);
                match operator {
                    CompareOp::Equal => (var_value - value).abs() < 0.001,
                    CompareOp::NotEqual => (var_value - value).abs() >= 0.001,
                    CompareOp::Greater => var_value > *value,
                    CompareOp::GreaterEqual => var_value >= *value,
                    CompareOp::Less => var_value < *value,
                    CompareOp::LessEqual => var_value <= *value,
                }
            }
            DialogueConditionType::TimeOfDay {
                min_hour,
                max_hour,
            } => {
                let current_hour = self.state.get_variable("game_hour");
                if min_hour <= max_hour {
                    current_hour >= *min_hour && current_hour <= *max_hour
                } else {
                    // Wraps around midnight
                    current_hour >= *min_hour || current_hour <= *max_hour
                }
            }
            DialogueConditionType::Weather { weather_type } => {
                let current = self.state.get_variable("current_weather");
                (current - Self::weather_to_value(weather_type)).abs() < 0.5
            }
            DialogueConditionType::DialogueSeen { dialogue_id } => {
                self.state.has_seen(dialogue_id)
            }
            DialogueConditionType::LuaScript { script } => {
                // In production, this would execute the Lua script
                // For now, treat as true (script would set flags/variables)
                log::debug!("Lua condition script: {}", script);
                true
            }
            DialogueConditionType::RandomChance { chance } => {
                // Simple random check using std::time for seeding
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as f32 / u32::MAX as f32)
                    .unwrap_or(0.5);
                seed <= *chance
            }
        }
    }

    fn weather_to_value(weather: &str) -> f32 {
        match weather {
            "clear" => 0.0,
            "rain" => 1.0,
            "storm" => 2.0,
            "snow" => 3.0,
            "fog" => 4.0,
            _ => 0.0,
        }
    }

    /// Apply a dialogue effect.
    pub fn apply_effect(&mut self, effect: &DialogueEffect) {
        match &effect.effect_type {
            DialogueEffectType::SetFlag { flag, value } => {
                self.state.set_flag(flag, *value);
            }
            DialogueEffectType::GiveItem { item_id, count } => {
                let key = format!("item_{}", item_id);
                let current = self.state.get_variable(&key);
                self.state.set_variable(&key, current + *count as f32);
            }
            DialogueEffectType::TakeItem { item_id, count } => {
                let key = format!("item_{}", item_id);
                let current = self.state.get_variable(&key);
                self.state
                    .set_variable(&key, (current - *count as f32).max(0.0));
            }
            DialogueEffectType::SetVariable { variable, value } => {
                self.state.set_variable(variable, *value);
            }
            DialogueEffectType::ModifyVariable {
                variable,
                operation,
                value,
            } => {
                self.state.modify_variable(variable, *operation, *value);
            }
            DialogueEffectType::AddReputation { faction, amount } => {
                let key = format!("rep_{}", faction);
                let current = self.state.get_variable(&key) as i32;
                self.state
                    .set_variable(&key, (current + amount).max(-1000) as f32);
            }
            DialogueEffectType::AddGold { amount } => {
                let current = self.state.get_variable("gold");
                self.state.set_variable("gold", current + *amount as f32);
            }
            DialogueEffectType::AddExperience { amount } => {
                let current = self.state.get_variable("experience");
                self.state
                    .set_variable("experience", current + *amount as f32);
            }
            DialogueEffectType::SetDialogueSeen { dialogue_id } => {
                self.state.mark_seen(dialogue_id);
            }
            _ => {
                // These effects are handled by game systems via pending_effects
                self.state.queue_effect(effect.clone());
            }
        }
    }

    /// Update dialogue timers.
    pub fn update(&mut self, dt: f32) {
        if self.paused {
            return;
        }

        self.current_time += dt;

        // Update auto-advance timer
        if let Some(ref mut timer) = self.state.auto_advance_timer {
            *timer -= dt;
            if *timer <= 0.0 {
                self.advance();
            }
        }

        // Check if current node has auto-advance and no timer set
        if self.state.is_in_dialog() {
            let dialogue_id = self.state.current_dialog.clone().unwrap();
            if let Some(tree) = self.trees.get(&dialogue_id) {
                if let Some(ref node_id) = self.state.current_node {
                    if let Some(node) = tree.get_node(node_id) {
                        if node.choices.is_empty()
                            && node.next_node.is_some()
                            && self.state.auto_advance_timer.is_none()
                        {
                            self.state.auto_advance_timer = node.auto_advance_delay;
                        }
                    }
                }
            }
        }
    }

    /// Get localized text for a key.
    pub fn get_localized_text(&self, key: &str) -> String {
        format!("{}.{}", self.localization_prefix, key)
    }

    /// Export dialogue trees to JSON.
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.trees)
    }

    /// Import dialogue trees from JSON.
    pub fn import_json(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let trees: HashMap<String, DialogueTree> = serde_json::from_str(json)?;
        self.trees = trees;
        Ok(())
    }
}

impl Default for DialogueSystem {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a test dialogue tree
    fn create_test_tree() -> DialogueTree {
        let mut tree = DialogueTree::new("test_dialogue");
        tree.start_node = "start".to_string();

        tree.add_node(DialogueNode::new(
            "start",
            "npc",
            "dialog.test.greeting",
        ));
        tree.add_node(DialogueNode::new(
            "quest",
            "npc",
            "dialog.test.quest",
        ));
        tree.add_node(DialogueNode::new("end", "npc", "dialog.test.farewell"));

        tree.add_choice("start", "quest", "dialog.test.choice_quest", None);
        tree.add_choice("start", "end", "dialog.test.choice_bye", None);

        tree
    }

    // =========================================================================
    // Dialogue Tree Tests
    // =========================================================================

    #[test]
    fn dialogue_tree_creation() {
        let tree = DialogueTree::new("test");
        assert_eq!(tree.id, "test");
        assert!(tree.nodes.is_empty());
    }

    #[test]
    fn dialogue_tree_builder() {
        let tree = DialogueTree::new("test")
            .with_name("dialog.test.name")
            .with_description("dialog.test.desc")
            .with_quest_id("quest_1")
            .with_cooldown(60.0)
            .min_level(5)
            .one_time()
            .priority(10)
            .add_tag("story")
            .add_tag("chapter1");

        assert_eq!(tree.name_key, "dialog.test.name");
        assert_eq!(tree.quest_id, Some("quest_1".to_string()));
        assert_eq!(tree.cooldown, Some(60.0));
        assert_eq!(tree.min_level, Some(5));
        assert!(tree.one_time);
        assert_eq!(tree.priority, 10);
        assert_eq!(tree.tags.len(), 2);
    }

    #[test]
    fn dialogue_tree_add_node() {
        let mut tree = DialogueTree::new("test");
        let node = DialogueNode::new("node1", "npc", "dialog.greeting");
        tree.add_node(node);

        assert!(tree.get_node("node1").is_some());
        assert!(tree.get_node("nonexistent").is_none());
    }

    #[test]
    fn dialogue_tree_add_choice() {
        let mut tree = DialogueTree::new("test");
        tree.add_node(DialogueNode::new("start", "npc", "dialog.start"));
        tree.add_node(DialogueNode::new("end", "npc", "dialog.end"));
        tree.start_node = "start".to_string();

        assert!(tree.add_choice(
            "start",
            "end",
            "dialog.choice",
            None
        ));
        assert!(!tree.add_choice(
            "nonexistent",
            "end",
            "dialog.choice",
            None
        ));
    }

    #[test]
    fn dialogue_tree_validate() {
        let mut tree = DialogueTree::new("test");
        // No start node - should error
        let errors = tree.validate();
        assert!(!errors.is_empty());

        tree = create_test_tree();
        let errors = tree.validate();
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn dialogue_tree_validate_unreachable_nodes() {
        let mut tree = create_test_tree();
        // Add unreachable node
        tree.add_node(DialogueNode::new(
            "unreachable",
            "npc",
            "dialog.unreachable",
        ));

        let errors = tree.validate();
        assert!(errors.iter().any(|e| e.contains("unreachable")));
    }

    #[test]
    fn dialogue_tree_find_reachable_nodes() {
        let tree = create_test_tree();
        let reachable = tree.find_reachable_nodes();

        assert!(reachable.contains(&"start".to_string()));
        assert!(reachable.contains(&"quest".to_string()));
        assert!(reachable.contains(&"end".to_string()));
    }

    // =========================================================================
    // Dialogue Node Tests
    // =========================================================================

    #[test]
    fn dialogue_node_creation() {
        let node = DialogueNode::new("test", "speaker", "dialog.test");
        assert_eq!(node.id, "test");
        assert_eq!(node.speaker, "speaker");
        assert_eq!(node.text_key, "dialog.test");
        assert_eq!(node.node_type, DialogueNodeType::Dialogue);
    }

    #[test]
    fn dialogue_node_builder() {
        let node = DialogueNode::new("test", "speaker", "dialog.test")
            .with_type(DialogueNodeType::Choice)
            .with_portrait("happy")
            .with_voice_clip("voice_1")
            .auto_advance("next", Some(2.0))
            .with_enter_script("print('hello')")
            .with_exit_script("print('bye')")
            .skippable();

        assert_eq!(node.node_type, DialogueNodeType::Choice);
        assert_eq!(node.portrait_emotion, Some("happy".to_string()));
        assert_eq!(node.voice_clip, Some("voice_1".to_string()));
        assert_eq!(node.next_node, Some("next".to_string()));
        assert_eq!(node.auto_advance_delay, Some(2.0));
        assert!(node.on_enter_script.is_some());
        assert!(node.on_exit_script.is_some());
        assert!(node.skippable);
    }

    // =========================================================================
    // Dialogue Choice Tests
    // =========================================================================

    #[test]
    fn dialogue_choice_creation() {
        let choice = DialogueChoice::new("dialog.choice", "next_node");
        assert_eq!(choice.text_key, "dialog.choice");
        assert_eq!(choice.next_node, "next_node");
        assert!(choice.condition.is_none());
    }

    #[test]
    fn dialogue_choice_builder() {
        let choice = DialogueChoice::new("dialog.choice", "next_node")
            .with_condition(DialogueCondition::flag_set("has_key"))
            .with_effect(DialogueEffect::set_flag("chose_option", true))
            .with_select_script("give_item('sword')")
            .time_limit(5.0)
            .required_item("key_item")
            .required_skill("persuasion", 10);

        assert!(choice.condition.is_some());
        assert!(!choice.on_select_effects.is_empty());
        assert!(choice.on_select_script.is_some());
        assert_eq!(choice.time_limit, Some(5.0));
        assert_eq!(choice.required_item, Some("key_item".to_string()));
        assert_eq!(
            choice.required_skill,
            Some(("persuasion".to_string(), 10))
        );
    }

    // =========================================================================
    // Dialogue Speaker Tests
    // =========================================================================

    #[test]
    fn dialogue_speaker_creation() {
        let speaker = DialogueSpeaker::new("npc", "dialog.npc_name");
        assert_eq!(speaker.id, "npc");
        assert_eq!(speaker.name_key, "dialog.npc_name");
    }

    #[test]
    fn dialogue_speaker_builder() {
        let speaker = DialogueSpeaker::new("npc", "dialog.npc_name")
            .add_portrait("happy", "portraits/happy.png")
            .with_default_portrait("portraits/default.png")
            .voice_pitch(0.8);

        assert_eq!(speaker.default_portrait, Some("portraits/default.png".to_string()));
        assert_eq!(speaker.voice_pitch, 0.8);
    }

    // =========================================================================
    // Dialogue Condition Tests
    // =========================================================================

    #[test]
    fn dialogue_condition_creation() {
        let cond = DialogueCondition::flag_set("talked_to_npc");
        assert!(matches!(
            cond.condition_type,
            DialogueConditionType::FlagSet { .. }
        ));
    }

    #[test]
    fn dialogue_condition_types() {
        let flag = DialogueCondition::flag_set("flag");
        assert!(matches!(
            flag.condition_type,
            DialogueConditionType::FlagSet { .. }
        ));

        let quest = DialogueCondition::quest_complete("quest_1");
        assert!(matches!(
            quest.condition_type,
            DialogueConditionType::QuestComplete { .. }
        ));

        let item = DialogueCondition::has_item("key", 1);
        assert!(matches!(
            item.condition_type,
            DialogueConditionType::HasItem { .. }
        ));

        let level = DialogueCondition::level(10);
        assert!(matches!(
            level.condition_type,
            DialogueConditionType::Level { .. }
        ));

        let lua = DialogueCondition::lua_script("return true");
        assert!(matches!(
            lua.condition_type,
            DialogueConditionType::LuaScript { .. }
        ));

        let chance = DialogueCondition::random_chance(0.5);
        if let DialogueConditionType::RandomChance { chance: c } = chance.condition_type {
            assert!((c - 0.5).abs() < 0.001);
        }
    }

    // =========================================================================
    // Dialogue Effect Tests
    // =========================================================================

    #[test]
    fn dialogue_effect_creation() {
        let effect = DialogueEffect::set_flag("talked", true);
        if let DialogueEffectType::SetFlag { flag, value } = effect.effect_type {
            assert_eq!(flag, "talked");
            assert!(value);
        } else {
            panic!("Expected SetFlag effect");
        }
    }

    #[test]
    fn dialogue_effect_types() {
        let give = DialogueEffect::give_item("sword", 1);
        assert!(matches!(give.effect_type, DialogueEffectType::GiveItem { .. }));

        let quest = DialogueEffect::start_quest("quest_1");
        assert!(matches!(quest.effect_type, DialogueEffectType::StartQuest { .. }));

        let rep = DialogueEffect::add_reputation("faction", 50);
        assert!(matches!(rep.effect_type, DialogueEffectType::AddReputation { .. }));

        let gold = DialogueEffect::add_gold(100);
        assert!(matches!(gold.effect_type, DialogueEffectType::AddGold { .. }));

        let xp = DialogueEffect::add_experience(500);
        assert!(matches!(xp.effect_type, DialogueEffectType::AddExperience { .. }));

        let sound = DialogueEffect::play_sound("click");
        assert!(matches!(sound.effect_type, DialogueEffectType::PlaySound { .. }));

        let spawn = DialogueEffect::spawn_entity("enemy");
        assert!(matches!(spawn.effect_type, DialogueEffectType::SpawnEntity { .. }));
    }

    // =========================================================================
    // Dialogue State Tests
    // =========================================================================

    #[test]
    fn dialogue_state_default() {
        let state = DialogueState::default();
        assert!(!state.active);
        assert!(state.current_dialog.is_none());
        assert!(!state.is_in_dialog());
    }

    #[test]
    fn dialogue_state_start_end() {
        let mut state = DialogueState::default();
        state.start_dialog("test", "start");

        assert!(state.active);
        assert_eq!(state.current_dialog, Some("test".to_string()));
        assert_eq!(state.current_node, Some("start".to_string()));
        assert!(state.is_in_dialog());

        state.end_dialog();
        assert!(!state.active);
        assert!(!state.is_in_dialog());
    }

    #[test]
    fn dialogue_state_flags() {
        let mut state = DialogueState::default();
        state.set_flag("talked", true);
        assert!(state.get_flag("talked"));
        assert!(!state.get_flag("not_set"));
    }

    #[test]
    fn dialogue_state_variables() {
        let mut state = DialogueState::default();
        state.set_variable("gold", 100.0);
        assert_eq!(state.get_variable("gold"), 100.0);
        assert_eq!(state.get_variable("missing"), 0.0);
    }

    #[test]
    fn dialogue_state_modify_variable() {
        let mut state = DialogueState::default();
        state.set_variable("gold", 100.0);

        state.modify_variable("gold", ModifyOp::Add, 50.0);
        assert_eq!(state.get_variable("gold"), 150.0);

        state.modify_variable("gold", ModifyOp::Multiply, 2.0);
        assert_eq!(state.get_variable("gold"), 300.0);

        state.modify_variable("gold", ModifyOp::Subtract, 100.0);
        assert_eq!(state.get_variable("gold"), 200.0);

        state.modify_variable("gold", ModifyOp::Set, 500.0);
        assert_eq!(state.get_variable("gold"), 500.0);

        // Division by zero protection
        state.modify_variable("gold", ModifyOp::Divide, 0.0);
        assert_eq!(state.get_variable("gold"), 500.0);
    }

    #[test]
    fn dialogue_state_seen_dialogues() {
        let mut state = DialogueState::default();
        assert!(!state.has_seen("dialog_1"));

        state.mark_seen("dialog_1");
        assert!(state.has_seen("dialog_1"));
    }

    #[test]
    fn dialogue_state_history() {
        let mut state = DialogueState::default();
        state.start_dialog("test", "node1");

        assert!(!state.can_go_back());

        state.advance_to("node2");
        assert!(state.can_go_back());
        assert_eq!(state.previous_node, Some("node1".to_string()));
        assert_eq!(state.history.len(), 1); // First advance pushes node1

        state.advance_to("node3");
        assert_eq!(state.history.len(), 2); // Second advance pushes node2

        state.go_back();
        assert_eq!(state.current_node, Some("node2".to_string()));
    }

    #[test]
    fn dialogue_state_pending_effects() {
        let mut state = DialogueState::default();
        state.queue_effect(DialogueEffect::set_flag("flag1", true));
        state.queue_effect(DialogueEffect::add_gold(100));

        let effects = state.take_pending_effects();
        assert_eq!(effects.len(), 2);
        assert!(state.pending_effects.is_empty());
    }

    #[test]
    fn dialogue_state_timestamp() {
        let mut state = DialogueState::default();
        state.record_timestamp("dialog_1", 10.0);

        assert_eq!(state.time_since_seen("dialog_1", 15.0), 5.0);
        assert_eq!(state.time_since_seen("dialog_2", 15.0), f32::MAX);
    }

    // =========================================================================
    // Dialogue System Tests
    // =========================================================================

    #[test]
    fn dialogue_system_creation() {
        let system = DialogueSystem::new();
        assert!(system.trees.is_empty());
        assert!(!system.state.active);
    }

    #[test]
    fn dialogue_system_load_and_start() {
        let mut system = DialogueSystem::new();
        let tree = create_test_tree();
        system.load_tree(tree);

        assert!(system.start("test_dialogue"));
        assert!(system.state.is_in_dialog());
        assert_eq!(system.state.current_node, Some("start".to_string()));
    }

    #[test]
    fn dialogue_system_start_nonexistent() {
        let mut system = DialogueSystem::new();
        assert!(!system.start("nonexistent"));
    }

    #[test]
    fn dialogue_system_select_response() {
        let mut system = DialogueSystem::new();
        system.load_tree(create_test_tree());
        system.start("test_dialogue");

        let available = system.get_available_responses();
        assert_eq!(available.len(), 2);

        assert!(system.select_response(0));
    }

    #[test]
    fn dialogue_system_cooldown() {
        let mut tree = create_test_tree();
        tree.cooldown = Some(10.0);

        let mut system = DialogueSystem::new();
        system.load_tree(tree);

        assert!(system.start("test_dialogue"));
        system.state.end_dialog();

        // Try to start again immediately - should fail due to cooldown
        assert!(!system.start("test_dialogue"));

        // Advance time past cooldown
        system.current_time = 15.0;
        assert!(system.start("test_dialogue"));
    }

    #[test]
    fn dialogue_system_one_time() {
        let mut tree = create_test_tree();
        tree.one_time = true;

        let mut system = DialogueSystem::new();
        system.load_tree(tree);

        assert!(system.start("test_dialogue"));
        system.state.end_dialog();

        // Try again - should fail
        assert!(!system.start("test_dialogue"));
    }

    #[test]
    fn dialogue_system_min_level() {
        let mut tree = create_test_tree();
        tree.min_level = Some(10);

        let mut system = DialogueSystem::new();
        system.load_tree(tree);
        system.player_level = 5;

        assert!(!system.start("test_dialogue"));

        system.player_level = 10;
        assert!(system.start("test_dialogue"));
    }

    #[test]
    fn dialogue_system_available_responses() {
        let mut tree = create_test_tree();
        // Add conditional choice
        if let Some(node) = tree.get_node_mut("start") {
            node.choices.push(DialogueChoice {
                text_key: "dialog.conditional".to_string(),
                next_node: "quest".to_string(),
                condition: Some(DialogueCondition::flag_set("has_flag")),
                on_select_effects: Vec::new(),
                on_select_script: None,
                time_limit: None,
                required_item: None,
                required_skill: None,
            });
        }

        let mut system = DialogueSystem::new();
        system.load_tree(tree);
        system.start("test_dialogue");

        // Without flag - 2 choices
        let available = system.get_available_responses();
        assert_eq!(available.len(), 2);

        // Set flag - 3 choices
        system.state.set_flag("has_flag", true);
        let available = system.get_available_responses();
        assert_eq!(available.len(), 3);
    }

    #[test]
    fn dialogue_system_evaluate_conditions() {
        let mut system = DialogueSystem::new();

        // Flag condition
        system.state.set_flag("test_flag", true);
        let cond = DialogueCondition::flag_set("test_flag");
        assert!(system.evaluate_condition(&Some(cond)));

        let cond = DialogueCondition::flag_not_set("test_flag");
        assert!(!system.evaluate_condition(&Some(cond)));

        // Level condition
        system.player_level = 10;
        let cond = DialogueCondition::level(5);
        assert!(system.evaluate_condition(&Some(cond)));

        let cond = DialogueCondition::level(15);
        assert!(!system.evaluate_condition(&Some(cond)));

        // Compare condition
        system.state.set_variable("health", 75.0);
        let cond = DialogueCondition::compare("health", CompareOp::GreaterEqual, 50.0);
        assert!(system.evaluate_condition(&Some(cond)));

        // No condition - always true
        assert!(system.evaluate_condition(&None));
    }

    #[test]
    fn dialogue_system_apply_effects() {
        let mut system = DialogueSystem::new();

        system.apply_effect(&DialogueEffect::set_flag("flag", true));
        assert!(system.state.get_flag("flag"));

        system.apply_effect(&DialogueEffect::give_item("sword", 1));
        assert_eq!(system.state.get_variable("item_sword"), 1.0);

        system.apply_effect(&DialogueEffect::add_gold(100));
        assert_eq!(system.state.get_variable("gold"), 100.0);

        system.apply_effect(&DialogueEffect::add_experience(500));
        assert_eq!(system.state.get_variable("experience"), 500.0);

        system.apply_effect(&DialogueEffect::add_reputation("faction", 50));
        assert_eq!(system.state.get_variable("rep_faction"), 50.0);

        system.apply_effect(&DialogueEffect::set_variable("custom", 42.0));
        assert_eq!(system.state.get_variable("custom"), 42.0);
    }

    #[test]
    fn dialogue_system_update_auto_advance() {
        let mut tree = DialogueTree::new("test");
        tree.start_node = "start".to_string();
        tree.add_node(
            DialogueNode::new("start", "npc", "dialog.start")
                .auto_advance("end", Some(2.0)),
        );
        tree.add_node(DialogueNode::new("end", "npc", "dialog.end"));

        let mut system = DialogueSystem::new();
        system.load_tree(tree);
        system.start("test");

        // Update should not advance yet (timer = 2.0)
        system.update(1.0);
        assert_eq!(system.state.current_node, Some("start".to_string()));

        // Update past timer
        system.update(1.5);
        assert_eq!(system.state.current_node, Some("end".to_string()));
    }

    #[test]
    fn dialogue_system_export_import() {
        let mut system = DialogueSystem::new();
        system.load_tree(create_test_tree());

        let json = system.export_json().unwrap();
        assert!(!json.is_empty());

        let mut system2 = DialogueSystem::new();
        system2.import_json(&json).unwrap();

        assert_eq!(system2.trees.len(), 1);
        assert!(system2.trees.contains_key("test_dialogue"));
    }

    #[test]
    fn dialogue_system_paused() {
        let mut system = DialogueSystem::new();
        system.paused = true;
        let old_time = system.current_time;

        system.update(1.0);
        assert_eq!(system.current_time, old_time); // Time shouldn't advance

        system.paused = false;
        system.update(1.0);
        assert_eq!(system.current_time, old_time + 1.0);
    }

    #[test]
    fn dialogue_system_get_localized_text() {
        let system = DialogueSystem::new();
        let text = system.get_localized_text("test.greeting");
        assert_eq!(text, "dialog.test.greeting");
    }
}
