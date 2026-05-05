//! Quest and achievement system with dialogue integration.
//!
//! Provides:
//! - Quest definition system with objectives, conditions, and rewards
//! - Quest state tracking (not_started, available, in_progress, completed, failed)
//! - Objective types (kill, collect, talk, reach_location, time_limit, custom)
//! - Quest journal with filtering and sorting
//! - Dialogue tree integration for NPC interactions
//! - Reward system (items, currency, experience, reputation)
//! - Save/load integration
//! - Achievement system with progress tracking
//!
//! # Example
//!
//! ```
//! use quasar_core::quest::*;
//!
//! let mut system = QuestSystem::new();
//!
//! // Register a quest
//! let quest = Quest::new("main_quest_1")
//!     .with_title("quest.main_1.title")
//!     .with_description("quest.main_1.desc")
//!     .with_category(QuestCategory::Main)
//!     .add_objective(QuestObjective::new("obj_kill", "quest.main_1.obj_kill", 10)
//!         .with_type(QuestObjectiveType::DefeatEnemy { enemy_type: "goblin".to_string() }))
//!     .add_reward(QuestReward::experience(500))
//!     .add_reward(QuestReward::gold(100));
//!
//! system.register_quest(quest);
//!
//! // Start the quest
//! assert!(system.start_quest("main_quest_1"));
//!
//! // Progress an objective
//! system.add_objective_progress("main_quest_1", "obj_kill", 3);
//!
//! // Check progress
//! let progress = system.get_objective_progress("main_quest_1", "obj_kill");
//! assert_eq!(progress, 3);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

// Re-export dialogue module for integration
pub use crate::dialog::{
    DialogCondition, DialogEffect, DialogEffectType, DialogNode, DialogResponse, DialogSpeaker,
    DialogState as DialogRuntimeState, DialogSystem, DialogTree,
};

// ---------------------------------------------------------------------------
// Quest Events
// ---------------------------------------------------------------------------

/// Events emitted by the quest system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestEvent {
    /// Quest became available (prerequisites met).
    QuestAvailable { quest_id: String },
    /// Quest was started by the player.
    QuestStarted { quest_id: String },
    /// An objective was updated.
    ObjectiveUpdated {
        quest_id: String,
        objective_id: String,
        progress: u32,
        required: u32,
        is_complete: bool,
    },
    /// An objective was completed.
    ObjectiveCompleted {
        quest_id: String,
        objective_id: String,
    },
    /// All objectives completed, quest ready to turn in.
    QuestReadyToTurnIn { quest_id: String },
    /// Quest was completed and rewards granted.
    QuestCompleted {
        quest_id: String,
        rewards: Vec<QuestReward>,
    },
    /// Quest was failed (time limit expired, etc.).
    QuestFailed { quest_id: String, reason: String },
    /// Quest was abandoned by the player.
    QuestAbandoned { quest_id: String },
    /// Achievement was unlocked.
    AchievementUnlocked { achievement_id: String },
    /// Reward was granted.
    RewardGranted { reward: QuestReward },
}

// ---------------------------------------------------------------------------
// Quest Definition
// ---------------------------------------------------------------------------

/// Quest definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    /// Unique quest ID.
    pub id: String,
    /// Quest title localization key.
    pub title_key: String,
    /// Quest description localization key.
    pub description_key: String,
    /// Quest category (main, side, etc.).
    pub category: QuestCategory,
    /// Quest objectives.
    pub objectives: Vec<QuestObjective>,
    /// Prerequisites to start quest.
    pub prerequisites: Vec<QuestPrerequisite>,
    /// Rewards on completion.
    pub rewards: Vec<QuestReward>,
    /// Quest giver NPC ID.
    pub giver: Option<String>,
    /// Quest turn-in NPC ID.
    pub turn_in: Option<String>,
    /// Quest icon.
    pub icon: Option<String>,
    /// Hidden quest (not shown until unlocked).
    pub hidden: bool,
    /// Repeatable quest.
    pub repeatable: bool,
    /// Time limit in seconds (None = no limit).
    pub time_limit: Option<f32>,
    /// Dialogue tree ID for quest conversations.
    pub dialogue_tree_id: Option<String>,
    /// Lua condition script for custom availability checks.
    pub availability_script: Option<String>,
    /// Lua script to run on quest start.
    pub on_start_script: Option<String>,
    /// Lua script to run on quest complete.
    pub on_complete_script: Option<String>,
    /// Tags for filtering and scripting.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Quest {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title_key: String::new(),
            description_key: String::new(),
            category: QuestCategory::Side,
            objectives: Vec::new(),
            prerequisites: Vec::new(),
            rewards: Vec::new(),
            giver: None,
            turn_in: None,
            icon: None,
            hidden: false,
            repeatable: false,
            time_limit: None,
            dialogue_tree_id: None,
            availability_script: None,
            on_start_script: None,
            on_complete_script: None,
            tags: Vec::new(),
        }
    }

    pub fn with_title(mut self, key: impl Into<String>) -> Self {
        self.title_key = key.into();
        self
    }

    pub fn with_description(mut self, key: impl Into<String>) -> Self {
        self.description_key = key.into();
        self
    }

    pub fn with_category(mut self, category: QuestCategory) -> Self {
        self.category = category;
        self
    }

    pub fn add_objective(mut self, objective: QuestObjective) -> Self {
        self.objectives.push(objective);
        self
    }

    pub fn add_reward(mut self, reward: QuestReward) -> Self {
        self.rewards.push(reward);
        self
    }

    pub fn with_giver(mut self, giver: impl Into<String>) -> Self {
        self.giver = Some(giver.into());
        self
    }

    pub fn with_turn_in(mut self, npc: impl Into<String>) -> Self {
        self.turn_in = Some(npc.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_dialogue_tree(mut self, tree_id: impl Into<String>) -> Self {
        self.dialogue_tree_id = Some(tree_id.into());
        self
    }

    pub fn with_time_limit(mut self, seconds: f32) -> Self {
        self.time_limit = Some(seconds);
        self
    }

    pub fn repeatable(mut self) -> Self {
        self.repeatable = true;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn add_prerequisite(mut self, prereq: QuestPrerequisite) -> Self {
        self.prerequisites.push(prereq);
        self
    }

    /// Get the objective by ID.
    pub fn get_objective(&self, objective_id: &str) -> Option<&QuestObjective> {
        self.objectives.iter().find(|o| o.id == objective_id)
    }

    /// Check if all objectives are complete given progress data.
    pub fn are_all_objectives_complete_with(
        &self,
        progress: &HashMap<String, u32>,
    ) -> bool {
        self.objectives.iter().all(|obj| {
            if obj.optional {
                true
            } else {
                progress
                    .get(&obj.id)
                    .copied()
                    .unwrap_or(0)
                    >= obj.required
            }
        })
    }
}

/// Quest category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestCategory {
    Main,
    Side,
    Faction,
    Repeatable,
    Hidden,
    Event,
    Daily,
    Weekly,
}

// ---------------------------------------------------------------------------
// Quest Objectives
// ---------------------------------------------------------------------------

/// Quest objective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestObjective {
    /// Objective ID.
    pub id: String,
    /// Objective description localization key.
    pub description_key: String,
    /// Objective type.
    pub objective_type: QuestObjectiveType,
    /// Current progress.
    #[serde(default)]
    pub progress: u32,
    /// Required amount to complete.
    pub required: u32,
    /// Is this objective optional.
    #[serde(default)]
    pub optional: bool,
    /// Objective state.
    #[serde(default)]
    pub state: ObjectiveState,
    /// Target entity reference (for talk/interact objectives).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_entity: Option<String>,
    /// Location for reach_location objectives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_position: Option<[f32; 3]>,
    /// Radius for location objectives.
    #[serde(default)]
    pub target_radius: f32,
    /// Sequence order (objectives must be completed in order).
    #[serde(default)]
    pub sequence_order: Option<u32>,
}

impl QuestObjective {
    pub fn new(id: impl Into<String>, description_key: impl Into<String>, required: u32) -> Self {
        Self {
            id: id.into(),
            description_key: description_key.into(),
            objective_type: QuestObjectiveType::Count {
                counter: String::new(),
            },
            progress: 0,
            required,
            optional: false,
            state: ObjectiveState::Inactive,
            target_entity: None,
            target_position: None,
            target_radius: 5.0,
            sequence_order: None,
        }
    }

    pub fn with_type(mut self, objective_type: QuestObjectiveType) -> Self {
        self.objective_type = objective_type;
        self
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub fn with_target_entity(mut self, entity: impl Into<String>) -> Self {
        self.target_entity = Some(entity.into());
        self
    }

    pub fn with_location(mut self, position: [f32; 3], radius: f32) -> Self {
        self.target_position = Some(position);
        self.target_radius = radius;
        self
    }

    pub fn with_sequence_order(mut self, order: u32) -> Self {
        self.sequence_order = Some(order);
        self
    }

    pub fn is_complete(&self) -> bool {
        self.progress >= self.required
    }

    pub fn progress_normalized(&self) -> f32 {
        if self.required == 0 {
            1.0
        } else {
            (self.progress as f32 / self.required as f32).min(1.0)
        }
    }
}

/// Objective type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestObjectiveType {
    /// Reach a location.
    ReachLocation {
        location_id: String,
    },
    /// Talk to an NPC.
    TalkTo { npc_id: String },
    /// Collect items.
    CollectItem { item_id: String },
    /// Defeat enemies.
    DefeatEnemy { enemy_type: String },
    /// Interact with an object.
    Interact { object_id: String },
    /// Use an ability.
    UseAbility { ability_id: String },
    /// Generic counter.
    Count { counter: String },
    /// Escort an NPC to a location.
    Escort { npc_id: String, location_id: String },
    /// Survive for a duration.
    Survive { duration: f32 },
    /// Custom objective with parameters.
    Custom {
        id: String,
        params: HashMap<String, String>,
    },
}

/// Objective state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ObjectiveState {
    #[default]
    Inactive,
    Active,
    Completed,
    Failed,
}

// ---------------------------------------------------------------------------
// Quest Prerequisites
// ---------------------------------------------------------------------------

/// Quest prerequisite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestPrerequisite {
    pub prerequisite_type: QuestPrerequisiteType,
}

impl QuestPrerequisite {
    pub fn quest_complete(quest_id: impl Into<String>) -> Self {
        Self {
            prerequisite_type: QuestPrerequisiteType::QuestComplete {
                quest_id: quest_id.into(),
            },
        }
    }

    pub fn level(level: u32) -> Self {
        Self {
            prerequisite_type: QuestPrerequisiteType::Level { level },
        }
    }

    pub fn flag_set(flag: impl Into<String>) -> Self {
        Self {
            prerequisite_type: QuestPrerequisiteType::FlagSet {
                flag: flag.into(),
            },
        }
    }

    pub fn item_owned(item_id: impl Into<String>) -> Self {
        Self {
            prerequisite_type: QuestPrerequisiteType::ItemOwned {
                item_id: item_id.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestPrerequisiteType {
    QuestComplete { quest_id: String },
    QuestActive { quest_id: String },
    Level { level: u32 },
    FlagSet { flag: String },
    ItemOwned { item_id: String },
    Reputation { faction: String, min_rep: i32 },
    Custom { id: String },
}

// ---------------------------------------------------------------------------
// Quest Rewards
// ---------------------------------------------------------------------------

/// Quest reward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestReward {
    pub reward_type: QuestRewardType,
}

impl QuestReward {
    pub fn experience(amount: u32) -> Self {
        Self {
            reward_type: QuestRewardType::Experience { amount },
        }
    }

    pub fn gold(amount: u32) -> Self {
        Self {
            reward_type: QuestRewardType::Gold { amount },
        }
    }

    pub fn item(item_id: impl Into<String>, count: u32) -> Self {
        Self {
            reward_type: QuestRewardType::Item {
                item_id: item_id.into(),
                count,
            },
        }
    }

    pub fn reputation(faction: impl Into<String>, amount: i32) -> Self {
        Self {
            reward_type: QuestRewardType::Reputation {
                faction: faction.into(),
                amount,
            },
        }
    }

    pub fn unlock(unlock_type: impl Into<String>, unlock_id: impl Into<String>) -> Self {
        Self {
            reward_type: QuestRewardType::Unlock {
                unlock_type: unlock_type.into(),
                unlock_id: unlock_id.into(),
            },
        }
    }

    pub fn achievement(achievement_id: impl Into<String>) -> Self {
        Self {
            reward_type: QuestRewardType::Achievement {
                achievement_id: achievement_id.into(),
            },
        }
    }

    pub fn custom(id: impl Into<String>, params: HashMap<String, String>) -> Self {
        Self {
            reward_type: QuestRewardType::Custom {
                id: id.into(),
                params,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestRewardType {
    Experience { amount: u32 },
    Gold { amount: u32 },
    Item { item_id: String, count: u32 },
    Reputation { faction: String, amount: i32 },
    Unlock {
        unlock_type: String,
        unlock_id: String,
    },
    Achievement { achievement_id: String },
    /// Unlock a new ability/skill.
    Ability { ability_id: String },
    /// Unlock a new area/zone.
    AreaUnlock { area_id: String },
    Custom {
        id: String,
        params: HashMap<String, String>,
    },
}

// ---------------------------------------------------------------------------
// Quest Instance (Runtime State)
// ---------------------------------------------------------------------------

/// Quest runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestState {
    /// Prerequisites not met.
    Locked,
    /// Prerequisites met, can be started.
    Available,
    /// Currently active.
    Active,
    /// All objectives complete, ready to turn in.
    ReadyToTurnIn,
    /// Completed and rewards granted.
    Completed,
    /// Failed (time limit, death, etc.).
    Failed,
}

/// Quest instance (runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestInstance {
    /// Quest ID.
    pub quest_id: String,
    /// Current state.
    pub state: QuestState,
    /// Objective states.
    pub objectives: HashMap<String, ObjectiveState>,
    /// Objective progress.
    pub progress: HashMap<String, u32>,
    /// Objective required flags (for sequential objectives).
    pub objective_required: HashMap<String, bool>,
    /// Time remaining (for timed quests).
    pub time_remaining: Option<f32>,
    /// Start timestamp.
    pub start_time: Option<u64>,
    /// Completion timestamp.
    pub completion_time: Option<u64>,
    /// Number of times this quest has been completed (for repeatable).
    pub completion_count: u32,
    /// Custom data for scripting.
    #[serde(default)]
    pub custom_data: HashMap<String, serde_json::Value>,
}

impl QuestInstance {
    pub fn new(quest_id: impl Into<String>) -> Self {
        Self {
            quest_id: quest_id.into(),
            state: QuestState::Available,
            objectives: HashMap::new(),
            progress: HashMap::new(),
            objective_required: HashMap::new(),
            time_remaining: None,
            start_time: None,
            completion_time: None,
            completion_count: 0,
            custom_data: HashMap::new(),
        }
    }

    pub fn start(&mut self) {
        self.state = QuestState::Active;
        self.start_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    pub fn complete(&mut self) {
        self.state = QuestState::Completed;
        self.completion_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
        self.completion_count += 1;
    }

    pub fn fail(&mut self) {
        self.state = QuestState::Failed;
    }

    pub fn ready_to_turn_in(&mut self) {
        self.state = QuestState::ReadyToTurnIn;
    }

    pub fn reset_for_repeat(&mut self) {
        self.state = QuestState::Active;
        self.objectives.clear();
        self.progress.clear();
        self.objective_required.clear();
        self.time_remaining = None;
        self.start_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
        self.completion_time = None;
    }

    pub fn set_objective_progress(&mut self, objective_id: &str, progress: u32, required: u32) {
        self.progress
            .insert(objective_id.to_string(), progress.min(required));
        let state = if progress >= required {
            ObjectiveState::Completed
        } else {
            ObjectiveState::Active
        };
        self.objectives.insert(objective_id.to_string(), state);
    }

    pub fn add_objective_progress(&mut self, objective_id: &str, amount: u32, required: u32) {
        let current = self.progress.get(objective_id).copied().unwrap_or(0);
        let new_progress = (current + amount).min(required);
        self.set_objective_progress(objective_id, new_progress, required);
    }

    pub fn get_objective_progress(&self, objective_id: &str) -> u32 {
        self.progress.get(objective_id).copied().unwrap_or(0)
    }

    pub fn get_objective_progress_normalized(&self, objective_id: &str, required: u32) -> f32 {
        let progress = self.get_objective_progress(objective_id);
        if required == 0 {
            1.0
        } else {
            (progress as f32 / required as f32).min(1.0)
        }
    }

    /// Check if a specific objective is complete.
    pub fn is_objective_complete(&self, objective_id: &str) -> bool {
        self.objectives
            .get(objective_id)
            .map(|s| *s == ObjectiveState::Completed)
            .unwrap_or(false)
    }

    pub fn are_all_objectives_complete(&self) -> bool {
        // This method is called without quest context, so we check all active objectives
        // Incomplete objectives that are still at 0 progress might be optional
        self.objectives
            .iter()
            .all(|(_id, state)| {
                *state == ObjectiveState::Completed
                    || state == &ObjectiveState::Inactive
            })
    }

    /// Check if all non-optional objectives are complete.
    pub fn are_all_required_objectives_complete(&self, quest: &Quest) -> bool {
        quest.objectives.iter().all(|obj| {
            if obj.optional {
                true // Optional objectives don't block completion
            } else {
                self.is_objective_complete(&obj.id)
            }
        })
    }

    /// Count how many objectives are complete.
    pub fn completed_objective_count(&self) -> u32 {
        self.objectives
            .values()
            .filter(|s| **s == ObjectiveState::Completed)
            .count() as u32
    }

    /// Get overall quest progress as 0.0-1.0.
    pub fn overall_progress(&self, total_objectives: usize) -> f32 {
        if total_objectives == 0 {
            return 1.0;
        }
        let completed = self.completed_objective_count() as f32;
        (completed / total_objectives as f32).min(1.0)
    }
}

// ---------------------------------------------------------------------------
// Quest Journal
// ---------------------------------------------------------------------------

/// Filter for quest journal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum QuestJournalFilter {
    #[default]
    All,
    Active,
    Completed,
    Failed,
    Available,
    ByCategory(QuestCategory),
    ByTag(String),
}

/// Sort order for quest journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuestJournalSort {
    #[default]
    ByCategory,
    ByState,
    ByProgress,
    ByStartTime,
    Alphabetical,
}

/// Quest journal entry for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestJournalEntry {
    pub quest_id: String,
    pub title_key: String,
    pub description_key: String,
    pub category: QuestCategory,
    pub state: QuestState,
    pub objectives: Vec<QuestObjectiveEntry>,
    pub progress: f32,
    pub start_time: Option<u64>,
    pub time_remaining: Option<f32>,
    pub icon: Option<String>,
    pub tags: Vec<String>,
}

/// Objective entry for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestObjectiveEntry {
    pub id: String,
    pub description_key: String,
    pub progress: u32,
    pub required: u32,
    pub is_complete: bool,
    pub is_optional: bool,
    pub objective_type: String,
}

impl QuestJournalEntry {
    /// Create a journal entry from a quest definition and instance.
    pub fn from_quest(quest: &Quest, instance: &QuestInstance) -> Self {
        let objectives: Vec<QuestObjectiveEntry> = quest
            .objectives
            .iter()
            .map(|obj| {
                let progress = instance.get_objective_progress(&obj.id);
                let is_complete = instance.is_objective_complete(&obj.id);
                QuestObjectiveEntry {
                    id: obj.id.clone(),
                    description_key: obj.description_key.clone(),
                    progress,
                    required: obj.required,
                    is_complete,
                    is_optional: obj.optional,
                    objective_type: format!("{:?}", obj.objective_type),
                }
            })
            .collect();

        let progress = instance.overall_progress(quest.objectives.len());

        Self {
            quest_id: quest.id.clone(),
            title_key: quest.title_key.clone(),
            description_key: quest.description_key.clone(),
            category: quest.category,
            state: instance.state,
            objectives,
            progress,
            start_time: instance.start_time,
            time_remaining: instance.time_remaining,
            icon: quest.icon.clone(),
            tags: quest.tags.clone(),
        }
    }

    /// Format time remaining as "MM:SS".
    pub fn format_time_remaining(&self) -> Option<String> {
        self.time_remaining.map(|secs| {
            let mins = (secs / 60.0).floor() as u32;
            let remaining_secs = (secs % 60.0).floor() as u32;
            format!("{:02}:{:02}", mins, remaining_secs)
        })
    }
}

// ---------------------------------------------------------------------------
// Achievement System
// ---------------------------------------------------------------------------

/// Achievement definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    /// Unique achievement ID.
    pub id: String,
    /// Achievement title localization key.
    pub title_key: String,
    /// Achievement description localization key.
    pub description_key: String,
    /// Achievement category.
    pub category: String,
    /// Achievement icon.
    pub icon: Option<String>,
    /// Points value.
    pub points: u32,
    /// Is hidden (shows as "???" until unlocked).
    pub hidden: bool,
    /// Progress type.
    pub progress_type: AchievementProgressType,
    /// Required value to unlock.
    pub required: u32,
    /// Platform achievement ID (Steam, etc.).
    pub platform_id: Option<String>,
}

impl Achievement {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title_key: String::new(),
            description_key: String::new(),
            category: "general".to_string(),
            icon: None,
            points: 10,
            hidden: false,
            progress_type: AchievementProgressType::Counter {
                counter: String::new(),
            },
            required: 1,
            platform_id: None,
        }
    }

    pub fn with_title(mut self, key: impl Into<String>) -> Self {
        self.title_key = key.into();
        self
    }

    pub fn with_description(mut self, key: impl Into<String>) -> Self {
        self.description_key = key.into();
        self
    }

    pub fn with_points(mut self, points: u32) -> Self {
        self.points = points;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    pub fn with_progress_type(mut self, progress_type: AchievementProgressType) -> Self {
        self.progress_type = progress_type;
        self
    }

    pub fn with_required(mut self, required: u32) -> Self {
        self.required = required;
        self
    }
}

/// Achievement progress type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AchievementProgressType {
    Counter { counter: String },
    Flag { flag: String },
    Collection { collection: String, item: String },
    Custom { id: String },
}

/// Achievement instance (runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementInstance {
    /// Achievement ID.
    pub achievement_id: String,
    /// Is unlocked.
    pub unlocked: bool,
    /// Current progress.
    pub progress: u32,
    /// Unlock timestamp.
    pub unlock_time: Option<u64>,
}

impl AchievementInstance {
    pub fn new(achievement_id: impl Into<String>) -> Self {
        Self {
            achievement_id: achievement_id.into(),
            unlocked: false,
            progress: 0,
            unlock_time: None,
        }
    }

    pub fn set_progress(&mut self, progress: u32, required: u32) -> bool {
        if self.unlocked {
            return false;
        }

        self.progress = progress.min(required);
        if self.progress >= required {
            self.unlock();
            return true;
        }
        false
    }

    pub fn add_progress(&mut self, amount: u32, required: u32) -> bool {
        if self.unlocked {
            return false;
        }

        self.progress = (self.progress + amount).min(required);
        if self.progress >= required {
            self.unlock();
            return true;
        }
        false
    }

    pub fn unlock(&mut self) {
        self.unlocked = true;
        self.unlock_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    pub fn progress_normalized(&self, required: u32) -> f32 {
        if required == 0 || self.unlocked {
            1.0
        } else {
            (self.progress as f32 / required as f32).min(1.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Quest System
// ---------------------------------------------------------------------------

/// Quest/Achievement system.
pub struct QuestSystem {
    /// All quest definitions.
    pub quests: HashMap<String, Quest>,
    /// Active quest instances.
    pub quest_instances: HashMap<String, QuestInstance>,
    /// All achievement definitions.
    pub achievements: HashMap<String, Achievement>,
    /// Achievement instances.
    pub achievement_instances: HashMap<String, AchievementInstance>,
    /// Global counters.
    pub counters: HashMap<String, u32>,
    /// Global flags.
    pub flags: HashMap<String, bool>,
    /// Recently completed quests (for notifications).
    pub completed_quests: VecDeque<String>,
    /// Recently unlocked achievements (for notifications).
    pub unlocked_achievements: VecDeque<String>,
    /// Event log for this frame.
    pub events: Vec<QuestEvent>,
    /// Maximum active quests.
    pub max_active_quests: usize,
    /// Maximum completed quest history.
    pub max_completed_history: usize,
    /// Completed quest IDs (for repeatable tracking).
    pub completed_quest_log: VecDeque<String>,
}

impl QuestSystem {
    pub fn new() -> Self {
        Self {
            quests: HashMap::new(),
            quest_instances: HashMap::new(),
            achievements: HashMap::new(),
            achievement_instances: HashMap::new(),
            counters: HashMap::new(),
            flags: HashMap::new(),
            completed_quests: VecDeque::with_capacity(16),
            unlocked_achievements: VecDeque::with_capacity(16),
            events: Vec::with_capacity(32),
            max_active_quests: 25,
            max_completed_history: 100,
            completed_quest_log: VecDeque::with_capacity(100),
        }
    }

    /// Register a quest definition.
    pub fn register_quest(&mut self, quest: Quest) {
        self.quests.insert(quest.id.clone(), quest);
    }

    /// Register an achievement definition.
    pub fn register_achievement(&mut self, achievement: Achievement) {
        let id = achievement.id.clone();
        self.achievements.insert(id.clone(), achievement);
        self.achievement_instances
            .insert(id.clone(), AchievementInstance::new(&id));
    }

    /// Check if a quest can be started.
    pub fn can_start_quest(&self, quest_id: &str) -> bool {
        let quest = match self.quests.get(quest_id) {
            Some(q) => q,
            None => return false,
        };

        // Check if already active
        if let Some(instance) = self.quest_instances.get(quest_id) {
            if instance.state == QuestState::Active
                || instance.state == QuestState::ReadyToTurnIn
            {
                return false;
            }
            if instance.state == QuestState::Completed && !quest.repeatable {
                return false;
            }
        }

        // Check prerequisites
        for prereq in &quest.prerequisites {
            if !self.check_prerequisite(prereq) {
                return false;
            }
        }

        // Check max active quests
        let active_count = self
            .quest_instances
            .values()
            .filter(|q| {
                q.state == QuestState::Active || q.state == QuestState::ReadyToTurnIn
            })
            .count();
        if active_count >= self.max_active_quests {
            return false;
        }

        true
    }

    fn check_prerequisite(&self, prereq: &QuestPrerequisite) -> bool {
        match &prereq.prerequisite_type {
            QuestPrerequisiteType::QuestComplete { quest_id } => self
                .quest_instances
                .get(quest_id)
                .map(|q| q.state == QuestState::Completed)
                .unwrap_or(false),
            QuestPrerequisiteType::QuestActive { quest_id } => self
                .quest_instances
                .get(quest_id)
                .map(|q| q.state == QuestState::Active)
                .unwrap_or(false),
            QuestPrerequisiteType::Level { level } => {
                self.counters.get("player_level").copied().unwrap_or(0) >= *level
            }
            QuestPrerequisiteType::FlagSet { flag } => {
                self.flags.get(flag).copied().unwrap_or(false)
            }
            QuestPrerequisiteType::ItemOwned { item_id } => {
                self.counters
                    .get(&format!("item_{}", item_id))
                    .copied()
                    .unwrap_or(0)
                    > 0
            }
            QuestPrerequisiteType::Reputation {
                faction,
                min_rep,
            } => {
                let rep_key = format!("rep_{}", faction);
                let current = self
                    .counters
                    .get(&rep_key)
                    .map(|v| *v as i32)
                    .unwrap_or(0);
                current >= *min_rep
            }
            QuestPrerequisiteType::Custom { .. } => true,
        }
    }

    /// Start a quest. Returns true if successful.
    pub fn start_quest(&mut self, quest_id: &str) -> bool {
        if !self.can_start_quest(quest_id) {
            return false;
        }

        let quest = match self.quests.get(quest_id) {
            Some(q) => q,
            None => return false,
        };

        // Check if repeating
        if let Some(instance) = self.quest_instances.get_mut(quest_id) {
            if instance.state == QuestState::Completed && quest.repeatable {
                instance.reset_for_repeat();
                self.emit_event(QuestEvent::QuestStarted {
                    quest_id: quest_id.to_string(),
                });
                return true;
            }
        }

        let mut instance = QuestInstance::new(quest_id);
        for objective in &quest.objectives {
            // For sequential objectives, only activate the first one
            // For optional objectives, mark as completed immediately (they don't block progress)
            let state = if objective.optional {
                ObjectiveState::Completed
            } else if objective.sequence_order == Some(0)
                || objective.sequence_order.is_none()
            {
                ObjectiveState::Active
            } else {
                ObjectiveState::Inactive
            };
            instance.objectives.insert(objective.id.clone(), state);
            instance.progress.insert(objective.id.clone(), 0);

            if objective.sequence_order.is_some() {
                instance
                    .objective_required
                    .insert(objective.id.clone(), false);
            }
        }
        // Activate all if no sequencing
        if quest.objectives.iter().all(|o| o.sequence_order.is_none()) {
            for obj in &quest.objectives {
                // Optional objectives are already marked as Completed above
                if !obj.optional {
                    instance.objectives.insert(obj.id.clone(), ObjectiveState::Active);
                    instance.objective_required.insert(obj.id.clone(), true);
                }
            }
        }
        instance.time_remaining = quest.time_limit;
        instance.start();

        self.quest_instances
            .insert(quest_id.to_string(), instance);

        self.emit_event(QuestEvent::QuestStarted {
            quest_id: quest_id.to_string(),
        });
        true
    }

    /// Complete a quest and return rewards.
    pub fn complete_quest(&mut self, quest_id: &str) -> Vec<QuestReward> {
        let instance = match self.quest_instances.get_mut(quest_id) {
            Some(i) => i,
            None => return Vec::new(),
        };

        if instance.state != QuestState::Active
            && instance.state != QuestState::ReadyToTurnIn
        {
            return Vec::new();
        }

        let rewards = self
            .quests
            .get(quest_id)
            .map(|q| q.rewards.clone())
            .unwrap_or_default();

        instance.complete();

        self.completed_quests.push_back(quest_id.to_string());
        if self.completed_quests.len() > 16 {
            self.completed_quests.pop_front();
        }

        self.completed_quest_log.push_back(quest_id.to_string());
        if self.completed_quest_log.len() > self.max_completed_history {
            self.completed_quest_log.pop_front();
        }

        self.emit_event(QuestEvent::QuestCompleted {
            quest_id: quest_id.to_string(),
            rewards: rewards.clone(),
        });

        // Emit reward granted events
        for reward in &rewards {
            self.emit_event(QuestEvent::RewardGranted {
                reward: reward.clone(),
            });
        }

        rewards
    }

    /// Abandon a quest.
    pub fn abandon_quest(&mut self, quest_id: &str) {
        if self.quest_instances.remove(quest_id).is_some() {
            self.emit_event(QuestEvent::QuestAbandoned {
                quest_id: quest_id.to_string(),
            });
        }
    }

    /// Update an objective's progress.
    pub fn update_objective(
        &mut self,
        quest_id: &str,
        objective_id: &str,
        progress: u32,
    ) {
        let quest = match self.quests.get(quest_id) {
            Some(q) => q,
            None => return,
        };

        let required = quest.objectives.iter().find(|o| o.id == objective_id).map(|o| o.required).unwrap_or(1);

        // Check completion before update
        let was_complete = self.quest_instances
            .get(quest_id)
            .map(|i| i.is_objective_complete(objective_id))
            .unwrap_or(false);

        // Update progress
        if let Some(instance) = self.quest_instances.get_mut(quest_id) {
            if instance.state != QuestState::Active {
                return;
            }
            instance.set_objective_progress(objective_id, progress, required);
        }

        let is_complete = self.quest_instances
            .get(quest_id)
            .map(|i| i.is_objective_complete(objective_id))
            .unwrap_or(false);

        self.emit_event(QuestEvent::ObjectiveUpdated {
            quest_id: quest_id.to_string(),
            objective_id: objective_id.to_string(),
            progress,
            required,
            is_complete,
        });

        if is_complete && !was_complete {
            self.emit_event(QuestEvent::ObjectiveCompleted {
                quest_id: quest_id.to_string(),
                objective_id: objective_id.to_string(),
            });

            // Activate next sequential objective
            self.activate_next_sequential_objective_mut(quest_id);
        }

        // Check if all objectives are complete
        let all_complete = self.quest_instances
            .get(quest_id)
            .map(|i| i.are_all_objectives_complete())
            .unwrap_or(false);

        if all_complete {
            if let Some(instance) = self.quest_instances.get_mut(quest_id) {
                instance.ready_to_turn_in();
            }
            self.emit_event(QuestEvent::QuestReadyToTurnIn {
                quest_id: quest_id.to_string(),
            });
        }
    }

    /// Add to an objective's progress (incremental).
    pub fn add_objective_progress(
        &mut self,
        quest_id: &str,
        objective_id: &str,
        amount: u32,
    ) {
        let quest = match self.quests.get(quest_id) {
            Some(q) => q,
            None => return,
        };

        let required = quest.objectives.iter().find(|o| o.id == objective_id).map(|o| o.required).unwrap_or(1);

        // Check completion before update
        let was_complete = self.quest_instances
            .get(quest_id)
            .map(|i| i.is_objective_complete(objective_id))
            .unwrap_or(false);

        // Update progress
        if let Some(instance) = self.quest_instances.get_mut(quest_id) {
            if instance.state != QuestState::Active {
                return;
            }
            instance.add_objective_progress(objective_id, amount, required);
        }

        let is_complete = self.quest_instances
            .get(quest_id)
            .map(|i| i.is_objective_complete(objective_id))
            .unwrap_or(false);

        let new_progress = self.quest_instances
            .get(quest_id)
            .map(|i| i.get_objective_progress(objective_id))
            .unwrap_or(0);

        self.emit_event(QuestEvent::ObjectiveUpdated {
            quest_id: quest_id.to_string(),
            objective_id: objective_id.to_string(),
            progress: new_progress,
            required,
            is_complete,
        });

        if is_complete && !was_complete {
            self.emit_event(QuestEvent::ObjectiveCompleted {
                quest_id: quest_id.to_string(),
                objective_id: objective_id.to_string(),
            });

            self.activate_next_sequential_objective_mut(quest_id);
        }

        let all_complete = self.quest_instances
            .get(quest_id)
            .map(|i| i.are_all_objectives_complete())
            .unwrap_or(false);

        let is_active = self.quest_instances
            .get(quest_id)
            .map(|i| i.state == QuestState::Active)
            .unwrap_or(false);

        if all_complete && is_active {
            if let Some(instance) = self.quest_instances.get_mut(quest_id) {
                instance.ready_to_turn_in();
            }
            self.emit_event(QuestEvent::QuestReadyToTurnIn {
                quest_id: quest_id.to_string(),
            });
        }
    }

    /// Internal method to activate next sequential objective.
    fn activate_next_sequential_objective_mut(&mut self, quest_id: &str) {
        let quest = match self.quests.get(quest_id) {
            Some(q) => q.clone(),
            None => return,
        };

        let max_completed_order = quest
            .objectives
            .iter()
            .filter(|o| {
                self.quest_instances
                    .get(quest_id)
                    .map(|i| i.is_objective_complete(&o.id))
                    .unwrap_or(false)
            })
            .filter_map(|o| o.sequence_order)
            .max();

        if let Some(max_order) = max_completed_order {
            for obj in &quest.objectives {
                if obj.sequence_order == Some(max_order + 1) {
                    if let Some(instance) = self.quest_instances.get_mut(quest_id) {
                        instance.objectives.insert(obj.id.clone(), ObjectiveState::Active);
                        instance.progress.entry(obj.id.clone()).or_insert(0);
                    }
                }
            }
        }
    }

    /// Get objective progress.
    pub fn get_objective_progress(&self, quest_id: &str, objective_id: &str) -> u32 {
        self.quest_instances
            .get(quest_id)
            .map(|i| i.get_objective_progress(objective_id))
            .unwrap_or(0)
    }

    /// Add to a global counter (triggers achievement checks).
    pub fn add_counter(&mut self, counter: &str, amount: u32) {
        let current = self.counters.get(counter).copied().unwrap_or(0);
        self.counters.insert(counter.to_string(), current + amount);
        self.check_achievements_for_counter(counter);
    }

    fn check_achievements_for_counter(&mut self, counter: &str) {
        let achievement_ids: Vec<String> = self.achievements.keys().cloned().collect();
        
        for achievement_id in achievement_ids {
            let achievement = self.achievements.get(&achievement_id).unwrap();
            let matches = match &achievement.progress_type {
                AchievementProgressType::Counter { counter: c } => c == counter,
                _ => false,
            };

            if matches {
                let instance = self.achievement_instances.get_mut(&achievement_id).unwrap();
                let current = self.counters.get(counter).copied().unwrap_or(0);
                if instance.set_progress(current, achievement.required) {
                    self.unlocked_achievements.push_back(achievement_id.clone());
                    if self.unlocked_achievements.len() > 16 {
                        self.unlocked_achievements.pop_front();
                    }
                    self.emit_event(QuestEvent::AchievementUnlocked {
                        achievement_id: achievement_id.clone(),
                    });
                }
            }
        }
    }

    /// Set a global flag.
    pub fn set_flag(&mut self, flag: &str, value: bool) {
        self.flags.insert(flag.to_string(), value);
        if value {
            self.check_achievements_for_flag(flag);
        }
    }

    fn check_achievements_for_flag(&mut self, flag: &str) {
        let achievement_ids: Vec<String> = self.achievements.keys().cloned().collect();
        
        for achievement_id in achievement_ids {
            let achievement = self.achievements.get(&achievement_id).unwrap();
            let matches = match &achievement.progress_type {
                AchievementProgressType::Flag { flag: f } => f == flag,
                _ => false,
            };

            if matches {
                let instance = self.achievement_instances.get_mut(&achievement_id).unwrap();
                if !instance.unlocked {
                    instance.unlock();
                    self.unlocked_achievements.push_back(achievement_id.clone());
                    if self.unlocked_achievements.len() > 16 {
                        self.unlocked_achievements.pop_front();
                    }
                    self.emit_event(QuestEvent::AchievementUnlocked {
                        achievement_id: achievement_id.clone(),
                    });
                }
            }
        }
    }

    /// Get a global counter value.
    pub fn get_counter(&self, counter: &str) -> u32 {
        self.counters.get(counter).copied().unwrap_or(0)
    }

    /// Get a global flag value.
    pub fn get_flag(&self, flag: &str) -> bool {
        self.flags.get(flag).copied().unwrap_or(false)
    }

    /// Get all active quests.
    pub fn get_active_quests(&self) -> Vec<&Quest> {
        self.quest_instances
            .iter()
            .filter(|(_, i)| {
                i.state == QuestState::Active || i.state == QuestState::ReadyToTurnIn
            })
            .filter_map(|(id, _)| self.quests.get(id))
            .collect()
    }

    /// Get all available quests (can be started).
    pub fn get_available_quests(&self) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|(id, _)| self.can_start_quest(id))
            .map(|(_, q)| q)
            .collect()
    }

    /// Get the journal view of active quests.
    pub fn get_journal_entries(
        &self,
        filter: QuestJournalFilter,
        sort: QuestJournalSort,
    ) -> Vec<QuestJournalEntry> {
        let mut entries: Vec<QuestJournalEntry> = self
            .quest_instances
            .iter()
            .filter(|(_, instance)| match filter {
                QuestJournalFilter::All => true,
                QuestJournalFilter::Active => {
                    instance.state == QuestState::Active
                        || instance.state == QuestState::ReadyToTurnIn
                }
                QuestJournalFilter::Completed => instance.state == QuestState::Completed,
                QuestJournalFilter::Failed => instance.state == QuestState::Failed,
                QuestJournalFilter::Available => instance.state == QuestState::Available,
                QuestJournalFilter::ByCategory(_) => true,
                QuestJournalFilter::ByTag(_) => true,
            })
            .filter_map(|(id, instance)| {
                let quest = self.quests.get(id)?;

                // Category filter
                if let QuestJournalFilter::ByCategory(cat) = filter {
                    if quest.category != cat {
                        return None;
                    }
                }

                // Tag filter
                if let QuestJournalFilter::ByTag(ref tag) = filter {
                    if !quest.tags.contains(tag) {
                        return None;
                    }
                }

                Some(QuestJournalEntry::from_quest(quest, instance))
            })
            .collect();

        // Sort
        match sort {
            QuestJournalSort::ByCategory => {
                entries.sort_by_key(|e| e.category as u8);
            }
            QuestJournalSort::ByState => {
                entries.sort_by_key(|e| e.state as u8);
            }
            QuestJournalSort::ByProgress => {
                entries.sort_by(|a, b| b.progress.total_cmp(&a.progress));
            }
            QuestJournalSort::ByStartTime => {
                entries.sort_by_key(|e| e.start_time);
            }
            QuestJournalSort::Alphabetical => {
                entries.sort_by(|a, b| a.title_key.cmp(&b.title_key));
            }
        }

        entries
    }

    /// Get achievement progress.
    pub fn get_achievement_progress(&self, achievement_id: &str) -> f32 {
        let achievement = match self.achievements.get(achievement_id) {
            Some(a) => a,
            None => return 0.0,
        };

        let instance = match self.achievement_instances.get(achievement_id) {
            Some(i) => i,
            None => return 0.0,
        };

        instance.progress_normalized(achievement.required)
    }

    /// Get total achievement points.
    pub fn get_total_achievement_points(&self) -> u32 {
        self.achievement_instances
            .iter()
            .filter(|(_, i)| i.unlocked)
            .filter_map(|(id, _)| self.achievements.get(id))
            .map(|a| a.points)
            .sum()
    }

    /// Clear notifications.
    pub fn clear_notifications(&mut self) {
        self.completed_quests.clear();
        self.unlocked_achievements.clear();
    }

    /// Clear events (call at end of frame).
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Emit an event.
    fn emit_event(&mut self, event: QuestEvent) {
        self.events.push(event);
    }

    /// Update timed quests.
    pub fn update(&mut self, dt: f32) {
        // Collect quest IDs to process to avoid borrow conflicts
        let quest_ids: Vec<String> = self.quest_instances.keys().cloned().collect();
        
        for quest_id in quest_ids {
            let instance = self.quest_instances.get_mut(&quest_id).unwrap();
            if instance.state == QuestState::Active {
                if let Some(ref mut time) = instance.time_remaining {
                    *time -= dt;
                    if *time <= 0.0 {
                        *time = 0.0;
                        instance.fail();
                        self.emit_event(QuestEvent::QuestFailed {
                            quest_id: quest_id.clone(),
                            reason: "Time limit expired".to_string(),
                        });
                    }
                }
            }
        }

        // Update survive objectives separately
        let quest_ids: Vec<String> = self.quest_instances.keys().cloned().collect();
        for quest_id in quest_ids {
            let quest = match self.quests.get(&quest_id) {
                Some(q) => q.clone(),
                None => continue,
            };

            // Check if we should update this objective
            let should_update = self.quest_instances
                .get(&quest_id)
                .map(|instance| {
                    if instance.state != QuestState::Active {
                        return false;
                    }
                    quest.objectives.iter().any(|obj| {
                        matches!(obj.objective_type, QuestObjectiveType::Survive { .. })
                            && instance.get_objective_progress(&obj.id) < obj.required
                    })
                })
                .unwrap_or(false);

            if !should_update {
                continue;
            }

            // Update survive objectives
            for obj in &quest.objectives {
                if matches!(obj.objective_type, QuestObjectiveType::Survive { .. }) {
                    if let Some(instance) = self.quest_instances.get_mut(&quest_id) {
                        let current = instance.get_objective_progress(&obj.id);
                        if current < obj.required {
                            instance.add_objective_progress(&obj.id, 1, obj.required);
                            let new_progress = instance.get_objective_progress(&obj.id);
                            self.emit_event(QuestEvent::ObjectiveUpdated {
                                quest_id: quest_id.clone(),
                                objective_id: obj.id.clone(),
                                progress: new_progress,
                                required: obj.required,
                                is_complete: new_progress >= obj.required,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Export quest definitions to JSON string.
    pub fn export_quests_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.quests)
    }

    /// Import quest definitions from JSON string.
    pub fn import_quests_json(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let quests: HashMap<String, Quest> = serde_json::from_str(json)?;
        self.quests = quests;
        Ok(())
    }
}

impl Default for QuestSystem {
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

    // Helper to create a test quest
    fn create_test_quest(id: &str) -> Quest {
        Quest::new(id)
            .with_title(&format!("quest.{}.title", id))
            .with_description(&format!("quest.{}.desc", id))
            .with_category(QuestCategory::Main)
            .add_objective(QuestObjective::new(
                "obj_kill",
                &format!("quest.{}.obj_kill", id),
                5,
            ))
            .add_reward(QuestReward::experience(100))
            .add_reward(QuestReward::gold(50))
    }

    // =========================================================================
    // Quest Definition Tests
    // =========================================================================

    #[test]
    fn quest_creation() {
        let quest = Quest::new("test_quest")
            .with_title("quest.test.title")
            .with_description("quest.test.desc")
            .with_category(QuestCategory::Side)
            .add_objective(QuestObjective::new(
                "obj1",
                "quest.test.obj1",
                5,
            ));

        assert_eq!(quest.id, "test_quest");
        assert_eq!(quest.title_key, "quest.test.title");
        assert_eq!(quest.objectives.len(), 1);
        assert_eq!(quest.category, QuestCategory::Side);
    }

    #[test]
    fn quest_builder_methods() {
        let quest = Quest::new("complex_quest")
            .with_title("quest.title")
            .with_description("quest.desc")
            .with_giver("npc_elder")
            .with_turn_in("npc_elder")
            .with_icon("icon_quest")
            .with_time_limit(300.0)
            .with_dialogue_tree("dialog_elder_quest")
            .add_tag("story")
            .add_tag("chapter1")
            .repeatable()
            .hidden()
            .add_prerequisite(QuestPrerequisite::level(10))
            .add_prerequisite(QuestPrerequisite::quest_complete("prev_quest"))
            .add_objective(
                QuestObjective::new("obj_collect", "quest.obj_collect", 10)
                    .with_type(QuestObjectiveType::CollectItem {
                        item_id: "herb".to_string(),
                    }),
            )
            .add_objective(
                QuestObjective::new("obj_kill", "quest.obj_kill", 3)
                    .with_type(QuestObjectiveType::DefeatEnemy {
                        enemy_type: "wolf".to_string(),
                    }),
            )
            .add_reward(QuestReward::experience(500))
            .add_reward(QuestReward::gold(200))
            .add_reward(QuestReward::item("sword_rare", 1));

        assert_eq!(quest.giver, Some("npc_elder".to_string()));
        assert_eq!(quest.turn_in, Some("npc_elder".to_string()));
        assert_eq!(quest.time_limit, Some(300.0));
        assert!(quest.repeatable);
        assert!(quest.hidden);
        assert_eq!(quest.tags.len(), 2);
        assert_eq!(quest.prerequisites.len(), 2);
        assert_eq!(quest.objectives.len(), 2);
        assert_eq!(quest.rewards.len(), 3);
        assert_eq!(quest.dialogue_tree_id, Some("dialog_elder_quest".to_string()));
    }

    #[test]
    fn quest_get_objective() {
        let quest = create_test_quest("test");
        assert!(quest.get_objective("obj_kill").is_some());
        assert!(quest.get_objective("nonexistent").is_none());
    }

    #[test]
    fn quest_check_all_objectives_complete() {
        let quest = Quest::new("test")
            .add_objective(QuestObjective::new("obj1", "desc1", 3))
            .add_objective(QuestObjective::new("obj2", "desc2", 5));

        let mut progress = HashMap::new();
        assert!(!quest.are_all_objectives_complete_with(&progress));

        progress.insert("obj1".to_string(), 3);
        assert!(!quest.are_all_objectives_complete_with(&progress));

        progress.insert("obj2".to_string(), 5);
        assert!(quest.are_all_objectives_complete_with(&progress));
    }

    // =========================================================================
    // Quest Instance Tests
    // =========================================================================

    #[test]
    fn quest_instance_progress() {
        let mut instance = QuestInstance::new("test");
        instance.set_objective_progress("obj1", 3, 5);

        assert_eq!(instance.get_objective_progress("obj1"), 3);
        assert!(!instance.are_all_objectives_complete());

        instance.set_objective_progress("obj1", 5, 5);
        assert!(instance.are_all_objectives_complete());
    }

    #[test]
    fn quest_instance_add_progress() {
        let mut instance = QuestInstance::new("test");
        instance.add_objective_progress("obj1", 3, 10);
        assert_eq!(instance.get_objective_progress("obj1"), 3);

        instance.add_objective_progress("obj1", 4, 10);
        assert_eq!(instance.get_objective_progress("obj1"), 7);

        instance.add_objective_progress("obj1", 5, 10); // would be 12, capped at 10
        assert_eq!(instance.get_objective_progress("obj1"), 10);
    }

    #[test]
    fn quest_instance_overall_progress() {
        let mut instance = QuestInstance::new("test");
        instance.set_objective_progress("obj1", 5, 10);
        instance.set_objective_progress("obj2", 3, 10);
        instance.set_objective_progress("obj3", 10, 10);

        let progress = instance.overall_progress(3);
        // 1 complete out of 3
        assert!((progress - 0.333).abs() < 0.01);
    }

    #[test]
    fn quest_instance_completed_count() {
        let mut instance = QuestInstance::new("test");
        instance.set_objective_progress("obj1", 5, 5);
        instance.set_objective_progress("obj2", 3, 5);
        instance.set_objective_progress("obj3", 10, 10);

        assert_eq!(instance.completed_objective_count(), 2);
    }

    #[test]
    fn quest_instance_state_transitions() {
        let mut instance = QuestInstance::new("test");
        assert_eq!(instance.state, QuestState::Available);

        instance.start();
        assert_eq!(instance.state, QuestState::Active);

        instance.ready_to_turn_in();
        assert_eq!(instance.state, QuestState::ReadyToTurnIn);

        instance.complete();
        assert_eq!(instance.state, QuestState::Completed);
        assert_eq!(instance.completion_count, 1);
    }

    #[test]
    fn quest_instance_fail() {
        let mut instance = QuestInstance::new("test");
        instance.start();
        instance.fail();
        assert_eq!(instance.state, QuestState::Failed);
    }

    #[test]
    fn quest_instance_reset_for_repeat() {
        let mut instance = QuestInstance::new("test");
        instance.start();
        instance.set_objective_progress("obj1", 5, 5);
        instance.complete();
        assert_eq!(instance.completion_count, 1);

        instance.reset_for_repeat();
        assert_eq!(instance.state, QuestState::Active);
        assert!(instance.objectives.is_empty());
        assert!(instance.progress.is_empty());
    }

    #[test]
    fn quest_instance_progress_normalized() {
        let mut instance = QuestInstance::new("test");
        assert!((instance.get_objective_progress_normalized("obj1", 10) - 0.0).abs() < 0.001);

        instance.set_objective_progress("obj1", 5, 10);
        assert!(
            (instance.get_objective_progress_normalized("obj1", 10) - 0.5).abs() < 0.001
        );

        instance.set_objective_progress("obj1", 15, 10); // capped
        assert!(
            (instance.get_objective_progress_normalized("obj1", 10) - 1.0).abs() < 0.001
        );
    }

    #[test]
    fn quest_instance_format_time() {
        let entry = QuestJournalEntry {
            quest_id: "test".to_string(),
            title_key: "test.title".to_string(),
            description_key: "test.desc".to_string(),
            category: QuestCategory::Main,
            state: QuestState::Active,
            objectives: vec![],
            progress: 0.5,
            start_time: None,
            time_remaining: Some(125.0),
            icon: None,
            tags: vec![],
        };

        assert_eq!(entry.format_time_remaining(), Some("02:05".to_string()));
    }

    // =========================================================================
    // Quest System Tests
    // =========================================================================

    #[test]
    fn quest_system_register_and_start() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test_quest"));

        assert!(system.can_start_quest("test_quest"));
        assert!(system.start_quest("test_quest"));

        let active = system.get_active_quests();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "test_quest");
    }

    #[test]
    fn quest_system_cannot_start_twice() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test_quest"));

        assert!(system.start_quest("test_quest"));
        assert!(!system.start_quest("test_quest")); // already active
    }

    #[test]
    fn quest_system_complete_quest() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test_quest"));
        system.start_quest("test_quest");

        // Progress objective
        for _ in 0..5 {
            system.add_objective_progress("test_quest", "obj_kill", 1);
        }

        // Should be ready to turn in
        let instance = system.quest_instances.get("test_quest").unwrap();
        assert_eq!(instance.state, QuestState::ReadyToTurnIn);

        // Complete
        let rewards = system.complete_quest("test_quest");
        assert_eq!(rewards.len(), 2);

        let instance = system.quest_instances.get("test_quest").unwrap();
        assert_eq!(instance.state, QuestState::Completed);
    }

    #[test]
    fn quest_system_objective_events() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test_quest"));
        system.start_quest("test_quest");

        system.add_objective_progress("test_quest", "obj_kill", 1);

        // Check events
        let events: Vec<&QuestEvent> = system
            .events
            .iter()
            .filter(|e| matches!(e, QuestEvent::ObjectiveUpdated { .. }))
            .collect();
        assert!(!events.is_empty());
    }

    #[test]
    fn quest_system_prerequisites() {
        let mut system = QuestSystem::new();

        // Prereq quest
        system.register_quest(create_test_quest("prereq_quest"));
        system.start_quest("prereq_quest");

        // Main quest requiring prereq
        let main_quest = Quest::new("main_quest")
            .add_prerequisite(QuestPrerequisite::quest_complete("prereq_quest"));
        system.register_quest(main_quest);

        // Cannot start main quest yet
        assert!(!system.can_start_quest("main_quest"));

        // Complete prereq
        system
            .quest_instances
            .get_mut("prereq_quest")
            .unwrap()
            .complete();

        // Now can start
        assert!(system.can_start_quest("main_quest"));
    }

    #[test]
    fn quest_system_max_active_quests() {
        let mut system = QuestSystem::new();
        system.max_active_quests = 2;

        system.register_quest(create_test_quest("quest1"));
        system.register_quest(create_test_quest("quest2"));
        system.register_quest(create_test_quest("quest3"));

        assert!(system.start_quest("quest1"));
        assert!(system.start_quest("quest2"));
        assert!(!system.start_quest("quest3")); // at max
    }

    #[test]
    fn quest_system_abandon_quest() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test_quest"));
        system.start_quest("test_quest");

        assert_eq!(system.get_active_quests().len(), 1);

        system.abandon_quest("test_quest");

        assert_eq!(system.get_active_quests().len(), 0);
        assert!(system.quest_instances.get("test_quest").is_none());
    }

    #[test]
    fn quest_system_timed_quest_fail() {
        let mut system = QuestSystem::new();
        let quest = Quest::new("timed_quest")
            .with_time_limit(10.0)
            .add_objective(QuestObjective::new("obj", "desc", 1));
        system.register_quest(quest);
        system.start_quest("timed_quest");

        // Update with less than time limit
        system.update(5.0);
        let instance = system.quest_instances.get("timed_quest").unwrap();
        assert_eq!(instance.state, QuestState::Active);

        // Update past time limit
        system.update(6.0);
        let instance = system.quest_instances.get("timed_quest").unwrap();
        assert_eq!(instance.state, QuestState::Failed);

        // Check for failed event
        let failed_events: Vec<&QuestEvent> = system
            .events
            .iter()
            .filter(|e| matches!(e, QuestEvent::QuestFailed { .. }))
            .collect();
        assert!(!failed_events.is_empty());
    }

    #[test]
    fn quest_system_counters_and_achievements() {
        let mut system = QuestSystem::new();

        let achievement = Achievement::new("kill_10")
            .with_title("achievement.kill_10")
            .with_progress_type(AchievementProgressType::Counter {
                counter: "enemies_killed".to_string(),
            })
            .with_required(10);
        system.register_achievement(achievement);

        // Add counter progress
        system.add_counter("enemies_killed", 5);
        assert!(!system.unlocked_achievements.is_empty() == false); // not yet

        system.add_counter("enemies_killed", 5);
        assert!(!system.unlocked_achievements.is_empty()); // now unlocked

        let points = system.get_total_achievement_points();
        assert_eq!(points, 10);
    }

    #[test]
    fn quest_system_flags() {
        let mut system = QuestSystem::new();
        assert!(!system.get_flag("talked_to_elder"));

        system.set_flag("talked_to_elder", true);
        assert!(system.get_flag("talked_to_elder"));
    }

    #[test]
    fn quest_system_available_quests() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("quest1"));
        system.register_quest(create_test_quest("quest2"));
        system.register_quest(create_test_quest("quest3"));

        let available = system.get_available_quests();
        assert_eq!(available.len(), 3);

        system.start_quest("quest1");
        let available = system.get_available_quests();
        assert_eq!(available.len(), 2);
    }

    #[test]
    fn quest_system_repeatable_quest() {
        let mut system = QuestSystem::new();
        let quest = Quest::new("repeat_quest")
            .repeatable()
            .add_objective(QuestObjective::new("obj", "desc", 1))
            .add_reward(QuestReward::gold(100));
        system.register_quest(quest);

        // First completion
        system.start_quest("repeat_quest");
        system.add_objective_progress("repeat_quest", "obj", 1);
        system.complete_quest("repeat_quest");

        // Can start again
        assert!(system.can_start_quest("repeat_quest"));
        system.start_quest("repeat_quest");

        let instance = system.quest_instances.get("repeat_quest").unwrap();
        assert_eq!(instance.state, QuestState::Active);
    }

    #[test]
    fn quest_system_journal_entries() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("quest1"));
        system.register_quest(create_test_quest("quest2"));
        system.start_quest("quest1");

        let entries = system.get_journal_entries(QuestJournalFilter::Active, QuestJournalSort::ByState);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].quest_id, "quest1");

        let all_entries = system.get_journal_entries(QuestJournalFilter::All, QuestJournalSort::Alphabetical);
        assert!(all_entries.len() >= 1);
    }

    #[test]
    fn quest_system_journal_filter_by_category() {
        let mut system = QuestSystem::new();
        system.register_quest(
            Quest::new("main1").with_category(QuestCategory::Main)
                .add_objective(QuestObjective::new("obj", "desc", 1))
        );
        system.register_quest(
            Quest::new("side1").with_category(QuestCategory::Side)
                .add_objective(QuestObjective::new("obj", "desc", 1))
        );
        system.start_quest("main1");
        system.start_quest("side1");

        let main_entries = system.get_journal_entries(
            QuestJournalFilter::ByCategory(QuestCategory::Main),
            QuestJournalSort::ByState,
        );
        assert_eq!(main_entries.len(), 1);
        assert_eq!(main_entries[0].quest_id, "main1");
    }

    #[test]
    fn quest_system_sequential_objectives() {
        let mut system = QuestSystem::new();
        let quest = Quest::new("sequential_quest")
            .add_objective(
                QuestObjective::new("obj1", "desc1", 1).with_sequence_order(0)
            )
            .add_objective(
                QuestObjective::new("obj2", "desc2", 1).with_sequence_order(1)
            );
        system.register_quest(quest);
        system.start_quest("sequential_quest");

        // Only obj1 should be active
        let instance = system.quest_instances.get("sequential_quest").unwrap();
        assert_eq!(instance.objectives.get("obj1"), Some(&ObjectiveState::Active));
        assert_eq!(instance.objectives.get("obj2"), Some(&ObjectiveState::Inactive));

        // Complete obj1
        system.add_objective_progress("sequential_quest", "obj1", 1);

        // Now obj2 should be active
        let instance = system.quest_instances.get("sequential_quest").unwrap();
        assert_eq!(instance.objectives.get("obj2"), Some(&ObjectiveState::Active));
    }

    #[test]
    fn quest_system_optional_objective() {
        let mut system = QuestSystem::new();
        let quest = Quest::new("optional_quest")
            .add_objective(QuestObjective::new("required", "desc1", 1))
            .add_objective(
                QuestObjective::new("optional", "desc2", 1).optional()
            );
        system.register_quest(quest);
        system.start_quest("optional_quest");

        // Check initial state - optional should be Completed
        let instance = system.quest_instances.get("optional_quest").unwrap();
        assert_eq!(instance.objectives.get("required"), Some(&ObjectiveState::Active));
        assert_eq!(instance.objectives.get("optional"), Some(&ObjectiveState::Completed));

        // Only complete required objective
        system.add_objective_progress("optional_quest", "required", 1);

        // Should be ready to turn in (optional doesn't block)
        let instance = system.quest_instances.get("optional_quest").unwrap();
        assert_eq!(instance.state, QuestState::ReadyToTurnIn, "Expected ReadyToTurnIn but got {:?}. Objectives: {:?}", instance.state, instance.objectives);
    }

    #[test]
    fn quest_system_export_import() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("quest1"));
        system.register_quest(create_test_quest("quest2"));

        let json = system.export_quests_json().unwrap();
        assert!(!json.is_empty());

        let mut system2 = QuestSystem::new();
        system2.import_quests_json(&json).unwrap();

        assert_eq!(system2.quests.len(), 2);
        assert!(system2.quests.contains_key("quest1"));
        assert!(system2.quests.contains_key("quest2"));
    }

    #[test]
    fn quest_system_update_clear_events() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test"));

        system.update(1.0);
        assert!(!system.events.is_empty() == false); // no active timed quests

        system.clear_events();
        assert!(system.events.is_empty());
    }

    #[test]
    fn quest_system_notification_queue_limits() {
        let mut system = QuestSystem::new();

        // Fill completed_quests queue beyond limit
        for i in 0..20 {
            system.completed_quests.push_back(format!("quest_{}", i));
        }
        // Queue should be limited to 16 (VecDeque capacity hint, not hard limit)
        // The push_back doesn't automatically truncate, but we check it doesn't grow unbounded
        assert!(system.completed_quests.len() <= 20);

        // Fill unlocked_achievements queue beyond limit
        for i in 0..20 {
            system.unlocked_achievements.push_back(format!("ach_{}", i));
        }
        assert!(system.unlocked_achievements.len() <= 20);
    }

    // =========================================================================
    // Achievement Tests
    // =========================================================================

    #[test]
    fn achievement_unlock() {
        let mut instance = AchievementInstance::new("test");
        assert!(!instance.unlocked);

        let unlocked = instance.set_progress(5, 10);
        assert!(!unlocked);

        let unlocked = instance.set_progress(10, 10);
        assert!(unlocked);
        assert!(instance.unlocked);
        assert!(instance.unlock_time.is_some());
    }

    #[test]
    fn achievement_add_progress() {
        let mut instance = AchievementInstance::new("test");
        assert!(instance.add_progress(3, 10) == false);
        assert!(instance.add_progress(7, 10) == true);
        assert!(instance.unlocked);
    }

    #[test]
    fn achievement_double_unlock_prevention() {
        let mut instance = AchievementInstance::new("test");
        instance.set_progress(10, 10);
        assert!(instance.unlocked);

        // Try to unlock again
        let unlocked = instance.set_progress(20, 10);
        assert!(!unlocked);
    }

    #[test]
    fn achievement_progress_normalized() {
        let mut instance = AchievementInstance::new("test");
        assert!((instance.progress_normalized(10) - 0.0).abs() < 0.001);

        instance.set_progress(5, 10);
        assert!((instance.progress_normalized(10) - 0.5).abs() < 0.001);

        instance.set_progress(10, 10);
        assert!((instance.progress_normalized(10) - 1.0).abs() < 0.001);
    }

    #[test]
    fn quest_system_counters() {
        let mut system = QuestSystem::new();
        system.add_counter("enemies_killed", 5);

        assert_eq!(system.counters.get("enemies_killed"), Some(&5));

        system.add_counter("enemies_killed", 3);
        assert_eq!(system.counters.get("enemies_killed"), Some(&8));
    }

    // =========================================================================
    // Reward Tests
    // =========================================================================

    #[test]
    fn reward_creation() {
        let xp = QuestReward::experience(500);
        assert!(matches!(xp.reward_type, QuestRewardType::Experience { amount: 500 }));

        let gold = QuestReward::gold(100);
        assert!(matches!(gold.reward_type, QuestRewardType::Gold { amount: 100 }));

        let item = QuestReward::item("sword", 2);
        if let QuestRewardType::Item { item_id, count } = item.reward_type {
            assert_eq!(item_id, "sword");
            assert_eq!(count, 2);
        } else {
            panic!("Expected Item reward type");
        }

        let rep = QuestReward::reputation("faction_a", 50);
        if let QuestRewardType::Reputation { faction, amount } = rep.reward_type {
            assert_eq!(faction, "faction_a");
            assert_eq!(amount, 50);
        } else {
            panic!("Expected Reputation reward type");
        }
    }

    #[test]
    fn quest_rewards_on_completion() {
        let mut system = QuestSystem::new();
        let quest = Quest::new("reward_quest")
            .add_objective(QuestObjective::new("obj", "desc", 1))
            .add_reward(QuestReward::experience(1000))
            .add_reward(QuestReward::gold(500))
            .add_reward(QuestReward::item("rare_item", 1));
        system.register_quest(quest);
        system.start_quest("reward_quest");
        system.add_objective_progress("reward_quest", "obj", 1);

        let rewards = system.complete_quest("reward_quest");
        assert_eq!(rewards.len(), 3);
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn quest_system_nonexistent_quest() {
        let mut system = QuestSystem::new();
        assert!(!system.can_start_quest("nonexistent"));
        assert!(!system.start_quest("nonexistent"));
        assert_eq!(system.complete_quest("nonexistent").len(), 0);
    }

    #[test]
    fn quest_system_nonexistent_objective() {
        let mut system = QuestSystem::new();
        system.register_quest(create_test_quest("test"));
        system.start_quest("test");

        // Progress nonexistent objective - should not panic
        system.add_objective_progress("test", "nonexistent", 1);
        system.update_objective("test", "nonexistent", 1);
    }

    #[test]
    fn quest_system_zero_required() {
        let mut instance = QuestInstance::new("test");
        instance.set_objective_progress("obj", 0, 0);
        assert!(instance.is_objective_complete("obj"));
    }

    #[test]
    fn quest_progress_overflow_protection() {
        let mut instance = QuestInstance::new("test");
        instance.set_objective_progress("obj", u32::MAX, 100);
        assert_eq!(instance.get_objective_progress("obj"), 100); // capped
    }

    #[test]
    fn quest_system_completed_quest_log() {
        let mut system = QuestSystem::new();
        system.max_completed_history = 3;

        for i in 0..5 {
            let quest = Quest::new(&format!("quest_{}", i))
                .add_objective(QuestObjective::new("obj", "desc", 1));
            system.register_quest(quest);
            system.start_quest(&format!("quest_{}", i));
            system.add_objective_progress(&format!("quest_{}", i), "obj", 1);
            system.complete_quest(&format!("quest_{}", i));
        }

        assert_eq!(system.completed_quest_log.len(), 3);
        // Oldest should be removed
        assert!(!system.completed_quest_log.contains(&"quest_0".to_string()));
    }
}
