//! Inventory system template.
//!
//! Provides:
//! - Item definitions and instances
//! - Inventory slots with stacking
//! - Equipment system
//! - Item categories and filtering

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Item definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDefinition {
    /// Unique item ID.
    pub id: String,
    /// Item name localization key.
    pub name_key: String,
    /// Item description localization key.
    pub description_key: String,
    /// Item category.
    pub category: ItemCategory,
    /// Maximum stack size.
    pub max_stack: u32,
    /// Item rarity.
    pub rarity: ItemRarity,
    /// Item icon path.
    pub icon: Option<String>,
    /// Item model path.
    pub model: Option<String>,
    /// Item weight.
    pub weight: f32,
    /// Base value in gold.
    pub value: u32,
    /// Is this item equippable.
    pub equippable: bool,
    /// Equipment slot (if equippable).
    pub equipment_slot: Option<EquipmentSlot>,
    /// Item stats.
    pub stats: HashMap<String, f32>,
    /// Item tags for filtering.
    pub tags: Vec<String>,
    /// Is consumable.
    pub consumable: bool,
    /// Consumable effect.
    pub consume_effect: Option<ConsumeEffect>,
    /// Is quest item.
    pub quest_item: bool,
}

impl ItemDefinition {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name_key: String::new(),
            description_key: String::new(),
            category: ItemCategory::Misc,
            max_stack: 1,
            rarity: ItemRarity::Common,
            icon: None,
            model: None,
            weight: 0.0,
            value: 0,
            equippable: false,
            equipment_slot: None,
            stats: HashMap::new(),
            tags: Vec::new(),
            consumable: false,
            consume_effect: None,
            quest_item: false,
        }
    }

    pub fn with_name(mut self, key: impl Into<String>) -> Self {
        self.name_key = key.into();
        self
    }

    pub fn with_description(mut self, key: impl Into<String>) -> Self {
        self.description_key = key.into();
        self
    }

    pub fn with_category(mut self, category: ItemCategory) -> Self {
        self.category = category;
        self
    }

    pub fn with_stack_size(mut self, max_stack: u32) -> Self {
        self.max_stack = max_stack;
        self
    }

    pub fn with_rarity(mut self, rarity: ItemRarity) -> Self {
        self.rarity = rarity;
        self
    }

    pub fn equippable(mut self, slot: EquipmentSlot) -> Self {
        self.equippable = true;
        self.equipment_slot = Some(slot);
        self
    }

    pub fn consumable(mut self, effect: ConsumeEffect) -> Self {
        self.consumable = true;
        self.consume_effect = Some(effect);
        self
    }

    pub fn add_stat(mut self, name: impl Into<String>, value: f32) -> Self {
        self.stats.insert(name.into(), value);
        self
    }

    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn quest_item(mut self) -> Self {
        self.quest_item = true;
        self
    }
}

/// Item category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum ItemCategory {
    Weapon,
    Armor,
    Accessory,
    Consumable,
    Material,
    Quest,
    Key,
    Misc,
    Currency,
}

/// Item rarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum ItemRarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
    Unique,
}

impl ItemRarity {
    pub fn color(&self) -> [f32; 4] {
        match self {
            Self::Common => [0.7, 0.7, 0.7, 1.0],
            Self::Uncommon => [0.3, 1.0, 0.3, 1.0],
            Self::Rare => [0.3, 0.5, 1.0, 1.0],
            Self::Epic => [0.7, 0.3, 1.0, 1.0],
            Self::Legendary => [1.0, 0.6, 0.0, 1.0],
            Self::Unique => [1.0, 0.3, 0.3, 1.0],
        }
    }
}

/// Equipment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipmentSlot {
    MainHand,
    OffHand,
    Head,
    Chest,
    Hands,
    Legs,
    Feet,
    Accessory1,
    Accessory2,
    Necklace,
    Ring1,
    Ring2,
}

/// Consume effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsumeEffect {
    RestoreHealth {
        amount: f32,
    },
    RestoreMana {
        amount: f32,
    },
    RestoreStamina {
        amount: f32,
    },
    Buff {
        stat: String,
        amount: f32,
        duration: f32,
    },
    RemoveStatus {
        status: String,
    },
    Custom {
        id: String,
    },
}

/// Item instance in inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemInstance {
    /// Unique instance ID.
    pub instance_id: u64,
    /// Item definition ID.
    pub item_id: String,
    /// Stack count.
    pub count: u32,
    /// Custom stats (enchantments, etc.).
    pub custom_stats: HashMap<String, f32>,
    /// Durability (None = infinite).
    pub durability: Option<f32>,
    /// Max durability.
    pub max_durability: Option<f32>,
    /// Custom name (renamed item).
    pub custom_name: Option<String>,
}

impl ItemInstance {
    pub fn new(instance_id: u64, item_id: impl Into<String>, count: u32) -> Self {
        Self {
            instance_id,
            item_id: item_id.into(),
            count,
            custom_stats: HashMap::new(),
            durability: None,
            max_durability: None,
            custom_name: None,
        }
    }

    pub fn with_durability(mut self, current: f32, max: f32) -> Self {
        self.durability = Some(current);
        self.max_durability = Some(max);
        self
    }

    pub fn is_broken(&self) -> bool {
        self.durability.map(|d| d <= 0.0).unwrap_or(false)
    }

    pub fn durability_percent(&self) -> f32 {
        match (self.durability, self.max_durability) {
            (Some(current), Some(max)) => current / max,
            _ => 1.0,
        }
    }
}

/// Inventory slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    /// Slot index.
    pub index: usize,
    /// Item in slot (None if empty).
    pub item: Option<ItemInstance>,
    /// Is slot locked.
    pub locked: bool,
}

impl InventorySlot {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            item: None,
            locked: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.item.is_none()
    }
}

/// Inventory system.
pub struct Inventory {
    /// Item definitions.
    pub definitions: HashMap<String, ItemDefinition>,
    /// Inventory slots.
    pub slots: Vec<InventorySlot>,
    /// Equipment slots.
    pub equipment: HashMap<EquipmentSlot, Option<ItemInstance>>,
    /// Maximum slots.
    pub max_slots: usize,
    /// Next instance ID.
    pub next_instance_id: u64,
    /// Gold amount.
    pub gold: u32,
    /// Maximum gold.
    pub max_gold: u32,
}

impl Inventory {
    pub fn new(max_slots: usize) -> Self {
        let slots = (0..max_slots).map(InventorySlot::new).collect();

        let mut equipment = HashMap::new();
        equipment.insert(EquipmentSlot::MainHand, None);
        equipment.insert(EquipmentSlot::OffHand, None);
        equipment.insert(EquipmentSlot::Head, None);
        equipment.insert(EquipmentSlot::Chest, None);
        equipment.insert(EquipmentSlot::Hands, None);
        equipment.insert(EquipmentSlot::Legs, None);
        equipment.insert(EquipmentSlot::Feet, None);
        equipment.insert(EquipmentSlot::Accessory1, None);
        equipment.insert(EquipmentSlot::Accessory2, None);
        equipment.insert(EquipmentSlot::Necklace, None);
        equipment.insert(EquipmentSlot::Ring1, None);
        equipment.insert(EquipmentSlot::Ring2, None);

        Self {
            definitions: HashMap::new(),
            slots,
            equipment,
            max_slots,
            next_instance_id: 1,
            gold: 0,
            max_gold: 999999,
        }
    }

    pub fn register_item(&mut self, definition: ItemDefinition) {
        self.definitions.insert(definition.id.clone(), definition);
    }

    pub fn add_item(&mut self, item_id: &str, count: u32) -> bool {
        let definition = match self.definitions.get(item_id) {
            Some(d) => d,
            None => return false,
        };

        let mut remaining = count;

        // Try to stack with existing items
        if definition.max_stack > 1 {
            for slot in &mut self.slots {
                if let Some(ref mut item) = slot.item {
                    if item.item_id == item_id && item.count < definition.max_stack {
                        let space = definition.max_stack - item.count;
                        let add = remaining.min(space);
                        item.count += add;
                        remaining -= add;

                        if remaining == 0 {
                            return true;
                        }
                    }
                }
            }
        }

        // Add to empty slots
        while remaining > 0 {
            let empty_slot = self.slots.iter_mut().find(|s| s.is_empty() && !s.locked);

            let slot = match empty_slot {
                Some(s) => s,
                None => return false,
            };

            let add = remaining.min(definition.max_stack);
            let instance = ItemInstance::new(self.next_instance_id, item_id, add);
            self.next_instance_id += 1;

            slot.item = Some(instance);
            remaining -= add;
        }

        true
    }

    pub fn remove_item(&mut self, item_id: &str, count: u32) -> bool {
        let mut remaining = count;

        // Remove from slots (reverse order to remove partial stacks first)
        for slot in self.slots.iter_mut().rev() {
            if let Some(ref mut item) = slot.item {
                if item.item_id == item_id {
                    let remove = remaining.min(item.count);
                    item.count -= remove;
                    remaining -= remove;

                    if item.count == 0 {
                        slot.item = None;
                    }

                    if remaining == 0 {
                        return true;
                    }
                }
            }
        }

        remaining == 0
    }

    pub fn get_item_count(&self, item_id: &str) -> u32 {
        let mut count = 0;

        for slot in &self.slots {
            if let Some(ref item) = slot.item {
                if item.item_id == item_id {
                    count += item.count;
                }
            }
        }

        // Check equipment too
        for equipped in self.equipment.values() {
            if let Some(ref item) = equipped {
                if item.item_id == item_id {
                    count += item.count;
                }
            }
        }

        count
    }

    pub fn has_item(&self, item_id: &str, count: u32) -> bool {
        self.get_item_count(item_id) >= count
    }

    pub fn equip(&mut self, slot_index: usize) -> bool {
        let item = match self.slots.get(slot_index).and_then(|s| s.item.clone()) {
            Some(i) => i,
            None => return false,
        };

        let definition = match self.definitions.get(&item.item_id) {
            Some(d) => d.clone(),
            None => return false,
        };

        if !definition.equippable {
            return false;
        }

        let equip_slot = match definition.equipment_slot {
            Some(s) => s,
            None => return false,
        };

        // Remove from inventory
        self.slots[slot_index].item = None;

        // Unequip existing item
        let previous = self.equipment.get(&equip_slot).cloned();
        if let Some(Some(prev_item)) = previous {
            // Put previous item back in inventory
            let empty_slot = self.slots.iter_mut().find(|s| s.is_empty() && !s.locked);
            if let Some(slot) = empty_slot {
                slot.item = Some(prev_item);
            }
        }

        // Equip new item
        self.equipment.insert(equip_slot, Some(item));

        true
    }

    pub fn unequip(&mut self, slot: EquipmentSlot) -> bool {
        let item = match self.equipment.get(&slot).and_then(|i| i.clone()) {
            Some(i) => i,
            None => return false,
        };

        // Find empty slot
        let empty_slot = self.slots.iter_mut().find(|s| s.is_empty() && !s.locked);

        match empty_slot {
            Some(s) => {
                s.item = Some(item);
                self.equipment.insert(slot, None);
                true
            }
            None => false,
        }
    }

    pub fn get_equipped(&self, slot: EquipmentSlot) -> Option<&ItemInstance> {
        self.equipment.get(&slot).and_then(|i| i.as_ref())
    }

    pub fn use_item(&mut self, slot_index: usize) -> Option<ConsumeEffect> {
        let item = match self.slots.get(slot_index).and_then(|s| s.item.as_ref()) {
            Some(i) => i.clone(),
            None => return None,
        };

        let definition = match self.definitions.get(&item.item_id) {
            Some(d) => d.clone(),
            None => return None,
        };

        if !definition.consumable {
            return None;
        }

        let effect = definition.consume_effect.clone();

        // Remove one item from stack
        if let Some(ref mut slot) = self.slots.get_mut(slot_index) {
            if let Some(ref mut stack) = slot.item {
                stack.count -= 1;
                if stack.count == 0 {
                    slot.item = None;
                }
            }
        }

        effect
    }

    pub fn swap_slots(&mut self, from: usize, to: usize) {
        if from >= self.slots.len() || to >= self.slots.len() {
            return;
        }

        if self.slots[from].locked || self.slots[to].locked {
            return;
        }

        self.slots.swap(from, to);
    }

    pub fn split_stack(&mut self, slot_index: usize, count: u32) -> bool {
        let item = match self.slots.get(slot_index).and_then(|s| s.item.clone()) {
            Some(i) => i,
            None => return false,
        };

        if item.count <= count {
            return false;
        }

        let empty_slot = self.slots.iter_mut().find(|s| s.is_empty() && !s.locked);

        let new_slot = match empty_slot {
            Some(s) => s,
            None => return false,
        };

        // Create new stack
        new_slot.item = Some(ItemInstance::new(
            self.next_instance_id,
            item.item_id.clone(),
            count,
        ));
        self.next_instance_id += 1;

        // Update original stack
        if let Some(ref mut original) = self.slots.get_mut(slot_index).unwrap().item {
            original.count -= count;
        }

        true
    }

    pub fn merge_stacks(&mut self, from: usize, to: usize) -> bool {
        if from == to {
            return false;
        }

        let from_item = match self.slots.get(from).and_then(|s| s.item.as_ref()) {
            Some(i) => i.clone(),
            None => return false,
        };

        let to_item = match self.slots.get(to).and_then(|s| s.item.as_ref()) {
            Some(i) => i.clone(),
            None => return false,
        };

        if from_item.item_id != to_item.item_id {
            return false;
        }

        let definition = match self.definitions.get(&from_item.item_id) {
            Some(d) => d,
            None => return false,
        };

        let space = definition.max_stack - to_item.count;
        let move_count = from_item.count.min(space);

        if move_count == 0 {
            return false;
        }

        // Update destination stack
        if let Some(ref mut dest) = self.slots.get_mut(to).unwrap().item {
            dest.count += move_count;
        }

        // Update source stack
        if let Some(ref mut source) = self.slots.get_mut(from).unwrap().item {
            source.count -= move_count;
            if source.count == 0 {
                self.slots[from].item = None;
            }
        }

        true
    }

    pub fn get_total_weight(&self) -> f32 {
        let mut weight = 0.0;

        for slot in &self.slots {
            if let Some(ref item) = slot.item {
                if let Some(def) = self.definitions.get(&item.item_id) {
                    weight += def.weight * item.count as f32;
                }
            }
        }

        for equipped in self.equipment.values() {
            if let Some(ref item) = equipped {
                if let Some(def) = self.definitions.get(&item.item_id) {
                    weight += def.weight * item.count as f32;
                }
            }
        }

        weight
    }

    pub fn get_total_value(&self) -> u32 {
        let mut value = 0;

        for slot in &self.slots {
            if let Some(ref item) = slot.item {
                if let Some(def) = self.definitions.get(&item.item_id) {
                    value += def.value * item.count;
                }
            }
        }

        value
    }

    pub fn add_gold(&mut self, amount: u32) {
        self.gold = (self.gold + amount).min(self.max_gold);
    }

    pub fn remove_gold(&mut self, amount: u32) -> bool {
        if self.gold >= amount {
            self.gold -= amount;
            true
        } else {
            false
        }
    }

    pub fn sort(&mut self, by: SortCriteria) {
        self.slots
            .sort_by(|a, b| match (a.item.as_ref(), b.item.as_ref()) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(item_a), Some(item_b)) => {
                    let def_a = self.definitions.get(&item_a.item_id);
                    let def_b = self.definitions.get(&item_b.item_id);

                    match (def_a, def_b) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (Some(def_a), Some(def_b)) => match by {
                            SortCriteria::Name => def_a.name_key.cmp(&def_b.name_key),
                            SortCriteria::Category => def_a.category.cmp(&def_b.category),
                            SortCriteria::Rarity => def_b.rarity.cmp(&def_a.rarity),
                            SortCriteria::Value => def_b.value.cmp(&def_a.value),
                            SortCriteria::Weight => def_a
                                .weight
                                .partial_cmp(&def_b.weight)
                                .unwrap_or(std::cmp::Ordering::Equal),
                        },
                    }
                }
            });
    }

    pub fn filter(
        &self,
        category: Option<ItemCategory>,
        rarity: Option<ItemRarity>,
    ) -> Vec<&InventorySlot> {
        self.slots
            .iter()
            .filter(|slot| {
                if let Some(ref item) = slot.item {
                    if let Some(def) = self.definitions.get(&item.item_id) {
                        let category_match = category.map_or(true, |c| def.category == c);
                        let rarity_match = rarity.map_or(true, |r| def.rarity == r);
                        category_match && rarity_match
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortCriteria {
    Name,
    Category,
    Rarity,
    Value,
    Weight,
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new(40)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_definition() {
        let item = ItemDefinition::new("sword_001")
            .with_name("item.sword.name")
            .with_rarity(ItemRarity::Rare)
            .equippable(EquipmentSlot::MainHand);

        assert_eq!(item.id, "sword_001");
        assert!(item.equippable);
    }

    #[test]
    fn inventory_add_remove() {
        let mut inv = Inventory::new(10);
        inv.register_item(ItemDefinition::new("potion").with_stack_size(20));

        assert!(inv.add_item("potion", 5));
        assert_eq!(inv.get_item_count("potion"), 5);

        assert!(inv.remove_item("potion", 3));
        assert_eq!(inv.get_item_count("potion"), 2);
    }

    #[test]
    fn inventory_stacking() {
        let mut inv = Inventory::new(10);
        inv.register_item(ItemDefinition::new("arrow").with_stack_size(100));

        inv.add_item("arrow", 50);
        inv.add_item("arrow", 60);

        assert_eq!(inv.get_item_count("arrow"), 110);

        let stacks = inv.slots.iter().filter(|s| s.item.is_some()).count();
        assert_eq!(stacks, 2);
    }

    #[test]
    fn inventory_equip() {
        let mut inv = Inventory::new(10);
        inv.register_item(ItemDefinition::new("helmet").equippable(EquipmentSlot::Head));

        inv.add_item("helmet", 1);
        assert!(inv.equip(0));
        assert!(inv.get_equipped(EquipmentSlot::Head).is_some());
    }

    #[test]
    fn gold_operations() {
        let mut inv = Inventory::new(10);

        inv.add_gold(100);
        assert_eq!(inv.gold, 100);

        assert!(inv.remove_gold(50));
        assert_eq!(inv.gold, 50);

        assert!(!inv.remove_gold(100));
        assert_eq!(inv.gold, 50);
    }
}
