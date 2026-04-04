//! Quest and achievement system.
//!
//! Provides:
//! - Quest tracking with objectives
//! - Achievement system with unlocks
//! - Progress tracking and notifications
//! - Save/load integration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

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
    pub progress: u32,
    /// Required amount to complete.
    pub required: u32,
    /// Is this objective optional.
    pub optional: bool,
    /// Objective state.
    pub state: ObjectiveState,
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
        }
    }

    pub fn with_type(mut self, objective_type: QuestObjectiveType) -> Self {
        self.objective_type = objective_type;
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
    ReachLocation { location_id: String },
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
    /// Custom objective.
    Custom {
        id: String,
        params: HashMap<String, String>,
    },
}

/// Objective state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectiveState {
    Inactive,
    Active,
    Completed,
    Failed,
}

/// Quest prerequisite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestPrerequisite {
    pub prerequisite_type: QuestPrerequisiteType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestPrerequisiteType {
    QuestComplete { quest_id: String },
    QuestActive { quest_id: String },
    Level { level: u32 },
    FlagSet { flag: String },
    ItemOwned { item_id: String },
    Custom { id: String },
}

/// Quest reward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestReward {
    pub reward_type: QuestRewardType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuestRewardType {
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
    Custom {
        id: String,
        params: HashMap<String, String>,
    },
}

/// Quest runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestState {
    Locked,
    Available,
    Active,
    Completed,
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
    /// Time remaining (for timed quests).
    pub time_remaining: Option<f32>,
    /// Start timestamp.
    pub start_time: Option<u64>,
    /// Completion timestamp.
    pub completion_time: Option<u64>,
}

impl QuestInstance {
    pub fn new(quest_id: impl Into<String>) -> Self {
        Self {
            quest_id: quest_id.into(),
            state: QuestState::Available,
            objectives: HashMap::new(),
            progress: HashMap::new(),
            time_remaining: None,
            start_time: None,
            completion_time: None,
        }
    }

    pub fn start(&mut self) {
        self.state = QuestState::Active;
        self.start_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    pub fn complete(&mut self) {
        self.state = QuestState::Completed;
        self.completion_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );
    }

    pub fn fail(&mut self) {
        self.state = QuestState::Failed;
    }

    pub fn set_objective_progress(&mut self, objective_id: &str, progress: u32, required: u32) {
        self.progress.insert(objective_id.to_string(), progress);
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

    pub fn are_all_objectives_complete(&self) -> bool {
        self.objectives
            .values()
            .all(|s| *s == ObjectiveState::Completed)
    }
}

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

    pub fn unlock(&mut self) {
        self.unlocked = true;
        self.progress = self.progress.max(1);
        self.unlock_time = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
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
    pub completed_quests: Vec<String>,
    /// Recently unlocked achievements (for notifications).
    pub unlocked_achievements: Vec<String>,
    /// Maximum active quests.
    pub max_active_quests: usize,
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
            completed_quests: Vec::new(),
            unlocked_achievements: Vec::new(),
            max_active_quests: 25,
        }
    }

    pub fn register_quest(&mut self, quest: Quest) {
        self.quests.insert(quest.id.clone(), quest);
    }

    pub fn register_achievement(&mut self, achievement: Achievement) {
        let id = achievement.id.clone();
        self.achievements
            .insert(achievement.id.clone(), achievement);
        self.achievement_instances
            .insert(id.clone(), AchievementInstance::new(&id));
    }

    pub fn can_start_quest(&self, quest_id: &str) -> bool {
        let quest = match self.quests.get(quest_id) {
            Some(q) => q,
            None => return false,
        };

        // Check if already active or completed
        if let Some(instance) = self.quest_instances.get(quest_id) {
            if instance.state == QuestState::Active || instance.state == QuestState::Completed {
                if !quest.repeatable {
                    return false;
                }
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
            .filter(|q| q.state == QuestState::Active)
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
            QuestPrerequisiteType::Custom { .. } => true,
        }
    }

    pub fn start_quest(&mut self, quest_id: &str) -> bool {
        if !self.can_start_quest(quest_id) {
            return false;
        }

        let quest = match self.quests.get(quest_id) {
            Some(q) => q.clone(),
            None => return false,
        };

        let mut instance = QuestInstance::new(quest_id);
        for objective in &quest.objectives {
            instance
                .objectives
                .insert(objective.id.clone(), ObjectiveState::Active);
            instance.progress.insert(objective.id.clone(), 0);
        }
        instance.time_remaining = quest.time_limit;
        instance.start();

        self.quest_instances.insert(quest_id.to_string(), instance);
        true
    }

    pub fn complete_quest(&mut self, quest_id: &str) -> Vec<QuestReward> {
        let instance = match self.quest_instances.get_mut(quest_id) {
            Some(i) => i,
            None => return Vec::new(),
        };

        if instance.state != QuestState::Active {
            return Vec::new();
        }

        instance.complete();
        self.completed_quests.push(quest_id.to_string());

        // Return rewards
        self.quests
            .get(quest_id)
            .map(|q| q.rewards.clone())
            .unwrap_or_default()
    }

    pub fn update_objective(&mut self, quest_id: &str, objective_id: &str, progress: u32) {
        let quest = match self.quests.get(quest_id) {
            Some(q) => q,
            None => return,
        };

        let instance = match self.quest_instances.get_mut(quest_id) {
            Some(i) => i,
            None => return,
        };

        if instance.state != QuestState::Active {
            return;
        }

        let objective = quest.objectives.iter().find(|o| o.id == objective_id);

        let required = objective.map(|o| o.required).unwrap_or(1);
        instance.set_objective_progress(objective_id, progress, required);
    }

    pub fn add_counter(&mut self, counter: &str, amount: u32) {
        let current = self.counters.get(counter).copied().unwrap_or(0);
        self.counters.insert(counter.to_string(), current + amount);

        // Check achievements
        self.check_achievements_for_counter(counter);
    }

    fn check_achievements_for_counter(&mut self, counter: &str) {
        for (achievement_id, achievement) in &self.achievements {
            let matches = match &achievement.progress_type {
                AchievementProgressType::Counter { counter: c } => c == counter,
                _ => false,
            };

            if matches {
                let instance = self.achievement_instances.get_mut(achievement_id).unwrap();
                let current = self.counters.get(counter).copied().unwrap_or(0);
                if instance.set_progress(current, achievement.required) {
                    self.unlocked_achievements.push(achievement_id.clone());
                }
            }
        }
    }

    pub fn set_flag(&mut self, flag: &str, value: bool) {
        self.flags.insert(flag.to_string(), value);

        if value {
            self.check_achievements_for_flag(flag);
        }
    }

    fn check_achievements_for_flag(&mut self, flag: &str) {
        for (achievement_id, achievement) in &self.achievements {
            let matches = match &achievement.progress_type {
                AchievementProgressType::Flag { flag: f } => f == flag,
                _ => false,
            };

            if matches {
                let instance = self.achievement_instances.get_mut(achievement_id).unwrap();
                if !instance.unlocked {
                    instance.unlock();
                    self.unlocked_achievements.push(achievement_id.clone());
                }
            }
        }
    }

    pub fn get_active_quests(&self) -> Vec<&Quest> {
        self.quest_instances
            .iter()
            .filter(|(_, i)| i.state == QuestState::Active)
            .filter_map(|(id, _)| self.quests.get(id))
            .collect()
    }

    pub fn get_available_quests(&self) -> Vec<&Quest> {
        self.quests
            .iter()
            .filter(|(id, _)| self.can_start_quest(id))
            .map(|(_, q)| q)
            .collect()
    }

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

    pub fn get_total_achievement_points(&self) -> u32 {
        self.achievement_instances
            .iter()
            .filter(|(_, i)| i.unlocked)
            .filter_map(|(id, _)| self.achievements.get(id))
            .map(|a| a.points)
            .sum()
    }

    pub fn clear_notifications(&mut self) {
        self.completed_quests.clear();
        self.unlocked_achievements.clear();
    }

    pub fn update(&mut self, dt: f32) {
        // Update timed quests
        for (quest_id, instance) in &mut self.quest_instances {
            if instance.state == QuestState::Active {
                if let Some(ref mut time) = instance.time_remaining {
                    *time -= dt;
                    if *time <= 0.0 {
                        instance.fail();
                    }
                }
            }
        }
    }
}

impl Default for QuestSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_creation() {
        let quest = Quest::new("test_quest")
            .with_title("quest.test.title")
            .with_description("quest.test.desc")
            .add_objective(QuestObjective::new("obj1", "quest.test.obj1", 5));

        assert_eq!(quest.id, "test_quest");
        assert_eq!(quest.objectives.len(), 1);
    }

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
    fn achievement_unlock() {
        let mut instance = AchievementInstance::new("test");
        assert!(!instance.unlocked);

        let unlocked = instance.set_progress(5, 10);
        assert!(!unlocked);

        let unlocked = instance.set_progress(10, 10);
        assert!(unlocked);
        assert!(instance.unlocked);
    }

    #[test]
    fn quest_system_counters() {
        let mut system = QuestSystem::new();
        system.add_counter("enemies_killed", 5);

        assert_eq!(system.counters.get("enemies_killed"), Some(&5));

        system.add_counter("enemies_killed", 3);
        assert_eq!(system.counters.get("enemies_killed"), Some(&8));
    }
}
