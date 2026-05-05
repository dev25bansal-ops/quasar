//! Item definition system with properties and metadata.
//!
//! Provides:
//! - Item templates with rich metadata (name, description, icon, rarity)
//! - Item types (weapon, armor, consumable, material, quest, currency)
//! - Item properties (stackable, consumable, equippable, tradable)
//! - Equipment slots and stat modifiers
//! - Consumable effects (health, mana, buffs)
//! - Item quality/rarity system
//! - Localization support
//!
//! # Example
//!
//! ```
//! use quasar_core::item::*;
//!
//! // Define a weapon
//! let sword = ItemTemplate::new("iron_sword")
//!     .with_name("item.iron_sword.name")
//!     .with_description("item.iron_sword.desc")
//!     .with_item_type(ItemType::Weapon(WeaponType::Sword))
//!     .with_rarity(ItemRarity::Common)
//!     .with_max_stack(1)
//!     .with_equippable(EquipmentConfig {
//!         slot: EquipmentSlot::MainHand,
//!         stat_modifiers: vec![
//!             StatModifier::new("attack_power", 15),
//!         ],
//!         damage_range: Some((10, 25)),
//!         attack_speed: Some(1.2),
//!     });
//!
//! // Define a consumable
//! let potion = ItemTemplate::new("health_potion")
//!     .with_name("item.health_potion.name")
//!     .with_item_type(ItemType::Consumable(ConsumableType::Health))
//!     .with_rarity(ItemRarity::Common)
//!     .with_max_stack(99)
//!     .with_consumable(ConsumableConfig {
//!         effects: vec![ConsumableEffect {
//!             stat: "health".to_string(),
//!             value: 50.0,
//!             duration: 0.0, // instant
//!         }],
//!         use_time: 1.0,
//!         cooldown: 5.0,
//!     });
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Item Template
// ---------------------------------------------------------------------------

/// Template for an item definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemTemplate {
    /// Unique item ID.
    pub id: String,
    /// Display name localization key.
    pub name_key: String,
    /// Description localization key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    /// Icon path or atlas index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Item type (weapon, armor, consumable, etc.).
    pub item_type: ItemType,
    /// Item rarity/quality.
    #[serde(default)]
    pub rarity: ItemRarity,
    /// Maximum stack size (1 = not stackable).
    #[serde(default = "default_max_stack")]
    pub max_stack: u32,
    /// Base value in gold coins.
    #[serde(default)]
    pub base_value: u32,
    /// Item level requirement.
    #[serde(default)]
    pub level_requirement: u32,
    /// Weight (for inventory capacity systems).
    #[serde(default)]
    pub weight: f32,
    /// Equipment configuration (if equippable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment: Option<EquipmentConfig>,
    /// Consumable configuration (if consumable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumable: Option<ConsumableConfig>,
    /// Crafting material properties.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<MaterialConfig>,
    /// Quest item flag (cannot be dropped/sold).
    #[serde(default)]
    pub quest_item: bool,
    /// Tradable flag.
    #[serde(default = "default_true")]
    pub tradable: bool,
    /// Droppable flag.
    #[serde(default = "default_true")]
    pub droppable: bool,
    /// Destroyable flag (can be deleted from inventory).
    #[serde(default = "default_true")]
    pub destroyable: bool,
    /// Tags for filtering and scripting.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Custom metadata for game-specific use.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
    /// Lua script to run when item is picked up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_pickup_script: Option<String>,
    /// Lua script to run when item is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_use_script: Option<String>,
    /// Lua script to run when item is equipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_equip_script: Option<String>,
    /// Lua script to run when item is unequipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_unequip_script: Option<String>,
}

fn default_max_stack() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

impl ItemTemplate {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: String::new(),
            description_key: None,
            icon: None,
            item_type: ItemType::Material,
            rarity: ItemRarity::Common,
            max_stack: 1,
            base_value: 0,
            level_requirement: 0,
            weight: 0.0,
            equipment: None,
            consumable: None,
            material: None,
            quest_item: false,
            tradable: true,
            droppable: true,
            destroyable: true,
            tags: Vec::new(),
            custom_data: HashMap::new(),
            on_pickup_script: None,
            on_use_script: None,
            on_equip_script: None,
            on_unequip_script: None,
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

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_item_type(mut self, item_type: ItemType) -> Self {
        self.item_type = item_type;
        self
    }

    pub fn with_rarity(mut self, rarity: ItemRarity) -> Self {
        self.rarity = rarity;
        self
    }

    pub fn with_max_stack(mut self, stack: u32) -> Self {
        self.max_stack = stack;
        self
    }

    pub fn with_value(mut self, value: u32) -> Self {
        self.base_value = value;
        self
    }

    pub fn with_level_requirement(mut self, level: u32) -> Self {
        self.level_requirement = level;
        self
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_equippable(mut self, config: EquipmentConfig) -> Self {
        self.equipment = Some(config);
        self
    }

    pub fn with_consumable(mut self, config: ConsumableConfig) -> Self {
        self.consumable = Some(config);
        self
    }

    pub fn with_material(mut self, config: MaterialConfig) -> Self {
        self.material = Some(config);
        self
    }

    pub fn quest_item(mut self) -> Self {
        self.quest_item = true;
        self
    }

    pub fn not_tradable(mut self) -> Self {
        self.tradable = false;
        self
    }

    pub fn not_droppable(mut self) -> Self {
        self.droppable = false;
        self
    }

    pub fn not_destroyable(mut self) -> Self {
        self.destroyable = false;
        self
    }

    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Check if this item is stackable.
    pub fn is_stackable(&self) -> bool {
        self.max_stack > 1
    }

    /// Check if this item is equippable.
    pub fn is_equippable(&self) -> bool {
        self.equipment.is_some()
    }

    /// Check if this item is consumable.
    pub fn is_consumable(&self) -> bool {
        self.consumable.is_some()
    }

    /// Get the equipment slot if this item is equippable.
    pub fn equipment_slot(&self) -> Option<EquipmentSlot> {
        self.equipment.as_ref().map(|e| e.slot)
    }

    /// Get the display color based on rarity.
    pub fn rarity_color(&self) -> [f32; 3] {
        self.rarity.color()
    }

    /// Calculate the effective value with rarity multiplier.
    pub fn effective_value(&self) -> u32 {
        let multiplier = self.rarity.value_multiplier();
        (self.base_value as f32 * multiplier) as u32
    }
}

// ---------------------------------------------------------------------------
// Item Types
// ---------------------------------------------------------------------------

/// Item type classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ItemType {
    /// Weapon item.
    Weapon(WeaponType),
    /// Armor item.
    Armor(ArmorType),
    /// Accessory item (rings, amulets, etc.).
    Accessory(AccessoryType),
    /// Consumable item (potions, food, etc.).
    Consumable(ConsumableType),
    /// Crafting material.
    Material,
    /// Quest-specific item.
    Quest,
    /// Currency (gold, gems, tokens).
    Currency(CurrencyType),
    /// Container (bags, pouches).
    Container,
    /// Mount or pet.
    Mount,
    /// Recipe or blueprint.
    Recipe,
    /// Custom item type with parameters.
    Custom {
        type_id: String,
        params: HashMap<String, String>,
    },
}

impl Default for ItemType {
    fn default() -> Self {
        ItemType::Material
    }
}

/// Weapon subtypes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WeaponType {
    Sword,
    Axe,
    Mace,
    Dagger,
    Staff,
    Wand,
    Bow,
    Crossbow,
    Polearm,
    FistWeapon,
    Thrown,
    Shield,
    Custom(String),
}

/// Armor subtypes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArmorType {
    Cloth,
    Leather,
    Mail,
    Plate,
    Custom(String),
}

/// Accessory subtypes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccessoryType {
    Ring,
    Amulet,
    Trinket,
    Relic,
    Custom(String),
}

/// Consumable subtypes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsumableType {
    Health,
    Mana,
    Stamina,
    Food,
    Elixir,
    Scroll,
    Potion,
    Bomb,
    Key,
    Custom(String),
}

/// Currency types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CurrencyType {
    Gold,
    Silver,
    Copper,
    Gems,
    Tokens,
    Reputation,
    Custom(String),
}

// ---------------------------------------------------------------------------
// Item Rarity
// ---------------------------------------------------------------------------

/// Item rarity/quality tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ItemRarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
    Mythic,
    Artifact,
}

impl ItemRarity {
    /// Get the display color as RGB.
    pub fn color(&self) -> [f32; 3] {
        match self {
            ItemRarity::Common => [0.6, 0.6, 0.6],     // Gray
            ItemRarity::Uncommon => [0.1, 1.0, 0.1],   // Green
            ItemRarity::Rare => [0.0, 0.44, 0.92],     // Blue
            ItemRarity::Epic => [0.64, 0.21, 0.93],    // Purple
            ItemRarity::Legendary => [1.0, 0.5, 0.0],   // Orange
            ItemRarity::Mythic => [1.0, 0.0, 0.0],      // Red
            ItemRarity::Artifact => [1.0, 0.84, 0.0],   // Gold
        }
    }

    /// Get value multiplier based on rarity.
    pub fn value_multiplier(&self) -> f32 {
        match self {
            ItemRarity::Common => 1.0,
            ItemRarity::Uncommon => 1.5,
            ItemRarity::Rare => 2.5,
            ItemRarity::Epic => 5.0,
            ItemRarity::Legendary => 10.0,
            ItemRarity::Mythic => 20.0,
            ItemRarity::Artifact => 50.0,
        }
    }

    /// Get the rarity name for display.
    pub fn name(&self) -> &'static str {
        match self {
            ItemRarity::Common => "Common",
            ItemRarity::Uncommon => "Uncommon",
            ItemRarity::Rare => "Rare",
            ItemRarity::Epic => "Epic",
            ItemRarity::Legendary => "Legendary",
            ItemRarity::Mythic => "Mythic",
            ItemRarity::Artifact => "Artifact",
        }
    }
}

// ---------------------------------------------------------------------------
// Equipment System
// ---------------------------------------------------------------------------

/// Equipment configuration for equippable items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentConfig {
    /// Equipment slot this item occupies.
    pub slot: EquipmentSlot,
    /// Stat modifiers when equipped.
    #[serde(default)]
    pub stat_modifiers: Vec<StatModifier>,
    /// Damage range for weapons (min, max).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_range: Option<(u32, u32)>,
    /// Attack speed for weapons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_speed: Option<f32>,
    /// Armor value for armor items.
    #[serde(default)]
    pub armor_value: u32,
    /// Block chance for shields.
    #[serde(default)]
    pub block_chance: f32,
    /// Durability max value.
    #[serde(default)]
    pub durability_max: u32,
    /// Set ID for set bonuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_id: Option<String>,
    /// Required stats to equip.
    #[serde(default)]
    pub stat_requirements: HashMap<String, u32>,
    /// Required class to equip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_requirement: Option<String>,
    /// Binds on equip flag.
    #[serde(default)]
    pub bind_on_equip: bool,
    /// Unique equipped (can only have one of this item equipped).
    #[serde(default)]
    pub unique_equipped: bool,
    /// Soulbound flag (cannot be traded after pickup).
    #[serde(default)]
    pub soulbound: bool,
}

/// Equipment slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipmentSlot {
    Head,
    Neck,
    Shoulder,
    Back,
    Chest,
    Wrist,
    Hands,
    Waist,
    Legs,
    Feet,
    Finger1,
    Finger2,
    Trinket1,
    Trinket2,
    MainHand,
    OffHand,
    Ranged,
    Ammo,
    Tabard,
    Shirt,
    Custom(&'static str),
}

impl EquipmentSlot {
    /// Get all equipment slots.
    pub fn all_slots() -> &'static [EquipmentSlot] {
        &[
            EquipmentSlot::Head,
            EquipmentSlot::Neck,
            EquipmentSlot::Shoulder,
            EquipmentSlot::Back,
            EquipmentSlot::Chest,
            EquipmentSlot::Wrist,
            EquipmentSlot::Hands,
            EquipmentSlot::Waist,
            EquipmentSlot::Legs,
            EquipmentSlot::Feet,
            EquipmentSlot::Finger1,
            EquipmentSlot::Finger2,
            EquipmentSlot::Trinket1,
            EquipmentSlot::Trinket2,
            EquipmentSlot::MainHand,
            EquipmentSlot::OffHand,
            EquipmentSlot::Ranged,
            EquipmentSlot::Ammo,
            EquipmentSlot::Tabard,
            EquipmentSlot::Shirt,
        ]
    }

    /// Check if this is a weapon slot.
    pub fn is_weapon_slot(&self) -> bool {
        matches!(
            self,
            EquipmentSlot::MainHand | EquipmentSlot::OffHand | EquipmentSlot::Ranged
        )
    }

    /// Check if this is an accessory slot.
    pub fn is_accessory_slot(&self) -> bool {
        matches!(
            self,
            EquipmentSlot::Neck
                | EquipmentSlot::Finger1
                | EquipmentSlot::Finger2
                | EquipmentSlot::Trinket1
                | EquipmentSlot::Trinket2
        )
    }
}

/// Stat modifier applied when item is equipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatModifier {
    /// Stat name (e.g., "strength", "agility", "attack_power").
    pub stat: String,
    /// Modifier value (positive or negative).
    pub value: f32,
    /// Modifier type (flat, percentage, multiplicative).
    #[serde(default)]
    pub modifier_type: StatModifierType,
}

impl StatModifier {
    pub fn new(stat: impl Into<String>, value: f32) -> Self {
        Self {
            stat: stat.into(),
            value,
            modifier_type: StatModifierType::Flat,
        }
    }

    pub fn percentage(stat: impl Into<String>, value: f32) -> Self {
        Self {
            stat: stat.into(),
            value,
            modifier_type: StatModifierType::Percentage,
        }
    }

    pub fn multiplicative(stat: impl Into<String>, value: f32) -> Self {
        Self {
            stat: stat.into(),
            value,
            modifier_type: StatModifierType::Multiplicative,
        }
    }
}

/// Stat modifier type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StatModifierType {
    #[default]
    Flat,
    Percentage,
    Multiplicative,
}

// ---------------------------------------------------------------------------
// Consumable System
// ---------------------------------------------------------------------------

/// Consumable configuration for usable items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableConfig {
    /// Effects applied when consumed.
    pub effects: Vec<ConsumableEffect>,
    /// Time to use the item (seconds).
    #[serde(default = "default_use_time")]
    pub use_time: f32,
    /// Cooldown after use (seconds).
    #[serde(default)]
    pub cooldown: f32,
    /// Level requirement to use.
    #[serde(default)]
    pub level_requirement: u32,
    /// Cast interruption on damage.
    #[serde(default = "default_true")]
    pub interruptible: bool,
    /// Only usable out of combat.
    #[serde(default)]
    pub out_of_combat_only: bool,
}

fn default_use_time() -> f32 {
    1.0
}

/// Effect applied when consumable is used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableEffect {
    /// Stat to modify (e.g., "health", "mana", "stamina").
    pub stat: String,
    /// Value to add (or percentage if is_percentage is true).
    pub value: f32,
    /// Duration in seconds (0 = instant).
    #[serde(default)]
    pub duration: f32,
    /// Tick interval for periodic effects (0 = no ticks).
    #[serde(default)]
    pub tick_interval: f32,
    /// Whether value is a percentage.
    #[serde(default)]
    pub is_percentage: bool,
    /// Buff/debuff name for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_name: Option<String>,
    /// Icon for the buff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buff_icon: Option<String>,
    /// Priority for buff ordering.
    #[serde(default)]
    pub buff_priority: i32,
}

// ---------------------------------------------------------------------------
// Material System
// ---------------------------------------------------------------------------

/// Material configuration for crafting items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialConfig {
    /// Material category.
    pub category: MaterialCategory,
    /// Material grade/quality.
    #[serde(default)]
    pub grade: u32,
    /// Crafting recipes this material is used in.
    #[serde(default)]
    pub used_in_recipes: Vec<String>,
    /// Special properties.
    #[serde(default)]
    pub properties: Vec<String>,
}

/// Material category classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MaterialCategory {
    Ore,
    Cloth,
    Leather,
    Wood,
    Herb,
    Gem,
    Meat,
    Fish,
    Elemental,
    Enchanting,
    Alchemy,
    Junk,
    #[default]
    Other,
}

// ---------------------------------------------------------------------------
// Item Instance
// ---------------------------------------------------------------------------

/// Runtime item instance in inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStack {
    /// Item template ID.
    pub template_id: String,
    /// Quantity in this stack.
    pub quantity: u32,
    /// Unique instance ID (for non-stackable items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// Current durability (for equippable items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability: Option<u32>,
    /// Bound to player flag.
    #[serde(default)]
    pub bound: bool,
    /// Item level override from template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_level: Option<u32>,
    /// Enchantments applied to this item.
    #[serde(default)]
    pub enchantments: Vec<ItemEnchantment>,
    /// Gems socketed in this item.
    #[serde(default)]
    pub gems: Vec<String>,
    /// Custom name (player-named items).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    /// Timestamp when item was acquired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquired_at: Option<u64>,
    /// Custom data for scripting.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

impl ItemStack {
    pub fn new(template_id: impl Into<String>, quantity: u32) -> Self {
        Self {
            template_id: template_id.into(),
            quantity,
            instance_id: None,
            durability: None,
            bound: false,
            item_level: None,
            enchantments: Vec::new(),
            gems: Vec::new(),
            custom_name: None,
            acquired_at: None,
            custom_data: HashMap::new(),
        }
    }

    pub fn with_instance_id(mut self, id: impl Into<String>) -> Self {
        self.instance_id = Some(id.into());
        self
    }

    pub fn with_durability(mut self, durability: u32) -> Self {
        self.durability = Some(durability);
        self
    }

    pub fn bound(mut self) -> Self {
        self.bound = true;
        self
    }

    pub fn with_enchantment(mut self, enchantment: ItemEnchantment) -> Self {
        self.enchantments.push(enchantment);
        self
    }

    /// Check if this stack is full.
    pub fn is_full(&self, max_stack: u32) -> bool {
        self.quantity >= max_stack
    }

    /// Check if this stack is empty.
    pub fn is_empty(&self) -> bool {
        self.quantity == 0
    }

    /// Add to stack, returns overflow.
    pub fn add(&mut self, amount: u32, max_stack: u32) -> u32 {
        let space = max_stack - self.quantity;
        if amount <= space {
            self.quantity += amount;
            0
        } else {
            self.quantity = max_stack;
            amount - space
        }
    }

    /// Remove from stack, returns true if stack is now empty.
    pub fn remove(&mut self, amount: u32) -> bool {
        if amount >= self.quantity {
            self.quantity = 0;
            true
        } else {
            self.quantity -= amount;
            false
        }
    }

    /// Split this stack, returning a new stack with the specified amount.
    pub fn split(&mut self, amount: u32) -> Option<ItemStack> {
        if amount >= self.quantity {
            None
        } else {
            let mut new_stack = self.clone();
            new_stack.quantity = amount;
            self.quantity -= amount;
            Some(new_stack)
        }
    }

    /// Get the display name key.
    pub fn display_name_key(&self) -> &str {
        self.custom_name.as_deref().unwrap_or("")
    }
}

/// Enchantment applied to an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemEnchantment {
    /// Enchantment ID.
    pub id: String,
    /// Stat modifiers from enchantment.
    pub stat_modifiers: Vec<StatModifier>,
    /// Enchantment level.
    #[serde(default)]
    pub level: u32,
    /// Duration in seconds (0 = permanent).
    #[serde(default)]
    pub duration: f32,
}

// ---------------------------------------------------------------------------
// Item Registry
// ---------------------------------------------------------------------------

/// Registry of all item templates.
#[derive(Debug, Clone, Default)]
pub struct ItemRegistry {
    templates: HashMap<String, ItemTemplate>,
}

impl ItemRegistry {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Register an item template.
    pub fn register(&mut self, template: ItemTemplate) {
        self.templates.insert(template.id.clone(), template);
    }

    /// Get a template by ID.
    pub fn get(&self, id: &str) -> Option<&ItemTemplate> {
        self.templates.get(id)
    }

    /// Remove a template by ID.
    pub fn remove(&mut self, id: &str) -> Option<ItemTemplate> {
        self.templates.remove(id)
    }

    /// Check if a template exists.
    pub fn contains(&self, id: &str) -> bool {
        self.templates.contains_key(id)
    }

    /// Get all templates.
    pub fn all(&self) -> impl Iterator<Item = &ItemTemplate> {
        self.templates.values()
    }

    /// Get templates by type.
    pub fn by_type(&self, item_type: &ItemType) -> Vec<&ItemTemplate> {
        self.templates
            .values()
            .filter(|t| &t.item_type == item_type)
            .collect()
    }

    /// Get templates by rarity.
    pub fn by_rarity(&self, rarity: ItemRarity) -> Vec<&ItemTemplate> {
        self.templates
            .values()
            .filter(|t| t.rarity == rarity)
            .collect()
    }

    /// Get templates by tag.
    pub fn by_tag(&self, tag: &str) -> Vec<&ItemTemplate> {
        self.templates
            .values()
            .filter(|t| t.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Search templates by name key (partial match).
    pub fn search(&self, query: &str) -> Vec<&ItemTemplate> {
        self.templates
            .values()
            .filter(|t| {
                t.name_key.to_lowercase().contains(&query.to_lowercase())
                    || t.id.to_lowercase().contains(&query.to_lowercase())
            })
            .collect()
    }

    /// Load templates from JSON file.
    pub fn load_from_json(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let templates: Vec<ItemTemplate> = serde_json::from_str(json)?;
        for template in templates {
            self.register(template);
        }
        Ok(())
    }

    /// Export all templates to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let templates: Vec<&ItemTemplate> = self.templates.values().collect();
        serde_json::to_string_pretty(&templates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_template_creation() {
        let sword = ItemTemplate::new("iron_sword")
            .with_name("item.iron_sword.name")
            .with_item_type(ItemType::Weapon(WeaponType::Sword))
            .with_rarity(ItemRarity::Common)
            .with_max_stack(1)
            .with_value(100);

        assert_eq!(sword.id, "iron_sword");
        assert_eq!(sword.max_stack, 1);
        assert!(sword.is_equippable());
        assert!(!sword.is_stackable());
    }

    #[test]
    fn test_item_stack_operations() {
        let mut stack = ItemStack::new("health_potion", 10);

        // Add to stack
        let overflow = stack.add(5, 99);
        assert_eq!(stack.quantity, 15);
        assert_eq!(overflow, 0);

        // Add beyond max
        let overflow = stack.add(90, 99);
        assert_eq!(stack.quantity, 99);
        assert_eq!(overflow, 6);

        // Remove from stack
        let empty = stack.remove(50);
        assert!(!empty);
        assert_eq!(stack.quantity, 49);

        // Remove all
        let empty = stack.remove(49);
        assert!(empty);
        assert_eq!(stack.quantity, 0);
    }

    #[test]
    fn test_item_split() {
        let mut stack = ItemStack::new("iron_ore", 20);
        let split = stack.split(10);

        assert!(split.is_some());
        assert_eq!(stack.quantity, 10);
        assert_eq!(split.unwrap().quantity, 10);

        // Can't split more than available
        let fail_split = stack.split(15);
        assert!(fail_split.is_none());
    }

    #[test]
    fn test_rarity_color() {
        assert_eq!(ItemRarity::Common.color(), [0.6, 0.6, 0.6]);
        assert_eq!(ItemRarity::Legendary.color(), [1.0, 0.5, 0.0]);
        assert_eq!(ItemRarity::Epic.color(), [0.64, 0.21, 0.93]);
    }

    #[test]
    fn test_rarity_value_multiplier() {
        assert_eq!(ItemRarity::Common.value_multiplier(), 1.0);
        assert_eq!(ItemRarity::Rare.value_multiplier(), 2.5);
        assert_eq!(ItemRarity::Legendary.value_multiplier(), 10.0);
    }

    #[test]
    fn test_item_registry() {
        let mut registry = ItemRegistry::new();
        registry.register(
            ItemTemplate::new("test_item")
                .with_name("item.test.name")
                .add_tag("test")
        );

        assert!(registry.contains("test_item"));
        assert!(registry.get("test_item").is_some());
        assert_eq!(registry.by_tag("test").len(), 1);
        assert_eq!(registry.search("test").len(), 1);
    }
}
