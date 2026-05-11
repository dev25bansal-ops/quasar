//! Quest Editor Panel for Quasar Engine.
//!
//! Provides:
//! - Quest list with search/filter
//! - Quest definition editor (name, description, objectives, rewards)
//! - Objective editor with type-specific UI
//! - Reward editor (items, currency, XP)
//! - Dialogue tree editor integration
//! - Quest testing (start, progress, complete)
//! - Export to JSON for runtime loading
//!
//! Built with egui for integration with the Quasar editor.

use egui::{self, Color32, RichText, Ui};
use quasar_core::quest::{
    ObjectiveState, Quest, QuestCategory, QuestInstance, QuestObjective, QuestObjectiveType,
    QuestPrerequisite, QuestPrerequisiteType, QuestReward, QuestRewardType, QuestState,
    QuestSystem,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Quest Editor State
// ---------------------------------------------------------------------------

/// State for the quest editor panel.
pub struct QuestEditor {
    /// Whether the editor is visible.
    pub visible: bool,
    /// All quest definitions.
    pub quests: Vec<QuestDef>,
    /// Selected quest index.
    pub selected_quest: Option<usize>,
    /// Search query.
    pub search_query: String,
    /// Category filter.
    pub category_filter: Option<QuestCategory>,
    /// Current editing tab.
    pub active_tab: EditorTab,
    /// New quest name input.
    pub new_quest_name: String,
    /// Undo stack.
    pub undo_stack: Vec<EditorState>,
    /// Redo stack.
    pub redo_stack: Vec<EditorState>,
    /// Test mode state.
    pub test_mode: TestModeState,
    /// Validation errors.
    pub validation_errors: Vec<String>,
    /// Show validation panel.
    pub show_validation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTab {
    #[default]
    General,
    Objectives,
    Rewards,
    Prerequisites,
    Dialogue,
    Testing,
}

/// Quest definition for editor (editable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: QuestCategory,
    pub giver: String,
    pub turn_in: String,
    pub icon: String,
    pub hidden: bool,
    pub repeatable: bool,
    pub time_limit: Option<f32>,
    pub objectives: Vec<ObjectiveDef>,
    pub rewards: Vec<RewardDef>,
    pub prerequisites: Vec<PrerequisiteDef>,
    pub dialogue_tree_id: String,
    pub tags: Vec<String>,
    pub availability_script: String,
    pub on_start_script: String,
    pub on_complete_script: String,
}

impl QuestDef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            description: String::new(),
            category: QuestCategory::Side,
            giver: String::new(),
            turn_in: String::new(),
            icon: String::new(),
            hidden: false,
            repeatable: false,
            time_limit: None,
            objectives: Vec::new(),
            rewards: Vec::new(),
            prerequisites: Vec::new(),
            dialogue_tree_id: String::new(),
            tags: Vec::new(),
            availability_script: String::new(),
            on_start_script: String::new(),
            on_complete_script: String::new(),
        }
    }

    /// Convert to runtime Quest.
    pub fn to_runtime(&self) -> Quest {
        let mut quest = Quest::new(&self.id)
            .with_title(&self.name)
            .with_description(&self.description)
            .with_category(self.category);

        if !self.giver.is_empty() {
            quest = quest.with_giver(&self.giver);
        }
        if !self.turn_in.is_empty() {
            quest = quest.with_turn_in(&self.turn_in);
        }
        if !self.icon.is_empty() {
            quest = quest.with_icon(&self.icon);
        }
        if self.repeatable {
            quest = quest.repeatable();
        }
        if self.hidden {
            quest = quest.hidden();
        }
        if let Some(limit) = self.time_limit {
            quest = quest.with_time_limit(limit);
        }
        if !self.dialogue_tree_id.is_empty() {
            quest = quest.with_dialogue_tree(&self.dialogue_tree_id);
        }
        if !self.availability_script.is_empty() {
            quest.availability_script = Some(self.availability_script.clone());
        }
        if !self.on_start_script.is_empty() {
            quest.on_start_script = Some(self.on_start_script.clone());
        }
        if !self.on_complete_script.is_empty() {
            quest.on_complete_script = Some(self.on_complete_script.clone());
        }
        for tag in &self.tags {
            quest = quest.add_tag(tag);
        }

        for obj in &self.objectives {
            quest = quest.add_objective(obj.to_runtime());
        }

        for reward in &self.rewards {
            quest = quest.add_reward(reward.to_runtime());
        }

        for prereq in &self.prerequisites {
            quest = quest.add_prerequisite(prereq.to_runtime());
        }

        quest
    }

    /// Load from runtime Quest.
    pub fn from_runtime(quest: &Quest) -> Self {
        Self {
            id: quest.id.clone(),
            name: quest.title_key.clone(),
            description: quest.description_key.clone(),
            category: quest.category,
            giver: quest.giver.clone().unwrap_or_default(),
            turn_in: quest.turn_in.clone().unwrap_or_default(),
            icon: quest.icon.clone().unwrap_or_default(),
            hidden: quest.hidden,
            repeatable: quest.repeatable,
            time_limit: quest.time_limit,
            objectives: quest
                .objectives
                .iter()
                .map(ObjectiveDef::from_runtime)
                .collect(),
            rewards: quest.rewards.iter().map(RewardDef::from_runtime).collect(),
            prerequisites: quest
                .prerequisites
                .iter()
                .map(PrerequisiteDef::from_runtime)
                .collect(),
            dialogue_tree_id: quest.dialogue_tree_id.clone().unwrap_or_default(),
            tags: quest.tags.clone(),
            availability_script: quest.availability_script.clone().unwrap_or_default(),
            on_start_script: quest.on_start_script.clone().unwrap_or_default(),
            on_complete_script: quest.on_complete_script.clone().unwrap_or_default(),
        }
    }

    /// Validate the quest definition.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.id.is_empty() {
            errors.push("Quest ID is required".to_string());
        }
        if self.name.is_empty() {
            errors.push("Quest name is required".to_string());
        }
        if self.objectives.is_empty() {
            errors.push("Quest must have at least one objective".to_string());
        }
        if self.rewards.is_empty() {
            errors.push("Quest should have at least one reward".to_string());
        }

        // Check objective IDs are unique
        let mut obj_ids = std::collections::HashSet::new();
        for obj in &self.objectives {
            if !obj_ids.insert(&obj.id) {
                errors.push(format!("Duplicate objective ID: {}", obj.id));
            }
        }

        // Check time limit is positive
        if let Some(limit) = self.time_limit {
            if limit <= 0.0 {
                errors.push("Time limit must be positive".to_string());
            }
        }

        errors
    }
}

/// Objective definition for editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveDef {
    pub id: String,
    pub description: String,
    pub objective_type: ObjectiveTypeEditor,
    pub required: u32,
    pub optional: bool,
    pub sequence_order: Option<u32>,
    pub target_entity: String,
    pub target_position: [f32; 3],
    pub target_radius: f32,
}

impl ObjectiveDef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: String::new(),
            objective_type: ObjectiveTypeEditor::Count {
                counter: String::new(),
            },
            required: 1,
            optional: false,
            sequence_order: None,
            target_entity: String::new(),
            target_position: [0.0, 0.0, 0.0],
            target_radius: 5.0,
        }
    }

    pub fn to_runtime(&self) -> QuestObjective {
        let mut obj = QuestObjective::new(&self.id, &self.description, self.required);

        obj = obj.with_type(self.objective_type.to_runtime());

        if self.optional {
            obj = obj.optional();
        }
        if !self.target_entity.is_empty() {
            obj = obj.with_target_entity(&self.target_entity);
        }
        if self.target_position != [0.0, 0.0, 0.0] {
            obj = obj.with_location(self.target_position, self.target_radius);
        }
        if let Some(order) = self.sequence_order {
            obj = obj.with_sequence_order(order);
        }

        obj
    }

    pub fn from_runtime(obj: &QuestObjective) -> Self {
        Self {
            id: obj.id.clone(),
            description: obj.description_key.clone(),
            objective_type: ObjectiveTypeEditor::from_runtime(&obj.objective_type),
            required: obj.required,
            optional: obj.optional,
            sequence_order: obj.sequence_order,
            target_entity: obj.target_entity.clone().unwrap_or_default(),
            target_position: obj.target_position.unwrap_or([0.0, 0.0, 0.0]),
            target_radius: obj.target_radius,
        }
    }
}

/// Objective type editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectiveTypeEditor {
    ReachLocation { location_id: String },
    TalkTo { npc_id: String },
    CollectItem { item_id: String },
    DefeatEnemy { enemy_type: String },
    Interact { object_id: String },
    UseAbility { ability_id: String },
    Count { counter: String },
    Escort { npc_id: String, location_id: String },
    Survive { duration: f32 },
    Custom { id: String, params: Vec<ParamDef> },
}

impl ObjectiveTypeEditor {
    pub fn to_runtime(&self) -> QuestObjectiveType {
        match self {
            ObjectiveTypeEditor::ReachLocation { location_id } => {
                QuestObjectiveType::ReachLocation {
                    location_id: location_id.clone(),
                }
            }
            ObjectiveTypeEditor::TalkTo { npc_id } => QuestObjectiveType::TalkTo {
                npc_id: npc_id.clone(),
            },
            ObjectiveTypeEditor::CollectItem { item_id } => QuestObjectiveType::CollectItem {
                item_id: item_id.clone(),
            },
            ObjectiveTypeEditor::DefeatEnemy { enemy_type } => QuestObjectiveType::DefeatEnemy {
                enemy_type: enemy_type.clone(),
            },
            ObjectiveTypeEditor::Interact { object_id } => QuestObjectiveType::Interact {
                object_id: object_id.clone(),
            },
            ObjectiveTypeEditor::UseAbility { ability_id } => QuestObjectiveType::UseAbility {
                ability_id: ability_id.clone(),
            },
            ObjectiveTypeEditor::Count { counter } => QuestObjectiveType::Count {
                counter: counter.clone(),
            },
            ObjectiveTypeEditor::Escort {
                npc_id,
                location_id,
            } => QuestObjectiveType::Escort {
                npc_id: npc_id.clone(),
                location_id: location_id.clone(),
            },
            ObjectiveTypeEditor::Survive { duration } => QuestObjectiveType::Survive {
                duration: *duration,
            },
            ObjectiveTypeEditor::Custom { id, params } => {
                let params_map: HashMap<String, String> = params
                    .iter()
                    .map(|p| (p.key.clone(), p.value.clone()))
                    .collect();
                QuestObjectiveType::Custom {
                    id: id.clone(),
                    params: params_map,
                }
            }
        }
    }

    pub fn from_runtime(t: &QuestObjectiveType) -> Self {
        match t {
            QuestObjectiveType::ReachLocation { location_id } => {
                ObjectiveTypeEditor::ReachLocation {
                    location_id: location_id.clone(),
                }
            }
            QuestObjectiveType::TalkTo { npc_id } => ObjectiveTypeEditor::TalkTo {
                npc_id: npc_id.clone(),
            },
            QuestObjectiveType::CollectItem { item_id } => ObjectiveTypeEditor::CollectItem {
                item_id: item_id.clone(),
            },
            QuestObjectiveType::DefeatEnemy { enemy_type } => ObjectiveTypeEditor::DefeatEnemy {
                enemy_type: enemy_type.clone(),
            },
            QuestObjectiveType::Interact { object_id } => ObjectiveTypeEditor::Interact {
                object_id: object_id.clone(),
            },
            QuestObjectiveType::UseAbility { ability_id } => ObjectiveTypeEditor::UseAbility {
                ability_id: ability_id.clone(),
            },
            QuestObjectiveType::Count { counter } => ObjectiveTypeEditor::Count {
                counter: counter.clone(),
            },
            QuestObjectiveType::Escort {
                npc_id,
                location_id,
            } => ObjectiveTypeEditor::Escort {
                npc_id: npc_id.clone(),
                location_id: location_id.clone(),
            },
            QuestObjectiveType::Survive { duration } => ObjectiveTypeEditor::Survive {
                duration: *duration,
            },
            QuestObjectiveType::Custom { id, params } => {
                let params_vec: Vec<ParamDef> = params
                    .iter()
                    .map(|(k, v)| ParamDef {
                        key: k.clone(),
                        value: v.clone(),
                    })
                    .collect();
                ObjectiveTypeEditor::Custom {
                    id: id.clone(),
                    params: params_vec,
                }
            }
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ObjectiveTypeEditor::ReachLocation { .. } => "Reach Location",
            ObjectiveTypeEditor::TalkTo { .. } => "Talk To",
            ObjectiveTypeEditor::CollectItem { .. } => "Collect Item",
            ObjectiveTypeEditor::DefeatEnemy { .. } => "Defeat Enemy",
            ObjectiveTypeEditor::Interact { .. } => "Interact",
            ObjectiveTypeEditor::UseAbility { .. } => "Use Ability",
            ObjectiveTypeEditor::Count { .. } => "Count",
            ObjectiveTypeEditor::Escort { .. } => "Escort",
            ObjectiveTypeEditor::Survive { .. } => "Survive",
            ObjectiveTypeEditor::Custom { .. } => "Custom",
        }
    }
}

/// Key-value parameter definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamDef {
    pub key: String,
    pub value: String,
}

/// Reward definition for editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardDef {
    pub reward_type: RewardTypeEditor,
}

impl RewardDef {
    pub fn experience(amount: u32) -> Self {
        Self {
            reward_type: RewardTypeEditor::Experience { amount },
        }
    }

    pub fn gold(amount: u32) -> Self {
        Self {
            reward_type: RewardTypeEditor::Gold { amount },
        }
    }

    pub fn item(item_id: impl Into<String>, count: u32) -> Self {
        Self {
            reward_type: RewardTypeEditor::Item {
                item_id: item_id.into(),
                count,
            },
        }
    }

    pub fn to_runtime(&self) -> QuestReward {
        QuestReward {
            reward_type: self.reward_type.to_runtime(),
        }
    }

    pub fn from_runtime(reward: &QuestReward) -> Self {
        Self {
            reward_type: RewardTypeEditor::from_runtime(&reward.reward_type),
        }
    }
}

/// Reward type editor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RewardTypeEditor {
    Experience {
        amount: u32,
    },
    Gold {
        amount: u32,
    },
    Item {
        item_id: String,
        count: u32,
    },
    Reputation {
        faction: String,
        amount: i32,
    },
    Unlock {
        unlock_type: String,
        unlock_id: String,
    },
    Achievement {
        achievement_id: String,
    },
    Ability {
        ability_id: String,
    },
    AreaUnlock {
        area_id: String,
    },
    Custom {
        id: String,
        params: Vec<ParamDef>,
    },
}

impl RewardTypeEditor {
    pub fn to_runtime(&self) -> QuestRewardType {
        match self {
            RewardTypeEditor::Experience { amount } => {
                QuestRewardType::Experience { amount: *amount }
            }
            RewardTypeEditor::Gold { amount } => QuestRewardType::Gold { amount: *amount },
            RewardTypeEditor::Item { item_id, count } => QuestRewardType::Item {
                item_id: item_id.clone(),
                count: *count,
            },
            RewardTypeEditor::Reputation { faction, amount } => QuestRewardType::Reputation {
                faction: faction.clone(),
                amount: *amount,
            },
            RewardTypeEditor::Unlock {
                unlock_type,
                unlock_id,
            } => QuestRewardType::Unlock {
                unlock_type: unlock_type.clone(),
                unlock_id: unlock_id.clone(),
            },
            RewardTypeEditor::Achievement { achievement_id } => QuestRewardType::Achievement {
                achievement_id: achievement_id.clone(),
            },
            RewardTypeEditor::Ability { ability_id } => QuestRewardType::Ability {
                ability_id: ability_id.clone(),
            },
            RewardTypeEditor::AreaUnlock { area_id } => QuestRewardType::AreaUnlock {
                area_id: area_id.clone(),
            },
            RewardTypeEditor::Custom { id, params } => {
                let params_map: HashMap<String, String> = params
                    .iter()
                    .map(|p| (p.key.clone(), p.value.clone()))
                    .collect();
                QuestRewardType::Custom {
                    id: id.clone(),
                    params: params_map,
                }
            }
        }
    }

    pub fn from_runtime(t: &QuestRewardType) -> Self {
        match t {
            QuestRewardType::Experience { amount } => {
                RewardTypeEditor::Experience { amount: *amount }
            }
            QuestRewardType::Gold { amount } => RewardTypeEditor::Gold { amount: *amount },
            QuestRewardType::Item { item_id, count } => RewardTypeEditor::Item {
                item_id: item_id.clone(),
                count: *count,
            },
            QuestRewardType::Reputation { faction, amount } => RewardTypeEditor::Reputation {
                faction: faction.clone(),
                amount: *amount,
            },
            QuestRewardType::Unlock {
                unlock_type,
                unlock_id,
            } => RewardTypeEditor::Unlock {
                unlock_type: unlock_type.clone(),
                unlock_id: unlock_id.clone(),
            },
            QuestRewardType::Achievement { achievement_id } => RewardTypeEditor::Achievement {
                achievement_id: achievement_id.clone(),
            },
            QuestRewardType::Ability { ability_id } => RewardTypeEditor::Ability {
                ability_id: ability_id.clone(),
            },
            QuestRewardType::AreaUnlock { area_id } => RewardTypeEditor::AreaUnlock {
                area_id: area_id.clone(),
            },
            QuestRewardType::Custom { id, params } => {
                let params_vec: Vec<ParamDef> = params
                    .iter()
                    .map(|(k, v)| ParamDef {
                        key: k.clone(),
                        value: v.clone(),
                    })
                    .collect();
                RewardTypeEditor::Custom {
                    id: id.clone(),
                    params: params_vec,
                }
            }
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            RewardTypeEditor::Experience { .. } => "Experience",
            RewardTypeEditor::Gold { .. } => "Gold",
            RewardTypeEditor::Item { .. } => "Item",
            RewardTypeEditor::Reputation { .. } => "Reputation",
            RewardTypeEditor::Unlock { .. } => "Unlock",
            RewardTypeEditor::Achievement { .. } => "Achievement",
            RewardTypeEditor::Ability { .. } => "Ability",
            RewardTypeEditor::AreaUnlock { .. } => "Area Unlock",
            RewardTypeEditor::Custom { .. } => "Custom",
        }
    }
}

/// Prerequisite definition for editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrerequisiteDef {
    pub prerequisite_type: PrerequisiteTypeEditor,
}

impl PrerequisiteDef {
    pub fn to_runtime(&self) -> QuestPrerequisite {
        QuestPrerequisite {
            prerequisite_type: self.prerequisite_type.to_runtime(),
        }
    }

    pub fn from_runtime(prereq: &QuestPrerequisite) -> Self {
        Self {
            prerequisite_type: PrerequisiteTypeEditor::from_runtime(&prereq.prerequisite_type),
        }
    }
}

/// Prerequisite type editor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrerequisiteTypeEditor {
    QuestComplete { quest_id: String },
    QuestActive { quest_id: String },
    Level { level: u32 },
    FlagSet { flag: String },
    ItemOwned { item_id: String },
    Reputation { faction: String, min_rep: i32 },
    Custom { id: String },
}

impl PrerequisiteTypeEditor {
    pub fn to_runtime(&self) -> QuestPrerequisiteType {
        match self {
            PrerequisiteTypeEditor::QuestComplete { quest_id } => {
                QuestPrerequisiteType::QuestComplete {
                    quest_id: quest_id.clone(),
                }
            }
            PrerequisiteTypeEditor::QuestActive { quest_id } => {
                QuestPrerequisiteType::QuestActive {
                    quest_id: quest_id.clone(),
                }
            }
            PrerequisiteTypeEditor::Level { level } => {
                QuestPrerequisiteType::Level { level: *level }
            }
            PrerequisiteTypeEditor::FlagSet { flag } => {
                QuestPrerequisiteType::FlagSet { flag: flag.clone() }
            }
            PrerequisiteTypeEditor::ItemOwned { item_id } => QuestPrerequisiteType::ItemOwned {
                item_id: item_id.clone(),
            },
            PrerequisiteTypeEditor::Reputation { faction, min_rep } => {
                QuestPrerequisiteType::Reputation {
                    faction: faction.clone(),
                    min_rep: *min_rep,
                }
            }
            PrerequisiteTypeEditor::Custom { id } => {
                QuestPrerequisiteType::Custom { id: id.clone() }
            }
        }
    }

    pub fn from_runtime(t: &QuestPrerequisiteType) -> Self {
        match t {
            QuestPrerequisiteType::QuestComplete { quest_id } => {
                PrerequisiteTypeEditor::QuestComplete {
                    quest_id: quest_id.clone(),
                }
            }
            QuestPrerequisiteType::QuestActive { quest_id } => {
                PrerequisiteTypeEditor::QuestActive {
                    quest_id: quest_id.clone(),
                }
            }
            QuestPrerequisiteType::Level { level } => {
                PrerequisiteTypeEditor::Level { level: *level }
            }
            QuestPrerequisiteType::FlagSet { flag } => {
                PrerequisiteTypeEditor::FlagSet { flag: flag.clone() }
            }
            QuestPrerequisiteType::ItemOwned { item_id } => PrerequisiteTypeEditor::ItemOwned {
                item_id: item_id.clone(),
            },
            QuestPrerequisiteType::Reputation { faction, min_rep } => {
                PrerequisiteTypeEditor::Reputation {
                    faction: faction.clone(),
                    min_rep: *min_rep,
                }
            }
            QuestPrerequisiteType::Custom { id } => {
                PrerequisiteTypeEditor::Custom { id: id.clone() }
            }
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            PrerequisiteTypeEditor::QuestComplete { .. } => "Quest Complete",
            PrerequisiteTypeEditor::QuestActive { .. } => "Quest Active",
            PrerequisiteTypeEditor::Level { .. } => "Level",
            PrerequisiteTypeEditor::FlagSet { .. } => "Flag Set",
            PrerequisiteTypeEditor::ItemOwned { .. } => "Item Owned",
            PrerequisiteTypeEditor::Reputation { .. } => "Reputation",
            PrerequisiteTypeEditor::Custom { .. } => "Custom",
        }
    }
}

/// Editor state for undo/redo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorState {
    pub quests: Vec<QuestDef>,
}

/// Test mode state for quest testing.
pub struct TestModeState {
    pub active: bool,
    pub quest_system: QuestSystem,
    pub log: Vec<String>,
}

impl Default for TestModeState {
    fn default() -> Self {
        Self::new()
    }
}

impl TestModeState {
    pub fn new() -> Self {
        Self {
            active: false,
            quest_system: QuestSystem::new(),
            log: Vec::new(),
        }
    }

    pub fn start_test(&mut self, quest_def: &QuestDef) {
        self.active = true;
        self.log.clear();

        let quest = quest_def.to_runtime();
        self.quest_system.register_quest(quest);

        if self.quest_system.start_quest(&quest_def.id) {
            self.log.push(format!("Started quest: {}", quest_def.name));
        } else {
            self.log
                .push(format!("Failed to start quest: {}", quest_def.name));
        }
    }

    pub fn progress_objective(&mut self, quest_id: &str, objective_id: &str, amount: u32) {
        if !self.active {
            return;
        }

        let current = self
            .quest_system
            .get_objective_progress(quest_id, objective_id);
        self.quest_system
            .add_objective_progress(quest_id, objective_id, amount);
        let new = self
            .quest_system
            .get_objective_progress(quest_id, objective_id);

        self.log.push(format!(
            "Objective {}: {} -> {}",
            objective_id, current, new
        ));

        // Check for completion
        let events = &self.quest_system.events;
        for event in events {
            if let quasar_core::quest::QuestEvent::ObjectiveCompleted {
                quest_id: eq,
                objective_id: eo,
            } = event
            {
                if eq == quest_id && eo == objective_id {
                    self.log
                        .push(format!("Objective completed: {}", objective_id));
                }
            }
        }
    }

    pub fn complete_quest(&mut self, quest_id: &str) {
        if !self.active {
            return;
        }

        let rewards = self.quest_system.complete_quest(quest_id);
        if !rewards.is_empty() {
            self.log
                .push(format!("Quest completed with {} rewards", rewards.len()));
        } else {
            self.log.push("Quest completion failed".to_string());
        }
    }

    pub fn stop_test(&mut self) {
        self.active = false;
        self.quest_system = QuestSystem::new();
        self.log.clear();
    }
}

// ---------------------------------------------------------------------------
// Quest Editor UI
// ---------------------------------------------------------------------------

impl QuestEditor {
    pub fn new() -> Self {
        Self {
            visible: false,
            quests: Vec::new(),
            selected_quest: None,
            search_query: String::new(),
            category_filter: None,
            active_tab: EditorTab::General,
            new_quest_name: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            test_mode: TestModeState::new(),
            validation_errors: Vec::new(),
            show_validation: false,
        }
    }

    /// Save state for undo.
    fn save_state(&mut self) {
        self.undo_stack.push(EditorState {
            quests: self.quests.clone(),
        });
        self.redo_stack.clear();
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo.
    pub fn undo(&mut self) {
        if let Some(state) = self.undo_stack.pop() {
            self.redo_stack.push(EditorState {
                quests: self.quests.clone(),
            });
            self.quests = state.quests;
        }
    }

    /// Redo.
    pub fn redo(&mut self) {
        if let Some(state) = self.redo_stack.pop() {
            self.undo_stack.push(EditorState {
                quests: self.quests.clone(),
            });
            self.quests = state.quests;
        }
    }

    /// Add a new quest.
    pub fn add_quest(&mut self) {
        self.save_state();
        let id = if self.new_quest_name.is_empty() {
            format!("quest_{}", self.quests.len())
        } else {
            self.new_quest_name.clone()
        };
        self.quests.push(QuestDef::new(&id));
        self.selected_quest = Some(self.quests.len() - 1);
        self.new_quest_name.clear();
    }

    /// Delete selected quest.
    pub fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_quest {
            self.save_state();
            self.quests.remove(idx);
            self.selected_quest = None;
        }
    }

    /// Validate selected quest.
    pub fn validate_selected(&mut self) {
        if let Some(idx) = self.selected_quest {
            self.validation_errors = self.quests[idx].validate();
            self.show_validation = true;
        }
    }

    /// Export all quests to JSON.
    pub fn export_json(&self) -> Result<String, serde_json::Error> {
        let quests: HashMap<String, Quest> = self
            .quests
            .iter()
            .map(|q| (q.id.clone(), q.to_runtime()))
            .collect();
        serde_json::to_string_pretty(&quests)
    }

    /// Import quests from JSON.
    pub fn import_json(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let quests: HashMap<String, Quest> = serde_json::from_str(json)?;
        self.save_state();
        self.quests = quests.values().map(QuestDef::from_runtime).collect();
        Ok(())
    }

    /// Show the editor.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }

        let mut open = true;
        egui::Window::new("Quest Editor")
            .open(&mut open)
            .default_width(800.0)
            .default_height(600.0)
            .resizable(true)
            .show(ctx, |ui| {
                self.ui(ui);
            });

        self.visible = open;
    }

    /// Render the editor UI.
    pub fn ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            // Toolbar
            if ui.button("➕ New Quest").clicked() {
                self.add_quest();
            }
            if ui.button("➖ Delete").clicked() {
                self.delete_selected();
            }
            ui.separator();
            if ui.button("↩ Undo").clicked() {
                self.undo();
            }
            if ui.button("↪ Redo").clicked() {
                self.redo();
            }
            ui.separator();
            if ui.button("✓ Validate").clicked() {
                self.validate_selected();
            }
            if ui.button("📤 Export JSON").clicked() {
                if let Ok(json) = self.export_json() {
                    log::info!("Exported quests JSON ({} bytes)", json.len());
                }
            }
            ui.separator();

            // Search
            ui.text_edit_singleline(&mut self.search_query);

            // Category filter
            if ui.button("Filter ▼").clicked() {
                // Cycle through categories
                self.category_filter = match self.category_filter {
                    None => Some(QuestCategory::Main),
                    Some(QuestCategory::Main) => Some(QuestCategory::Side),
                    Some(QuestCategory::Side) => Some(QuestCategory::Faction),
                    Some(QuestCategory::Faction) => Some(QuestCategory::Repeatable),
                    Some(QuestCategory::Repeatable) => Some(QuestCategory::Hidden),
                    Some(QuestCategory::Hidden) => Some(QuestCategory::Event),
                    Some(QuestCategory::Event) => Some(QuestCategory::Daily),
                    Some(QuestCategory::Daily) => Some(QuestCategory::Weekly),
                    Some(QuestCategory::Weekly) => None,
                };
            }
            if let Some(cat) = self.category_filter {
                ui.label(format!("{:?}", cat));
            } else {
                ui.label("All");
            }
        });

        ui.separator();

        // Main layout: left panel (quest list) + right panel (editor)
        ui.horizontal(|ui| {
            // Left panel: Quest list
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_max_width(250.0);
                self.quest_list_panel(ui);
            });

            ui.separator();

            // Right panel: Quest editor
            if let Some(idx) = self.selected_quest {
                self.quest_editor_panel(ui, idx);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Select a quest to edit");
                });
            }
        });
    }

    fn quest_list_panel(&mut self, ui: &mut Ui) {
        for (i, quest) in self.quests.iter().enumerate() {
            // Apply filters
            if !self.search_query.is_empty() {
                let matches = quest
                    .id
                    .to_lowercase()
                    .contains(&self.search_query.to_lowercase())
                    || quest
                        .name
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase());
                if !matches {
                    continue;
                }
            }

            if let Some(filter_cat) = self.category_filter {
                if quest.category != filter_cat {
                    continue;
                }
            }

            let is_selected = self.selected_quest == Some(i);

            let category_icon = match quest.category {
                QuestCategory::Main => "📖",
                QuestCategory::Side => "📋",
                QuestCategory::Faction => "⚔️",
                QuestCategory::Repeatable => "🔄",
                QuestCategory::Hidden => "❓",
                QuestCategory::Event => "🎉",
                QuestCategory::Daily => "📅",
                QuestCategory::Weekly => "📆",
            };

            if ui
                .selectable_value(
                    &mut self.selected_quest,
                    Some(i),
                    format!("{} {}", category_icon, quest.name),
                )
                .clicked()
            {
                self.active_tab = EditorTab::General;
            }
        }

        if self.quests.is_empty() {
            ui.label("No quests defined");
        }
    }

    fn quest_editor_panel(&mut self, ui: &mut Ui, idx: usize) {
        // Tab bar
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.active_tab, EditorTab::General, "General");
            ui.selectable_value(&mut self.active_tab, EditorTab::Objectives, "Objectives");
            ui.selectable_value(&mut self.active_tab, EditorTab::Rewards, "Rewards");
            ui.selectable_value(
                &mut self.active_tab,
                EditorTab::Prerequisites,
                "Prerequisites",
            );
            ui.selectable_value(&mut self.active_tab, EditorTab::Dialogue, "Dialogue");
            ui.selectable_value(&mut self.active_tab, EditorTab::Testing, "Testing");
        });

        ui.separator();

        match self.active_tab {
            EditorTab::General => self.general_tab(ui, idx),
            EditorTab::Objectives => self.objectives_tab(ui, idx),
            EditorTab::Rewards => self.rewards_tab(ui, idx),
            EditorTab::Prerequisites => self.prerequisites_tab(ui, idx),
            EditorTab::Dialogue => self.dialogue_tab(ui, idx),
            EditorTab::Testing => self.testing_tab(ui, idx),
        }
    }

    fn general_tab(&mut self, ui: &mut Ui, idx: usize) {
        let quest = &mut self.quests[idx];

        ui.label(RichText::new("General Settings").strong());

        ui.horizontal(|ui| {
            ui.label("ID:");
            ui.label(&quest.id);
        });

        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(&mut quest.name);
        });

        ui.horizontal(|ui| {
            ui.label("Description:");
            ui.text_edit_multiline(&mut quest.description);
        });

        ui.horizontal(|ui| {
            ui.label("Category:");
            egui::ComboBox::from_id_salt("category")
                .selected_text(format!("{:?}", quest.category))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut quest.category, QuestCategory::Main, "Main");
                    ui.selectable_value(&mut quest.category, QuestCategory::Side, "Side");
                    ui.selectable_value(&mut quest.category, QuestCategory::Faction, "Faction");
                    ui.selectable_value(
                        &mut quest.category,
                        QuestCategory::Repeatable,
                        "Repeatable",
                    );
                    ui.selectable_value(&mut quest.category, QuestCategory::Hidden, "Hidden");
                    ui.selectable_value(&mut quest.category, QuestCategory::Event, "Event");
                    ui.selectable_value(&mut quest.category, QuestCategory::Daily, "Daily");
                    ui.selectable_value(&mut quest.category, QuestCategory::Weekly, "Weekly");
                });
        });

        ui.horizontal(|ui| {
            ui.label("Giver NPC:");
            ui.text_edit_singleline(&mut quest.giver);
        });

        ui.horizontal(|ui| {
            ui.label("Turn-in NPC:");
            ui.text_edit_singleline(&mut quest.turn_in);
        });

        ui.horizontal(|ui| {
            ui.label("Icon:");
            ui.text_edit_singleline(&mut quest.icon);
        });

        ui.horizontal(|ui| {
            ui.checkbox(&mut quest.hidden, "Hidden");
            ui.checkbox(&mut quest.repeatable, "Repeatable");
        });

        ui.horizontal(|ui| {
            ui.label("Time Limit (s):");
            let mut time_str = quest.time_limit.map(|t| t.to_string()).unwrap_or_default();
            if ui.text_edit_singleline(&mut time_str).lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                quest.time_limit = time_str.parse().ok();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Tags (comma-separated):");
            let mut tags_str = quest.tags.join(", ");
            ui.text_edit_singleline(&mut tags_str);
            quest.tags = tags_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        });
    }

    fn objectives_tab(&mut self, ui: &mut Ui, idx: usize) {
        ui.label(RichText::new("Objectives").strong());

        // Add objective button
        if ui.button("➕ Add Objective").clicked() {
            self.save_state();
            let quest = &mut self.quests[idx];
            let obj_id = format!("obj_{}", quest.objectives.len());
            quest.objectives.push(ObjectiveDef::new(&obj_id));
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut delete_objective = None;
            for i in 0..self.quests[idx].objectives.len() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.quests[idx].objectives[i].description);
                        if ui.button("🗑").clicked() {
                            delete_objective = Some(i);
                        }
                    });
                });
            }
            if delete_objective.is_some() {
                self.save_state();
            }
        });
    }

    fn rewards_tab(&mut self, ui: &mut Ui, idx: usize) {
        ui.label(RichText::new("Rewards").strong());

        if ui.button("➕ Add Reward").clicked() {
            self.save_state();
            self.quests[idx].rewards.push(RewardDef::gold(100));
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut delete_reward = None;
            for i in 0..self.quests[idx].rewards.len() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        match &mut self.quests[idx].rewards[i].reward_type {
                            RewardTypeEditor::Experience { amount: xp } => {
                                ui.label("XP:");
                                ui.add(egui::DragValue::new(xp));
                            }
                            RewardTypeEditor::Item { item_id, count } => {
                                ui.label("Item ID:");
                                ui.text_edit_singleline(item_id);
                                ui.label("Count:");
                                ui.add(egui::DragValue::new(count));
                            }
                            RewardTypeEditor::Gold { amount } => {
                                ui.label("Gold:");
                                ui.add(egui::DragValue::new(amount));
                            }
                            _ => {}
                        }
                        if ui.button("🗑").clicked() {
                            delete_reward = Some(i);
                        }
                    });
                });
            }
            if delete_reward.is_some() {
                self.save_state();
            }
        });
    }

    fn prerequisites_tab(&mut self, ui: &mut Ui, idx: usize) {
        ui.label(RichText::new("Prerequisites").strong());

        if ui.button("➕ Add Prerequisite").clicked() {
            self.save_state();
            self.quests[idx].prerequisites.push(PrerequisiteDef {
                prerequisite_type: PrerequisiteTypeEditor::Level { level: 1 },
            });
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut delete_prereq = None;
            for i in 0..self.quests[idx].prerequisites.len() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        match &mut self.quests[idx].prerequisites[i].prerequisite_type {
                            PrerequisiteTypeEditor::QuestComplete { quest_id } => {
                                ui.label("Quest ID:");
                                ui.text_edit_singleline(quest_id);
                            }
                            PrerequisiteTypeEditor::Level { level } => {
                                ui.label("Level:");
                                ui.add(egui::DragValue::new(level));
                            }
                            _ => {}
                        }
                        if ui.button("🗑").clicked() {
                            delete_prereq = Some(i);
                        }
                    });
                });
            }
            if delete_prereq.is_some() {
                self.save_state();
            }
        });
    }

    fn dialogue_tab(&mut self, ui: &mut Ui, idx: usize) {
        let quest = &mut self.quests[idx];

        ui.label(RichText::new("Dialogue Settings").strong());

        ui.horizontal(|ui| {
            ui.label("Dialogue Tree ID:");
            ui.text_edit_singleline(&mut quest.dialogue_tree_id);
        });

        ui.horizontal(|ui| {
            ui.label("Availability Script:");
            ui.text_edit_singleline(&mut quest.availability_script);
        });

        ui.horizontal(|ui| {
            ui.label("On Start Script:");
            ui.text_edit_singleline(&mut quest.on_start_script);
        });

        ui.horizontal(|ui| {
            ui.label("On Complete Script:");
            ui.text_edit_singleline(&mut quest.on_complete_script);
        });

        ui.separator();
        ui.label("Note: Use the Dialogue Tree Editor to create and edit dialogue trees.");
    }

    fn testing_tab(&mut self, ui: &mut Ui, idx: usize) {
        let quest = &self.quests[idx];

        ui.label(RichText::new("Quest Testing").strong());

        if !self.test_mode.active {
            if ui.button("▶ Start Test").clicked() {
                self.test_mode.start_test(quest);
            }
        } else {
            ui.horizontal(|ui| {
                if ui.button("⏹ Stop Test").clicked() {
                    self.test_mode.stop_test();
                }
                ui.label("Test Active");
            });

            ui.separator();

            // Show objectives for testing
            ui.label("Objectives:");
            for (i, obj) in quest.objectives.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}. {}", i + 1, obj.description));
                    if ui.button("+1 Progress").clicked() {
                        self.test_mode.progress_objective(&quest.id, &obj.id, 1);
                    }
                    if ui.button("+5 Progress").clicked() {
                        self.test_mode.progress_objective(&quest.id, &obj.id, 5);
                    }
                });
            }

            ui.separator();

            if ui.button("✓ Complete Quest").clicked() {
                self.test_mode.complete_quest(&quest.id);
            }

            ui.separator();

            // Test log
            ui.label("Test Log:");
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    for log_entry in &self.test_mode.log {
                        ui.label(log_entry);
                    }
                });
        }
    }
}

impl Default for QuestEditor {
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

    #[test]
    fn quest_def_creation() {
        let def = QuestDef::new("test_quest");
        assert_eq!(def.id, "test_quest");
        assert!(def.objectives.is_empty());
        assert!(def.rewards.is_empty());
    }

    #[test]
    fn quest_def_to_runtime() {
        let mut def = QuestDef::new("test_quest");
        def.name = "Test Quest".to_string();
        def.description = "A test quest".to_string();
        def.category = QuestCategory::Main;
        def.objectives.push(ObjectiveDef::new("obj1"));
        def.rewards.push(RewardDef::gold(100));

        let quest = def.to_runtime();
        assert_eq!(quest.id, "test_quest");
        assert_eq!(quest.title_key, "Test Quest");
        assert_eq!(quest.objectives.len(), 1);
        assert_eq!(quest.rewards.len(), 1);
    }

    #[test]
    fn quest_def_from_runtime() {
        let quest = Quest::new("runtime_quest")
            .with_title("Runtime Title")
            .with_description("Runtime Desc")
            .with_category(QuestCategory::Side)
            .add_objective(QuestObjective::new("obj1", "desc1", 5))
            .add_reward(QuestReward::experience(100));

        let def = QuestDef::from_runtime(&quest);
        assert_eq!(def.id, "runtime_quest");
        assert_eq!(def.name, "Runtime Title");
        assert_eq!(def.objectives.len(), 1);
        assert_eq!(def.rewards.len(), 1);
    }

    #[test]
    fn quest_def_validation() {
        let def = QuestDef::new("test");
        let errors = def.validate();
        assert!(!errors.is_empty()); // Missing name, objectives, rewards

        let mut def = QuestDef::new("test");
        def.name = "Test".to_string();
        def.objectives.push(ObjectiveDef::new("obj1"));
        def.rewards.push(RewardDef::gold(100));

        let errors = def.validate();
        assert!(errors.is_empty());
    }

    #[test]
    fn quest_def_validation_duplicate_objective_ids() {
        let mut def = QuestDef::new("test");
        def.name = "Test".to_string();
        def.objectives.push(ObjectiveDef::new("obj1"));
        def.objectives.push(ObjectiveDef::new("obj1")); // Duplicate
        def.rewards.push(RewardDef::gold(100));

        let errors = def.validate();
        assert!(errors.iter().any(|e| e.contains("Duplicate")));
    }

    #[test]
    fn objective_def_creation() {
        let obj = ObjectiveDef::new("obj1");
        assert_eq!(obj.id, "obj1");
        assert_eq!(obj.required, 1);
        assert!(!obj.optional);
    }

    #[test]
    fn objective_def_to_runtime() {
        let mut obj = ObjectiveDef::new("obj1");
        obj.description = "Collect items".to_string();
        obj.required = 10;
        obj.optional = true;
        obj.objective_type = ObjectiveTypeEditor::CollectItem {
            item_id: "herb".to_string(),
        };

        let runtime = obj.to_runtime();
        assert_eq!(runtime.id, "obj1");
        assert_eq!(runtime.required, 10);
        assert!(runtime.optional);
        assert!(matches!(
            runtime.objective_type,
            QuestObjectiveType::CollectItem { .. }
        ));
    }

    #[test]
    fn reward_def_creation() {
        let reward = RewardDef::experience(500);
        assert!(matches!(
            reward.reward_type,
            RewardTypeEditor::Experience { amount: 500 }
        ));

        let reward = RewardDef::gold(100);
        assert!(matches!(
            reward.reward_type,
            RewardTypeEditor::Gold { amount: 100 }
        ));
    }

    #[test]
    fn prerequisite_def_creation() {
        let prereq = PrerequisiteDef {
            prerequisite_type: PrerequisiteTypeEditor::Level { level: 10 },
        };
        assert!(matches!(
            prereq.prerequisite_type,
            PrerequisiteTypeEditor::Level { level: 10 }
        ));
    }

    #[test]
    fn quest_editor_creation() {
        let editor = QuestEditor::new();
        assert!(!editor.visible);
        assert!(editor.quests.is_empty());
        assert!(editor.selected_quest.is_none());
    }

    #[test]
    fn quest_editor_add_delete() {
        let mut editor = QuestEditor::new();
        editor.add_quest();
        assert_eq!(editor.quests.len(), 1);

        editor.selected_quest = Some(0);
        editor.delete_selected();
        assert_eq!(editor.quests.len(), 0);
    }

    #[test]
    fn quest_editor_undo_redo() {
        let mut editor = QuestEditor::new();
        editor.add_quest();
        assert_eq!(editor.quests.len(), 1);

        editor.undo();
        assert_eq!(editor.quests.len(), 0);

        editor.redo();
        assert_eq!(editor.quests.len(), 1);
    }

    #[test]
    fn quest_editor_export_import() {
        let mut editor = QuestEditor::new();
        let mut def = QuestDef::new("test_quest");
        def.name = "Test Quest".to_string();
        def.objectives.push(ObjectiveDef::new("obj1"));
        def.rewards.push(RewardDef::gold(100));
        editor.quests.push(def);

        let json = editor.export_json().unwrap();
        assert!(!json.is_empty());

        let mut editor2 = QuestEditor::new();
        editor2.import_json(&json).unwrap();
        assert_eq!(editor2.quests.len(), 1);
        assert_eq!(editor2.quests[0].id, "test_quest");
    }

    #[test]
    fn test_mode_start_progress_complete() {
        let mut test_mode = TestModeState::new();

        let mut def = QuestDef::new("test_quest");
        def.objectives.push(ObjectiveDef::new("obj1"));
        def.rewards.push(RewardDef::gold(100));

        test_mode.start_test(&def);
        assert!(test_mode.active);

        test_mode.progress_objective("test_quest", "obj1", 1);
        test_mode.complete_quest("test_quest");

        test_mode.stop_test();
        assert!(!test_mode.active);
    }

    #[test]
    fn test_mode_log_entries() {
        let mut test_mode = TestModeState::new();

        let mut def = QuestDef::new("test_quest");
        def.objectives.push(ObjectiveDef::new("obj1"));
        def.rewards.push(RewardDef::gold(100));

        test_mode.start_test(&def);
        assert!(!test_mode.log.is_empty());
    }
}
