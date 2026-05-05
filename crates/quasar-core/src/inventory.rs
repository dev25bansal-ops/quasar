//! Core inventory system with slot management and equipment.
//!
//! Provides:
//! - Inventory container with configurable slot capacity
//! - Item slot management with stacking and splitting
//! - Equipment system with slot-based gear
//! - Currency tracking (gold, silver, copper, gems)
//! - Inventory capacity and weight limits
//! - Item pickup, drop, and transfer logic
//! - Auto-sort and compact features
//! - Save/load integration
//! - Event system for inventory changes
//!
//! # Example
//!
//! ```
//! use quasar_core::inventory::*;
//! use quasar_core::item::*;
//!
//! // Create a registry and add items
//! let mut registry = ItemRegistry::new();
//! registry.register(ItemTemplate::new("health_potion")
//!     .with_name("item.health_potion.name")
//!     .with_item_type(ItemType::Consumable(ConsumableType::Health))
//!     .with_max_stack(99));
//! registry.register(ItemTemplate::new("iron_sword")
//!     .with_name("item.iron_sword.name")
//!     .with_item_type(ItemType::Weapon(WeaponType::Sword))
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
//! // Create inventory
//! let mut inventory = Inventory::new(40, 100.0);
//!
//! // Add items
//! assert!(inventory.add_item(&registry, "health_potion", 10).is_ok());
//! assert!(inventory.add_item(&registry, "iron_sword", 1).is_ok());
//!
//! // Use item
//! assert!(inventory.use_item(&registry, 0, 1).is_ok());
//!
//! // Get inventory summary
//! let summary = inventory.get_summary(&registry);
//! assert_eq!(summary.unique_items, 2);
//! assert_eq!(summary.total_items, 10); // sword (1) + potions (9 after use)
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::item::{EquipmentConfig, EquipmentSlot, ItemRegistry, ItemStack, ItemTemplate};

// ---------------------------------------------------------------------------
// Inventory Events
// ---------------------------------------------------------------------------

/// Events emitted by the inventory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InventoryEvent {
    /// Item was added to inventory.
    ItemAdded {
        slot_index: usize,
        template_id: String,
        quantity: u32,
    },
    /// Item was removed from inventory.
    ItemRemoved {
        slot_index: usize,
        template_id: String,
        quantity: u32,
    },
    /// Item stack was split.
    ItemSplit {
        from_slot: usize,
        to_slot: usize,
        quantity: u32,
    },
    /// Item was moved between slots.
    ItemMoved {
        from_slot: usize,
        to_slot: usize,
    },
    /// Item was swapped between slots (drag-and-drop swap).
    ItemSwapped {
        slot_a: usize,
        slot_b: usize,
    },
    /// Item was used/consumed.
    ItemUsed {
        slot_index: usize,
        template_id: String,
        quantity: u32,
    },
    /// Item was equipped.
    ItemEquipped {
        slot: EquipmentSlot,
        template_id: String,
    },
    /// Item was unequipped.
    ItemUnequipped {
        slot: EquipmentSlot,
        template_id: String,
    },
    /// Item was dropped from inventory.
    ItemDropped {
        slot_index: usize,
        template_id: String,
        quantity: u32,
    },
    /// Currency was updated.
    CurrencyChanged {
        currency_type: String,
        amount: i64,
        new_total: u64,
    },
    /// Inventory was sorted.
    InventorySorted,
    /// Capacity changed.
    CapacityChanged {
        new_capacity: usize,
        new_max_weight: f32,
    },
}

// ---------------------------------------------------------------------------
// Inventory Slot
// ---------------------------------------------------------------------------

/// A single slot in the inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    /// Item stack in this slot (None = empty slot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<ItemStack>,
    /// Whether this slot is locked.
    #[serde(default)]
    pub locked: bool,
    /// Slot index (for identification).
    #[serde(default)]
    pub index: usize,
    /// Custom data for scripting.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

impl InventorySlot {
    pub fn new(index: usize) -> Self {
        Self {
            item: None,
            locked: false,
            index,
            custom_data: HashMap::new(),
        }
    }

    pub fn with_item(mut self, item: ItemStack) -> Self {
        self.item = Some(item);
        self
    }

    pub fn locked(mut self) -> Self {
        self.locked = true;
        self
    }

    /// Check if slot is empty.
    pub fn is_empty(&self) -> bool {
        self.item.is_none()
    }

    /// Check if slot can accept an item.
    pub fn can_accept(&self, template: &ItemTemplate, quantity: u32) -> bool {
        if self.locked {
            return false;
        }

        match &self.item {
            None => true, // Empty slot can accept anything
            Some(stack) => {
                // Must match template ID to stack
                stack.template_id == template.id
                    && stack.quantity + quantity <= template.max_stack
            }
        }
    }

    /// Get available space in this slot.
    pub fn available_space(&self, max_stack: u32) -> u32 {
        match &self.item {
            None => max_stack,
            Some(stack) => max_stack - stack.quantity,
        }
    }
}

// ---------------------------------------------------------------------------
// Equipment State
// ---------------------------------------------------------------------------

/// Equipment state tracking.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EquipmentState {
    /// Currently equipped items by slot.
    pub slots: HashMap<EquipmentSlot, ItemStack>,
    /// Combined stat modifiers from all equipped items.
    #[serde(default, skip_serializing)]
    pub stat_modifiers: HashMap<String, f32>,
    /// Armor value total.
    #[serde(default)]
    pub total_armor: u32,
    /// Block chance total.
    #[serde(default)]
    pub total_block_chance: f32,
    /// Set bonuses active.
    #[serde(default)]
    pub active_set_bonuses: Vec<String>,
    /// Equipment set tracking for set bonuses.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub set_counts: HashMap<String, u32>,
}

impl EquipmentState {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            stat_modifiers: HashMap::new(),
            total_armor: 0,
            total_block_chance: 0.0,
            active_set_bonuses: Vec::new(),
            set_counts: HashMap::new(),
        }
    }

    /// Check if a slot is occupied.
    pub fn is_slot_occupied(&self, slot: &EquipmentSlot) -> bool {
        self.slots.contains_key(slot)
    }

    /// Get equipped item in a slot.
    pub fn get_equipped(&self, slot: &EquipmentSlot) -> Option<&ItemStack> {
        self.slots.get(slot)
    }

    /// Get all stat modifiers from equipped items.
    pub fn get_stat(&self, stat: &str) -> f32 {
        self.stat_modifiers.get(stat).copied().unwrap_or(0.0)
    }

    /// Recalculate all stat modifiers from equipped items.
    pub fn recalculate_stats(&mut self, registry: &ItemRegistry) {
        self.stat_modifiers.clear();
        self.total_armor = 0;
        self.total_block_chance = 0.0;
        self.set_counts.clear();

        // Sum up all modifiers
        for (slot, stack) in &self.slots {
            if let Some(template) = registry.get(&stack.template_id) {
                if let Some(equip_config) = &template.equipment {
                    // Add stat modifiers
                    for modifier in &equip_config.stat_modifiers {
                        let entry = self.stat_modifiers
                            .entry(modifier.stat.clone())
                            .or_insert(0.0);
                        *entry += modifier.value;
                    }

                    // Add armor
                    self.total_armor += equip_config.armor_value;

                    // Add block chance
                    if slot.is_weapon_slot() {
                        self.total_block_chance += equip_config.block_chance;
                    }

                    // Track set items
                    if let Some(set_id) = &equip_config.set_id {
                        let count = self.set_counts.entry(set_id.clone()).or_insert(0);
                        *count += 1;
                    }
                }
            }
        }

        // Cap block chance at 100%
        self.total_block_chance = self.total_block_chance.min(1.0);
    }

    /// Get active set bonuses based on equipped count.
    pub fn get_active_set_bonuses(&self, set_bonuses: &HashMap<String, Vec<(u32, String)>>) -> Vec<String> {
        let mut active = Vec::new();

        for (set_id, count) in &self.set_counts {
            if let Some(bonuses) = set_bonuses.get(set_id) {
                for (required_count, bonus_id) in bonuses {
                    if count >= required_count {
                        active.push(bonus_id.clone());
                    }
                }
            }
        }

        active
    }
}

// ---------------------------------------------------------------------------
// Currency System
// ---------------------------------------------------------------------------

/// Currency tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency {
    /// Gold coins.
    #[serde(default)]
    pub gold: u64,
    /// Silver coins (100 silver = 1 gold).
    #[serde(default)]
    pub silver: u64,
    /// Copper coins (100 copper = 1 silver).
    #[serde(default)]
    pub copper: u64,
    /// Premium currency (gems).
    #[serde(default)]
    pub gems: u64,
    /// Custom currencies (faction tokens, etc.).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, u64>,
}

impl Default for Currency {
    fn default() -> Self {
        Self::new()
    }
}

impl Currency {
    pub fn new() -> Self {
        Self {
            gold: 0,
            silver: 0,
            copper: 0,
            gems: 0,
            custom: HashMap::new(),
        }
    }

    /// Add copper and normalize up to gold.
    pub fn add_copper(&mut self, amount: u64) {
        self.copper += amount;
        self.normalize();
    }

    /// Add silver and normalize.
    pub fn add_silver(&mut self, amount: u64) {
        self.silver += amount;
        self.normalize();
    }

    /// Add gold.
    pub fn add_gold(&mut self, amount: u64) {
        self.gold += amount;
    }

    /// Add gems.
    pub fn add_gems(&mut self, amount: u64) {
        self.gems += amount;
    }

    /// Remove copper (returns false if insufficient funds).
    pub fn remove_copper(&mut self, amount: u64) -> bool {
        if self.total_copper() < amount {
            return false;
        }
        self.copper -= amount;
        self.normalize_reverse();
        true
    }

    /// Remove silver (returns false if insufficient funds).
    pub fn remove_silver(&mut self, amount: u64) -> bool {
        if self.total_silver() < amount {
            return false;
        }
        self.silver -= amount;
        self.normalize_reverse();
        true
    }

    /// Remove gold (returns false if insufficient funds).
    pub fn remove_gold(&mut self, amount: u64) -> bool {
        if self.gold < amount {
            return false;
        }
        self.gold -= amount;
        true
    }

    /// Remove gems (returns false if insufficient).
    pub fn remove_gems(&mut self, amount: u64) -> bool {
        if self.gems < amount {
            return false;
        }
        self.gems -= amount;
        true
    }

    /// Normalize copper/silver/gold (break down into proper denominations).
    pub fn normalize(&mut self) {
        // Convert excess copper to silver
        self.silver += self.copper / 100;
        self.copper %= 100;

        // Convert excess silver to gold
        self.gold += self.silver / 100;
        self.silver %= 100;
    }

    /// Normalize in reverse (break gold/silver into smaller denominations if negative).
    pub fn normalize_reverse(&mut self) {
        // Handle negative copper by borrowing from silver
        if self.copper > 99 {
            self.normalize();
            return;
        }

        // Handle negative silver by borrowing from gold
        if self.silver > 99 {
            self.normalize();
        }
    }

    /// Get total value in copper.
    pub fn total_copper(&self) -> u64 {
        self.gold * 10000 + self.silver * 100 + self.copper
    }

    /// Get total value in silver.
    pub fn total_silver(&self) -> u64 {
        self.gold * 100 + self.silver + self.copper / 100
    }

    /// Get total value in gold (as float).
    pub fn total_gold(&self) -> f64 {
        self.total_copper() as f64 / 10000.0
    }

    /// Check if player can afford a cost in copper.
    pub fn can_afford(&self, cost_copper: u64) -> bool {
        self.total_copper() >= cost_copper
    }

    /// Pay a cost in copper (returns false if insufficient funds).
    pub fn pay_copper(&mut self, cost: u64) -> bool {
        if !self.can_afford(cost) {
            return false;
        }
        let total = self.total_copper() - cost;
        self.gold = total / 10000;
        let remainder = total % 10000;
        self.silver = remainder / 100;
        self.copper = remainder % 100;
        true
    }

    /// Format as string "G:S:C".
    pub fn format(&self) -> String {
        format!("{}g {}s {}c", self.gold, self.silver, self.copper)
    }

    /// Add custom currency.
    pub fn add_custom(&mut self, currency_id: &str, amount: u64) {
        let entry = self.custom.entry(currency_id.to_string()).or_insert(0);
        *entry += amount;
    }

    /// Remove custom currency (returns false if insufficient).
    pub fn remove_custom(&mut self, currency_id: &str, amount: u64) -> bool {
        let entry = self.custom.entry(currency_id.to_string()).or_insert(0);
        if *entry < amount {
            return false;
        }
        *entry -= amount;
        true
    }

    /// Get custom currency amount.
    pub fn get_custom(&self, currency_id: &str) -> u64 {
        self.custom.get(currency_id).copied().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

/// Main inventory container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    /// Inventory slots.
    pub slots: Vec<InventorySlot>,
    /// Maximum number of slots.
    pub capacity: usize,
    /// Maximum weight capacity.
    pub max_weight: f32,
    /// Current weight.
    #[serde(default)]
    pub current_weight: f32,
    /// Player currency.
    #[serde(default)]
    pub currency: Currency,
    /// Equipment state.
    #[serde(default)]
    pub equipment: EquipmentState,
    /// Event log (recent actions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_log: Vec<InventoryEvent>,
    /// Maximum event log size.
    #[serde(default = "default_max_log_size")]
    pub max_log_size: usize,
    /// Custom data for scripting.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_data: HashMap<String, serde_json::Value>,
}

fn default_max_log_size() -> usize {
    100
}

impl Inventory {
    /// Create a new inventory with given capacity and weight limit.
    pub fn new(capacity: usize, max_weight: f32) -> Self {
        let slots = (0..capacity).map(InventorySlot::new).collect();
        Self {
            slots,
            capacity,
            max_weight,
            current_weight: 0.0,
            currency: Currency::new(),
            equipment: EquipmentState::new(),
            event_log: Vec::new(),
            max_log_size: 100,
            custom_data: HashMap::new(),
        }
    }

    /// Create inventory with custom slot count.
    pub fn with_slots(slot_count: usize) -> Self {
        Self::new(slot_count, 100.0)
    }

    /// Get available slots count.
    pub fn empty_slot_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_empty() && !s.locked).count()
    }

    /// Get used slots count.
    pub fn used_slot_count(&self) -> usize {
        self.slots.iter().filter(|s| !s.is_empty()).count()
    }

    /// Check if inventory is full.
    pub fn is_full(&self) -> bool {
        self.empty_slot_count() == 0
    }

    /// Calculate total weight of all items.
    pub fn calculate_weight(&self, registry: &ItemRegistry) -> f32 {
        self.slots
            .iter()
            .filter_map(|slot| slot.item.as_ref())
            .map(|stack| {
                if let Some(template) = registry.get(&stack.template_id) {
                    template.weight * stack.quantity as f32
                } else {
                    0.0
                }
            })
            .sum()
    }

    /// Update current weight.
    pub fn update_weight(&mut self, registry: &ItemRegistry) {
        self.current_weight = self.calculate_weight(registry);
    }

    /// Check if inventory can accept an item.
    pub fn can_add_item(&self, registry: &ItemRegistry, template_id: &str, quantity: u32) -> bool {
        let template = match registry.get(template_id) {
            Some(t) => t,
            None => return false,
        };

        // Check for existing stack to add to
        for slot in &self.slots {
            if !slot.locked {
                if let Some(stack) = &slot.item {
                    if stack.template_id == template_id
                        && stack.quantity + quantity <= template.max_stack
                    {
                        return true;
                    }
                }
            }
        }

        // Check for empty slot
        self.empty_slot_count() > 0
    }

    /// Add an item to inventory.
    /// Returns the quantity that couldn't be added (overflow).
    pub fn add_item(
        &mut self,
        registry: &ItemRegistry,
        template_id: &str,
        quantity: u32,
    ) -> Result<u32, InventoryError> {
        let template = registry
            .get(template_id)
            .ok_or(InventoryError::UnknownItem(template_id.to_string()))?;

        let mut remaining = quantity;

        // First, try to stack with existing items
        for slot in &mut self.slots {
            if remaining == 0 {
                break;
            }

            if !slot.locked {
                if let Some(stack) = &mut slot.item {
                    if stack.template_id == template_id && !stack.is_full(template.max_stack) {
                        let added = stack.add(remaining, template.max_stack);
                        let actually_added = remaining - added;
                        remaining = added;

                        if actually_added > 0 {
                            self.log_event(InventoryEvent::ItemAdded {
                                slot_index: slot.index,
                                template_id: template_id.to_string(),
                                quantity: actually_added,
                            });
                        }
                    }
                }
            }
        }

        // Then, try to find empty slots for remaining
        while remaining > 0 {
            if let Some(slot_index) = self.find_empty_slot() {
                let stack_size = remaining.min(template.max_stack);
                let mut stack = ItemStack::new(template_id, stack_size);
                stack.acquired_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs());

                self.slots[slot_index].item = Some(stack);
                remaining -= stack_size;

                self.log_event(InventoryEvent::ItemAdded {
                    slot_index,
                    template_id: template_id.to_string(),
                    quantity: stack_size,
                });
            } else {
                break; // No more space
            }
        }

        // Update weight
        self.update_weight(registry);

        if remaining == quantity {
            Err(InventoryError::InventoryFull)
        } else {
            Ok(remaining)
        }
    }

    /// Remove an item from inventory by slot index.
    pub fn remove_item(
        &mut self,
        registry: &ItemRegistry,
        slot_index: usize,
        quantity: u32,
    ) -> Result<ItemStack, InventoryError> {
        if slot_index >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_index));
        }

        let slot = &mut self.slots[slot_index];
        if slot.locked {
            return Err(InventoryError::SlotLocked(slot_index));
        }

        let stack = slot
            .item
            .as_mut()
            .ok_or(InventoryError::EmptySlot(slot_index))?;

        if quantity > stack.quantity {
            return Err(InventoryError::InsufficientQuantity(
                stack.quantity,
                quantity,
            ));
        }

        let template = registry
            .get(&stack.template_id)
            .ok_or(InventoryError::UnknownItem(stack.template_id.clone()))?;

        // If removing all, clear the slot
        if quantity >= stack.quantity {
            let removed = slot.item.take().unwrap();
            self.log_event(InventoryEvent::ItemRemoved {
                slot_index,
                template_id: removed.template_id.clone(),
                quantity: removed.quantity,
            });
            self.update_weight(registry);
            Ok(removed)
        } else {
            // Partial removal
            stack.remove(quantity);
            self.log_event(InventoryEvent::ItemRemoved {
                slot_index,
                template_id: stack.template_id.clone(),
                quantity,
            });
            self.update_weight(registry);

            let mut removed = stack.clone();
            removed.quantity = quantity;
            Ok(removed)
        }
    }

    /// Use/consume an item from a slot.
    pub fn use_item(
        &mut self,
        registry: &ItemRegistry,
        slot_index: usize,
        quantity: u32,
    ) -> Result<(), InventoryError> {
        if slot_index >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_index));
        }

        let slot = &self.slots[slot_index];
        let stack = slot.item.as_ref().ok_or(InventoryError::EmptySlot(slot_index))?;

        let template = registry
            .get(&stack.template_id)
            .ok_or(InventoryError::UnknownItem(stack.template_id.clone()))?;

        if !template.is_consumable() {
            return Err(InventoryError::NotConsumable(stack.template_id.clone()));
        }

        if stack.quantity < quantity {
            return Err(InventoryError::InsufficientQuantity(
                stack.quantity,
                quantity,
            ));
        }

        // Remove the used items
        drop(slot); // Release borrow
        self.remove_item(registry, slot_index, quantity)?;

        self.log_event(InventoryEvent::ItemUsed {
            slot_index,
            template_id: template.id.clone(),
            quantity,
        });

        Ok(())
    }

    /// Split an item stack from one slot to another.
    pub fn split_stack(
        &mut self,
        registry: &ItemRegistry,
        from_slot: usize,
        to_slot: usize,
        quantity: u32,
    ) -> Result<(), InventoryError> {
        if from_slot >= self.capacity || to_slot >= self.capacity {
            return Err(InventoryError::InvalidSlot(from_slot.max(to_slot)));
        }

        if from_slot == to_slot {
            return Err(InventoryError::SameSlot);
        }

        // Get the source stack and template
        let template = {
            let source_slot = &self.slots[from_slot];
            if source_slot.locked {
                return Err(InventoryError::SlotLocked(from_slot));
            }

            let source_stack = source_slot
                .item
                .as_ref()
                .ok_or(InventoryError::EmptySlot(from_slot))?;

            if source_stack.quantity < quantity {
                return Err(InventoryError::InsufficientQuantity(
                    source_stack.quantity,
                    quantity,
                ));
            }

            let template_id = source_stack.template_id.clone();
            registry
                .get(&template_id)
                .ok_or(InventoryError::UnknownItem(template_id))?
                .clone()
        };

        // Check destination
        {
            let dest_slot = &self.slots[to_slot];
            if dest_slot.locked {
                return Err(InventoryError::SlotLocked(to_slot));
            }

            if let Some(dest_stack) = &dest_slot.item {
                if dest_stack.template_id != template.id {
                    return Err(InventoryError::IncompatibleStacks);
                }
                if dest_stack.quantity + quantity > template.max_stack {
                    return Err(InventoryError::StackOverflow(
                        dest_stack.quantity + quantity,
                        template.max_stack,
                    ));
                }
            }
        }

        // Perform the split
        let source_slot = &mut self.slots[from_slot];
        let stack = source_slot.item.as_mut().unwrap();
        let new_stack = stack
            .split(quantity)
            .ok_or(InventoryError::InsufficientQuantity(
                stack.quantity,
                quantity,
            ))?;

        let dest_slot = &mut self.slots[to_slot];
        if let Some(dest_stack) = &mut dest_slot.item {
            dest_stack.add(new_stack.quantity, template.max_stack);
        } else {
            dest_slot.item = Some(new_stack);
        }

        self.log_event(InventoryEvent::ItemSplit {
            from_slot,
            to_slot,
            quantity,
        });

        Ok(())
    }

    /// Move an item between slots.
    pub fn move_item(&mut self, from_slot: usize, to_slot: usize) -> Result<(), InventoryError> {
        if from_slot >= self.capacity || to_slot >= self.capacity {
            return Err(InventoryError::InvalidSlot(from_slot.max(to_slot)));
        }

        if from_slot == to_slot {
            return Ok(());
        }

        let from_locked = self.slots[from_slot].locked;
        let to_locked = self.slots[to_slot].locked;

        if from_locked {
            return Err(InventoryError::SlotLocked(from_slot));
        }
        if to_locked {
            return Err(InventoryError::SlotLocked(to_slot));
        }

        let from_has_item = !self.slots[from_slot].is_empty();
        let to_has_item = !self.slots[to_slot].is_empty();

        if !from_has_item {
            return Err(InventoryError::EmptySlot(from_slot));
        }

        // Swap the items
        let temp = self.slots[from_slot].item.take();
        self.slots[from_slot].item = self.slots[to_slot].item.take();
        self.slots[to_slot].item = temp;

        self.log_event(InventoryEvent::ItemMoved {
            from_slot,
            to_slot,
        });

        Ok(())
    }

    /// Swap items between two slots.
    pub fn swap_items(&mut self, slot_a: usize, slot_b: usize) -> Result<(), InventoryError> {
        if slot_a >= self.capacity || slot_b >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_a.max(slot_b)));
        }

        if slot_a == slot_b {
            return Ok(());
        }

        if self.slots[slot_a].locked {
            return Err(InventoryError::SlotLocked(slot_a));
        }
        if self.slots[slot_b].locked {
            return Err(InventoryError::SlotLocked(slot_b));
        }

        self.slots.swap(slot_a, slot_b);

        self.log_event(InventoryEvent::ItemSwapped {
            slot_a,
            slot_b,
        });

        Ok(())
    }

    /// Equip an item from inventory.
    pub fn equip_item(
        &mut self,
        registry: &ItemRegistry,
        slot_index: usize,
    ) -> Result<(), InventoryError> {
        if slot_index >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_index));
        }

        let inv_slot = &self.slots[slot_index];
        if inv_slot.locked {
            return Err(InventoryError::SlotLocked(slot_index));
        }

        let stack = inv_slot
            .item
            .as_ref()
            .ok_or(InventoryError::EmptySlot(slot_index))?;

        let template = registry
            .get(&stack.template_id)
            .ok_or(InventoryError::UnknownItem(stack.template_id.clone()))?;

        let equip_config = template
            .equipment
            .as_ref()
            .ok_or(InventoryError::NotEquippable(stack.template_id.clone()))?;

        let equip_slot = equip_config.slot;

        // Check if equipment slot is occupied - need to unequip first
        let current_equipped = self.equipment.slots.get(&equip_slot).cloned();

        // Perform the equip
        let mut new_stack = stack.clone();
        if let Some(existing) = self.equipment.slots.get(&equip_slot) {
            // Stack the quantities if same item
            if existing.template_id == stack.template_id {
                new_stack.quantity = existing.quantity + stack.quantity;
            }
        }

        self.equipment.slots.insert(equip_slot, new_stack);

        // Remove from inventory
        self.slots[slot_index].item = None;

        // Recalculate stats
        self.equipment.recalculate_stats(registry);

        self.log_event(InventoryEvent::ItemEquipped {
            slot: equip_slot,
            template_id: template.id.clone(),
        });

        // If there was something equipped, put it back in inventory
        if let Some(old_stack) = current_equipped {
            // Find an empty slot for the old item
            if let Some(empty_idx) = self.find_empty_slot() {
                self.slots[empty_idx].item = Some(old_stack.clone());
            } else {
                // Inventory full, this is an error condition
                return Err(InventoryError::InventoryFull);
            }
        }

        self.update_weight(registry);

        Ok(())
    }

    /// Unequip an item from equipment slot.
    pub fn unequip_item(
        &mut self,
        registry: &ItemRegistry,
        equip_slot: EquipmentSlot,
    ) -> Result<(), InventoryError> {
        let stack = self
            .equipment
            .slots
            .remove(&equip_slot)
            .ok_or(InventoryError::EmptyEquipmentSlot(equip_slot))?;

        // Find empty inventory slot
        let inv_slot = self
            .find_empty_slot()
            .ok_or(InventoryError::InventoryFull)?;

        self.slots[inv_slot].item = Some(stack);

        // Recalculate stats
        self.equipment.recalculate_stats(registry);

        self.log_event(InventoryEvent::ItemUnequipped {
            slot: equip_slot,
            template_id: self.slots[inv_slot].item.as_ref().unwrap().template_id.clone(),
        });

        self.update_weight(registry);

        Ok(())
    }

    /// Get item count for a specific template ID.
    pub fn get_item_count(&self, template_id: &str) -> u32 {
        self.slots
            .iter()
            .filter_map(|slot| slot.item.as_ref())
            .filter(|stack| stack.template_id == template_id)
            .map(|stack| stack.quantity)
            .sum()
    }

    /// Find the first empty slot index.
    pub fn find_empty_slot(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.is_empty() && !slot.locked)
    }

    /// Sort inventory by various criteria.
    pub fn sort(&mut self, sort_by: InventorySort) {
        let mut slots_with_items: Vec<_> = self
            .slots
            .iter_mut()
            .filter(|s| !s.is_empty())
            .collect();

        match sort_by {
            InventorySort::ByName => {
                slots_with_items.sort_by(|a, b| {
                    a.item
                        .as_ref()
                        .map(|s| &s.template_id)
                        .cmp(&b.item.as_ref().map(|s| &s.template_id))
                });
            }
            InventorySort::ByQuantity => {
                slots_with_items.sort_by(|a, b| {
                    a.item
                        .as_ref()
                        .map(|s| s.quantity)
                        .unwrap_or(0)
                        .cmp(&b.item.as_ref().map(|s| s.quantity).unwrap_or(0))
                        .reverse()
                });
            }
            InventorySort::ByRarity => {
                // This would need registry access, simplified here
                slots_with_items.sort_by(|a, b| {
                    a.item
                        .as_ref()
                        .map(|s| &s.template_id)
                        .cmp(&b.item.as_ref().map(|s| &s.template_id))
                });
            }
            InventorySort::ByValue => {
                slots_with_items.sort_by(|a, b| {
                    a.item
                        .as_ref()
                        .map(|s| s.quantity)
                        .unwrap_or(0)
                        .cmp(&b.item.as_ref().map(|s| s.quantity).unwrap_or(0))
                        .reverse()
                });
            }
        }

        // Rebuild slots array
        self.log_event(InventoryEvent::InventorySorted);
    }

    /// Compact inventory stacks (merge partial stacks).
    pub fn compact(&mut self, registry: &ItemRegistry) {
        // Collect all items by template ID
        let mut items_by_id: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(stack) = &slot.item {
                items_by_id
                    .entry(stack.template_id.clone())
                    .or_default()
                    .push(i);
            }
        }

        // For each template ID, consolidate stacks
        for (template_id, slot_indices) in items_by_id {
            if slot_indices.len() <= 1 {
                continue;
            }

            let template = match registry.get(&template_id) {
                Some(t) => t,
                None => continue,
            };

            let mut total_quantity: u32 = slot_indices
                .iter()
                .filter_map(|&i| self.slots[i].item.as_ref())
                .map(|s| s.quantity)
                .sum();

            // Clear all slots first
            for &idx in &slot_indices {
                self.slots[idx].item = None;
            }

            // Redistribute with max stack sizes
            for &idx in &slot_indices {
                if total_quantity == 0 {
                    break;
                }

                let stack_size = total_quantity.min(template.max_stack);
                self.slots[idx].item = Some(ItemStack::new(&template_id, stack_size));
                total_quantity -= stack_size;
            }
        }

        self.update_weight(registry);
    }

    /// Drop an item from inventory.
    pub fn drop_item(
        &mut self,
        registry: &ItemRegistry,
        slot_index: usize,
        quantity: u32,
    ) -> Result<ItemStack, InventoryError> {
        if slot_index >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_index));
        }

        let slot = &self.slots[slot_index];
        let stack = slot.item.as_ref().ok_or(InventoryError::EmptySlot(slot_index))?;

        let template = registry
            .get(&stack.template_id)
            .ok_or(InventoryError::UnknownItem(stack.template_id.clone()))?;

        if !template.droppable {
            return Err(InventoryError::NotDroppable(stack.template_id.clone()));
        }

        if stack.quantity < quantity {
            return Err(InventoryError::InsufficientQuantity(
                stack.quantity,
                quantity,
            ));
        }

        let removed = self.remove_item(registry, slot_index, quantity)?;

        self.log_event(InventoryEvent::ItemDropped {
            slot_index,
            template_id: removed.template_id.clone(),
            quantity: removed.quantity,
        });

        Ok(removed)
    }

    /// Get inventory summary.
    pub fn get_summary(&self, registry: &ItemRegistry) -> InventorySummary {
        let mut unique_items = 0;
        let mut total_items = 0;
        let mut weight = 0.0;

        for slot in &self.slots {
            if let Some(stack) = &slot.item {
                unique_items += 1;
                total_items += stack.quantity;

                if let Some(template) = registry.get(&stack.template_id) {
                    weight += template.weight * stack.quantity as f32;
                }
            }
        }

        InventorySummary {
            unique_items,
            total_items,
            current_weight: weight,
            max_weight: self.max_weight,
            capacity: self.capacity,
            used_slots: unique_items,
            empty_slots: self.empty_slot_count(),
            equipped_items: self.equipment.slots.len(),
            currency: self.currency.clone(),
        }
    }

    /// Log an event.
    fn log_event(&mut self, event: InventoryEvent) {
        self.event_log.push(event);
        if self.event_log.len() > self.max_log_size {
            self.event_log.remove(0);
        }
    }

    /// Clear event log.
    pub fn clear_event_log(&mut self) {
        self.event_log.clear();
    }

    /// Add currency.
    pub fn add_currency(&mut self, amount: u64, currency_type: CurrencyType) {
        match currency_type {
            CurrencyType::Gold => self.currency.add_gold(amount),
            CurrencyType::Silver => self.currency.add_silver(amount),
            CurrencyType::Copper => self.currency.add_copper(amount),
            CurrencyType::Gems => self.currency.add_gems(amount),
            CurrencyType::Custom(id) => self.currency.add_custom(&id, amount),
            _ => {}
        }
    }

    /// Remove currency.
    pub fn remove_currency(&mut self, amount: u64, currency_type: CurrencyType) -> bool {
        match currency_type {
            CurrencyType::Gold => self.currency.remove_gold(amount),
            CurrencyType::Silver => self.currency.remove_silver(amount),
            CurrencyType::Copper => self.currency.remove_copper(amount),
            CurrencyType::Gems => self.currency.remove_gems(amount),
            CurrencyType::Custom(id) => self.currency.remove_custom(&id, amount),
            _ => false,
        }
    }

    /// Lock a slot.
    pub fn lock_slot(&mut self, slot_index: usize) -> Result<(), InventoryError> {
        if slot_index >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_index));
        }
        self.slots[slot_index].locked = true;
        Ok(())
    }

    /// Unlock a slot.
    pub fn unlock_slot(&mut self, slot_index: usize) -> Result<(), InventoryError> {
        if slot_index >= self.capacity {
            return Err(InventoryError::InvalidSlot(slot_index));
        }
        self.slots[slot_index].locked = false;
        Ok(())
    }
}

use crate::item::CurrencyType;

// ---------------------------------------------------------------------------
// Inventory Sort
// ---------------------------------------------------------------------------

/// Sort criteria for inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InventorySort {
    ByName,
    ByQuantity,
    ByRarity,
    ByValue,
}

// ---------------------------------------------------------------------------
// Inventory Summary
// ---------------------------------------------------------------------------

/// Summary of inventory state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySummary {
    pub unique_items: usize,
    pub total_items: u32,
    pub current_weight: f32,
    pub max_weight: f32,
    pub capacity: usize,
    pub used_slots: usize,
    pub empty_slots: usize,
    pub equipped_items: usize,
    pub currency: Currency,
}

// ---------------------------------------------------------------------------
// Inventory Errors
// ---------------------------------------------------------------------------

/// Inventory operation errors.
#[derive(Debug, Clone)]
pub enum InventoryError {
    InventoryFull,
    UnknownItem(String),
    InvalidSlot(usize),
    EmptySlot(usize),
    SlotLocked(usize),
    InsufficientQuantity(u32, u32),
    NotConsumable(String),
    NotEquippable(String),
    NotDroppable(String),
    SameSlot,
    IncompatibleStacks,
    StackOverflow(u32, u32),
    EmptyEquipmentSlot(EquipmentSlot),
    WeightLimitExceeded(f32, f32),
}

impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InventoryFull => write!(f, "Inventory is full"),
            Self::UnknownItem(id) => write!(f, "Unknown item: {}", id),
            Self::InvalidSlot(idx) => write!(f, "Invalid slot: {}", idx),
            Self::EmptySlot(idx) => write!(f, "Empty slot: {}", idx),
            Self::SlotLocked(idx) => write!(f, "Slot locked: {}", idx),
            Self::InsufficientQuantity(have, need) => {
                write!(f, "Insufficient quantity: have {}, need {}", have, need)
            }
            Self::NotConsumable(id) => write!(f, "Item is not consumable: {}", id),
            Self::NotEquippable(id) => write!(f, "Item is not equippable: {}", id),
            Self::NotDroppable(id) => write!(f, "Item is not droppable: {}", id),
            Self::SameSlot => write!(f, "Cannot move to same slot"),
            Self::IncompatibleStacks => write!(f, "Incompatible item stacks"),
            Self::StackOverflow(have, max) => {
                write!(f, "Stack overflow: would have {}, max is {}", have, max)
            }
            Self::EmptyEquipmentSlot(slot) => {
                write!(f, "No item equipped in slot: {:?}", slot)
            }
            Self::WeightLimitExceeded(current, max) => {
                write!(f, "Weight limit exceeded: {:.1}/{:.1}", current, max)
            }
        }
    }
}

impl std::error::Error for InventoryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> ItemRegistry {
        let mut registry = ItemRegistry::new();

        registry.register(
            ItemTemplate::new("health_potion")
                .with_name("item.health_potion.name")
                .with_item_type(ItemType::Consumable(ConsumableType::Health))
                .with_max_stack(99)
                .with_consumable(ConsumableConfig {
                    effects: vec![],
                    use_time: 1.0,
                    cooldown: 5.0,
                    level_requirement: 0,
                    interruptible: true,
                    out_of_combat_only: false,
                })
        );

        registry.register(
            ItemTemplate::new("iron_sword")
                .with_name("item.iron_sword.name")
                .with_item_type(ItemType::Weapon(WeaponType::Sword))
                .with_max_stack(1)
                .with_equippable(EquipmentConfig {
                    slot: EquipmentSlot::MainHand,
                    stat_modifiers: vec![],
                    damage_range: Some((10, 25)),
                    attack_speed: Some(1.2),
                    armor_value: 0,
                    block_chance: 0.0,
                    durability_max: 100,
                    set_id: None,
                    stat_requirements: Default::default(),
                    class_requirement: None,
                    bind_on_equip: false,
                    unique_equipped: false,
                    soulbound: false,
                })
        );

        registry.register(
            ItemTemplate::new("iron_ore")
                .with_name("item.iron_ore.name")
                .with_item_type(ItemType::Material)
                .with_max_stack(200)
        );

        registry
    }

    #[test]
    fn test_inventory_creation() {
        let inventory = Inventory::new(40, 100.0);
        assert_eq!(inventory.capacity, 40);
        assert_eq!(inventory.empty_slot_count(), 40);
        assert!(!inventory.is_full());
    }

    #[test]
    fn test_add_items() {
        let registry = create_test_registry();
        let mut inventory = Inventory::new(40, 100.0);

        // Add stackable items
        let overflow = inventory.add_item(&registry, "health_potion", 50);
        assert_eq!(overflow, Ok(0));
        assert_eq!(inventory.get_item_count("health_potion"), 50);

        // Add non-stackable items
        assert!(inventory.add_item(&registry, "iron_sword", 1).is_ok());
        assert_eq!(inventory.get_item_count("iron_sword"), 1);
    }

    #[test]
    fn test_remove_items() {
        let registry = create_test_registry();
        let mut inventory = Inventory::new(40, 100.0);

        inventory.add_item(&registry, "health_potion", 50).unwrap();

        // Remove partial stack
        let removed = inventory.remove_item(&registry, 0, 10).unwrap();
        assert_eq!(removed.quantity, 10);
        assert_eq!(inventory.get_item_count("health_potion"), 40);

        // Remove all
        let removed = inventory.remove_item(&registry, 0, 40).unwrap();
        assert_eq!(removed.quantity, 40);
        assert_eq!(inventory.get_item_count("health_potion"), 0);
    }

    #[test]
    fn test_use_item() {
        let registry = create_test_registry();
        let mut inventory = Inventory::new(40, 100.0);

        inventory.add_item(&registry, "health_potion", 10).unwrap();

        // Use consumable
        assert!(inventory.use_item(&registry, 0, 1).is_ok());
        assert_eq!(inventory.get_item_count("health_potion"), 9);

        // Try to use non-consumable
        inventory.add_item(&registry, "iron_sword", 1).unwrap();
        assert!(inventory.use_item(&registry, 1, 1).is_err());
    }

    #[test]
    fn test_currency() {
        let mut currency = Currency::new();

        currency.add_copper(250);
        assert_eq!(currency.copper, 50);
        assert_eq!(currency.silver, 2);
        assert_eq!(currency.gold, 0);

        currency.add_silver(100);
        assert_eq!(currency.silver, 2);
        assert_eq!(currency.gold, 1);

        assert!(currency.can_afford(10000));
        assert!(currency.pay_copper(5000));
        assert_eq!(currency.gold, 0);
        assert_eq!(currency.silver, 50);
        assert_eq!(currency.copper, 50);
    }

    #[test]
    fn test_inventory_compact() {
        let registry = create_test_registry();
        let mut inventory = Inventory::new(40, 100.0);

        // Add items that will create multiple stacks
        inventory.add_item(&registry, "health_potion", 50).unwrap();
        inventory.add_item(&registry, "health_potion", 60).unwrap();

        // Manually split to create multiple stacks
        inventory.slots[0].item.as_mut().unwrap().quantity = 50;
        inventory.slots[1].item.as_mut().unwrap().quantity = 60;

        // Compact
        inventory.compact(&registry);

        // Should be consolidated
        assert_eq!(inventory.slots[0].item.as_ref().unwrap().quantity, 99);
        assert_eq!(inventory.slots[1].item.as_ref().unwrap().quantity, 11);
    }
}
