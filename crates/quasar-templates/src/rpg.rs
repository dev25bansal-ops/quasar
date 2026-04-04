//! Role-Playing Game Template
//!
//! Provides components and systems for RPG games:
//! - Character stats and leveling
//! - Inventory system with items and equipment
//! - Quest system with objectives
//! - Skill and ability system
//! - NPC dialogue system

use glam::Vec3;
use quasar_core::ecs::{System, World};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{Health, InputState, TemplateTransform, TemplateVelocity};

pub mod prelude {
    pub use super::{
        Ability, CharacterStats, Equipment, EquipmentSlot, Inventory, InventoryItem, ItemType,
        NpcDialogue, Quest, QuestObjective, QuestState, RpgPlayer, RpgPlugin, Skill,
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Components
// ─────────────────────────────────────────────────────────────────────────────

/// RPG player controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpgPlayer {
    pub level: u32,
    pub experience: u64,
    pub experience_to_next: u64,
    pub gold: u32,
    pub stat_points: u32,
    pub skill_points: u32,
}

impl Default for RpgPlayer {
    fn default() -> Self {
        Self {
            level: 1,
            experience: 0,
            experience_to_next: 100,
            gold: 0,
            stat_points: 0,
            skill_points: 0,
        }
    }
}

impl RpgPlayer {
    pub fn add_experience(&mut self, amount: u64) -> bool {
        self.experience += amount;
        let mut leveled_up = false;
        while self.experience >= self.experience_to_next {
            self.experience -= self.experience_to_next;
            self.level += 1;
            self.experience_to_next = (self.experience_to_next as f32 * 1.5) as u64;
            self.stat_points += 3;
            self.skill_points += 1;
            leveled_up = true;
        }
        leveled_up
    }
}

/// Character statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub strength: u32,
    pub dexterity: u32,
    pub constitution: u32,
    pub intelligence: u32,
    pub wisdom: u32,
    pub charisma: u32,
    pub luck: u32,
}

impl Default for CharacterStats {
    fn default() -> Self {
        Self {
            strength: 10,
            dexterity: 10,
            constitution: 10,
            intelligence: 10,
            wisdom: 10,
            charisma: 10,
            luck: 10,
        }
    }
}

impl CharacterStats {
    pub fn physical_damage(&self) -> f32 {
        self.strength as f32 * 2.0
    }

    pub fn magic_damage(&self) -> f32 {
        self.intelligence as f32 * 2.0
    }

    pub fn max_health(&self) -> f32 {
        self.constitution as f32 * 10.0 + 50.0
    }

    pub fn max_mana(&self) -> f32 {
        self.wisdom as f32 * 5.0 + 20.0
    }

    pub fn critical_chance(&self) -> f32 {
        self.luck as f32 * 0.01
    }

    pub fn dodge_chance(&self) -> f32 {
        self.dexterity as f32 * 0.005
    }

    pub fn increase(&mut self, stat: StatType, amount: u32) {
        match stat {
            StatType::Strength => self.strength += amount,
            StatType::Dexterity => self.dexterity += amount,
            StatType::Constitution => self.constitution += amount,
            StatType::Intelligence => self.intelligence += amount,
            StatType::Wisdom => self.wisdom += amount,
            StatType::Charisma => self.charisma += amount,
            StatType::Luck => self.luck += amount,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatType {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
    Luck,
}

/// Item type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemType {
    Weapon,
    Armor,
    Consumable,
    Quest,
    Material,
    Key,
    Book,
    Gold,
}

/// Inventory item definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: u32,
    pub name: String,
    pub item_type: ItemType,
    pub description: String,
    pub stackable: bool,
    pub max_stack: u32,
    pub value: u32,
    pub weight: f32,
}

impl InventoryItem {
    pub fn new(id: u32, name: &str, item_type: ItemType) -> Self {
        Self {
            id,
            name: name.to_string(),
            item_type,
            description: String::new(),
            stackable: matches!(
                item_type,
                ItemType::Consumable | ItemType::Material | ItemType::Gold
            ),
            max_stack: if item_type == ItemType::Gold {
                9999
            } else {
                99
            },
            value: 0,
            weight: 0.0,
        }
    }
}

/// Inventory system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub items: HashMap<u32, (InventoryItem, u32)>,
    pub capacity: u32,
    pub gold: u32,
}

impl Inventory {
    pub fn new(capacity: u32) -> Self {
        Self {
            items: HashMap::new(),
            capacity,
            gold: 0,
        }
    }

    pub fn add_item(&mut self, item: InventoryItem, count: u32) -> u32 {
        if item.item_type == ItemType::Gold {
            self.gold += count;
            return count;
        }

        let entry = self.items.entry(item.id).or_insert((item, 0));

        if entry.0.stackable {
            let space = entry.0.max_stack.saturating_sub(entry.1);
            let to_add = count.min(space);
            entry.1 += to_add;
            to_add
        } else if entry.1 == 0 {
            entry.1 = 1;
            1
        } else {
            0
        }
    }

    pub fn remove_item(&mut self, item_id: u32, count: u32) -> u32 {
        if let Some(entry) = self.items.get_mut(&item_id) {
            let removed = count.min(entry.1);
            entry.1 -= removed;
            if entry.1 == 0 {
                self.items.remove(&item_id);
            }
            return removed;
        }
        0
    }

    pub fn has_item(&self, item_id: u32, count: u32) -> bool {
        self.items
            .get(&item_id)
            .map(|(_, c)| *c >= count)
            .unwrap_or(false)
    }

    pub fn total_weight(&self) -> f32 {
        self.items
            .values()
            .map(|(item, count)| item.weight * *count as f32)
            .sum()
    }

    pub fn is_full(&self) -> bool {
        self.items.len() as u32 >= self.capacity
    }
}

/// Equipment slot enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Head,
    Chest,
    Legs,
    Feet,
    Hands,
    Ring1,
    Ring2,
    Amulet,
    Belt,
    Cloak,
}

/// Equipment system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Equipment {
    pub slots: HashMap<EquipmentSlot, InventoryItem>,
}

impl Equipment {
    pub fn equip(&mut self, slot: EquipmentSlot, item: InventoryItem) -> Option<InventoryItem> {
        self.slots.insert(slot, item)
    }

    pub fn unequip(&mut self, slot: EquipmentSlot) -> Option<InventoryItem> {
        self.slots.remove(&slot)
    }

    pub fn get(&self, slot: EquipmentSlot) -> Option<&InventoryItem> {
        self.slots.get(&slot)
    }

    pub fn total_armor(&self) -> u32 {
        self.slots.values().map(|_| 10).sum()
    }
}

/// Skill definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub level: u32,
    pub max_level: u32,
    pub skill_type: SkillType,
    pub cooldown: f32,
    pub cooldown_timer: f32,
    pub mana_cost: u32,
    pub requires_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillType {
    Active,
    Passive,
    Toggle,
}

impl Skill {
    pub fn new(id: u32, name: &str, skill_type: SkillType) -> Self {
        Self {
            id,
            name: name.to_string(),
            description: String::new(),
            level: 0,
            max_level: 5,
            skill_type,
            cooldown: 1.0,
            cooldown_timer: 0.0,
            mana_cost: 0,
            requires_target: false,
        }
    }

    pub fn can_use(&self) -> bool {
        self.level > 0 && self.cooldown_timer <= 0.0
    }

    pub fn use_skill(&mut self) -> bool {
        if self.can_use() && self.skill_type == SkillType::Active {
            self.cooldown_timer = self.cooldown;
            return true;
        }
        false
    }

    pub fn upgrade(&mut self) -> bool {
        if self.level < self.max_level {
            self.level += 1;
            return true;
        }
        false
    }
}

/// Ability for combat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    pub id: u32,
    pub name: String,
    pub damage: f32,
    pub healing: f32,
    pub range: f32,
    pub area_of_effect: f32,
    pub cast_time: f32,
    pub channel_time: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
    pub resource_cost: u32,
}

impl Ability {
    pub fn new(id: u32, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            damage: 0.0,
            healing: 0.0,
            range: 5.0,
            area_of_effect: 0.0,
            cast_time: 0.0,
            channel_time: 0.0,
            cooldown: 1.0,
            cooldown_timer: 0.0,
            resource_cost: 0,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.cooldown_timer <= 0.0
    }

    pub fn activate(&mut self) -> bool {
        if self.is_ready() {
            self.cooldown_timer = self.cooldown;
            return true;
        }
        false
    }
}

/// Quest objective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestObjective {
    pub id: u32,
    pub description: String,
    pub objective_type: ObjectiveType,
    pub target_id: u32,
    pub required: u32,
    pub current: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectiveType {
    Kill,
    Collect,
    TalkTo,
    Explore,
    UseItem,
    ReachLevel,
}

impl QuestObjective {
    pub fn is_complete(&self) -> bool {
        self.current >= self.required
    }

    pub fn progress(&mut self, amount: u32) {
        self.current = (self.current + amount).min(self.required);
    }
}

/// Quest definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quest {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub objectives: Vec<QuestObjective>,
    pub rewards_gold: u32,
    pub rewards_experience: u64,
    pub rewards_items: Vec<u32>,
    pub prerequisite_quest: Option<u32>,
    pub state: QuestState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuestState {
    #[default]
    NotStarted,
    InProgress,
    Completed,
    TurnedIn,
}

impl Quest {
    pub fn new(id: u32, title: &str) -> Self {
        Self {
            id,
            title: title.to_string(),
            description: String::new(),
            objectives: Vec::new(),
            rewards_gold: 0,
            rewards_experience: 0,
            rewards_items: Vec::new(),
            prerequisite_quest: None,
            state: QuestState::NotStarted,
        }
    }

    pub fn add_objective(mut self, objective: QuestObjective) -> Self {
        self.objectives.push(objective);
        self
    }

    pub fn is_complete(&self) -> bool {
        self.objectives.iter().all(|o| o.is_complete())
    }

    pub fn start(&mut self) {
        if self.state == QuestState::NotStarted {
            self.state = QuestState::InProgress;
        }
    }

    pub fn complete(&mut self) {
        if self.state == QuestState::InProgress && self.is_complete() {
            self.state = QuestState::Completed;
        }
    }
}

/// NPC dialogue system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcDialogue {
    pub npc_id: u32,
    pub name: String,
    pub greeting: String,
    pub dialogue_tree: Vec<DialogueNode>,
    pub current_node: usize,
    pub has_quest: bool,
    pub quest_id: Option<u32>,
    pub shop_items: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueNode {
    pub id: u32,
    pub text: String,
    pub responses: Vec<DialogueResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueResponse {
    pub text: String,
    pub next_node: Option<u32>,
    pub action: Option<DialogueAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogueAction {
    AcceptQuest(u32),
    OpenShop,
    GiveItem(u32),
    EndDialogue,
}

// ─────────────────────────────────────────────────────────────────────────────
// Systems
// ─────────────────────────────────────────────────────────────────────────────

pub struct RpgStatSystem;

impl System for RpgStatSystem {
    fn name(&self) -> &str {
        "rpg_stats"
    }

    fn run(&mut self, world: &mut World) {
        world.for_each_mut2::<CharacterStats, Health, _>(|_entity, stats, health| {
            health.max = stats.max_health();
        });
    }
}

pub struct RpgSkillSystem;

impl System for RpgSkillSystem {
    fn name(&self) -> &str {
        "rpg_skills"
    }

    fn run(&mut self, world: &mut World) {
        let dt = 1.0 / 60.0;

        world.for_each_mut::<Skill, _>(|_entity, skill| {
            if skill.cooldown_timer > 0.0 {
                skill.cooldown_timer -= dt;
            }
        });

        world.for_each_mut::<Ability, _>(|_entity, ability| {
            if ability.cooldown_timer > 0.0 {
                ability.cooldown_timer -= dt;
            }
        });
    }
}

pub struct RpgQuestSystem;

impl System for RpgQuestSystem {
    fn name(&self) -> &str {
        "rpg_quests"
    }

    fn run(&mut self, world: &mut World) {
        world.for_each_mut::<Quest, _>(|_entity, quest| {
            if quest.state == QuestState::InProgress && quest.is_complete() {
                quest.state = QuestState::Completed;
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Plugin
// ─────────────────────────────────────────────────────────────────────────────

pub struct RpgPlugin;

impl quasar_core::Plugin for RpgPlugin {
    fn name(&self) -> &str {
        "rpg_template"
    }

    fn build(&self, app: &mut quasar_core::App) {
        app.world.insert_resource(InputState::default());

        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(RpgStatSystem),
        );
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(RpgSkillSystem),
        );
        app.schedule.add_system(
            quasar_core::ecs::SystemStage::Update,
            Box::new(RpgQuestSystem),
        );

        spawn_rpg_player(&mut app.world);
    }
}

fn spawn_rpg_player(world: &mut World) {
    let entity = world.spawn();
    world.insert(entity, RpgPlayer::default());
    world.insert(entity, CharacterStats::default());
    world.insert(entity, TemplateTransform::default());
    world.insert(entity, TemplateVelocity::default());
    world.insert(entity, Health::new(100.0));
    world.insert(entity, Inventory::new(20));
    world.insert(entity, Equipment::default());
}

pub fn spawn_rpg_npc(world: &mut World, position: Vec3, name: &str) {
    let entity = world.spawn();
    world.insert(
        entity,
        TemplateTransform {
            position,
            ..Default::default()
        },
    );
    world.insert(
        entity,
        NpcDialogue {
            npc_id: 0,
            name: name.to_string(),
            greeting: "Hello, traveler!".to_string(),
            dialogue_tree: Vec::new(),
            current_node: 0,
            has_quest: false,
            quest_id: None,
            shop_items: Vec::new(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpg_player_level_up() {
        let mut player = RpgPlayer::default();
        let leveled = player.add_experience(150);
        assert!(leveled);
        assert_eq!(player.level, 2);
    }

    #[test]
    fn rpg_player_no_level() {
        let mut player = RpgPlayer::default();
        let leveled = player.add_experience(50);
        assert!(!leveled);
        assert_eq!(player.level, 1);
    }

    #[test]
    fn character_stats_damage() {
        let stats = CharacterStats {
            strength: 20,
            ..Default::default()
        };
        assert_eq!(stats.physical_damage(), 40.0);
    }

    #[test]
    fn inventory_add_item() {
        let mut inv = Inventory::new(10);
        let item = InventoryItem::new(1, "Sword", ItemType::Weapon);
        inv.add_item(item, 1);
        assert!(inv.has_item(1, 1));
    }

    #[test]
    fn inventory_gold() {
        let mut inv = Inventory::new(10);
        let gold = InventoryItem::new(0, "Gold", ItemType::Gold);
        inv.add_item(gold, 100);
        assert_eq!(inv.gold, 100);
    }

    #[test]
    fn skill_upgrade() {
        let mut skill = Skill::new(1, "Fireball", SkillType::Active);
        assert!(skill.upgrade());
        assert_eq!(skill.level, 1);
    }

    #[test]
    fn quest_completion() {
        let quest = Quest::new(1, "Test Quest").add_objective(QuestObjective {
            id: 0,
            description: "Test".to_string(),
            objective_type: ObjectiveType::Kill,
            target_id: 1,
            required: 1,
            current: 1,
        });
        assert!(quest.is_complete());
    }

    #[test]
    fn equipment_armor() {
        let mut equip = Equipment::default();
        let armor = InventoryItem::new(1, "Iron Helm", ItemType::Armor);
        equip.equip(EquipmentSlot::Head, armor);
        assert!(equip.total_armor() > 0);
    }
}
