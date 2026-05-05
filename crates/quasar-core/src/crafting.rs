//! Crafting system with recipes and requirements.
//!
//! Provides:
//! - Recipe definition with ingredients and results
//! - Crafting requirements (level, skills, tools, location)
//! - Crafting queue for batch production
//! - Crafting stations and workbenches
//! - Recipe discovery and learning
//! - Crafting experience and skill progression
//! - Quality tiers for crafted items
//! - Blueprint and recipe item system
//!
//! # Example
//!
//! ```
//! use quasar_core::crafting::*;
//! use quasar_core::item::*;
//! use quasar_core::inventory::*;
//!
//! // Create a registry and add items
//! let mut registry = ItemRegistry::new();
//! registry.register(ItemTemplate::new("iron_ore").with_max_stack(200));
//! registry.register(ItemTemplate::new("iron_ingot").with_max_stack(100));
//! registry.register(ItemTemplate::new("iron_sword")
//!     .with_max_stack(1)
//!     .with_equippable(EquipmentConfig {
//!         slot: EquipmentSlot::MainHand,
//!         stat_modifiers: vec![],
//!         damage_range: Some((10, 25)),
//!         attack_speed: Some(1.2),
//!         armor_value: 0,
//!         block_chance: 0.0,
//!         durability_max: 100,
//!         set_id: None,
//!         stat_requirements: Default::default(),
//!         class_requirement: None,
//!         bind_on_equip: false,
//!         unique_equipped: false,
//!         soulbound: false,
//!     }));
//!
//! // Create crafting system
//! let mut crafting = CraftingSystem::new();
//!
//! // Register a recipe
//! let recipe = Recipe::new("iron_ingot_recipe")
//!     .with_name("recipe.iron_ingot.name")
//!     .with_result("iron_ingot", 1)
//!     .add_ingredient("iron_ore", 2)
//!     .with_craft_time(5.0)
//!     .with_station(CraftingStation::Forge)
//!     .with_skill_requirement("blacksmithing", 1);
//!
//! crafting.register_recipe(recipe);
//!
//! // Check if can craft
//! let inventory = Inventory::new(40, 100.0);
//! let can_craft = crafting.can_craft(&registry, &inventory, "iron_ingot_recipe");
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::inventory::{Inventory, InventoryError};
use crate::item::{ItemRegistry, ItemRarity, ItemStack};

// ---------------------------------------------------------------------------
// Crafting Events
// ---------------------------------------------------------------------------

/// Events emitted by the crafting system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CraftingEvent {
    /// Recipe was discovered/learned.
    RecipeDiscovered { recipe_id: String },
    /// Crafting started.
    CraftingStarted {
        recipe_id: String,
        quantity: u32,
    },
    /// Crafting completed.
    CraftingCompleted {
        recipe_id: String,
        result_item: String,
        quantity: u32,
        quality: CraftingQuality,
    },
    /// Crafting failed.
    CraftingFailed {
        recipe_id: String,
        reason: String,
    },
    /// Crafting was cancelled.
    CraftingCancelled {
        recipe_id: String,
    },
    /// Crafting experience gained.
    CraftingXpGained {
        recipe_id: String,
        xp_amount: u32,
    },
    /// Crafting skill increased.
    SkillIncreased {
        skill: String,
        old_level: u32,
        new_level: u32,
    },
    /// Recipe was discovered through crafting.
    RecipeLearned {
        recipe_id: String,
        source: String,
    },
}

// ---------------------------------------------------------------------------
// Recipe Definition
// ---------------------------------------------------------------------------

/// A crafting recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    /// Unique recipe ID.
    pub id: String,
    /// Display name localization key.
    pub name_key: String,
    /// Description localization key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    /// Result item template ID.
    pub result_item: String,
    /// Result quantity.
    #[serde(default = "default_result_quantity")]
    pub result_quantity: u32,
    /// Required ingredients (item_id -> quantity).
    pub ingredients: HashMap<String, u32>,
    /// Crafting time in seconds.
    #[serde(default = "default_craft_time")]
    pub craft_time: f32,
    /// Required crafting station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station: Option<CraftingStation>,
    /// Required skill and level.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skill_requirements: HashMap<String, u32>,
    /// Required player level.
    #[serde(default)]
    pub level_requirement: u32,
    /// Required tools (not consumed).
    #[serde(default)]
    pub tool_requirements: Vec<String>,
    /// Required location/biome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_requirement: Option<String>,
    /// Required quest completion.
    #[serde(default)]
    pub quest_requirements: Vec<String>,
    /// Required flags.
    #[serde(default)]
    pub flag_requirements: Vec<String>,
    /// Base experience gained.
    #[serde(default)]
    pub experience_reward: u32,
    /// Skill experience gained.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skill_experience: HashMap<String, u32>,
    /// Chance for bonus result (0.0-1.0).
    #[serde(default)]
    pub bonus_chance: f32,
    /// Bonus result quantity.
    #[serde(default = "default_bonus_quantity")]
    pub bonus_quantity: u32,
    /// Quality range for result.
    #[serde(default)]
    pub quality_range: CraftingQualityRange,
    /// Recipe category.
    #[serde(default)]
    pub category: RecipeCategory,
    /// Recipe tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether recipe is discovered by default.
    #[serde(default = "default_true")]
    pub discovered_by_default: bool,
    /// Whether recipe can be learned.
    #[serde(default = "default_true")]
    pub learnable: bool,
    /// Whether recipe is repeatable.
    #[serde(default = "default_true")]
    pub repeatable: bool,
    /// Cooldown between crafts (seconds).
    #[serde(default)]
    pub cooldown: f32,
    /// Maximum craft count per day.
    #[serde(default)]
    pub daily_limit: Option<u32>,
    /// Lua script to run before crafting (can cancel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_start_script: Option<String>,
    /// Lua script to run after crafting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_complete_script: Option<String>,
    /// Custom data for scripting.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

fn default_result_quantity() -> u32 {
    1
}

fn default_craft_time() -> f32 {
    1.0
}

fn default_bonus_quantity() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

impl Recipe {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: String::new(),
            description_key: None,
            result_item: String::new(),
            result_quantity: 1,
            ingredients: HashMap::new(),
            craft_time: 1.0,
            station: None,
            skill_requirements: HashMap::new(),
            level_requirement: 0,
            tool_requirements: Vec::new(),
            location_requirement: None,
            quest_requirements: Vec::new(),
            flag_requirements: Vec::new(),
            experience_reward: 0,
            skill_experience: HashMap::new(),
            bonus_chance: 0.0,
            bonus_quantity: 1,
            quality_range: CraftingQualityRange::default(),
            category: RecipeCategory::General,
            tags: Vec::new(),
            discovered_by_default: true,
            learnable: true,
            repeatable: true,
            cooldown: 0.0,
            daily_limit: None,
            on_start_script: None,
            on_complete_script: None,
            custom_data: HashMap::new(),
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

    pub fn with_result(mut self, item_id: impl Into<String>, quantity: u32) -> Self {
        self.result_item = item_id.into();
        self.result_quantity = quantity;
        self
    }

    pub fn add_ingredient(mut self, item_id: impl Into<String>, quantity: u32) -> Self {
        self.ingredients.insert(item_id.into(), quantity);
        self
    }

    pub fn with_craft_time(mut self, seconds: f32) -> Self {
        self.craft_time = seconds;
        self
    }

    pub fn with_station(mut self, station: CraftingStation) -> Self {
        self.station = Some(station);
        self
    }

    pub fn with_skill_requirement(mut self, skill: impl Into<String>, level: u32) -> Self {
        self.skill_requirements.insert(skill.into(), level);
        self
    }

    pub fn with_level_requirement(mut self, level: u32) -> Self {
        self.level_requirement = level;
        self
    }

    pub fn with_tool(mut self, tool_id: impl Into<String>) -> Self {
        self.tool_requirements.push(tool_id.into());
        self
    }

    pub fn with_location_requirement(mut self, location: impl Into<String>) -> Self {
        self.location_requirement = Some(location.into());
        self
    }

    pub fn add_quest_requirement(mut self, quest_id: impl Into<String>) -> Self {
        self.quest_requirements.push(quest_id.into());
        self
    }

    pub fn with_experience_reward(mut self, xp: u32) -> Self {
        self.experience_reward = xp;
        self
    }

    pub fn with_skill_experience(mut self, skill: impl Into<String>, xp: u32) -> Self {
        self.skill_experience.insert(skill.into(), xp);
        self
    }

    pub fn with_bonus_chance(mut self, chance: f32, quantity: u32) -> Self {
        self.bonus_chance = chance;
        self.bonus_quantity = quantity;
        self
    }

    pub fn with_quality_range(mut self, range: CraftingQualityRange) -> Self {
        self.quality_range = range;
        self
    }

    pub fn with_category(mut self, category: RecipeCategory) -> Self {
        self.category = category;
        self
    }

    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn not_discovered_by_default(mut self) -> Self {
        self.discovered_by_default = false;
        self
    }

    pub fn not_learnable(mut self) -> Self {
        self.learnable = false;
        self
    }

    pub fn with_cooldown(mut self, seconds: f32) -> Self {
        self.cooldown = seconds;
        self
    }

    pub fn with_daily_limit(mut self, limit: u32) -> Self {
        self.daily_limit = Some(limit);
        self
    }

    /// Get total ingredient count.
    pub fn total_ingredients(&self) -> u32 {
        self.ingredients.values().sum()
    }

    /// Get unique ingredient count.
    pub fn unique_ingredient_count(&self) -> usize {
        self.ingredients.len()
    }

    /// Check if recipe requires a specific item.
    pub fn requires_item(&self, item_id: &str) -> bool {
        self.ingredients.contains_key(item_id)
    }
}

// ---------------------------------------------------------------------------
// Crafting Station
// ---------------------------------------------------------------------------

/// Crafting station types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CraftingStation {
    None,
    Anvil,
    Forge,
    Workbench,
    AlchemyLab,
    EnchantingTable,
    CookingFire,
    Loom,
    TanningRack,
    CarpentryBench,
    JewelcraftingTable,
    EngineeringStation,
    TailoringTable,
    InscriptionDesk,
    Campfire,
    Custom(&'static str),
}

impl CraftingStation {
    /// Get all crafting stations.
    pub fn all_stations() -> &'static [CraftingStation] {
        &[
            CraftingStation::None,
            CraftingStation::Anvil,
            CraftingStation::Forge,
            CraftingStation::Workbench,
            CraftingStation::AlchemyLab,
            CraftingStation::EnchantingTable,
            CraftingStation::CookingFire,
            CraftingStation::Loom,
            CraftingStation::TanningRack,
            CraftingStation::CarpentryBench,
            CraftingStation::JewelcraftingTable,
            CraftingStation::EngineeringStation,
            CraftingStation::TailoringTable,
            CraftingStation::InscriptionDesk,
            CraftingStation::Campfire,
        ]
    }

    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            CraftingStation::None => "None",
            CraftingStation::Anvil => "Anvil",
            CraftingStation::Forge => "Forge",
            CraftingStation::Workbench => "Workbench",
            CraftingStation::AlchemyLab => "Alchemy Lab",
            CraftingStation::EnchantingTable => "Enchanting Table",
            CraftingStation::CookingFire => "Cooking Fire",
            CraftingStation::Loom => "Loom",
            CraftingStation::TanningRack => "Tanning Rack",
            CraftingStation::CarpentryBench => "Carpentry Bench",
            CraftingStation::JewelcraftingTable => "Jewelcrafting Table",
            CraftingStation::EngineeringStation => "Engineering Station",
            CraftingStation::TailoringTable => "Tailoring Table",
            CraftingStation::InscriptionDesk => "Inscription Desk",
            CraftingStation::Campfire => "Campfire",
            CraftingStation::Custom(_) => "Custom",
        }
    }
}

// ---------------------------------------------------------------------------
// Recipe Category
// ---------------------------------------------------------------------------

/// Recipe category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RecipeCategory {
    Weapon,
    Armor,
    Consumable,
    Material,
    Enchantment,
    Gem,
    Jewelry,
    Container,
    Mount,
    Pet,
    Recipe,
    #[default]
    General,
    Custom(String),
}

// ---------------------------------------------------------------------------
// Crafting Quality
// ---------------------------------------------------------------------------

/// Crafting quality tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum CraftingQuality {
    #[default]
    Poor,
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl CraftingQuality {
    /// Convert to item rarity.
    pub fn to_rarity(&self) -> ItemRarity {
        match self {
            CraftingQuality::Poor => ItemRarity::Common,
            CraftingQuality::Common => ItemRarity::Common,
            CraftingQuality::Uncommon => ItemRarity::Uncommon,
            CraftingQuality::Rare => ItemRarity::Rare,
            CraftingQuality::Epic => ItemRarity::Epic,
            CraftingQuality::Legendary => ItemRarity::Legendary,
        }
    }

    /// Get quality multiplier for stats.
    pub fn stat_multiplier(&self) -> f32 {
        match self {
            CraftingQuality::Poor => 0.5,
            CraftingQuality::Common => 1.0,
            CraftingQuality::Uncommon => 1.5,
            CraftingQuality::Rare => 2.0,
            CraftingQuality::Epic => 3.0,
            CraftingQuality::Legendary => 5.0,
        }
    }

    /// Get value multiplier.
    pub fn value_multiplier(&self) -> f32 {
        match self {
            CraftingQuality::Poor => 0.5,
            CraftingQuality::Common => 1.0,
            CraftingQuality::Uncommon => 1.5,
            CraftingQuality::Rare => 2.5,
            CraftingQuality::Epic => 5.0,
            CraftingQuality::Legendary => 10.0,
        }
    }
}

/// Quality range for crafted items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingQualityRange {
    pub min_quality: CraftingQuality,
    pub max_quality: CraftingQuality,
}

impl Default for CraftingQualityRange {
    fn default() -> Self {
        Self {
            min_quality: CraftingQuality::Common,
            max_quality: CraftingQuality::Common,
        }
    }
}

impl CraftingQualityRange {
    pub fn new(min: CraftingQuality, max: CraftingQuality) -> Self {
        Self {
            min_quality: min,
            max_quality: max,
        }
    }

    /// Roll a random quality within range.
    pub fn roll_quality(&self) -> CraftingQuality {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let qualities = [
            CraftingQuality::Poor,
            CraftingQuality::Common,
            CraftingQuality::Uncommon,
            CraftingQuality::Rare,
            CraftingQuality::Epic,
            CraftingQuality::Legendary,
        ];

        let min_idx = qualities.iter().position(|&q| q == self.min_quality).unwrap_or(0);
        let max_idx = qualities
            .iter()
            .position(|&q| q == self.max_quality)
            .unwrap_or(qualities.len() - 1);

        if min_idx >= max_idx {
            return self.min_quality;
        }

        let idx = rng.gen_range(min_idx..=max_idx);
        qualities[idx]
    }
}

// ---------------------------------------------------------------------------
// Crafting Skill
// ---------------------------------------------------------------------------

/// Crafting skill state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingSkill {
    /// Skill name.
    pub name: String,
    /// Current skill level.
    pub level: u32,
    /// Current experience.
    #[serde(default)]
    pub experience: u32,
    /// Experience needed for next level.
    #[serde(default = "default_xp_to_next")]
    pub experience_to_next: u32,
    /// Total recipes learned.
    #[serde(default)]
    pub recipes_learned: u32,
    /// Total items crafted.
    #[serde(default)]
    pub items_crafted: u32,
    /// Critical craft chance (bonus quality).
    #[serde(default)]
    pub crit_chance: f32,
    /// Material save chance.
    #[serde(default)]
    pub save_chance: f32,
    /// Speed reduction (percentage).
    #[serde(default)]
    pub speed_bonus: f32,
}

fn default_xp_to_next() -> u32 {
    100
}

impl CraftingSkill {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level: 0,
            experience: 0,
            experience_to_next: 100,
            recipes_learned: 0,
            items_crafted: 0,
            crit_chance: 0.0,
            save_chance: 0.0,
            speed_bonus: 0.0,
        }
    }

    /// Add experience and level up if needed.
    pub fn add_experience(&mut self, amount: u32) -> Vec<u32> {
        let mut level_ups = Vec::new();
        self.experience += amount;

        while self.experience >= self.experience_to_next {
            self.experience -= self.experience_to_next;
            self.level += 1;
            level_ups.push(self.level);

            // Increase XP requirement for next level
            self.experience_to_next = (self.experience_to_next as f32 * 1.2) as u32;

            // Improve secondary stats
            self.crit_chance = (self.level as f32 * 0.1).min(50.0);
            self.save_chance = (self.level as f32 * 0.05).min(25.0);
            self.speed_bonus = (self.level as f32 * 0.5).min(50.0);
        }

        level_ups
    }

    /// Get progress to next level (0.0-1.0).
    pub fn progress_to_next(&self) -> f32 {
        if self.experience_to_next == 0 {
            return 1.0;
        }
        (self.experience as f32 / self.experience_to_next as f32).min(1.0)
    }
}

// ---------------------------------------------------------------------------
// Crafting Queue Item
// ---------------------------------------------------------------------------

/// Item in the crafting queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    /// Recipe ID.
    pub recipe_id: String,
    /// Quantity to craft.
    pub quantity: u32,
    /// Quantity completed.
    #[serde(default)]
    pub completed: u32,
    /// Current progress (0.0-1.0 for current item).
    #[serde(default)]
    pub progress: f32,
    /// Quality results.
    #[serde(default)]
    pub results: Vec<CraftingQuality>,
    /// Start timestamp.
    pub started_at: u64,
    /// Estimated completion time.
    pub estimated_completion: u64,
}

impl QueueItem {
    pub fn new(recipe_id: impl Into<String>, quantity: u32, craft_time: f32) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let eta = now + (craft_time * quantity as f32) as u64;

        Self {
            recipe_id: recipe_id.into(),
            quantity,
            completed: 0,
            progress: 0.0,
            results: Vec::new(),
            started_at: now,
            estimated_completion: eta,
        }
    }

    /// Update progress.
    pub fn update_progress(&mut self, delta: f32) {
        self.progress = (self.progress + delta).min(1.0);

        if self.progress >= 1.0 {
            self.completed += 1;
            self.progress = 0.0;
        }
    }

    /// Check if queue item is complete.
    pub fn is_complete(&self) -> bool {
        self.completed >= self.quantity
    }

    /// Get overall progress (0.0-1.0).
    pub fn overall_progress(&self) -> f32 {
        if self.quantity == 0 {
            return 1.0;
        }
        let completed_progress = self.completed as f32 / self.quantity as f32;
        let current_item_progress = self.progress / self.quantity as f32;
        (completed_progress + current_item_progress).min(1.0)
    }
}

// ---------------------------------------------------------------------------
// Crafting System
// ---------------------------------------------------------------------------

/// Main crafting system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftingSystem {
    /// All registered recipes.
    pub recipes: HashMap<String, Recipe>,
    /// Discovered recipes (known to player).
    pub discovered_recipes: HashSet<String>,
    /// Crafting skills.
    pub skills: HashMap<String, CraftingSkill>,
    /// Crafting queue.
    pub queue: VecDeque<QueueItem>,
    /// Currently active station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_station: Option<CraftingStation>,
    /// Event log.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_log: Vec<CraftingEvent>,
    /// Maximum event log size.
    #[serde(default = "default_max_log_size")]
    pub max_log_size: usize,
    /// Crafting state (for resumable crafts).
    #[serde(default)]
    pub is_crafting: bool,
    /// Last craft timestamp (for cooldown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_craft_time: Option<u64>,
    /// Daily craft counts (recipe_id -> count).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub daily_craft_counts: HashMap<String, u32>,
    /// Custom data for scripting.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

fn default_max_log_size() -> usize {
    100
}

impl CraftingSystem {
    pub fn new() -> Self {
        Self {
            recipes: HashMap::new(),
            discovered_recipes: HashSet::new(),
            skills: HashMap::new(),
            queue: VecDeque::new(),
            active_station: None,
            event_log: Vec::new(),
            max_log_size: 100,
            is_crafting: false,
            last_craft_time: None,
            daily_craft_counts: HashMap::new(),
            custom_data: HashMap::new(),
        }
    }

    /// Register a recipe.
    pub fn register_recipe(&mut self, recipe: Recipe) {
        if recipe.discovered_by_default {
            self.discovered_recipes.insert(recipe.id.clone());
        }
        self.recipes.insert(recipe.id.clone(), recipe);
    }

    /// Get a recipe by ID.
    pub fn get_recipe(&self, id: &str) -> Option<&Recipe> {
        self.recipes.get(id)
    }

    /// Discover a recipe.
    pub fn discover_recipe(&mut self, recipe_id: &str) -> bool {
        if self.recipes.contains_key(recipe_id) {
            let discovered = self.discovered_recipes.insert(recipe_id.to_string());
            if discovered {
                self.log_event(CraftingEvent::RecipeDiscovered {
                    recipe_id: recipe_id.to_string(),
                });
            }
            discovered
        } else {
            false
        }
    }

    /// Check if recipe is discovered.
    pub fn is_recipe_discovered(&self, recipe_id: &str) -> bool {
        self.discovered_recipes.contains(recipe_id)
    }

    /// Add a crafting skill.
    pub fn add_skill(&mut self, skill: CraftingSkill) {
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Get a skill by name.
    pub fn get_skill(&self, name: &str) -> Option<&CraftingSkill> {
        self.skills.get(name)
    }

    /// Get mutable skill.
    pub fn get_skill_mut(&mut self, name: &str) -> Option<&mut CraftingSkill> {
        self.skills.get_mut(name)
    }

    /// Set active crafting station.
    pub fn set_station(&mut self, station: CraftingStation) {
        self.active_station = Some(station);
    }

    /// Clear active station.
    pub fn clear_station(&mut self) {
        self.active_station = None;
    }

    /// Check if player can craft a recipe.
    pub fn can_craft(
        &self,
        registry: &ItemRegistry,
        inventory: &Inventory,
        recipe_id: &str,
    ) -> Result<(), CraftingError> {
        let recipe = self
            .recipes
            .get(recipe_id)
            .ok_or(CraftingError::UnknownRecipe(recipe_id.to_string()))?;

        // Check if discovered
        if !self.is_recipe_discovered(recipe_id) {
            return Err(CraftingError::RecipeNotDiscovered(recipe_id.to_string()));
        }

        // Check if repeatable
        if !recipe.repeatable {
            // Check if already crafted (would need tracking)
        }

        // Check cooldown
        if recipe.cooldown > 0.0 {
            if let Some(last_time) = self.last_craft_time {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let elapsed = (now - last_time) as f32;
                if elapsed < recipe.cooldown {
                    return Err(CraftingError::Cooldown(
                        recipe.cooldown - elapsed,
                    ));
                }
            }
        }

        // Check daily limit
        if let Some(limit) = recipe.daily_limit {
            let count = self.daily_craft_counts.get(recipe_id).copied().unwrap_or(0);
            if count >= limit {
                return Err(CraftingError::DailyLimitExceeded(limit));
            }
        }

        // Check ingredients
        for (item_id, required_qty) in &recipe.ingredients {
            let have = inventory.get_item_count(item_id);
            if have < *required_qty {
                return Err(CraftingError::InsufficientIngredients(
                    item_id.clone(),
                    have,
                    *required_qty,
                ));
            }
        }

        // Check station
        if let Some(required_station) = recipe.station {
            if let Some(active_station) = self.active_station {
                if active_station != required_station {
                    return Err(CraftingError::WrongStation(
                        required_station,
                        active_station,
                    ));
                }
            } else {
                return Err(CraftingError::NoStation(required_station));
            }
        }

        // Check skill requirements
        for (skill_name, required_level) in &recipe.skill_requirements {
            if let Some(skill) = self.skills.get(skill_name) {
                if skill.level < *required_level {
                    return Err(CraftingError::InsufficientSkill(
                        skill_name.clone(),
                        skill.level,
                        *required_level,
                    ));
                }
            } else {
                return Err(CraftingError::InsufficientSkill(
                    skill_name.clone(),
                    0,
                    *required_level,
                ));
            }
        }

        // Check level requirement
        // This would need player level from game state

        // Check tools
        // This would need tool checking from inventory

        Ok(())
    }

    /// Craft an item immediately (instant craft).
    pub fn craft_item(
        &mut self,
        registry: &ItemRegistry,
        inventory: &mut Inventory,
        recipe_id: &str,
    ) -> Result<Vec<ItemStack>, CraftingError> {
        // Validate can craft
        self.can_craft(registry, inventory, recipe_id)?;

        let recipe = self.recipes.get(recipe_id).unwrap().clone();

        // Consume ingredients
        for (item_id, quantity) in &recipe.ingredients {
            // Find and remove items from inventory
            let mut remaining = *quantity;
            for slot_index in 0..inventory.capacity {
                if remaining == 0 {
                    break;
                }

                if let Some(stack) = &inventory.slots[slot_index].item {
                    if stack.template_id == *item_id {
                        let remove_amount = remaining.min(stack.quantity);
                        inventory.remove_item(registry, slot_index, remove_amount)?;
                        remaining -= remove_amount;
                    }
                }
            }

            if remaining > 0 {
                // This shouldn't happen due to can_craft check
                return Err(CraftingError::InsufficientIngredients(
                    item_id.clone(),
                    0,
                    *quantity,
                ));
            }
        }

        // Determine quality
        let quality = recipe.quality_range.roll_quality();

        // Create result items
        let mut results = Vec::new();
        let mut total_quantity = recipe.result_quantity;

        // Check for bonus result
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen::<f32>() < recipe.bonus_chance {
            total_quantity += recipe.bonus_quantity;
        }

        let mut result_stack = ItemStack::new(&recipe.result_item, total_quantity);

        // Apply quality-based item level adjustments
        if let Some(template) = registry.get(&recipe.result_item) {
            if let Some(base_level) = template.equipment.as_ref().map(|e| template.level_requirement) {
                let adjusted_level = (base_level as f32 * quality.stat_multiplier()) as u32;
                result_stack.item_level = Some(adjusted_level);
            }
        }

        results.push(result_stack);

        // Add result to inventory
        inventory.add_item(registry, &recipe.result_item, total_quantity)?;

        // Grant experience
        if recipe.experience_reward > 0 {
            // This would grant to player XP system
        }

        // Grant skill experience
        for (skill_name, xp) in &recipe.skill_experience {
            if let Some(skill) = self.skills.get_mut(skill_name) {
                let level_ups = skill.add_experience(*xp);

                if !level_ups.is_empty() {
                    self.log_event(CraftingEvent::SkillIncreased {
                        skill: skill_name.clone(),
                        old_level: skill.level - level_ups.len() as u32,
                        new_level: skill.level,
                    });
                }
            }
        }

        // Update tracking
        skill.items_crafted += 1;
        *self.daily_craft_counts.entry(recipe_id.to_string()).or_insert(0) += 1;
        self.last_craft_time = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        );

        // Log event
        self.log_event(CraftingEvent::CraftingCompleted {
            recipe_id: recipe_id.to_string(),
            result_item: recipe.result_item.clone(),
            quantity: total_quantity,
            quality,
        });

        Ok(results)
    }

    /// Start a queued craft.
    pub fn start_queue(
        &mut self,
        registry: &ItemRegistry,
        inventory: &Inventory,
        recipe_id: &str,
        quantity: u32,
    ) -> Result<(), CraftingError> {
        // Validate can craft
        self.can_craft(registry, inventory, recipe_id)?;

        let recipe = self
            .recipes
            .get(recipe_id)
            .ok_or(CraftingError::UnknownRecipe(recipe_id.to_string()))?;

        let queue_item = QueueItem::new(recipe_id, quantity, recipe.craft_time);
        self.queue.push_back(queue_item);
        self.is_crafting = true;

        self.log_event(CraftingEvent::CraftingStarted {
            recipe_id: recipe_id.to_string(),
            quantity,
        });

        Ok(())
    }

    /// Update crafting queue (call each frame/tick).
    pub fn update_queue(
        &mut self,
        registry: &ItemRegistry,
        inventory: &mut Inventory,
        delta_time: f32,
    ) -> Result<Vec<ItemStack>, CraftingError> {
        let mut completed_items = Vec::new();

        if let Some(queue_item) = self.queue.front_mut() {
            let recipe = self
                .recipes
                .get(&queue_item.recipe_id)
                .ok_or(CraftingError::UnknownRecipe(queue_item.recipe_id.clone()))?;

            // Update progress
            let progress_per_second = 1.0 / recipe.craft_time;
            queue_item.update_progress(progress_per_second * delta_time);

            // Check if current item completed
            if queue_item.progress == 0.0 && queue_item.completed > 0 {
                // Item completed, grant rewards
                let quality = recipe.quality_range.roll_quality();

                // Add to inventory
                let add_qty = recipe.result_quantity;
                inventory.add_item(registry, &recipe.result_item, add_qty)?;

                // Grant skill XP
                for (skill_name, xp) in &recipe.skill_experience {
                    if let Some(skill) = self.skills.get_mut(skill_name) {
                        skill.add_experience(*xp);
                    }
                }

                // Track daily count
                *self
                    .daily_craft_counts
                    .entry(queue_item.recipe_id.clone())
                    .or_insert(0) += 1;

                // Log completion
                self.log_event(CraftingEvent::CraftingCompleted {
                    recipe_id: queue_item.recipe_id.clone(),
                    result_item: recipe.result_item.clone(),
                    quantity: add_qty,
                    quality,
                });

                queue_item.results.push(quality);
            }

            // Check if entire queue is complete
            if queue_item.is_complete() {
                self.queue.pop_front();

                if self.queue.is_empty() {
                    self.is_crafting = false;
                }
            }
        }

        Ok(completed_items)
    }

    /// Cancel current crafting queue.
    pub fn cancel_queue(&mut self) -> Option<QueueItem> {
        if let Some(item) = self.queue.pop_front() {
            if self.queue.is_empty() {
                self.is_crafting = false;
            }

            self.log_event(CraftingEvent::CraftingCancelled {
                recipe_id: item.recipe_id.clone(),
            });

            Some(item)
        } else {
            None
        }
    }

    /// Get discovered recipes.
    pub fn get_discovered_recipes(&self) -> Vec<&Recipe> {
        self.recipes
            .values()
            .filter(|r| self.discovered_recipes.contains(&r.id))
            .collect()
    }

    /// Get recipes by category.
    pub fn get_recipes_by_category(&self, category: RecipeCategory) -> Vec<&Recipe> {
        self.get_discovered_recipes()
            .into_iter()
            .filter(|r| r.category == category)
            .collect()
    }

    /// Get recipes that can be crafted with current inventory.
    pub fn get_craftable_recipes(&self, registry: &ItemRegistry, inventory: &Inventory) -> Vec<&Recipe> {
        self.get_discovered_recipes()
            .into_iter()
            .filter(|r| self.can_craft(registry, inventory, &r.id).is_ok())
            .collect()
    }

    /// Search recipes by name.
    pub fn search_recipes(&self, query: &str) -> Vec<&Recipe> {
        self.get_discovered_recipes()
            .into_iter()
            .filter(|r| {
                r.name_key.to_lowercase().contains(&query.to_lowercase())
                    || r.id.to_lowercase().contains(&query.to_lowercase())
                    || r.result_item.to_lowercase().contains(&query.to_lowercase())
            })
            .collect()
    }

    /// Get recipes that produce a specific item.
    pub fn get_recipes_for_item(&self, item_id: &str) -> Vec<&Recipe> {
        self.get_discovered_recipes()
            .into_iter()
            .filter(|r| r.result_item == item_id)
            .collect()
    }

    /// Get recipes that use a specific ingredient.
    pub fn get_recipes_with_ingredient(&self, item_id: &str) -> Vec<&Recipe> {
        self.get_discovered_recipes()
            .into_iter()
            .filter(|r| r.ingredients.contains_key(item_id))
            .collect()
    }

    /// Reset daily craft counts.
    pub fn reset_daily_counts(&mut self) {
        self.daily_craft_counts.clear();
    }

    /// Log an event.
    fn log_event(&mut self, event: CraftingEvent) {
        self.event_log.push(event);
        if self.event_log.len() > self.max_log_size {
            self.event_log.remove(0);
        }
    }

    /// Clear event log.
    pub fn clear_event_log(&mut self) {
        self.event_log.clear();
    }

    /// Load recipes from JSON.
    pub fn load_from_json(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let recipes: Vec<Recipe> = serde_json::from_str(json)?;
        for recipe in recipes {
            self.register_recipe(recipe);
        }
        Ok(())
    }

    /// Export recipes to JSON.
    pub fn recipes_to_json(&self) -> Result<String, serde_json::Error> {
        let recipes: Vec<&Recipe> = self.recipes.values().collect();
        serde_json::to_string_pretty(&recipes)
    }
}

// ---------------------------------------------------------------------------
// Crafting Errors
// ---------------------------------------------------------------------------

/// Crafting operation errors.
#[derive(Debug, Clone)]
pub enum CraftingError {
    UnknownRecipe(String),
    RecipeNotDiscovered(String),
    InsufficientIngredients(String, u32, u32),
    WrongStation(CraftingStation, CraftingStation),
    NoStation(CraftingStation),
    InsufficientSkill(String, u32, u32),
    InsufficientLevel(u32, u32),
    MissingTool(String),
    WrongLocation(String),
    QuestNotComplete(String),
    FlagNotSet(String),
    Cooldown(f32),
    DailyLimitExceeded(u32),
    InventoryFull,
    CraftCancelled,
}

impl std::fmt::Display for CraftingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRecipe(id) => write!(f, "Unknown recipe: {}", id),
            Self::RecipeNotDiscovered(id) => write!(f, "Recipe not discovered: {}", id),
            Self::InsufficientIngredients(item, have, need) => {
                write!(
                    f,
                    "Insufficient ingredients for {}: have {}, need {}",
                    item, have, need
                )
            }
            Self::WrongStation(required, actual) => {
                write!(
                    f,
                    "Wrong crafting station: required {:?}, have {:?}",
                    required, actual
                )
            }
            Self::NoStation(required) => {
                write!(f, "No active station: required {:?}", required)
            }
            Self::InsufficientSkill(skill, have, need) => {
                write!(
                    f,
                    "Insufficient skill {}: level {}, need {}",
                    skill, have, need
                )
            }
            Self::InsufficientLevel(have, need) => {
                write!(f, "Insufficient level: {}, need {}", have, need)
            }
            Self::MissingTool(tool) => write!(f, "Missing tool: {}", tool),
            Self::WrongLocation(location) => write!(f, "Wrong location: {}", location),
            Self::QuestNotComplete(quest) => write!(f, "Quest not complete: {}", quest),
            Self::FlagNotSet(flag) => write!(f, "Flag not set: {}", flag),
            Self::Cooldown(remaining) => write!(f, "Crafting on cooldown: {:.1}s remaining", remaining),
            Self::DailyLimitExceeded(limit) => {
                write!(f, "Daily crafting limit exceeded: {}", limit)
            }
            Self::InventoryFull => write!(f, "Inventory is full"),
            Self::CraftCancelled => write!(f, "Crafting was cancelled"),
        }
    }
}

impl std::error::Error for CraftingError {}

// ---------------------------------------------------------------------------
// Convenience types for rand crate
// ---------------------------------------------------------------------------

// Note: Add rand to dependencies in Cargo.toml
// For now, we'll use a simple PRNG fallback
mod rand {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static SEED: Cell<u64> = Cell::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
    }

    pub trait Rng {
        fn gen(&mut self) -> f32;
        fn gen_range(&mut self, range: std::ops::RangeInclusive<usize>) -> usize;
    }

    pub struct ThreadRng;

    impl Rng for ThreadRng {
        fn gen(&mut self) -> f32 {
            let seed = SEED.with(|s| {
                let val = s.get();
                let new_seed = val.wrapping_mul(6364136223846793005).wrapping_add(1);
                s.set(new_seed);
                new_seed
            });
            (seed % 10000) as f32 / 10000.0
        }

        fn gen_range(&mut self, range: std::ops::RangeInclusive<usize>) -> usize {
            let val = self.gen();
            let span = range.end() - range.start() + 1;
            range.start() + (val * span as f32) as usize % span
        }
    }

    pub fn thread_rng() -> ThreadRng {
        ThreadRng
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> ItemRegistry {
        let mut registry = ItemRegistry::new();

        registry.register(
            ItemTemplate::new("iron_ore")
                .with_name("item.iron_ore.name")
                .with_max_stack(200)
        );

        registry.register(
            ItemTemplate::new("iron_ingot")
                .with_name("item.iron_ingot.name")
                .with_max_stack(100)
        );

        registry.register(
            ItemTemplate::new("iron_sword")
                .with_name("item.iron_sword.name")
                .with_max_stack(1)
        );

        registry
    }

    #[test]
    fn test_recipe_creation() {
        let recipe = Recipe::new("iron_ingot_recipe")
            .with_name("recipe.iron_ingot.name")
            .with_result("iron_ingot", 1)
            .add_ingredient("iron_ore", 2)
            .with_craft_time(5.0)
            .with_station(CraftingStation::Forge)
            .with_skill_requirement("blacksmithing", 1)
            .with_experience_reward(10)
            .with_skill_experience("blacksmithing", 15);

        assert_eq!(recipe.id, "iron_ingot_recipe");
        assert_eq!(recipe.result_item, "iron_ingot");
        assert_eq!(recipe.result_quantity, 1);
        assert_eq!(recipe.ingredients.get("iron_ore").copied(), Some(2));
        assert_eq!(recipe.station, Some(CraftingStation::Forge));
        assert_eq!(recipe.skill_requirements.get("blacksmithing").copied(), Some(1));
    }

    #[test]
    fn test_crafting_system() {
        let registry = create_test_registry();
        let mut crafting = CraftingSystem::new();

        // Register recipe
        crafting.register_recipe(
            Recipe::new("iron_ingot_recipe")
                .with_result("iron_ingot", 1)
                .add_ingredient("iron_ore", 2)
                .with_craft_time(1.0)
        );

        assert!(crafting.is_recipe_discovered("iron_ingot_recipe"));
        assert_eq!(crafting.get_discovered_recipes().len(), 1);
    }

    #[test]
    fn test_crafting_skill_progression() {
        let mut skill = CraftingSkill::new("blacksmithing");

        assert_eq!(skill.level, 0);
        assert_eq!(skill.experience, 0);

        let level_ups = skill.add_experience(100);
        assert_eq!(level_ups, vec![1]);
        assert_eq!(skill.level, 1);
        assert!(skill.experience < skill.experience_to_next);
    }

    #[test]
    fn test_quality_range() {
        let range = CraftingQualityRange::new(
            CraftingQuality::Common,
            CraftingQuality::Rare,
        );

        // Roll multiple times to ensure we get values in range
        for _ in 0..100 {
            let quality = range.roll_quality();
            assert!(quality >= CraftingQuality::Common);
            assert!(quality <= CraftingQuality::Rare);
        }
    }

    #[test]
    fn test_crafting_queue() {
        let mut queue_item = QueueItem::new("test_recipe", 5, 2.0);

        assert!(!queue_item.is_complete());
        assert_eq!(queue_item.completed, 0);

        // Simulate completing all items
        for _ in 0..10 {
            queue_item.update_progress(0.1);
        }

        assert_eq!(queue_item.completed, 1);
        assert!(!queue_item.is_complete());
    }
}
